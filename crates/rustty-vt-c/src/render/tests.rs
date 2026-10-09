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
    with_test_allocator(|alloc| {
        // var cols: size.CellCountInt = 0;
        let mut cols: size::CellCountInt = 0;
        // try testing.expectEqual(Result.invalid_value, get(null, .cols, @ptrCast(&cols)));
        assert_eq!(Result::InvalidValue, unsafe {
            get(
                std::ptr::null_mut(),
                RenderStateData::Cols,
                &mut cols as *mut _ as *mut c_void,
            )
        });

        // var state: RenderState = null;
        let mut state: RenderState = std::ptr::null_mut();
        // try testing.expectEqual(Result.success, new(&lib.alloc.test_allocator, &state));
        assert_eq!(Result::Success, unsafe { new(Some(alloc), &mut state) });
        // defer free(state);
        // try testing.expectEqual(Result.invalid_value, get(state, .cols, null));
        assert_eq!(Result::InvalidValue, unsafe {
            get(state, RenderStateData::Cols, std::ptr::null_mut())
        });

        unsafe { free(state) };
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
            get(state, RenderStateData::Invalid, std::ptr::null_mut())
        });

        unsafe { free(state) };
    });
}

// test "render: aggregate get invalid value"
#[test]
fn render_aggregate_get_invalid_value() {
    with_test_allocator(|alloc| {
        let mut state: RenderState = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe { new(Some(alloc), &mut state) });
        // defer free(state);

        // var colors: Colors = std.mem.zeroes(Colors);
        let mut colors: Colors = unsafe { std::mem::zeroed() };
        // colors.size = @sizeOf(Colors);
        colors.size = std::mem::size_of::<Colors>();

        // try testing.expectEqual(Result.invalid_value, get(null, .colors, &colors));
        assert_eq!(Result::InvalidValue, unsafe {
            get(
                std::ptr::null_mut(),
                RenderStateData::Colors,
                &mut colors as *mut _ as *mut c_void,
            )
        });
        // try testing.expectEqual(Result.invalid_value, get(state, .colors, null));
        assert_eq!(Result::InvalidValue, unsafe {
            get(state, RenderStateData::Colors, std::ptr::null_mut())
        });

        // colors.size = @sizeOf(usize) - 1;
        colors.size = std::mem::size_of::<usize>() - 1;
        // try testing.expectEqual(Result.invalid_value, get(state, .colors, &colors));
        assert_eq!(Result::InvalidValue, unsafe {
            get(
                state,
                RenderStateData::Colors,
                &mut colors as *mut _ as *mut c_void,
            )
        });

        // var cursor: Cursor = std.mem.zeroes(Cursor);
        let mut cursor: Cursor = unsafe { std::mem::zeroed() };
        // cursor.size = @sizeOf(Cursor);
        cursor.size = std::mem::size_of::<Cursor>();

        // try testing.expectEqual(Result.invalid_value, get(null, .cursor, &cursor));
        assert_eq!(Result::InvalidValue, unsafe {
            get(
                std::ptr::null_mut(),
                RenderStateData::Cursor,
                &mut cursor as *mut _ as *mut c_void,
            )
        });
        // try testing.expectEqual(Result.invalid_value, get(state, .cursor, null));
        assert_eq!(Result::InvalidValue, unsafe {
            get(state, RenderStateData::Cursor, std::ptr::null_mut())
        });

        // cursor.size = @sizeOf(usize) - 1;
        cursor.size = std::mem::size_of::<usize>() - 1;
        // try testing.expectEqual(Result.invalid_value, get(state, .cursor, &cursor));
        assert_eq!(Result::InvalidValue, unsafe {
            get(
                state,
                RenderStateData::Cursor,
                &mut cursor as *mut _ as *mut c_void,
            )
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
            RenderStateData::Dirty,
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
            get(
                state,
                RenderStateData::Dirty,
                &mut dirty as *mut _ as *mut c_void,
            )
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
            get(
                state,
                RenderStateData::Dirty,
                &mut dirty as *mut _ as *mut c_void,
            )
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
            get(
                state,
                RenderStateData::Dirty,
                &mut dirty as *mut _ as *mut c_void,
            )
        });
        // try testing.expectEqual(Dirty.full, dirty);
        assert_eq!(Dirty::Full, dirty);

        unsafe { free(state) };
    });
}

// test "render: clean"
#[test]
fn render_clean() {
    with_test_allocator(|alloc| {
        // var terminal: terminal_c.Terminal = null;
        let mut terminal: terminal_c::Terminal = std::ptr::null_mut();
        // try testing.expectEqual(Result.success, terminal_c.new(&lib.alloc.test_allocator, &terminal, 10, 3));
        assert_eq!(Result::Success, unsafe {
            terminal_c::new(alloc, &mut terminal, 10, 3)
        });
        // defer terminal_c.free(terminal);

        // var state: RenderState = null;
        let mut state: RenderState = std::ptr::null_mut();
        // try testing.expectEqual(Result.success, new(&lib.alloc.test_allocator, &state));
        assert_eq!(Result::Success, unsafe { new(Some(alloc), &mut state) });
        // defer free(state);

        // try testing.expectEqual(Result.invalid_value, clean(null));
        assert_eq!(Result::InvalidValue, unsafe { clean(std::ptr::null_mut()) });
        // try testing.expectEqual(Result.success, update(state, terminal));
        assert_eq!(Result::Success, unsafe { update(state, terminal) });
        // try testing.expectEqual(Dirty.full, state.?.state.dirty);
        assert_eq!(RenderDirty::Full, unsafe { (*state).state.dirty });

        // try testing.expectEqual(Result.success, clean(state));
        assert_eq!(Result::Success, unsafe { clean(state) });
        // try testing.expectEqual(Dirty.false, state.?.state.dirty);
        assert_eq!(RenderDirty::False, unsafe { (*state).state.dirty });
        // for (state.?.state.row_data.items(.dirty)) |dirty| try testing.expect(!dirty);
        for dirty in unsafe { &(*state).state.row_data }.iter().map(|r| r.dirty) {
            assert!(!dirty);
        }

        // Cleaning is idempotent, and an unchanged update remains clean.
        // try testing.expectEqual(Result.success, clean(state));
        assert_eq!(Result::Success, unsafe { clean(state) });
        // try testing.expectEqual(Result.success, update(state, terminal));
        assert_eq!(Result::Success, unsafe { update(state, terminal) });
        // try testing.expectEqual(Dirty.false, state.?.state.dirty);
        assert_eq!(RenderDirty::False, unsafe { (*state).state.dirty });
        // for (state.?.state.row_data.items(.dirty)) |dirty| try testing.expect(!dirty);
        for dirty in unsafe { &(*state).state.row_data }.iter().map(|r| r.dirty) {
            assert!(!dirty);
        }

        unsafe { free(state) };
        unsafe { terminal_c::free(terminal) };
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
// terminal and render state, hands them (with their UNASSERTED `new` Results)
// to the test closure, then tears down — the teardown is the equivalent of the
// reference's `defer` chain. The two `Result.success` expectEqual ASSERTIONS of
// the reference preamble live in each test body (assertion-for-assertion; G2
// counts source-level asserts per test).
fn with_terminal_state<F: FnOnce(Result, terminal_c::Terminal, Result, RenderState)>(
    alloc: &rustty_lib::allocator::Allocator,
    cols: size::CellCountInt,
    rows: size::CellCountInt,
    f: F,
) {
    let mut terminal: terminal_c::Terminal = std::ptr::null_mut();
    let terminal_new_result = unsafe { terminal_c::new(alloc, &mut terminal, cols, rows) };

    let mut state: RenderState = std::ptr::null_mut();
    let state_new_result = unsafe { new(Some(alloc), &mut state) };

    f(terminal_new_result, terminal, state_new_result, state);

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
                RenderStateData::RowIterator,
                &mut iterator as *mut _ as *mut c_void,
            )
        });

        unsafe { row_iterator_free(iterator) };
    });
}

// test "render: row iterator new/free"
// The four length asserts below intentionally go through `.map(|r| &r.<field>)`
// so each one names the reference SoA slice (`raws`/`cells`/`selection`/`dirty`)
// it stands in for (see the PORT NOTE inside); the map is the mapping record,
// not a logic step, so the suspicious_map lint is a false positive here.
#[allow(clippy::suspicious_map)]
#[test]
fn render_row_iterator_new_free() {
    with_test_allocator(|alloc| {
        with_terminal_state(
            alloc,
            80,
            24,
            |terminal_new_result, terminal, state_new_result, state| {
                // try testing.expectEqual(Result.success, terminal_c.new(&lib.alloc.test_allocator, &terminal, .{ ... }));
                assert_eq!(Result::Success, terminal_new_result);
                // try testing.expectEqual(Result.success, new(&lib.alloc.test_allocator, &state));
                assert_eq!(Result::Success, state_new_result);
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
                        RenderStateData::RowIterator,
                        &mut iterator as *mut _ as *mut c_void,
                    )
                });

                // const iterator_ptr = iterator.?;
                let iterator_ptr = unsafe { &*iterator };
                // const row_data = state.?.state.row_data.slice();
                let row_data = unsafe { &(*state).state.row_data };

                // try testing.expectEqual(position_none, iterator_ptr.y);
                assert_eq!(POSITION_NONE, iterator_ptr.y);
                // PORT NOTE: the reference iterator carries four parallel
                // struct-of-arrays slices (`raws`/`cells`/`selection`/`dirty`);
                // rustty's RowIterator carries ONE `rows` slice of per-row structs,
                // so each reference length assert maps to the length of the
                // corresponding per-row field VIEW below (assertion-for-assertion).
                let rows = unsafe { &*iterator_ptr.rows };
                // try testing.expectEqual(row_data.items(.raw).len, iterator_ptr.raws.len);
                assert_eq!(row_data.len(), rows.iter().map(|r| &r.raw).count());
                // try testing.expectEqual(row_data.items(.cells).len, iterator_ptr.cells.len);
                assert_eq!(row_data.len(), rows.iter().map(|r| &r.cells).count());
                // try testing.expectEqual(row_data.items(.selection).len, iterator_ptr.selection.len);
                assert_eq!(row_data.len(), rows.iter().map(|r| &r.selection).count());
                // try testing.expectEqual(row_data.items(.dirty).len, iterator_ptr.dirty.len);
                assert_eq!(row_data.len(), rows.iter().map(|r| &r.dirty).count());

                unsafe { row_iterator_free(iterator) };
            },
        );
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
            RenderStateRowData::Dirty,
            &mut dirty as *mut _ as *mut c_void,
        )
    });
}

// test "render: row get invalid data"
#[test]
fn render_row_get_invalid_data() {
    with_test_allocator(|alloc| {
        with_terminal_state(
            alloc,
            80,
            24,
            |terminal_new_result, terminal, state_new_result, state| {
                // try testing.expectEqual(Result.success, terminal_c.new(&lib.alloc.test_allocator, &terminal, .{ ... }));
                assert_eq!(Result::Success, terminal_new_result);
                // try testing.expectEqual(Result.success, new(&lib.alloc.test_allocator, &state));
                assert_eq!(Result::Success, state_new_result);
                assert_eq!(Result::Success, unsafe { update(state, terminal) });

                let mut iterator: RowIterator = std::ptr::null_mut();
                assert_eq!(Result::Success, unsafe {
                    row_iterator_new(Some(alloc), &mut iterator)
                });
                // defer row_iterator_free(iterator);

                assert_eq!(Result::Success, unsafe {
                    get(
                        state,
                        RenderStateData::RowIterator,
                        &mut iterator as *mut _ as *mut c_void,
                    )
                });
                // try testing.expect(row_iterator_next(iterator));
                assert!(unsafe { row_iterator_next(iterator) });
                // try testing.expectEqual(Result.invalid_value, row_get(iterator, .invalid, null));
                assert_eq!(Result::InvalidValue, unsafe {
                    row_get(iterator, RenderStateRowData::Invalid, std::ptr::null_mut())
                });

                unsafe { row_iterator_free(iterator) };
            },
        );
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
        with_terminal_state(
            alloc,
            80,
            24,
            |terminal_new_result, terminal, state_new_result, state| {
                // try testing.expectEqual(Result.success, terminal_c.new(&lib.alloc.test_allocator, &terminal, .{ ... }));
                assert_eq!(Result::Success, terminal_new_result);
                // try testing.expectEqual(Result.success, new(&lib.alloc.test_allocator, &state));
                assert_eq!(Result::Success, state_new_result);
                assert_eq!(Result::Success, unsafe { update(state, terminal) });

                let mut iterator: RowIterator = std::ptr::null_mut();
                assert_eq!(Result::Success, unsafe {
                    row_iterator_new(Some(alloc), &mut iterator)
                });
                // defer row_iterator_free(iterator);

                assert_eq!(Result::Success, unsafe {
                    get(
                        state,
                        RenderStateData::RowIterator,
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
            },
        );
    });
}

// test "render: row get before iteration"
#[test]
fn render_row_get_before_iteration() {
    with_test_allocator(|alloc| {
        with_terminal_state(
            alloc,
            80,
            24,
            |terminal_new_result, terminal, state_new_result, state| {
                // try testing.expectEqual(Result.success, terminal_c.new(&lib.alloc.test_allocator, &terminal, .{ ... }));
                assert_eq!(Result::Success, terminal_new_result);
                // try testing.expectEqual(Result.success, new(&lib.alloc.test_allocator, &state));
                assert_eq!(Result::Success, state_new_result);
                assert_eq!(Result::Success, unsafe { update(state, terminal) });

                let mut iterator: RowIterator = std::ptr::null_mut();
                assert_eq!(Result::Success, unsafe {
                    row_iterator_new(Some(alloc), &mut iterator)
                });
                // defer row_iterator_free(iterator);

                assert_eq!(Result::Success, unsafe {
                    get(
                        state,
                        RenderStateData::RowIterator,
                        &mut iterator as *mut _ as *mut c_void,
                    )
                });
                // var dirty: bool = undefined;
                let mut dirty: bool = false;
                // try testing.expectEqual(Result.invalid_value, row_get(iterator, .dirty, @ptrCast(&dirty)));
                assert_eq!(Result::InvalidValue, unsafe {
                    row_get(
                        iterator,
                        RenderStateRowData::Dirty,
                        &mut dirty as *mut _ as *mut c_void,
                    )
                });

                unsafe { row_iterator_free(iterator) };
            },
        );
    });
}

// test "render: row get/set dirty"
#[test]
fn render_row_get_set_dirty() {
    with_test_allocator(|alloc| {
        with_terminal_state(
            alloc,
            80,
            24,
            |terminal_new_result, terminal, state_new_result, state| {
                // try testing.expectEqual(Result.success, terminal_c.new(&lib.alloc.test_allocator, &terminal, .{ ... }));
                assert_eq!(Result::Success, terminal_new_result);
                // try testing.expectEqual(Result.success, new(&lib.alloc.test_allocator, &state));
                assert_eq!(Result::Success, state_new_result);
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
                    get(
                        state,
                        RenderStateData::RowIterator,
                        &mut it as *mut _ as *mut c_void,
                    )
                });
                assert!(unsafe { row_iterator_next(it) });
                let mut dirty: bool = false;
                assert_eq!(Result::Success, unsafe {
                    row_get(
                        it,
                        RenderStateRowData::Dirty,
                        &mut dirty as *mut _ as *mut c_void,
                    )
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
                    get(
                        state,
                        RenderStateData::RowIterator,
                        &mut it2 as *mut _ as *mut c_void,
                    )
                });
                assert!(unsafe { row_iterator_next(it2) });
                assert_eq!(Result::Success, unsafe {
                    row_get(
                        it2,
                        RenderStateRowData::Dirty,
                        &mut dirty as *mut _ as *mut c_void,
                    )
                });
                // try testing.expect(!dirty);
                assert!(!dirty);

                unsafe { row_iterator_free(it2) };
                unsafe { row_iterator_free(it) };
            },
        );
    });
}

// test "render: row get selection"
#[test]
fn render_row_get_selection() {
    with_test_allocator(|alloc| {
        with_terminal_state(
            alloc,
            10,
            3,
            |terminal_new_result, terminal, state_new_result, state| {
                // try testing.expectEqual(Result.success, terminal_c.new(&lib.alloc.test_allocator, &terminal, .{ ... }));
                assert_eq!(Result::Success, terminal_new_result);
                // try testing.expectEqual(Result.success, new(&lib.alloc.test_allocator, &state));
                assert_eq!(Result::Success, state_new_result);
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
                    get(
                        state,
                        RenderStateData::RowIterator,
                        &mut it as *mut _ as *mut c_void,
                    )
                });

                // var sel: RowSelection = .{};
                let mut sel: RowSelection = RowSelection::default();
                // try testing.expect(row_iterator_next(it));
                assert!(unsafe { row_iterator_next(it) });
                // try testing.expectEqual(Result.no_value, row_get(it, .selection, @ptrCast(&sel)));
                assert_eq!(Result::NoValue, unsafe {
                    row_get(
                        it,
                        RenderStateRowData::Selection,
                        &mut sel as *mut _ as *mut c_void,
                    )
                });

                assert!(unsafe { row_iterator_next(it) });
                // sel = .{};
                sel = RowSelection::default();
                assert_eq!(Result::Success, unsafe {
                    row_get(
                        it,
                        RenderStateRowData::Selection,
                        &mut sel as *mut _ as *mut c_void,
                    )
                });
                // try testing.expectEqual(@as(u16, 2), sel.start_x);
                assert_eq!(2u16, sel.start_x);
                // try testing.expectEqual(@as(u16, 4), sel.end_x);
                assert_eq!(4u16, sel.end_x);

                assert!(unsafe { row_iterator_next(it) });
                sel = RowSelection::default();
                assert_eq!(Result::NoValue, unsafe {
                    row_get(
                        it,
                        RenderStateRowData::Selection,
                        &mut sel as *mut _ as *mut c_void,
                    )
                });

                unsafe { row_iterator_free(it) };
            },
        );
    });
}

// test "render: row get cells_raw"
#[test]
fn render_row_get_cells_raw() {
    with_test_allocator(|alloc| {
        // var terminal: terminal_c.Terminal = null;
        let mut terminal: terminal_c::Terminal = std::ptr::null_mut();
        // try testing.expectEqual(Result.success, terminal_c.new(&lib.alloc.test_allocator, &terminal, 10, 3));
        assert_eq!(Result::Success, unsafe {
            terminal_c::new(alloc, &mut terminal, 10, 3)
        });
        // defer terminal_c.free(terminal);

        // terminal_c.vt_write(terminal, "AB", 2);
        unsafe { terminal_c::vt_write(terminal, b"AB".as_ptr(), 2) };

        // var state: RenderState = null;
        let mut state: RenderState = std::ptr::null_mut();
        // try testing.expectEqual(Result.success, new(&lib.alloc.test_allocator, &state));
        assert_eq!(Result::Success, unsafe { new(Some(alloc), &mut state) });
        // defer free(state);

        // try testing.expectEqual(Result.success, update(state, terminal));
        assert_eq!(Result::Success, unsafe { update(state, terminal) });

        // var it: RowIterator = null;
        let mut it: RowIterator = std::ptr::null_mut();
        // try testing.expectEqual(Result.success, row_iterator_new(&lib.alloc.test_allocator, &it));
        assert_eq!(Result::Success, unsafe {
            row_iterator_new(Some(alloc), &mut it)
        });
        // defer row_iterator_free(it);

        // try testing.expectEqual(Result.success, get(state, .row_iterator, @ptrCast(&it)));
        assert_eq!(Result::Success, unsafe {
            get(
                state,
                RenderStateData::RowIterator,
                &mut it as *mut _ as *mut c_void,
            )
        });

        // Not positioned on a row yet.
        // var view: cell_c.CellsView = undefined;
        let mut view: crate::cell::CellsView = crate::cell::CellsView {
            ptr: std::ptr::null(),
            len: 0,
        };
        // try testing.expectEqual(Result.invalid_value, row_get(it, .cells_raw, @ptrCast(&view)));
        assert_eq!(Result::InvalidValue, unsafe {
            row_get(
                it,
                RenderStateRowData::CellsRaw,
                &mut view as *mut _ as *mut c_void,
            )
        });

        // try testing.expect(row_iterator_next(it));
        assert!(unsafe { row_iterator_next(it) });
        // try testing.expectEqual(Result.success, row_get(it, .cells_raw, @ptrCast(&view)));
        assert_eq!(Result::Success, unsafe {
            row_get(
                it,
                RenderStateRowData::CellsRaw,
                &mut view as *mut _ as *mut c_void,
            )
        });
        // try testing.expectEqual(@as(usize, 10), view.len);
        assert_eq!(10usize, view.len);

        // The view must match the per-cell raw reads.
        // var cells: RowCells = null;
        let mut cells: RowCells = std::ptr::null_mut();
        // try testing.expectEqual(Result.success, row_cells_new(&lib.alloc.test_allocator, &cells));
        assert_eq!(Result::Success, unsafe {
            row_cells_new(Some(alloc), &mut cells)
        });
        // defer row_cells_free(cells);
        // try testing.expectEqual(Result.success, row_get(it, .cells, @ptrCast(&cells)));
        assert_eq!(Result::Success, unsafe {
            row_get(
                it,
                RenderStateRowData::Cells,
                &mut cells as *mut _ as *mut c_void,
            )
        });
        // const ptr = view.ptr.?;
        let ptr = view.ptr;
        assert!(!ptr.is_null());
        // var x: u16 = 0;
        // while (x < view.len) : (x += 1) { ... }
        let mut x: u16 = 0;
        while (x as usize) < view.len {
            // var raw: page.Cell.C = undefined;
            let mut raw: u64 = 0;
            // try testing.expectEqual(Result.success, row_cells_select(cells, x));
            assert_eq!(Result::Success, unsafe { row_cells_select(cells, x) });
            // try testing.expectEqual(Result.success, row_cells_get(cells, .raw, @ptrCast(&raw)));
            assert_eq!(Result::Success, unsafe {
                row_cells_get(cells, RowCellsData::Raw, &mut raw as *mut _ as *mut c_void)
            });
            // try testing.expectEqual(raw, ptr[x]);
            assert_eq!(raw, unsafe { *ptr.add(x as usize) });
            x += 1;
        }

        // Contents sanity: first two cells hold our text.
        // const first: page.Cell = @bitCast(ptr[0]);
        let first = page::Cell::from_bits(unsafe { *ptr });
        // const second: page.Cell = @bitCast(ptr[1]);
        let second = page::Cell::from_bits(unsafe { *ptr.add(1) });
        // try testing.expectEqual(@as(u21, 'A'), first.codepoint());
        assert_eq!('A' as u32, first.codepoint());
        // try testing.expectEqual(@as(u21, 'B'), second.codepoint());
        assert_eq!('B' as u32, second.codepoint());

        unsafe { row_cells_free(cells) };
        unsafe { row_iterator_free(it) };
        unsafe { free(state) };
        unsafe { terminal_c::free(terminal) };
    });
}

// test "render: row cells get selected"
#[test]
fn render_row_cells_get_selected() {
    with_test_allocator(|alloc| {
        with_terminal_state(
            alloc,
            10,
            3,
            |terminal_new_result, terminal, state_new_result, state| {
                // try testing.expectEqual(Result.success, terminal_c.new(&lib.alloc.test_allocator, &terminal, .{ ... }));
                assert_eq!(Result::Success, terminal_new_result);
                // try testing.expectEqual(Result.success, new(&lib.alloc.test_allocator, &state));
                assert_eq!(Result::Success, state_new_result);
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
                    get(
                        state,
                        RenderStateData::RowIterator,
                        &mut it as *mut _ as *mut c_void,
                    )
                });

                assert!(unsafe { row_iterator_next(it) });
                assert_eq!(Result::Success, unsafe {
                    row_get(
                        it,
                        RenderStateRowData::Cells,
                        &mut cells as *mut _ as *mut c_void,
                    )
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
                    row_get(
                        it,
                        RenderStateRowData::Cells,
                        &mut cells as *mut _ as *mut c_void,
                    )
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
                        // PORT NOTE (W34-B02): the `keys` array crosses the C
                        // boundary as raw ints; `*_get_multi` reads each element
                        // as a `c_int` and range-checks it.
                        keys.as_ptr().cast(),
                        values.as_mut_ptr(),
                        &mut written,
                    )
                });
                assert_eq!(keys.len(), written);
                assert!(selected);

                unsafe { row_cells_free(cells) };
                unsafe { row_iterator_free(it) };
            },
        );
    });
}

// test "render: row cells get has_styling"
#[test]
fn render_row_cells_get_has_styling() {
    with_test_allocator(|alloc| {
        with_terminal_state(
            alloc,
            10,
            3,
            |terminal_new_result, terminal, state_new_result, state| {
                // try testing.expectEqual(Result.success, terminal_c.new(&lib.alloc.test_allocator, &terminal, .{ ... }));
                assert_eq!(Result::Success, terminal_new_result);
                // try testing.expectEqual(Result.success, new(&lib.alloc.test_allocator, &state));
                assert_eq!(Result::Success, state_new_result);
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
                    get(
                        state,
                        RenderStateData::RowIterator,
                        &mut it as *mut _ as *mut c_void,
                    )
                });
                assert!(unsafe { row_iterator_next(it) });
                assert_eq!(Result::Success, unsafe {
                    row_get(
                        it,
                        RenderStateRowData::Cells,
                        &mut cells as *mut _ as *mut c_void,
                    )
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
                        // PORT NOTE (W34-B02): the `keys` array crosses the C
                        // boundary as raw ints; `*_get_multi` reads each element
                        // as a `c_int` and range-checks it.
                        keys.as_ptr().cast(),
                        values.as_mut_ptr(),
                        &mut written,
                    )
                });
                assert_eq!(keys.len(), written);
                assert!(has_styling);

                unsafe { row_cells_free(cells) };
                unsafe { row_iterator_free(it) };
            },
        );
    });
}

// test "render: row cells get graphemes utf8"
//
// PORT NOTE (oracle aea63d71f): the overflow the oracle commit fixes is
// structurally N/A in Rust — `render.rs`'s `let mut needed = cp0.len_utf8();`
// is already `usize`, where Zig's `utf8CodepointSequenceLength` returns a `u3`
// that the oracle accumulated into. The commit's TEST TABLE is not N/A, and it
// is what pins the behaviour: the three cases below make `needed` exceed 7 (the
// 8-byte flag cluster) and 15 (the 18-byte family cluster), so a `u3`/`u4`-width
// accumulator would report a wrong probe size here.
#[test]
fn render_row_cells_get_graphemes_utf8() {
    // N625: this test reaches `sys::log_fn`; hold the sys guard so its
    // warnings cannot land in another test's log sink.
    let _guard = crate::sys::tests::reset_global();
    // const cases = [_]struct { terminal_input: []const u8, expected: []const u8 }{ ... };
    struct Case {
        terminal_input: &'static [u8],
        expected: &'static [u8],
    }
    let cases = [
        // .{ .terminal_input = "e\u{301}", .expected = "e\u{301}" },
        Case {
            terminal_input: "e\u{301}".as_bytes(),
            expected: "e\u{301}".as_bytes(),
        },
        // .{ .terminal_input = "\x1b[?2027h\u{1F1FA}\u{1F1F8}", .expected = "\u{1F1FA}\u{1F1F8}" },
        Case {
            terminal_input: "\x1b[?2027h\u{1F1FA}\u{1F1F8}".as_bytes(),
            expected: "\u{1F1FA}\u{1F1F8}".as_bytes(),
        },
        // .{ .terminal_input = "\x1b[?2027h\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}", .expected = ... },
        Case {
            terminal_input: "\x1b[?2027h\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}".as_bytes(),
            expected: "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}".as_bytes(),
        },
    ];

    // for (cases) |case| { ... }
    for case in cases {
        with_test_allocator(|alloc| {
            with_terminal_state(
                alloc,
                10,
                3,
                |terminal_new_result, terminal, state_new_result, state| {
                    assert_eq!(Result::Success, terminal_new_result);
                    assert_eq!(Result::Success, state_new_result);

                    // terminal_c.vt_write(terminal, case.terminal_input.ptr, case.terminal_input.len);
                    unsafe {
                        terminal_c::vt_write(
                            terminal,
                            case.terminal_input.as_ptr(),
                            case.terminal_input.len(),
                        )
                    };

                    // try testing.expectEqual(Result.success, update(state, terminal));
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
                        get(
                            state,
                            RenderStateData::RowIterator,
                            &mut it as *mut _ as *mut c_void,
                        )
                    });
                    assert!(unsafe { row_iterator_next(it) });
                    assert_eq!(Result::Success, unsafe {
                        row_get(
                            it,
                            RenderStateRowData::Cells,
                            &mut cells as *mut _ as *mut c_void,
                        )
                    });

                    // try testing.expectEqual(Result.success, row_cells_select(cells, 0));
                    assert_eq!(Result::Success, unsafe { row_cells_select(cells, 0) });

                    // var text: lib.Buffer = .{};
                    // try testing.expectEqual(Result.out_of_space, row_cells_get(...));
                    // try testing.expectEqual(case.expected.len, text.len);
                    let mut text: Buffer = Buffer::default();
                    assert_eq!(Result::OutOfSpace, unsafe {
                        row_cells_get(
                            cells,
                            RowCellsData::GraphemesUtf8,
                            &mut text as *mut _ as *mut c_void,
                        )
                    });
                    assert_eq!(case.expected.len(), text.len);

                    // var small = [_]u8{'x'} ** 32;
                    // const small_cap = case.expected.len - 1;
                    // text = .{ .ptr = small[0..small_cap].ptr, .cap = small_cap };
                    //
                    // NEGATIVE CONTROL (P3-W6 T1 Step 6): `cap == needed - 1` must
                    // return out_of_space and write NOTHING. A test that only
                    // asserted `text.len == expected.len` here would pass against an
                    // implementation that partially filled the buffer first, which is
                    // exactly the class of bug the oracle's u3 overflow produced. The
                    // whole 32-byte array is compared, not just the first `small_cap`
                    // bytes, so an overrun past `cap` is caught too.
                    let mut small = [b'x'; 32];
                    let small_cap = case.expected.len() - 1;
                    text = Buffer {
                        ptr: small.as_mut_ptr(),
                        cap: small_cap,
                        len: 0,
                    };
                    assert_eq!(Result::OutOfSpace, unsafe {
                        row_cells_get(
                            cells,
                            RowCellsData::GraphemesUtf8,
                            &mut text as *mut _ as *mut c_void,
                        )
                    });
                    assert_eq!(case.expected.len(), text.len);
                    // try testing.expectEqualSlices(u8, &([_]u8{'x'} ** 32), &small);
                    assert_eq!([b'x'; 32], small);

                    // var buf: [32]u8 = undefined;
                    // text = .{ .ptr = &buf, .cap = case.expected.len };
                    // -> the EXACT-SIZE success case.
                    let mut buf = [0u8; 32];
                    text = Buffer {
                        ptr: buf.as_mut_ptr(),
                        cap: case.expected.len(),
                        len: 0,
                    };
                    assert_eq!(Result::Success, unsafe {
                        row_cells_get(
                            cells,
                            RowCellsData::GraphemesUtf8,
                            &mut text as *mut _ as *mut c_void,
                        )
                    });
                    assert_eq!(case.expected.len(), text.len);
                    // try testing.expectEqualStrings(case.expected, buf[0..text.len]);
                    assert_eq!(case.expected, &buf[0..text.len]);

                    unsafe { row_cells_free(cells) };
                    unsafe { row_iterator_free(it) };
                },
            );
        });
    }

    with_test_allocator(|alloc| {
        with_terminal_state(
            alloc,
            10,
            3,
            |terminal_new_result, terminal, state_new_result, state| {
                // try testing.expectEqual(Result.success, terminal_c.new(&lib.alloc.test_allocator, &terminal, .{ ... }));
                assert_eq!(Result::Success, terminal_new_result);
                // try testing.expectEqual(Result.success, new(&lib.alloc.test_allocator, &state));
                assert_eq!(Result::Success, state_new_result);
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
                    get(
                        state,
                        RenderStateData::RowIterator,
                        &mut it as *mut _ as *mut c_void,
                    )
                });
                assert!(unsafe { row_iterator_next(it) });
                assert_eq!(Result::Success, unsafe {
                    row_get(
                        it,
                        RenderStateRowData::Cells,
                        &mut cells as *mut _ as *mut c_void,
                    )
                });

                // try testing.expectEqual(Result.success, row_cells_select(cells, 1));
                assert_eq!(Result::Success, unsafe { row_cells_select(cells, 1) });
                // var buf: [8]u8 = undefined;
                // var text: lib.Buffer = .{ .ptr = &buf, .cap = buf.len };
                let mut buf = [0u8; 8];
                let mut text = Buffer {
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
            },
        );
    });
}

// test "render: row iterator next"
#[test]
fn render_row_iterator_next() {
    with_test_allocator(|alloc| {
        with_terminal_state(
            alloc,
            80,
            24,
            |terminal_new_result, terminal, state_new_result, state| {
                // try testing.expectEqual(Result.success, terminal_c.new(&lib.alloc.test_allocator, &terminal, .{ ... }));
                assert_eq!(Result::Success, terminal_new_result);
                // try testing.expectEqual(Result.success, new(&lib.alloc.test_allocator, &state));
                assert_eq!(Result::Success, state_new_result);
                assert_eq!(Result::Success, unsafe { update(state, terminal) });

                let mut iterator: RowIterator = std::ptr::null_mut();
                assert_eq!(Result::Success, unsafe {
                    row_iterator_new(Some(alloc), &mut iterator)
                });
                // defer row_iterator_free(iterator);

                assert_eq!(Result::Success, unsafe {
                    get(
                        state,
                        RenderStateData::RowIterator,
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
                // try testing.expectEqual(@as(usize, 0), iterator.?.y);
                assert_eq!(0usize, unsafe { (*iterator).y });

                // var i: size.CellCountInt = 1;
                let mut i: size::CellCountInt = 1;
                // while (i < rows) : (i += 1) { ... }
                while i < rows {
                    assert!(unsafe { row_iterator_next(iterator) });
                    // try testing.expectEqual(i, iterator.?.y);
                    assert_eq!(i as usize, unsafe { (*iterator).y });
                    i += 1;
                }

                // try testing.expect(!row_iterator_next(iterator));
                assert!(!unsafe { row_iterator_next(iterator) });
                // try testing.expectEqual(@as(usize, rows - 1), iterator.?.y);
                assert_eq!((rows - 1) as usize, unsafe { (*iterator).y });

                unsafe { row_iterator_free(iterator) };
            },
        );
    });
}

// test "render: row iterator next dirty"
#[test]
fn render_row_iterator_next_dirty() {
    with_test_allocator(|alloc| {
        // var terminal: terminal_c.Terminal = null;
        let mut terminal: terminal_c::Terminal = std::ptr::null_mut();
        // try testing.expectEqual(Result.success, terminal_c.new(&lib.alloc.test_allocator, &terminal, 10, 4));
        assert_eq!(Result::Success, unsafe {
            terminal_c::new(alloc, &mut terminal, 10, 4)
        });
        // defer terminal_c.free(terminal);

        // var state: RenderState = null;
        let mut state: RenderState = std::ptr::null_mut();
        // try testing.expectEqual(Result.success, new(&lib.alloc.test_allocator, &state));
        assert_eq!(Result::Success, unsafe { new(Some(alloc), &mut state) });
        // defer free(state);
        // try testing.expectEqual(Result.success, update(state, terminal));
        assert_eq!(Result::Success, unsafe { update(state, terminal) });

        // var iterator: RowIterator = null;
        let mut iterator: RowIterator = std::ptr::null_mut();
        // try testing.expectEqual(Result.success, row_iterator_new(&lib.alloc.test_allocator, &iterator));
        assert_eq!(Result::Success, unsafe {
            row_iterator_new(Some(alloc), &mut iterator)
        });
        // defer row_iterator_free(iterator);

        // Invalid arguments neither write the output nor advance the iterator.
        // try testing.expectEqual(Result.success, get(state, .row_iterator, @ptrCast(&iterator)));
        assert_eq!(Result::Success, unsafe {
            get(
                state,
                RenderStateData::RowIterator,
                &mut iterator as *mut _ as *mut c_void,
            )
        });
        // var out_y: size.CellCountInt = 0xCAFE;
        let mut out_y: size::CellCountInt = 0xCAFE;
        // try testing.expect(!row_iterator_next_dirty(null, &out_y));
        assert!(!unsafe { row_iterator_next_dirty(std::ptr::null_mut(), &mut out_y) });
        // try testing.expectEqual(@as(size.CellCountInt, 0xCAFE), out_y);
        assert_eq!(0xCAFEu16, out_y);
        // try testing.expect(!row_iterator_next_dirty(iterator, null));
        assert!(!unsafe { row_iterator_next_dirty(iterator, std::ptr::null_mut()) });
        // try testing.expectEqual(position_none, iterator.?.y);
        assert_eq!(POSITION_NONE, unsafe { (*iterator).y });

        // A full redraw returns every row, even if its row flag was cleared.
        // @memset(state.?.state.row_data.items(.dirty), false);
        for row in unsafe { &mut (*state).state.row_data }.iter_mut() {
            row.dirty = false;
        }
        // var expected_y: size.CellCountInt = 0;
        let mut expected_y: size::CellCountInt = 0;
        // while (row_iterator_next_dirty(iterator, &out_y)) : (expected_y += 1) { ... }
        while unsafe { row_iterator_next_dirty(iterator, &mut out_y) } {
            // try testing.expectEqual(expected_y, out_y);
            assert_eq!(expected_y, out_y);
            expected_y += 1;
        }
        // try testing.expectEqual(@as(size.CellCountInt, 4), expected_y);
        assert_eq!(4u16, expected_y);
        // try testing.expectEqual(@as(size.CellCountInt, 3), out_y);
        assert_eq!(3u16, out_y);

        // A partial redraw skips clean rows without consuming dirty flags.
        // const dirty = state.?.state.row_data.items(.dirty);
        // state.?.state.dirty = .partial;
        // @memset(dirty, false);
        // dirty[1] = true;
        // dirty[3] = true;
        unsafe { (*state).state.dirty = RenderDirty::Partial };
        {
            let row_data = unsafe { &mut (*state).state.row_data };
            for row in row_data.iter_mut() {
                row.dirty = false;
            }
            row_data[1].dirty = true;
            row_data[3].dirty = true;
        }
        // try testing.expectEqual(Result.success, get(state, .row_iterator, @ptrCast(&iterator)));
        assert_eq!(Result::Success, unsafe {
            get(
                state,
                RenderStateData::RowIterator,
                &mut iterator as *mut _ as *mut c_void,
            )
        });
        // try testing.expect(row_iterator_next_dirty(iterator, &out_y));
        assert!(unsafe { row_iterator_next_dirty(iterator, &mut out_y) });
        // try testing.expectEqual(@as(size.CellCountInt, 1), out_y);
        assert_eq!(1u16, out_y);
        assert!(unsafe { row_iterator_next_dirty(iterator, &mut out_y) });
        // try testing.expectEqual(@as(size.CellCountInt, 3), out_y);
        assert_eq!(3u16, out_y);
        assert!(!unsafe { row_iterator_next_dirty(iterator, &mut out_y) });
        assert_eq!(3u16, out_y);
        // try testing.expect(dirty[1]);
        assert!(unsafe { &(*state).state.row_data }[1].dirty);
        // try testing.expect(dirty[3]);
        assert!(unsafe { &(*state).state.row_data }[3].dirty);

        // The global clean state is authoritative, even if stale row flags exist.
        // state.?.state.dirty = .false;
        unsafe { (*state).state.dirty = RenderDirty::False };
        assert_eq!(Result::Success, unsafe {
            get(
                state,
                RenderStateData::RowIterator,
                &mut iterator as *mut _ as *mut c_void,
            )
        });
        // out_y = 0xCAFE;
        out_y = 0xCAFE;
        assert!(!unsafe { row_iterator_next_dirty(iterator, &mut out_y) });
        assert_eq!(0xCAFEu16, out_y);

        // Existing iterators observe cleaning through their borrowed state.
        // state.?.state.dirty = .partial;
        // dirty[1] = true;
        unsafe { (*state).state.dirty = RenderDirty::Partial };
        unsafe { (&mut (*state).state.row_data)[1].dirty = true };
        assert_eq!(Result::Success, unsafe {
            get(
                state,
                RenderStateData::RowIterator,
                &mut iterator as *mut _ as *mut c_void,
            )
        });
        // try testing.expectEqual(Result.success, clean(state));
        assert_eq!(Result::Success, unsafe { clean(state) });
        // try testing.expect(!row_iterator_next_dirty(iterator, &out_y));
        assert!(!unsafe { row_iterator_next_dirty(iterator, &mut out_y) });

        unsafe { row_iterator_free(iterator) };
        unsafe { free(state) };
        unsafe { terminal_c::free(terminal) };
    });
}

// test "render: update"
#[test]
fn render_update() {
    with_test_allocator(|alloc| {
        with_terminal_state(
            alloc,
            80,
            24,
            |terminal_new_result, terminal, state_new_result, state| {
                // try testing.expectEqual(Result.success, terminal_c.new(&lib.alloc.test_allocator, &terminal, .{ ... }));
                assert_eq!(Result::Success, terminal_new_result);
                // try testing.expectEqual(Result.success, new(&lib.alloc.test_allocator, &state));
                assert_eq!(Result::Success, state_new_result);
                assert_eq!(Result::Success, unsafe { update(state, terminal) });

                let input = b"hello";
                unsafe { terminal_c::vt_write(terminal, input.as_ptr(), 5) };
                assert_eq!(Result::Success, unsafe { update(state, terminal) });

                // var cols: size.CellCountInt = 0; var rows_val: size.CellCountInt = 0;
                let mut cols: size::CellCountInt = 0;
                let mut rows_val: size::CellCountInt = 0;
                assert_eq!(Result::Success, unsafe {
                    get(
                        state,
                        RenderStateData::Cols,
                        &mut cols as *mut _ as *mut c_void,
                    )
                });
                assert_eq!(Result::Success, unsafe {
                    get(
                        state,
                        RenderStateData::Rows,
                        &mut rows_val as *mut _ as *mut c_void,
                    )
                });
                // try testing.expectEqual(@as(size.CellCountInt, 80), cols);
                assert_eq!(80u16, cols);
                // try testing.expectEqual(@as(size.CellCountInt, 24), rows_val);
                assert_eq!(24u16, rows_val);
            },
        );
    });
}

// test "render: colors data get"
#[test]
fn render_colors_data_get() {
    with_test_allocator(|alloc| {
        with_terminal_state(
            alloc,
            80,
            24,
            |terminal_new_result, terminal, state_new_result, state| {
                // try testing.expectEqual(Result.success, terminal_c.new(&lib.alloc.test_allocator, &terminal, .{ ... }));
                assert_eq!(Result::Success, terminal_new_result);
                // try testing.expectEqual(Result.success, new(&lib.alloc.test_allocator, &state));
                assert_eq!(Result::Success, state_new_result);
                assert_eq!(Result::Success, unsafe { update(state, terminal) });

                // var colors: Colors = std.mem.zeroes(Colors);
                let mut colors: Colors = unsafe { std::mem::zeroed() };
                colors.size = std::mem::size_of::<Colors>();
                // try testing.expectEqual(Result.success, get(state, .colors, &colors));
                assert_eq!(Result::Success, unsafe {
                    get(
                        state,
                        RenderStateData::Colors,
                        &mut colors as *mut _ as *mut c_void,
                    )
                });

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
            },
        );
    });
}

// test "render: cursor data get matches scalar getters"
#[test]
fn render_cursor_data_get_matches_scalar_getters() {
    with_test_allocator(|alloc| {
        with_terminal_state(
            alloc,
            80,
            24,
            |terminal_new_result, terminal, state_new_result, state| {
                // try testing.expectEqual(Result.success, terminal_c.new(&lib.alloc.test_allocator, &terminal, 80, 24));
                assert_eq!(Result::Success, terminal_new_result);
                // try testing.expectEqual(Result.success, new(&lib.alloc.test_allocator, &state));
                assert_eq!(Result::Success, state_new_result);

                // try testing.expectEqual(Result.success, update(state, terminal));
                assert_eq!(Result::Success, unsafe { update(state, terminal) });

                // var cursor: Cursor = std.mem.zeroes(Cursor);
                let mut cursor: Cursor = unsafe { std::mem::zeroed() };
                // cursor.size = @sizeOf(Cursor);
                cursor.size = std::mem::size_of::<Cursor>();
                // try testing.expectEqual(Result.success, get(state, .cursor, &cursor));
                assert_eq!(Result::Success, unsafe {
                    get(
                        state,
                        RenderStateData::Cursor,
                        &mut cursor as *mut _ as *mut c_void,
                    )
                });

                // var viewport_has_value: bool = undefined; ... (the scalar reads)
                let mut viewport_has_value: bool = false;
                let mut viewport_x: u16 = 0;
                let mut viewport_y: u16 = 0;
                let mut wide_tail: bool = false;
                let mut visible: bool = false;
                let mut blinking: bool = false;
                let mut password_input: bool = false;
                let mut visual_style: CursorVisualStyle = CursorVisualStyle::Bar;
                assert_eq!(Result::Success, unsafe {
                    get(
                        state,
                        RenderStateData::CursorViewportHasValue,
                        &mut viewport_has_value as *mut _ as *mut c_void,
                    )
                });
                assert_eq!(Result::Success, unsafe {
                    get(
                        state,
                        RenderStateData::CursorViewportX,
                        &mut viewport_x as *mut _ as *mut c_void,
                    )
                });
                assert_eq!(Result::Success, unsafe {
                    get(
                        state,
                        RenderStateData::CursorViewportY,
                        &mut viewport_y as *mut _ as *mut c_void,
                    )
                });
                assert_eq!(Result::Success, unsafe {
                    get(
                        state,
                        RenderStateData::CursorViewportWideTail,
                        &mut wide_tail as *mut _ as *mut c_void,
                    )
                });
                assert_eq!(Result::Success, unsafe {
                    get(
                        state,
                        RenderStateData::CursorVisible,
                        &mut visible as *mut _ as *mut c_void,
                    )
                });
                assert_eq!(Result::Success, unsafe {
                    get(
                        state,
                        RenderStateData::CursorBlinking,
                        &mut blinking as *mut _ as *mut c_void,
                    )
                });
                assert_eq!(Result::Success, unsafe {
                    get(
                        state,
                        RenderStateData::CursorPasswordInput,
                        &mut password_input as *mut _ as *mut c_void,
                    )
                });
                assert_eq!(Result::Success, unsafe {
                    get(
                        state,
                        RenderStateData::CursorVisualStyle,
                        &mut visual_style as *mut _ as *mut c_void,
                    )
                });

                // try testing.expectEqual(viewport_has_value, cursor.viewport_has_value); ...
                assert_eq!(viewport_has_value, cursor.viewport_has_value);
                assert_eq!(viewport_x, cursor.viewport_x);
                assert_eq!(viewport_y, cursor.viewport_y);
                assert_eq!(wide_tail, cursor.wide_tail);
                assert_eq!(visible, cursor.visible);
                assert_eq!(blinking, cursor.blinking);
                assert_eq!(password_input, cursor.password_input);
                assert_eq!(visual_style, cursor.visual_style);
            },
        );
    });
}

// test "render: cursor data get without viewport"
#[test]
fn render_cursor_data_get_without_viewport() {
    with_test_allocator(|alloc| {
        // var state: RenderState = null;
        let mut state: RenderState = std::ptr::null_mut();
        // try testing.expectEqual(Result.success, new(&lib.alloc.test_allocator, &state));
        assert_eq!(Result::Success, unsafe { new(Some(alloc), &mut state) });
        // defer free(state);

        // state.?.state.cursor.viewport = null;
        // state.?.state.cursor.visible = true;
        // state.?.state.cursor.blinking = true;
        // state.?.state.cursor.password_input = true;
        // state.?.state.cursor.visual_style = .underline;
        unsafe {
            (*state).state.cursor.viewport = None;
            (*state).state.cursor.visible = true;
            (*state).state.cursor.blinking = true;
            (*state).state.cursor.password_input = true;
            (*state).state.cursor.visual_style = cursor::Style::Underline;
        }

        // var cursor: Cursor = std.mem.zeroes(Cursor);
        let mut c: Cursor = unsafe { std::mem::zeroed() };
        // cursor.size = @sizeOf(Cursor);
        c.size = std::mem::size_of::<Cursor>();
        // cursor.viewport_x = 0xAAAA; cursor.viewport_y = 0xBBBB; cursor.wide_tail = true;
        c.viewport_x = 0xAAAA;
        c.viewport_y = 0xBBBB;
        c.wide_tail = true;
        // try testing.expectEqual(Result.success, get(state, .cursor, &cursor));
        assert_eq!(Result::Success, unsafe {
            get(
                state,
                RenderStateData::Cursor,
                &mut c as *mut _ as *mut c_void,
            )
        });

        // try testing.expect(!cursor.viewport_has_value);
        assert!(!c.viewport_has_value);
        // try testing.expectEqual(@as(u16, 0xAAAA), cursor.viewport_x);
        assert_eq!(0xAAAAu16, c.viewport_x);
        // try testing.expectEqual(@as(u16, 0xBBBB), cursor.viewport_y);
        assert_eq!(0xBBBBu16, c.viewport_y);
        // try testing.expect(cursor.wide_tail);
        assert!(c.wide_tail);
        // try testing.expect(cursor.visible);
        assert!(c.visible);
        // try testing.expect(cursor.blinking);
        assert!(c.blinking);
        // try testing.expect(cursor.password_input);
        assert!(c.password_input);
        // try testing.expectEqual(CursorVisualStyle.underline, cursor.visual_style);
        assert_eq!(CursorVisualStyle::Underline, c.visual_style);

        unsafe { free(state) };
    });
}

// test "render: cursor data get supports truncated sized struct"
#[test]
fn render_cursor_data_get_supports_truncated_sized_struct() {
    with_test_allocator(|alloc| {
        with_terminal_state(
            alloc,
            80,
            24,
            |terminal_new_result, terminal, state_new_result, state| {
                // try testing.expectEqual(Result.success, terminal_c.new(&lib.alloc.test_allocator, &terminal, 80, 24));
                assert_eq!(Result::Success, terminal_new_result);
                // try testing.expectEqual(Result.success, new(&lib.alloc.test_allocator, &state));
                assert_eq!(Result::Success, state_new_result);

                // try testing.expectEqual(Result.success, update(state, terminal));
                assert_eq!(Result::Success, unsafe { update(state, terminal) });
                // const expected = state.?.state.cursor;
                // const viewport = expected.viewport.?;
                let (expected_visible, viewport_x, viewport_y, viewport_wide_tail) = unsafe {
                    let cur = &(*state).state.cursor;
                    let viewport = cur.viewport.as_ref().unwrap();
                    (cur.visible, viewport.x, viewport.y, viewport.wide_tail)
                };

                // var cursor: Cursor = std.mem.zeroes(Cursor);
                let mut c: Cursor = unsafe { std::mem::zeroed() };
                // cursor.size = @offsetOf(Cursor, "viewport_y") + @sizeOf(u16);
                c.size = core::mem::offset_of!(Cursor, viewport_y) + std::mem::size_of::<u16>();
                // cursor.wide_tail = !viewport.wide_tail;
                c.wide_tail = !viewport_wide_tail;
                // cursor.visible = !expected.visible;
                c.visible = !expected_visible;
                // try testing.expectEqual(Result.success, get(state, .cursor, &cursor));
                assert_eq!(Result::Success, unsafe {
                    get(
                        state,
                        RenderStateData::Cursor,
                        &mut c as *mut _ as *mut c_void,
                    )
                });

                // try testing.expect(cursor.viewport_has_value);
                assert!(c.viewport_has_value);
                // try testing.expectEqual(viewport.x, cursor.viewport_x);
                assert_eq!(viewport_x, c.viewport_x);
                // try testing.expectEqual(viewport.y, cursor.viewport_y);
                assert_eq!(viewport_y, c.viewport_y);
                // try testing.expectEqual(!viewport.wide_tail, cursor.wide_tail);
                assert_eq!(!viewport_wide_tail, c.wide_tail);
                // try testing.expectEqual(!expected.visible, cursor.visible);
                assert_eq!(!expected_visible, c.visible);
            },
        );
    });
}

// test "render: row cells bg_color no background"
#[test]
fn render_row_cells_bg_color_no_background() {
    with_test_allocator(|alloc| {
        with_terminal_state(
            alloc,
            80,
            24,
            |terminal_new_result, terminal, state_new_result, state| {
                // try testing.expectEqual(Result.success, terminal_c.new(&lib.alloc.test_allocator, &terminal, .{ ... }));
                assert_eq!(Result::Success, terminal_new_result);
                // try testing.expectEqual(Result.success, new(&lib.alloc.test_allocator, &state));
                assert_eq!(Result::Success, state_new_result);
                // Write plain text (no background color set).
                let input = b"hello";
                unsafe { terminal_c::vt_write(terminal, input.as_ptr(), 5) };

                assert_eq!(Result::Success, unsafe { update(state, terminal) });

                let mut it: RowIterator = std::ptr::null_mut();
                assert_eq!(Result::Success, unsafe {
                    row_iterator_new(Some(alloc), &mut it)
                });
                assert_eq!(Result::Success, unsafe {
                    get(
                        state,
                        RenderStateData::RowIterator,
                        &mut it as *mut _ as *mut c_void,
                    )
                });
                assert!(unsafe { row_iterator_next(it) });

                let mut cells: RowCells = std::ptr::null_mut();
                assert_eq!(Result::Success, unsafe {
                    row_cells_new(Some(alloc), &mut cells)
                });

                assert_eq!(Result::Success, unsafe {
                    row_get(
                        it,
                        RenderStateRowData::Cells,
                        &mut cells as *mut _ as *mut c_void,
                    )
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
            },
        );
    });
}

// test "render: row cells bg_color from style"
#[test]
fn render_row_cells_bg_color_from_style() {
    with_test_allocator(|alloc| {
        with_terminal_state(
            alloc,
            80,
            24,
            |terminal_new_result, terminal, state_new_result, state| {
                // try testing.expectEqual(Result.success, terminal_c.new(&lib.alloc.test_allocator, &terminal, .{ ... }));
                assert_eq!(Result::Success, terminal_new_result);
                // try testing.expectEqual(Result.success, new(&lib.alloc.test_allocator, &state));
                assert_eq!(Result::Success, state_new_result);
                // Set an RGB background via SGR 48;2;R;G;B and write text.
                // terminal_c.vt_write(terminal, "\x1b[48;2;10;20;30mA", 18);
                //
                // The reference's Zig string literal is sentinel-terminated
                // (`[17:0]u8`), so passing count 18 reads an in-bounds 0x00 as the
                // 18th byte. Rust byte-string literals carry no sentinel, so we add
                // the trailing 0x00 explicitly to make the allocation byte-identical
                // to what the reference passes (and avoid a real over-read).
                let input = b"\x1b[48;2;10;20;30mA\x00";
                unsafe { terminal_c::vt_write(terminal, input.as_ptr(), 18) };

                assert_eq!(Result::Success, unsafe { update(state, terminal) });

                let mut it: RowIterator = std::ptr::null_mut();
                assert_eq!(Result::Success, unsafe {
                    row_iterator_new(Some(alloc), &mut it)
                });
                assert_eq!(Result::Success, unsafe {
                    get(
                        state,
                        RenderStateData::RowIterator,
                        &mut it as *mut _ as *mut c_void,
                    )
                });
                assert!(unsafe { row_iterator_next(it) });

                let mut cells: RowCells = std::ptr::null_mut();
                assert_eq!(Result::Success, unsafe {
                    row_cells_new(Some(alloc), &mut cells)
                });

                assert_eq!(Result::Success, unsafe {
                    row_get(
                        it,
                        RenderStateRowData::Cells,
                        &mut cells as *mut _ as *mut c_void,
                    )
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
            },
        );
    });
}

// test "render: row cells bg_color from content tag"
#[test]
fn render_row_cells_bg_color_from_content_tag() {
    with_test_allocator(|alloc| {
        with_terminal_state(
            alloc,
            80,
            24,
            |terminal_new_result, terminal, state_new_result, state| {
                // try testing.expectEqual(Result.success, terminal_c.new(&lib.alloc.test_allocator, &terminal, .{ ... }));
                assert_eq!(Result::Success, terminal_new_result);
                // try testing.expectEqual(Result.success, new(&lib.alloc.test_allocator, &state));
                assert_eq!(Result::Success, state_new_result);
                // Set an RGB background and then erase the line. The erased cells
                // should carry the background color via the content tag (bg_color_rgb)
                // rather than through the style.
                // terminal_c.vt_write(terminal, "\x1b[48;2;10;20;30m\x1b[2K", 21);
                //
                // Sentinel-terminated to match the reference's `[20:0]u8` literal:
                // count 21 reads an in-bounds 0x00 as the 21st byte. See
                // render_row_cells_bg_color_from_style for the rationale.
                let input = b"\x1b[48;2;10;20;30m\x1b[2K\x00";
                unsafe { terminal_c::vt_write(terminal, input.as_ptr(), 21) };

                assert_eq!(Result::Success, unsafe { update(state, terminal) });

                let mut it: RowIterator = std::ptr::null_mut();
                assert_eq!(Result::Success, unsafe {
                    row_iterator_new(Some(alloc), &mut it)
                });
                assert_eq!(Result::Success, unsafe {
                    get(
                        state,
                        RenderStateData::RowIterator,
                        &mut it as *mut _ as *mut c_void,
                    )
                });
                assert!(unsafe { row_iterator_next(it) });

                let mut cells: RowCells = std::ptr::null_mut();
                assert_eq!(Result::Success, unsafe {
                    row_cells_new(Some(alloc), &mut cells)
                });

                assert_eq!(Result::Success, unsafe {
                    row_get(
                        it,
                        RenderStateRowData::Cells,
                        &mut cells as *mut _ as *mut c_void,
                    )
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
            },
        );
    });
}

// test "render: row cells fg_color no foreground"
#[test]
fn render_row_cells_fg_color_no_foreground() {
    with_test_allocator(|alloc| {
        with_terminal_state(
            alloc,
            80,
            24,
            |terminal_new_result, terminal, state_new_result, state| {
                // try testing.expectEqual(Result.success, terminal_c.new(&lib.alloc.test_allocator, &terminal, .{ ... }));
                assert_eq!(Result::Success, terminal_new_result);
                // try testing.expectEqual(Result.success, new(&lib.alloc.test_allocator, &state));
                assert_eq!(Result::Success, state_new_result);
                // Write plain text (no foreground color set).
                let input = b"hello";
                unsafe { terminal_c::vt_write(terminal, input.as_ptr(), 5) };

                assert_eq!(Result::Success, unsafe { update(state, terminal) });

                let mut it: RowIterator = std::ptr::null_mut();
                assert_eq!(Result::Success, unsafe {
                    row_iterator_new(Some(alloc), &mut it)
                });
                assert_eq!(Result::Success, unsafe {
                    get(
                        state,
                        RenderStateData::RowIterator,
                        &mut it as *mut _ as *mut c_void,
                    )
                });
                assert!(unsafe { row_iterator_next(it) });

                let mut cells: RowCells = std::ptr::null_mut();
                assert_eq!(Result::Success, unsafe {
                    row_cells_new(Some(alloc), &mut cells)
                });

                assert_eq!(Result::Success, unsafe {
                    row_get(
                        it,
                        RenderStateRowData::Cells,
                        &mut cells as *mut _ as *mut c_void,
                    )
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
            },
        );
    });
}

// test "render: row cells fg_color from style"
#[test]
fn render_row_cells_fg_color_from_style() {
    with_test_allocator(|alloc| {
        with_terminal_state(
            alloc,
            80,
            24,
            |terminal_new_result, terminal, state_new_result, state| {
                // try testing.expectEqual(Result.success, terminal_c.new(&lib.alloc.test_allocator, &terminal, .{ ... }));
                assert_eq!(Result::Success, terminal_new_result);
                // try testing.expectEqual(Result.success, new(&lib.alloc.test_allocator, &state));
                assert_eq!(Result::Success, state_new_result);
                // Set an RGB foreground via SGR 38;2;R;G;B and write text.
                // terminal_c.vt_write(terminal, "\x1b[38;2;10;20;30mA", 18);
                //
                // Sentinel-terminated to match the reference's `[17:0]u8` literal:
                // count 18 reads an in-bounds 0x00 as the 18th byte. See
                // render_row_cells_bg_color_from_style for the rationale.
                let input = b"\x1b[38;2;10;20;30mA\x00";
                unsafe { terminal_c::vt_write(terminal, input.as_ptr(), 18) };

                assert_eq!(Result::Success, unsafe { update(state, terminal) });

                let mut it: RowIterator = std::ptr::null_mut();
                assert_eq!(Result::Success, unsafe {
                    row_iterator_new(Some(alloc), &mut it)
                });
                assert_eq!(Result::Success, unsafe {
                    get(
                        state,
                        RenderStateData::RowIterator,
                        &mut it as *mut _ as *mut c_void,
                    )
                });
                assert!(unsafe { row_iterator_next(it) });

                let mut cells: RowCells = std::ptr::null_mut();
                assert_eq!(Result::Success, unsafe {
                    row_cells_new(Some(alloc), &mut cells)
                });

                assert_eq!(Result::Success, unsafe {
                    row_get(
                        it,
                        RenderStateRowData::Cells,
                        &mut cells as *mut _ as *mut c_void,
                    )
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
            },
        );
    });
}

// test "render: colors data get supports truncated sized struct"
#[test]
fn render_colors_data_get_supports_truncated_sized_struct() {
    with_test_allocator(|alloc| {
        with_terminal_state(
            alloc,
            80,
            24,
            |terminal_new_result, terminal, state_new_result, state| {
                // try testing.expectEqual(Result.success, terminal_c.new(&lib.alloc.test_allocator, &terminal, .{ ... }));
                assert_eq!(Result::Success, terminal_new_result);
                // try testing.expectEqual(Result.success, new(&lib.alloc.test_allocator, &state));
                assert_eq!(Result::Success, state_new_result);
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
                // try testing.expectEqual(Result.success, get(state, .colors, &colors));
                assert_eq!(Result::Success, unsafe {
                    get(
                        state,
                        RenderStateData::Colors,
                        &mut colors as *mut _ as *mut c_void,
                    )
                });

                let state_colors = unsafe { &(*state).state.colors };
                // try testing.expectEqual(state_colors.palette[0].cval(), colors.palette[0]);
                assert_eq!(state_colors.palette[0], colors.palette[0]);
                assert_eq!(state_colors.palette[1], colors.palette[1]);
                // try testing.expectEqual(sentinel, colors.palette[2]);
                assert_eq!(sentinel, colors.palette[2]);
            },
        );
    });
}

// test "render: get_multi success"
#[test]
fn render_get_multi_success() {
    with_test_allocator(|alloc| {
        with_terminal_state(
            alloc,
            80,
            24,
            |terminal_new_result, terminal, state_new_result, state| {
                // try testing.expectEqual(Result.success, terminal_c.new(&lib.alloc.test_allocator, &terminal, .{ ... }));
                assert_eq!(Result::Success, terminal_new_result);
                // try testing.expectEqual(Result.success, new(&lib.alloc.test_allocator, &state));
                assert_eq!(Result::Success, state_new_result);
                assert_eq!(Result::Success, unsafe { update(state, terminal) });

                // var cols: u16 = 0; var rows: u16 = 0; var written: usize = 0;
                let mut cols: u16 = 0;
                let mut rows: u16 = 0;
                let mut written: usize = 0;

                // const keys = [_]Data{ .cols, .rows };
                let keys = [RenderStateData::Cols, RenderStateData::Rows];
                let mut values = [
                    &mut cols as *mut _ as *mut c_void,
                    &mut rows as *mut _ as *mut c_void,
                ];
                assert_eq!(Result::Success, unsafe {
                    get_multi(
                        state,
                        keys.len(),
                        // PORT NOTE (W34-B02): the `keys` array crosses the C
                        // boundary as raw ints; `*_get_multi` reads each element
                        // as a `c_int` and range-checks it.
                        keys.as_ptr().cast(),
                        values.as_mut_ptr(),
                        &mut written,
                    )
                });
                assert_eq!(keys.len(), written);
                assert_eq!(80, cols);
                assert_eq!(24, rows);
            },
        );
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
        with_terminal_state(
            alloc,
            80,
            24,
            |terminal_new_result, terminal, state_new_result, state| {
                // try testing.expectEqual(Result.success, terminal_c.new(&lib.alloc.test_allocator, &terminal, .{ ... }));
                assert_eq!(Result::Success, terminal_new_result);
                // try testing.expectEqual(Result.success, new(&lib.alloc.test_allocator, &state));
                assert_eq!(Result::Success, state_new_result);
                assert_eq!(Result::Success, unsafe { update(state, terminal) });

                let mut it: RowIterator = std::ptr::null_mut();
                assert_eq!(Result::Success, unsafe {
                    row_iterator_new(Some(alloc), &mut it)
                });

                assert_eq!(Result::Success, unsafe {
                    get(
                        state,
                        RenderStateData::RowIterator,
                        &mut it as *mut _ as *mut c_void,
                    )
                });
                assert!(unsafe { row_iterator_next(it) });

                // var dirty: bool = true; var written: usize = 0;
                let mut dirty: bool = true;
                let mut written: usize = 0;

                // const keys = [_]RowData{.dirty};
                let keys = [RenderStateRowData::Dirty];
                let mut values = [&mut dirty as *mut _ as *mut c_void];
                assert_eq!(Result::Success, unsafe {
                    row_get_multi(
                        it,
                        keys.len(),
                        // PORT NOTE (W34-B02): the `keys` array crosses the C
                        // boundary as raw ints; `*_get_multi` reads each element
                        // as a `c_int` and range-checks it.
                        keys.as_ptr().cast(),
                        values.as_mut_ptr(),
                        &mut written,
                    )
                });
                assert_eq!(keys.len(), written);

                unsafe { row_iterator_free(it) };
            },
        );
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
        with_terminal_state(
            alloc,
            80,
            24,
            |terminal_new_result, terminal, state_new_result, state| {
                // try testing.expectEqual(Result.success, terminal_c.new(&lib.alloc.test_allocator, &terminal, .{ ... }));
                assert_eq!(Result::Success, terminal_new_result);
                // try testing.expectEqual(Result.success, new(&lib.alloc.test_allocator, &state));
                assert_eq!(Result::Success, state_new_result);
                let input = b"A";
                unsafe { terminal_c::vt_write(terminal, input.as_ptr(), 1) };

                assert_eq!(Result::Success, unsafe { update(state, terminal) });

                let mut it: RowIterator = std::ptr::null_mut();
                assert_eq!(Result::Success, unsafe {
                    row_iterator_new(Some(alloc), &mut it)
                });

                assert_eq!(Result::Success, unsafe {
                    get(
                        state,
                        RenderStateData::RowIterator,
                        &mut it as *mut _ as *mut c_void,
                    )
                });
                assert!(unsafe { row_iterator_next(it) });

                let mut cells: RowCells = std::ptr::null_mut();
                assert_eq!(Result::Success, unsafe {
                    row_cells_new(Some(alloc), &mut cells)
                });

                assert_eq!(Result::Success, unsafe {
                    row_get(
                        it,
                        RenderStateRowData::Cells,
                        &mut cells as *mut _ as *mut c_void,
                    )
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
                        // PORT NOTE (W34-B02): the `keys` array crosses the C
                        // boundary as raw ints; `*_get_multi` reads each element
                        // as a `c_int` and range-checks it.
                        keys.as_ptr().cast(),
                        values.as_mut_ptr(),
                        &mut written,
                    )
                });
                assert_eq!(keys.len(), written);

                unsafe { row_cells_free(cells) };
                unsafe { row_iterator_free(it) };
            },
        );
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

// test "render: begin/end update invalid value"
#[test]
fn render_begin_end_update_invalid_value() {
    with_test_allocator(|alloc| {
        let mut state: RenderState = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe { new(Some(alloc), &mut state) });
        // defer free(state);

        // try testing.expectEqual(Result.invalid_value, begin_update(null, null));
        assert_eq!(Result::InvalidValue, unsafe {
            begin_update(std::ptr::null_mut(), std::ptr::null_mut())
        });
        // try testing.expectEqual(Result.invalid_value, begin_update(state, null));
        assert_eq!(Result::InvalidValue, unsafe {
            begin_update(state, std::ptr::null_mut())
        });
        // try testing.expectEqual(Result.invalid_value, end_update(null));
        assert_eq!(Result::InvalidValue, unsafe {
            end_update(std::ptr::null_mut())
        });

        // End without a begin is safe.
        assert_eq!(Result::Success, unsafe { end_update(state) });

        unsafe { free(state) };
    });
}

// test "render: begin/end update"
#[test]
fn render_begin_end_update() {
    with_test_allocator(|alloc| {
        with_terminal_state(alloc, 10, 3, |t_res, terminal, s_res, state| {
            assert_eq!(Result::Success, t_res);
            assert_eq!(Result::Success, s_res);

            // Write some styled text so that the update has deferred work.
            // const t = terminal.?.terminal; var s = t.vtStream();
            // s.nextSlice("\x1b[1mAB"); // Bold
            let t = unsafe { (*terminal).terminal };
            {
                let mut s = unsafe { (*t).vt_stream() };
                s.next_slice(b"\x1b[1mAB");
                s.deinit();
            }

            // Begin should record pending work, end should complete it.
            assert_eq!(Result::Success, unsafe { begin_update(state, terminal) });
            assert!(!unsafe { (*state).state.pending_styles.is_empty() });
            assert_eq!(Result::Success, unsafe { end_update(state) });
            assert_eq!(0, unsafe { (*state).state.pending_styles.len() });

            // The cell styles should be complete.
            let row_data = unsafe { &(*state).state.row_data };
            assert!(row_data[0].cells.get(0).style.flags.bold);
            assert!(row_data[0].cells.get(1).style.flags.bold);
        });
    });
}

// PORT NOTE (W12-01 proving tests, Rust-only — the reference has no unit test
// for this; the contract is each constructor's `alloc.create(...) catch {
// result.* = null; return .out_of_memory; }` + each free's `alloc.destroy`):
// the caller-supplied C allocator must OWN each wrapper's lifecycle storage.

// render_state new/free through a counting caller allocator.
#[test]
fn render_new_free_uses_caller_allocator() {
    let stats = crate::test_util::CountingCAllocator::new();
    let calloc = stats.c_allocator();
    let mut state: RenderState = std::ptr::null_mut();
    assert_eq!(Result::Success, unsafe { new(Some(&calloc), &mut state) });
    assert!(!state.is_null());
    assert_eq!(1, stats.alloc_calls.get());
    unsafe { free(state) };
    assert_eq!(1, stats.free_calls.get());
    assert_eq!(0, stats.live_bytes.get());
}

#[test]
fn render_new_rejecting_allocator_out_of_memory() {
    let stats = crate::test_util::CountingCAllocator::new();
    stats.reject.set(true);
    let calloc = stats.c_allocator();
    let mut state: RenderState = std::ptr::null_mut();
    assert_eq!(Result::OutOfMemory, unsafe {
        new(Some(&calloc), &mut state)
    });
    assert!(state.is_null());
    assert_eq!(1, stats.alloc_calls.get());
}

#[test]
fn row_iterator_new_free_uses_caller_allocator() {
    let stats = crate::test_util::CountingCAllocator::new();
    let calloc = stats.c_allocator();
    let mut it: RowIterator = std::ptr::null_mut();
    assert_eq!(Result::Success, unsafe {
        row_iterator_new(Some(&calloc), &mut it)
    });
    assert!(!it.is_null());
    assert_eq!(1, stats.alloc_calls.get());
    unsafe { row_iterator_free(it) };
    assert_eq!(1, stats.free_calls.get());
    assert_eq!(0, stats.live_bytes.get());
}

#[test]
fn row_iterator_new_rejecting_allocator_out_of_memory() {
    let stats = crate::test_util::CountingCAllocator::new();
    stats.reject.set(true);
    let calloc = stats.c_allocator();
    let mut it: RowIterator = std::ptr::null_mut();
    assert_eq!(Result::OutOfMemory, unsafe {
        row_iterator_new(Some(&calloc), &mut it)
    });
    assert!(it.is_null());
    assert_eq!(1, stats.alloc_calls.get());
}

#[test]
fn row_cells_new_free_uses_caller_allocator() {
    let stats = crate::test_util::CountingCAllocator::new();
    let calloc = stats.c_allocator();
    let mut cells: RowCells = std::ptr::null_mut();
    assert_eq!(Result::Success, unsafe {
        row_cells_new(Some(&calloc), &mut cells)
    });
    assert!(!cells.is_null());
    assert_eq!(1, stats.alloc_calls.get());
    unsafe { row_cells_free(cells) };
    assert_eq!(1, stats.free_calls.get());
    assert_eq!(0, stats.live_bytes.get());
}

#[test]
fn row_cells_new_rejecting_allocator_out_of_memory() {
    let stats = crate::test_util::CountingCAllocator::new();
    stats.reject.set(true);
    let calloc = stats.c_allocator();
    let mut cells: RowCells = std::ptr::null_mut();
    assert_eq!(Result::OutOfMemory, unsafe {
        row_cells_new(Some(&calloc), &mut cells)
    });
    assert!(cells.is_null());
    assert_eq!(1, stats.alloc_calls.get());
}

// ======================================================================
// W34-B02 / W34-B03 regression tests (no reference `test {}` block; these
// pin C-boundary contracts the reference gets from its Zig-level guards).
// ======================================================================

// W34-B02-01: the `keys` array of a multi-get is C memory. Each element must be
// range-checked before it becomes a `Data`/`RowData`/`RowCellsData`, exactly as
// the reference reaches its `std.enums.fromInt` guard by routing every element
// through the guarded `get`/`row_get`/`row_cells_get`. On an undeclared element
// the call returns `.invalid_value` and reports `out_written = i` (the number
// of elements that DID succeed).
const C_INVALID_VALUE: c_int = -2;

#[test]
fn render_get_multi_out_of_range_key_is_rejected() {
    // N625: this test reaches `sys::log_fn`; hold the sys guard so its
    // warnings cannot land in another test's log sink.
    let _guard = crate::sys::tests::reset_global();
    with_test_allocator(|alloc| {
        with_terminal_state(
            alloc,
            80,
            24,
            |terminal_new_result, terminal, state_new_result, state| {
                assert_eq!(Result::Success, terminal_new_result);
                assert_eq!(Result::Success, state_new_result);
                assert_eq!(Result::Success, unsafe { update(state, terminal) });

                let mut cols: u16 = 0;
                let mut unused: u32 = 0;
                let mut written: usize = usize::MAX;

                // keys[1] is an UNDECLARED selector.
                let keys: [c_int; 2] = [RenderStateData::Cols as c_int, 999];
                let mut values = [
                    &mut cols as *mut _ as *mut c_void,
                    &mut unused as *mut _ as *mut c_void,
                ];

                assert_eq!(C_INVALID_VALUE, unsafe {
                    crate::rustty_render_state_get_multi(
                        state,
                        keys.len(),
                        keys.as_ptr().cast(),
                        values.as_mut_ptr(),
                        &mut written,
                    )
                });
                // The first element still ran; the second aborted the loop.
                assert_eq!(1, written);
                assert_eq!(80, cols);
            },
        );
    });
}

#[test]
fn render_row_get_multi_out_of_range_key_is_rejected() {
    // N625: this test reaches `sys::log_fn`; hold the sys guard so its
    // warnings cannot land in another test's log sink.
    let _guard = crate::sys::tests::reset_global();
    with_test_allocator(|alloc| {
        with_terminal_state(
            alloc,
            80,
            24,
            |terminal_new_result, terminal, state_new_result, state| {
                assert_eq!(Result::Success, terminal_new_result);
                assert_eq!(Result::Success, state_new_result);
                assert_eq!(Result::Success, unsafe { update(state, terminal) });

                let mut it: RowIterator = std::ptr::null_mut();
                assert_eq!(Result::Success, unsafe {
                    row_iterator_new(Some(alloc), &mut it)
                });
                assert_eq!(Result::Success, unsafe {
                    get(
                        state,
                        RenderStateData::RowIterator,
                        &mut it as *mut _ as *mut c_void,
                    )
                });
                assert!(unsafe { row_iterator_next(it) });

                let mut dirty: bool = false;
                let mut unused: u32 = 0;
                let mut written: usize = usize::MAX;

                let keys: [c_int; 2] = [RenderStateRowData::Dirty as c_int, 999];
                let mut values = [
                    &mut dirty as *mut _ as *mut c_void,
                    &mut unused as *mut _ as *mut c_void,
                ];

                assert_eq!(C_INVALID_VALUE, unsafe {
                    crate::rustty_render_state_row_get_multi(
                        it,
                        keys.len(),
                        keys.as_ptr().cast(),
                        values.as_mut_ptr(),
                        &mut written,
                    )
                });
                assert_eq!(1, written);

                unsafe { row_iterator_free(it) };
            },
        );
    });
}

#[test]
fn render_row_cells_get_multi_out_of_range_key_is_rejected() {
    // N625: this test reaches `sys::log_fn`; hold the sys guard so its
    // warnings cannot land in another test's log sink.
    let _guard = crate::sys::tests::reset_global();
    with_test_allocator(|alloc| {
        with_terminal_state(
            alloc,
            80,
            24,
            |terminal_new_result, terminal, state_new_result, state| {
                assert_eq!(Result::Success, terminal_new_result);
                assert_eq!(Result::Success, state_new_result);
                assert_eq!(Result::Success, unsafe { update(state, terminal) });

                let mut it: RowIterator = std::ptr::null_mut();
                assert_eq!(Result::Success, unsafe {
                    row_iterator_new(Some(alloc), &mut it)
                });
                assert_eq!(Result::Success, unsafe {
                    get(
                        state,
                        RenderStateData::RowIterator,
                        &mut it as *mut _ as *mut c_void,
                    )
                });
                assert!(unsafe { row_iterator_next(it) });

                let mut cells: RowCells = std::ptr::null_mut();
                assert_eq!(Result::Success, unsafe {
                    row_cells_new(Some(alloc), &mut cells)
                });
                assert_eq!(Result::Success, unsafe {
                    row_get(
                        it,
                        RenderStateRowData::Cells,
                        &mut cells as *mut _ as *mut c_void,
                    )
                });
                assert!(unsafe { row_cells_next(cells) });

                let mut raw: page::Cell = Default::default();
                let mut unused: u32 = 0;
                let mut written: usize = usize::MAX;

                let keys: [c_int; 2] = [RowCellsData::Raw as c_int, 999];
                let mut values = [
                    &mut raw as *mut _ as *mut c_void,
                    &mut unused as *mut _ as *mut c_void,
                ];

                assert_eq!(C_INVALID_VALUE, unsafe {
                    crate::rustty_render_state_row_cells_get_multi(
                        cells,
                        keys.len(),
                        keys.as_ptr().cast(),
                        values.as_mut_ptr(),
                        &mut written,
                    )
                });
                assert_eq!(1, written);

                unsafe { row_cells_free(cells) };
                unsafe { row_iterator_free(it) };
            },
        );
    });
}

// W34-B02-02: the invalid-selector diagnostic is the reference's
// `log.warn` on the `.render_state_c` scope, which `terminal/c/sys.zig`'s
// `logFn` routes to the C log callback and DROPS when no callback is installed
// (`if (global.log == null) return;`). It must not go to stderr directly.
mod log_capture {
    use crate::sys::LogLevel;
    use std::sync::Mutex;

    pub(super) static CAPTURED: Mutex<Vec<(LogLevel, String, String)>> = Mutex::new(Vec::new());

    pub(super) extern "C" fn log_cb(
        _userdata: *mut std::ffi::c_void,
        level: LogLevel,
        scope_ptr: *const u8,
        scope_len: usize,
        message_ptr: *const u8,
        message_len: usize,
    ) {
        let scope = unsafe { std::slice::from_raw_parts(scope_ptr, scope_len) };
        let message = unsafe { std::slice::from_raw_parts(message_ptr, message_len) };
        CAPTURED.lock().unwrap().push((
            level,
            String::from_utf8_lossy(scope).into_owned(),
            String::from_utf8_lossy(message).into_owned(),
        ));
    }
}

#[test]
fn render_invalid_selector_warns_through_the_scoped_log_sink() {
    // Serialize against the other tests that mutate the process-wide sys GLOBAL.
    let _guard = crate::sys::tests::reset_global();
    log_capture::CAPTURED.lock().unwrap().clear();

    // A selector value used by no other test, so the capture can be filtered:
    // the render tests run in parallel and several of them also trip the
    // invalid-selector warning while this callback is installed.
    const SEL: c_int = 31337;

    // No callback installed: silently discarded (the C-ABI contract), and the
    // call still reports `.invalid_value`.
    assert_eq!(C_INVALID_VALUE, unsafe {
        crate::rustty_render_state_get(std::ptr::null_mut(), SEL, std::ptr::null_mut())
    });
    assert!(log_capture::CAPTURED.lock().unwrap().is_empty());

    // Install a capturing callback: every render invalid-selector/option
    // diagnostic arrives on the `render_state_c` scope at warning level.
    assert_eq!(Result::Success, unsafe {
        crate::sys::set(
            crate::sys::Option_::Log,
            log_capture::log_cb as *const std::ffi::c_void,
        )
    });

    // PORT NOTE (0d37f2d34/e3056658d END STATE): the get-family selector guard
    // now sits AFTER the handle/position unwrap (`getDispatch`/`rowGetDispatch`
    // /`rowCellsGetDispatch`), so a NULL handle returns `.invalid_value`
    // WITHOUT reaching the diagnostic. Each get-family warn is therefore
    // driven through a live, positioned handle below.
    with_test_allocator(|alloc| {
        let mut terminal: terminal_c::Terminal = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe {
            terminal_c::new(alloc, &mut terminal, 10, 3)
        });
        let mut state: RenderState = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe { new(Some(alloc), &mut state) });
        assert_eq!(Result::Success, unsafe { update(state, terminal) });

        let mut it: RowIterator = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe {
            row_iterator_new(Some(alloc), &mut it)
        });
        assert_eq!(Result::Success, unsafe {
            get(
                state,
                RenderStateData::RowIterator,
                &mut it as *mut _ as *mut c_void,
            )
        });
        assert!(unsafe { row_iterator_next(it) });

        let mut cells: RowCells = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe {
            row_cells_new(Some(alloc), &mut cells)
        });
        assert_eq!(Result::Success, unsafe {
            row_get(
                it,
                RenderStateRowData::Cells,
                &mut cells as *mut _ as *mut c_void,
            )
        });
        assert!(unsafe { row_cells_next(cells) });

        let mut sink: u64 = 0;
        assert_eq!(C_INVALID_VALUE, unsafe {
            crate::rustty_render_state_get(state, SEL, &mut sink as *mut _ as *mut c_void)
        });
        assert_eq!(C_INVALID_VALUE, unsafe {
            crate::rustty_render_state_row_get(it, SEL, &mut sink as *mut _ as *mut c_void)
        });
        assert_eq!(C_INVALID_VALUE, unsafe {
            crate::rustty_render_state_row_cells_get(cells, SEL, &mut sink as *mut _ as *mut c_void)
        });
        assert_eq!(C_INVALID_VALUE, unsafe {
            crate::rustty_render_state_set(std::ptr::null_mut(), SEL, std::ptr::null())
        });
        assert_eq!(C_INVALID_VALUE, unsafe {
            crate::rustty_render_state_row_set(std::ptr::null_mut(), SEL, std::ptr::null())
        });

        unsafe { row_cells_free(cells) };
        unsafe { row_iterator_free(it) };
        unsafe { free(state) };
        unsafe { terminal_c::free(terminal) };
    });

    // Clear the callback before asserting, so a failure cannot strand it.
    assert_eq!(Result::Success, unsafe {
        crate::sys::set(crate::sys::Option_::Log, std::ptr::null())
    });

    let all = log_capture::CAPTURED.lock().unwrap().clone();
    // FLAKE FIX (task 15): this loop used to run over EVERY captured entry and
    // assert it was a `render_state_c` warning. `reset_global()` serializes the
    // tests that MUTATE the sys global, but it does not — and cannot — stop a
    // concurrently running test from EMITTING a diagnostic while this callback
    // is installed, so any `debug`-level line from elsewhere in the binary made
    // this test fail (`left: Warning, right: Debug`, reproducible about one run
    // in three). The test's own comment above already says the SEL value exists
    // precisely so the capture can be filtered; assert over the entries this
    // test caused, which is the only set it controls. Coverage of this test's
    // subject is unchanged — every diagnostic it provokes is still checked for
    // scope, level, order and text.
    let mine: Vec<&(crate::sys::LogLevel, String, String)> = all
        .iter()
        .filter(|(_, _, m)| m.ends_with("=31337"))
        .collect();
    for (level, scope, _) in mine.iter() {
        assert_eq!(crate::sys::LogLevel::Warning, *level);
        assert_eq!(LOG_SCOPE, scope);
    }
    let captured: Vec<&String> = mine.iter().map(|(_, _, m)| m).collect();
    let expected = [
        "render_state_get invalid data value=31337",
        "render_state_row_get invalid data value=31337",
        "render_state_row_cells_get invalid data value=31337",
        "render_state_set invalid option value=31337",
        "render_state_row_set invalid option value=31337",
    ];
    assert_eq!(expected.len(), captured.len(), "captured: {all:?}");
    for (i, message) in captured.iter().enumerate() {
        assert_eq!(expected[i], *message);
    }
}

// W34-B03: `RusttyRenderStateDirty` is an int-sized C enum (the header declares
// `RUSTTY_ENUM_MAX_VALUE`, and the reference's `lib.Enum` is `c_int`-backed), so
// `get(.dirty)` must write — and `set(.dirty)` must read — the caller's FULL
// int slot, not one byte of it.
#[test]
fn render_dirty_payload_is_int_sized() {
    assert_eq!(std::mem::size_of::<c_int>(), std::mem::size_of::<Dirty>());
    assert_eq!(std::mem::align_of::<c_int>(), std::mem::align_of::<Dirty>());
    assert_eq!(0, Dirty::False as c_int);
    assert_eq!(1, Dirty::Partial as c_int);
    assert_eq!(2, Dirty::Full as c_int);
}

#[test]
fn render_get_dirty_fills_the_whole_int_slot() {
    with_test_allocator(|alloc| {
        with_terminal_state(
            alloc,
            80,
            24,
            |terminal_new_result, terminal, state_new_result, state| {
                assert_eq!(Result::Success, terminal_new_result);
                assert_eq!(Result::Success, state_new_result);
                assert_eq!(Result::Success, unsafe { update(state, terminal) });

                for (want, tag) in [
                    (Dirty::False, 0 as c_int),
                    (Dirty::Partial, 1),
                    (Dirty::Full, 2),
                ] {
                    // Set through the full-width C payload.
                    assert_eq!(Result::Success, unsafe {
                        set(
                            state,
                            SetOption::Dirty,
                            &tag as *const c_int as *const c_void,
                        )
                    });

                    // A C caller's `RusttyRenderStateDirty dirty;` local is an
                    // UNINITIALIZED int: model that with poisoned high bytes.
                    // `get` must overwrite all four, or the caller's `switch`
                    // matches no case.
                    let mut slot: c_int = 0x7F7F_7F01u32 as c_int;
                    assert_eq!(Result::Success, unsafe {
                        get(
                            state,
                            RenderStateData::Dirty,
                            &mut slot as *mut c_int as *mut c_void,
                        )
                    });
                    assert_eq!(tag, slot);

                    // ...and the typed read agrees.
                    let mut dirty = Dirty::False;
                    assert_eq!(Result::Success, unsafe {
                        get(
                            state,
                            RenderStateData::Dirty,
                            &mut dirty as *mut _ as *mut c_void,
                        )
                    });
                    assert_eq!(want, dirty);
                }
            },
        );
    });
}

#[test]
fn render_set_dirty_reads_the_whole_int_slot() {
    with_test_allocator(|alloc| {
        with_terminal_state(
            alloc,
            80,
            24,
            |terminal_new_result, terminal, state_new_result, state| {
                assert_eq!(Result::Success, terminal_new_result);
                assert_eq!(Result::Success, state_new_result);
                assert_eq!(Result::Success, unsafe { update(state, terminal) });

                // A full-width payload whose LOW byte alone would be `.full`
                // but whose high bytes are junk is NOT a declared value: the
                // reference's `c_int`-backed load sees the whole word, and Rust
                // must not transmute the low byte into a `Dirty`.
                // Park the state on `.partial` so the junk value below (whose
                // LOW byte is `.full`) would be observable if it were applied.
                let partial: c_int = Dirty::Partial as c_int;
                assert_eq!(Result::Success, unsafe {
                    set(
                        state,
                        SetOption::Dirty,
                        &partial as *const c_int as *const c_void,
                    )
                });

                let junk: c_int = 0x7F7F_7F02u32 as c_int;
                assert_eq!(Result::InvalidValue, unsafe {
                    set(
                        state,
                        SetOption::Dirty,
                        &junk as *const c_int as *const c_void,
                    )
                });
                // ...and the state is untouched.
                let mut dirty = Dirty::False;
                assert_eq!(Result::Success, unsafe {
                    get(
                        state,
                        RenderStateData::Dirty,
                        &mut dirty as *mut _ as *mut c_void,
                    )
                });
                assert_eq!(Dirty::Partial, dirty);

                // The declared full-width value is accepted.
                let full: c_int = Dirty::Full as c_int;
                assert_eq!(Result::Success, unsafe {
                    set(
                        state,
                        SetOption::Dirty,
                        &full as *const c_int as *const c_void,
                    )
                });
                assert_eq!(Result::Success, unsafe {
                    get(
                        state,
                        RenderStateData::Dirty,
                        &mut dirty as *mut _ as *mut c_void,
                    )
                });
                assert_eq!(Dirty::Full, dirty);
            },
        );
    });
}

// =============================================================================
// Parity tail S7-5 (S175-F1, S4-F2's render half; Rust-only — the reference
// has no test for this): the render state's CONTENTS allocate through the
// allocator the state was created with (`state.alloc`), not the terminal's and
// not the Rust heap, and a refused allocation is `OUT_OF_MEMORY`.
// =============================================================================

fn vtw(terminal: terminal_c::Terminal, bytes: &[u8]) {
    // SAFETY: `terminal` is a live handle; `bytes` is valid for its length.
    unsafe { terminal_c::vt_write(terminal, bytes.as_ptr(), bytes.len()) };
}

// A render state over a counting allocator, on a terminal over the default
// (NULL) allocator: `new` is the wrapper; the first update adds `row_data`,
// the three rows' three cell columns, the pending style runs, row 0's applied
// runs and row 0's grapheme buffer; a steady update adds nothing; `free`
// returns every byte.
// The terminal's own allocations never reach the state's allocator.
#[test]
fn render_state_internals_route_through_the_callers_allocator() {
    use rustty_vt::alloc::testing::CountingAllocator;

    let counting = CountingAllocator::over_heap();
    let calloc = counting.allocator();
    let mut terminal: terminal_c::Terminal = std::ptr::null_mut();
    assert_eq!(Result::Success, unsafe {
        terminal_c::new(std::ptr::null(), &mut terminal, 10, 3)
    });
    let mut state: RenderState = std::ptr::null_mut();
    assert_eq!(Result::Success, unsafe { new(Some(&calloc), &mut state) });
    assert_eq!(1, counting.alloc_calls.get(), "the wrapper");

    vtw(
        terminal,
        "\x1b[1mA\x1b[0m \u{1F468}\u{200D}\u{1F469}".as_bytes(),
    );
    assert_eq!(
        1,
        counting.alloc_calls.get(),
        "writes allocate on the terminal's allocator"
    );
    assert_eq!(Result::Success, unsafe { begin_update(state, terminal) });
    assert_eq!(Result::Success, unsafe { end_update(state) });
    assert_eq!(
        1 + 1 + 3 * 3 + 3,
        counting.alloc_calls.get(),
        "row_data, 3 rows x 3 cell columns, pending runs, applied runs, graphemes"
    );
    // SAFETY: `state` is live; a read of the Rust-side contents.
    unsafe {
        let s = &(*state).state;
        assert_eq!(3, s.row_data.len());
        assert!(s.row_data[0].cells.get(0).style.flags.bold);
        assert!(!s.row_data[0].cells.allocator().is_global());
        assert!(!s.row_data.allocator().is_global());
    }

    vtw(terminal, b"\x1b[2;1Hsecond row");
    assert_eq!(Result::Success, unsafe { update(state, terminal) });
    assert_eq!(
        14,
        counting.alloc_calls.get(),
        "a steady update allocates nothing"
    );
    // Growth past the capacities: `row_data` and the old rows' cell columns
    // move (alloc + free each), the new rows get their columns.
    assert_eq!(Result::Success, unsafe {
        terminal_c::resize(terminal, 40, 12, 1, 1)
    });
    let (allocs, frees) = (counting.alloc_calls.get(), counting.free_calls.get());
    assert_eq!(Result::Success, unsafe { update(state, terminal) });
    assert_eq!(allocs + 1 + 3 * 3 + 9 * 3, counting.alloc_calls.get());
    assert_eq!(frees + 1 + 3 * 3, counting.free_calls.get());

    unsafe { free(state) };
    assert_eq!(0, counting.live_bytes.get(), "free returns every byte");
    assert_eq!(counting.alloc_calls.get(), counting.free_calls.get());
    unsafe { terminal_c::free(terminal) };
}

/// One C-API sweep scenario for a render state over `failing` (the terminal
/// on the default allocator): `new`, updates in both forms over styled text,
/// graphemes and background-color erases, growth past every capacity, a
/// shrink, `free`. Every result is `SUCCESS` or `OUT_OF_MEMORY`; after an
/// `OUT_OF_MEMORY` update the failure is lifted and the retried update must
/// equal a fresh state's (R13, UD315). Returns (whether `new` failed, the
/// calls that reported `OUT_OF_MEMORY`).
fn render_sweep_once(
    failing: &rustty_vt::alloc::testing::FailingAllocator,
    what: &str,
) -> (bool, usize) {
    let calloc = failing.allocator();
    let mut terminal: terminal_c::Terminal = std::ptr::null_mut();
    assert_eq!(Result::Success, unsafe {
        terminal_c::new(std::ptr::null(), &mut terminal, 10, 3)
    });
    let mut state: RenderState = std::ptr::null_mut();
    let r = unsafe { new(Some(&calloc), &mut state) };
    let mut call_failures = 0usize;
    let new_failed = match r {
        Result::Success => {
            let mut step = |r: Result, name: &str| {
                assert!(
                    matches!(r, Result::Success | Result::OutOfMemory),
                    "{what} {name}: unexpected result {r:?}"
                );
                if r == Result::OutOfMemory {
                    call_failures += 1;
                    failing.fail_index.set(usize::MAX);
                    assert_eq!(Result::Success, unsafe { update(state, terminal) });
                }
                assert_matches_fresh(state, terminal, &format!("{what} {name}"));
            };

            vtw(terminal, b"\x1b[1mbold\x1b[0m plain\r\n");
            step(unsafe { update(state, terminal) }, "first update");

            let lines = if cfg!(miri) { 2 } else { 8 };
            for i in 0..lines {
                vtw(
                    terminal,
                    format!(
                        "\x1b[3{}m{i}\x1b[0m e\u{301} \u{1F468}\u{200D}\u{1F469}\x1b[4{}m\x1b[K\x1b[0m\r\n",
                        i % 8,
                        (i + 1) % 8
                    )
                    .as_bytes(),
                );
            }
            let r = unsafe { begin_update(state, terminal) };
            assert_eq!(Result::Success, unsafe { end_update(state) });
            step(r, "two-phase update");

            assert_eq!(Result::Success, unsafe {
                terminal_c::resize(terminal, 40, 14, 1, 1)
            });
            vtw(
                terminal,
                b"\x1b[1mA\x1b[0mB\x1b[1mC\x1b[0mD\x1b[1mE\x1b[0mF\x1b[1mG\x1b[0mH\x1b[1mI\x1b[0m\r\n",
            );
            step(unsafe { update(state, terminal) }, "growth");

            assert_eq!(Result::Success, unsafe {
                terminal_c::resize(terminal, 20, 4, 1, 1)
            });
            step(unsafe { update(state, terminal) }, "shrink");

            unsafe { free(state) };
            false
        }
        Result::OutOfMemory => {
            assert!(state.is_null(), "{what}: a failed new returns no state");
            true
        }
        other => panic!("{what}: new returned {other:?}"),
    };
    unsafe { terminal_c::free(terminal) };
    assert_eq!(
        failing.allocated_bytes.get(),
        failing.freed_bytes.get(),
        "{what}: every byte taken from the caller's allocator is returned"
    );
    assert_eq!(
        failing.allocations.get(),
        failing.deallocations.get(),
        "{what}"
    );
    (new_failed, call_failures)
}

/// The state's rows equal those of a fresh render state (over the default
/// allocator) updated from the same terminal: dimensions, and every row's raw
/// data and raw cells.
fn assert_matches_fresh(state: RenderState, terminal: terminal_c::Terminal, what: &str) {
    let mut fresh: RenderState = std::ptr::null_mut();
    assert_eq!(Result::Success, unsafe { new(None, &mut fresh) });
    assert_eq!(Result::Success, unsafe { update(fresh, terminal) });
    // SAFETY: both states are live; reads of their Rust-side contents.
    unsafe {
        let (got, want) = (&(*state).state, &(*fresh).state);
        assert_eq!((want.rows, want.cols), (got.rows, got.cols), "{what}: size");
        assert_eq!(want.row_data.len(), got.row_data.len(), "{what}: rows");
        for (y, (g, w)) in got.row_data.iter().zip(want.row_data.iter()).enumerate() {
            assert_eq!(w.raw.cval(), g.raw.cval(), "{what}: row {y}");
            let (g, w) = (g.cells.raw(), w.cells.raw());
            assert_eq!(w.len(), g.len(), "{what}: row {y} cells");
            for (x, (g, w)) in g.iter().zip(w.iter()).enumerate() {
                assert_eq!(w.cval(), g.cval(), "{what}: cell {x},{y}");
            }
        }
    }
    unsafe { free(fresh) };
}

// T2 through the C API (parity tail S7-5; the Zig `checkAllAllocationFailures`
// idea): for every n, a render state whose allocator refuses its n-th
// allocation. Nothing aborts, every result is `SUCCESS` or `OUT_OF_MEMORY`, a
// retried update equals a fresh state, and `free` returns every byte. The
// sweep ends at the first n with no induced failure. A second sweep refuses
// `remap`/`resize` from their n-th call on: growth falls back to
// alloc + copy + free and nothing fails. Under Miri the corpus is smaller.
#[test]
#[cfg_attr(
    miri,
    ignore = "MIRI EXCEPTION (rustty-vt-c, parity tail C-1): intractable under Miri, one full render-state update per allocation of the sweep; measured past a 20-minute bound; runs natively and under ASan (see MIRI_EXCEPTIONS.md)"
)]
fn render_state_allocation_failure_sweep_through_the_c_api() {
    use rustty_vt::alloc::testing::{Config, FailingAllocator};

    let mut n = 0usize;
    let mut new_failures = 0usize;
    let mut call_failures = 0usize;
    loop {
        let failing = FailingAllocator::over_heap(Config {
            fail_index: n,
            ..Config::default()
        });
        let (new_failed, calls) = render_sweep_once(&failing, &format!("alloc n={n}"));
        new_failures += usize::from(new_failed);
        call_failures += calls;
        if !failing.has_induced_failure.get() {
            break;
        }
        assert_eq!(
            1,
            usize::from(new_failed) + calls,
            "alloc n={n}: exactly the refused call fails"
        );
        n += 1;
        assert!(n < 10_000, "the sweep must terminate");
    }
    let min_swept = if cfg!(miri) { 10 } else { 25 };
    assert!(n > min_swept, "only {n} allocations were swept");
    assert_eq!(1, new_failures, "only the wrapper's allocation fails new");
    assert_eq!(n - 1, call_failures);

    let clean = FailingAllocator::over_heap(Config::default());
    assert_eq!((false, 0), render_sweep_once(&clean, "remap clean run"));
    let remaps = clean.resize_index.get();
    assert!(remaps > 0, "the corpus makes no remap call");
    for n in 0..=remaps {
        let failing = FailingAllocator::over_heap(Config {
            resize_fail_index: n,
            ..Config::default()
        });
        assert_eq!(
            (false, 0),
            render_sweep_once(&failing, &format!("remap n={n}")),
            "remap n={n}: a refused remap falls back"
        );
        assert!(failing.resize_index.get() <= n, "remap n={n}");
    }
}

// ---------------------------------------------------------------------------
// P7-W3 T2 (`2ea1eeae2`): RenderState overscan and row identity in the C API.
// ---------------------------------------------------------------------------

/// Test helper: a terminal of the given size with `lines` numbered lines
/// written, so everything above the last `rows` lines is scrollback.
fn test_terminal_with_lines(
    alloc: &CAllocator,
    cols: size::CellCountInt,
    rows: size::CellCountInt,
    lines: usize,
) -> terminal_c::Terminal {
    let mut terminal: terminal_c::Terminal = std::ptr::null_mut();
    assert_eq!(Result::Success, unsafe {
        terminal_c::new(alloc, &mut terminal, cols, rows)
    });

    for i in 0..lines {
        let line = format!("{}line {i}", if i == 0 { "" } else { "\r\n" });
        vtw(terminal, line.as_bytes());
    }

    terminal
}

/// Scroll the terminal's viewport (the reference calls
/// `terminal.?.terminal.scrollViewport(...)` directly).
fn test_scroll_viewport(
    terminal: terminal_c::Terminal,
    behavior: rustty_vt::terminal::ScrollViewport,
) {
    // SAFETY: `terminal` is a live handle; no other borrow of its terminal is live.
    unsafe { (*(*terminal).terminal).scroll_viewport(behavior) };
}

fn set_overscan(state: RenderState, req: &Overscan) -> Result {
    unsafe {
        set(
            state,
            SetOption::Overscan,
            req as *const Overscan as *const c_void,
        )
    }
}

fn get_overscan(state: RenderState, data: RenderStateData, out: &mut Overscan) -> Result {
    unsafe { get(state, data, out as *mut Overscan as *mut c_void) }
}

/// Test helper: a row id and its viewport y from one iteration.
#[derive(Clone, Copy, Debug)]
struct TestRowEntry {
    id: RowId,
    viewport_y: i32,
}

/// Test helper: collect the id and viewport y of every captured row.
fn test_collect_rows(alloc: &CAllocator, state: RenderState) -> Vec<TestRowEntry> {
    let mut it: RowIterator = std::ptr::null_mut();
    assert_eq!(Result::Success, unsafe {
        row_iterator_new(Some(alloc), &mut it)
    });
    assert_eq!(Result::Success, unsafe {
        get(
            state,
            RenderStateData::RowIterator,
            &mut it as *mut _ as *mut c_void,
        )
    });

    let mut list = Vec::new();
    while unsafe { row_iterator_next(it) } {
        let mut entry = TestRowEntry {
            id: RowId::default(),
            viewport_y: 0,
        };
        assert_eq!(Result::Success, unsafe {
            row_get(
                it,
                RenderStateRowData::Id,
                &mut entry.id as *mut _ as *mut c_void,
            )
        });
        assert_eq!(Result::Success, unsafe {
            row_get(
                it,
                RenderStateRowData::ViewportY,
                &mut entry.viewport_y as *mut _ as *mut c_void,
            )
        });
        list.push(entry);
    }

    unsafe { row_iterator_free(it) };
    list
}

// test "render: overscan request applies on the next update"
#[test]
fn render_overscan_request_applies_on_the_next_update() {
    with_test_allocator(|alloc| {
        let rows: usize = 10;
        let terminal = test_terminal_with_lines(alloc, 10, rows as u16, 50);
        test_scroll_viewport(terminal, rustty_vt::terminal::ScrollViewport::Delta(-5));

        let mut state: RenderState = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe { new(Some(alloc), &mut state) });
        let req = Overscan { above: 3, below: 2 };
        assert_eq!(Result::Success, set_overscan(state, &req));
        assert_eq!(Result::Success, unsafe { update(state, terminal) });

        // Changing the request after an update must not affect reading the
        // rows of that update.
        let none = Overscan::default();
        assert_eq!(Result::Success, set_overscan(state, &none));
        let mut out = Overscan::default();
        assert_eq!(
            Result::Success,
            get_overscan(state, RenderStateData::OverscanRequest, &mut out)
        );
        assert_eq!(none, out);
        assert_eq!(
            Result::Success,
            get_overscan(state, RenderStateData::Overscan, &mut out)
        );
        assert_eq!(req, out);

        let list = test_collect_rows(alloc, state);
        assert_eq!(3 + rows + 2, list.len());
        assert_eq!(-3i32, list[0].viewport_y);

        // The next update uses the new request.
        assert_eq!(Result::Success, unsafe { update(state, terminal) });
        let list = test_collect_rows(alloc, state);
        assert_eq!(rows, list.len());
        assert_eq!(0i32, list[0].viewport_y);

        unsafe { free(state) };
        unsafe { terminal_c::free(terminal) };
    });
}

// test "render: overscan option roundtrip"
#[test]
fn render_overscan_option_roundtrip() {
    with_test_allocator(|alloc| {
        let mut state: RenderState = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe { new(Some(alloc), &mut state) });

        let req = Overscan { above: 4, below: 1 };
        assert_eq!(Result::Success, set_overscan(state, &req));

        let mut out = Overscan {
            above: 99,
            below: 99,
        };
        assert_eq!(
            Result::Success,
            get_overscan(state, RenderStateData::OverscanRequest, &mut out)
        );
        assert_eq!(Overscan { above: 4, below: 1 }, out);

        // Nothing is captured before an update.
        assert_eq!(
            Result::Success,
            get_overscan(state, RenderStateData::Overscan, &mut out)
        );
        assert_eq!(Overscan::default(), out);

        unsafe { free(state) };
    });
}

// test "render: overscan clamps to existing rows"
#[test]
fn render_overscan_clamps_to_existing_rows() {
    with_test_allocator(|alloc| {
        let terminal = test_terminal_with_lines(alloc, 10, 10, 50);

        let mut state: RenderState = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe { new(Some(alloc), &mut state) });

        let req = Overscan { above: 3, below: 2 };
        assert_eq!(Result::Success, set_overscan(state, &req));

        let mut out = Overscan::default();

        // At the bottom there is nothing below the viewport.
        assert_eq!(Result::Success, unsafe { update(state, terminal) });
        assert_eq!(
            Result::Success,
            get_overscan(state, RenderStateData::Overscan, &mut out)
        );
        assert_eq!(Overscan { above: 3, below: 0 }, out);

        // Scrolled up, both sides are available.
        test_scroll_viewport(terminal, rustty_vt::terminal::ScrollViewport::Delta(-5));
        assert_eq!(Result::Success, unsafe { update(state, terminal) });
        assert_eq!(
            Result::Success,
            get_overscan(state, RenderStateData::Overscan, &mut out)
        );
        assert_eq!(Overscan { above: 3, below: 2 }, out);

        // At the top there is nothing above the viewport.
        test_scroll_viewport(terminal, rustty_vt::terminal::ScrollViewport::Top);
        assert_eq!(Result::Success, unsafe { update(state, terminal) });
        assert_eq!(
            Result::Success,
            get_overscan(state, RenderStateData::Overscan, &mut out)
        );
        assert_eq!(Overscan { above: 0, below: 2 }, out);

        // The request is reported unchanged.
        assert_eq!(
            Result::Success,
            get_overscan(state, RenderStateData::OverscanRequest, &mut out)
        );
        assert_eq!(req, out);

        unsafe { free(state) };
        unsafe { terminal_c::free(terminal) };
    });
}

// test "render: row iterator unchanged without overscan"
#[test]
fn render_row_iterator_unchanged_without_overscan() {
    with_test_allocator(|alloc| {
        let rows: usize = 10;
        let terminal = test_terminal_with_lines(alloc, 10, rows as u16, 50);
        test_scroll_viewport(terminal, rustty_vt::terminal::ScrollViewport::Delta(-5));

        let mut state: RenderState = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe { new(Some(alloc), &mut state) });
        assert_eq!(Result::Success, unsafe { update(state, terminal) });

        let mut it: RowIterator = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe {
            row_iterator_new(Some(alloc), &mut it)
        });
        assert_eq!(Result::Success, unsafe {
            get(
                state,
                RenderStateData::RowIterator,
                &mut it as *mut _ as *mut c_void,
            )
        });

        // The first update is a full redraw so next_dirty visits every row.
        let mut count: usize = 0;
        let mut y: size::CellCountInt = 0;
        while unsafe { row_iterator_next_dirty(it, &mut y) } {
            assert_eq!(count, y as usize);
            let mut vy: i32 = 0;
            assert_eq!(Result::Success, unsafe {
                row_get(
                    it,
                    RenderStateRowData::ViewportY,
                    &mut vy as *mut _ as *mut c_void,
                )
            });
            assert_eq!(i32::from(y), vy);
            count += 1;
        }
        assert_eq!(rows, count);

        unsafe { row_iterator_free(it) };
        unsafe { free(state) };
        unsafe { terminal_c::free(terminal) };
    });
}

// test "render: row iterator covers overscan"
#[test]
fn render_row_iterator_covers_overscan() {
    with_test_allocator(|alloc| {
        let rows: usize = 10;
        let terminal = test_terminal_with_lines(alloc, 10, rows as u16, 50);
        test_scroll_viewport(terminal, rustty_vt::terminal::ScrollViewport::Delta(-5));

        // A reference state with no overscan for comparison.
        let mut plain: RenderState = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe { new(Some(alloc), &mut plain) });
        assert_eq!(Result::Success, unsafe { update(plain, terminal) });

        let mut plain_it: RowIterator = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe {
            row_iterator_new(Some(alloc), &mut plain_it)
        });
        assert_eq!(Result::Success, unsafe {
            get(
                plain,
                RenderStateData::RowIterator,
                &mut plain_it as *mut _ as *mut c_void,
            )
        });
        assert!(unsafe { row_iterator_next(plain_it) });
        let mut plain_raw: row::CRow = 0;
        let mut plain_id = RowId::default();
        assert_eq!(Result::Success, unsafe {
            row_get(
                plain_it,
                RenderStateRowData::Raw,
                &mut plain_raw as *mut _ as *mut c_void,
            )
        });
        assert_eq!(Result::Success, unsafe {
            row_get(
                plain_it,
                RenderStateRowData::Id,
                &mut plain_id as *mut _ as *mut c_void,
            )
        });

        let mut state: RenderState = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe { new(Some(alloc), &mut state) });
        let req = Overscan { above: 3, below: 2 };
        assert_eq!(Result::Success, set_overscan(state, &req));
        assert_eq!(Result::Success, unsafe { update(state, terminal) });

        let mut it: RowIterator = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe {
            row_iterator_new(Some(alloc), &mut it)
        });
        assert_eq!(Result::Success, unsafe {
            get(
                state,
                RenderStateData::RowIterator,
                &mut it as *mut _ as *mut c_void,
            )
        });

        let mut count: usize = 0;
        let mut found_top = false;
        while unsafe { row_iterator_next(it) } {
            let mut vy: i32 = 0;
            assert_eq!(Result::Success, unsafe {
                row_get(
                    it,
                    RenderStateRowData::ViewportY,
                    &mut vy as *mut _ as *mut c_void,
                )
            });
            assert_eq!(count as i32 - 3, vy);

            if vy == 0 {
                found_top = true;
                let mut raw: row::CRow = 0;
                let mut id = RowId::default();
                assert_eq!(Result::Success, unsafe {
                    row_get(
                        it,
                        RenderStateRowData::Raw,
                        &mut raw as *mut _ as *mut c_void,
                    )
                });
                assert_eq!(Result::Success, unsafe {
                    row_get(it, RenderStateRowData::Id, &mut id as *mut _ as *mut c_void)
                });
                assert_eq!(plain_raw, raw);
                assert_eq!(plain_id, id);
            }
            count += 1;
        }
        assert_eq!(3 + rows + 2, count);
        assert!(found_top);

        unsafe { row_iterator_free(it) };
        unsafe { free(state) };
        unsafe { row_iterator_free(plain_it) };
        unsafe { free(plain) };
        unsafe { terminal_c::free(terminal) };
    });
}

// test "render: row iterator overscan at the bottom"
#[test]
fn render_row_iterator_overscan_at_the_bottom() {
    with_test_allocator(|alloc| {
        let rows: usize = 10;
        let terminal = test_terminal_with_lines(alloc, 10, rows as u16, 50);

        let mut state: RenderState = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe { new(Some(alloc), &mut state) });
        let req = Overscan { above: 3, below: 2 };
        assert_eq!(Result::Success, set_overscan(state, &req));
        assert_eq!(Result::Success, unsafe { update(state, terminal) });

        let list = test_collect_rows(alloc, state);
        assert_eq!(3 + rows, list.len());
        assert_eq!(-3i32, list[0].viewport_y);
        assert_eq!(rows as i32 - 1, list[list.len() - 1].viewport_y);

        unsafe { free(state) };
        unsafe { terminal_c::free(terminal) };
    });
}

// test "render: row ids stable across scroll"
#[test]
fn render_row_ids_stable_across_scroll() {
    with_test_allocator(|alloc| {
        let rows: i32 = 10;
        let terminal = test_terminal_with_lines(alloc, 10, rows as u16, 50);
        test_scroll_viewport(terminal, rustty_vt::terminal::ScrollViewport::Delta(-5));

        let mut state: RenderState = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe { new(Some(alloc), &mut state) });
        let req = Overscan { above: 3, below: 2 };
        assert_eq!(Result::Success, set_overscan(state, &req));
        assert_eq!(Result::Success, unsafe { update(state, terminal) });

        let before = test_collect_rows(alloc, state);

        test_scroll_viewport(terminal, rustty_vt::terminal::ScrollViewport::Delta(-1));
        assert_eq!(Result::Success, unsafe { update(state, terminal) });

        let after = test_collect_rows(alloc, state);
        assert_eq!(before.len(), after.len());

        // Every row except the last one (which scrolled out of the captured
        // rows at the bottom) is still present, one row further down.
        for old in &before {
            if old.viewport_y == rows + 1 {
                continue;
            }
            let new_vy: Option<i32> = after
                .iter()
                .find(|cur| cur.id == old.id)
                .map(|cur| cur.viewport_y);
            assert_eq!(old.viewport_y + 1, new_vy.unwrap());
        }

        unsafe { free(state) };
        unsafe { terminal_c::free(terminal) };
    });
}

// test "render: row id without overscan"
#[test]
fn render_row_id_without_overscan() {
    with_test_allocator(|alloc| {
        let terminal = test_terminal_with_lines(alloc, 10, 5, 20);

        let mut state: RenderState = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe { new(Some(alloc), &mut state) });

        assert_eq!(Result::Success, unsafe { update(state, terminal) });
        let first = test_collect_rows(alloc, state);

        assert_eq!(Result::Success, unsafe { update(state, terminal) });
        let second = test_collect_rows(alloc, state);

        assert_eq!(5, first.len());
        assert_eq!(first.len(), second.len());
        for (a, b) in first.iter().zip(second.iter()) {
            assert_eq!(a.id, b.id);
            assert!(a.id != RowId::default());
        }

        unsafe { free(state) };
        unsafe { terminal_c::free(terminal) };
    });
}

// test "render: overscan invalid values"
#[test]
fn render_overscan_invalid_values() {
    with_test_allocator(|alloc| {
        let mut state: RenderState = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe { new(Some(alloc), &mut state) });

        assert_eq!(Result::InvalidValue, unsafe {
            get(state, RenderStateData::Overscan, std::ptr::null_mut())
        });
        assert_eq!(Result::InvalidValue, unsafe {
            set(state, SetOption::Overscan, std::ptr::null())
        });

        let terminal = test_terminal_with_lines(alloc, 10, 5, 1);
        assert_eq!(Result::Success, unsafe { update(state, terminal) });

        let mut it: RowIterator = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe {
            row_iterator_new(Some(alloc), &mut it)
        });
        assert_eq!(Result::Success, unsafe {
            get(
                state,
                RenderStateData::RowIterator,
                &mut it as *mut _ as *mut c_void,
            )
        });

        // Before the first next, there is no current row.
        let mut id = RowId::default();
        assert_eq!(Result::InvalidValue, unsafe {
            row_get(it, RenderStateRowData::Id, &mut id as *mut _ as *mut c_void)
        });

        unsafe { row_iterator_free(it) };
        unsafe { terminal_c::free(terminal) };
        unsafe { free(state) };
    });
}

// test "render: overscan get_multi and row_get_multi"
#[test]
fn render_overscan_get_multi_and_row_get_multi() {
    with_test_allocator(|alloc| {
        let terminal = test_terminal_with_lines(alloc, 10, 5, 20);
        test_scroll_viewport(terminal, rustty_vt::terminal::ScrollViewport::Delta(-5));

        let mut state: RenderState = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe { new(Some(alloc), &mut state) });
        let req = Overscan { above: 1, below: 1 };
        assert_eq!(Result::Success, set_overscan(state, &req));
        assert_eq!(Result::Success, unsafe { update(state, terminal) });

        let mut rows: u16 = 0;
        let mut overscan = Overscan::default();
        let mut overscan_req = Overscan::default();
        let mut written: usize = 0;
        let keys = [
            RenderStateData::Rows as c_int,
            RenderStateData::Overscan as c_int,
            RenderStateData::OverscanRequest as c_int,
        ];
        let mut values: [*mut c_void; 3] = [
            &mut rows as *mut _ as *mut c_void,
            &mut overscan as *mut _ as *mut c_void,
            &mut overscan_req as *mut _ as *mut c_void,
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
        assert_eq!(5, rows);
        assert_eq!(Overscan { above: 1, below: 1 }, overscan);
        assert_eq!(req, overscan_req);

        let mut it: RowIterator = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe {
            row_iterator_new(Some(alloc), &mut it)
        });
        assert_eq!(Result::Success, unsafe {
            get(
                state,
                RenderStateData::RowIterator,
                &mut it as *mut _ as *mut c_void,
            )
        });
        assert!(unsafe { row_iterator_next(it) });

        let mut dirty: bool = false;
        let mut vy: i32 = 0;
        let mut id = RowId::default();
        let row_keys = [
            RenderStateRowData::Dirty as c_int,
            RenderStateRowData::ViewportY as c_int,
            RenderStateRowData::Id as c_int,
        ];
        let mut row_values: [*mut c_void; 3] = [
            &mut dirty as *mut _ as *mut c_void,
            &mut vy as *mut _ as *mut c_void,
            &mut id as *mut _ as *mut c_void,
        ];
        assert_eq!(Result::Success, unsafe {
            row_get_multi(
                it,
                row_keys.len(),
                row_keys.as_ptr(),
                row_values.as_mut_ptr(),
                &mut written,
            )
        });
        assert_eq!(row_keys.len(), written);
        assert_eq!(-1i32, vy);
        assert!(id != RowId::default());

        unsafe { row_iterator_free(it) };
        unsafe { free(state) };
        unsafe { terminal_c::free(terminal) };
    });
}

// PORT-SPECIFIC (P7-W3 T2, Stacked Borrows): the overscan iterator is a
// SUB-slice of `row_data`. `rustty_render_state_clean` then mutates every row
// through a fresh `&mut` while the iterator stays live (the "existing
// iterators observe cleaning" contract), and the iterator is read and written
// afterwards. The sub-slice must be built by raw pointer arithmetic off the
// buffer root; a `&mut row_data[range]`-derived slice is invalidated by
// `clean`'s reborrow, which Miri reports as UB here.
#[test]
fn render_clean_interleaves_with_a_live_overscan_iterator() {
    with_test_allocator(|alloc| {
        let terminal = test_terminal_with_lines(alloc, 10, 5, 20);
        test_scroll_viewport(terminal, rustty_vt::terminal::ScrollViewport::Delta(-5));

        let mut state: RenderState = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe { new(Some(alloc), &mut state) });
        let req = Overscan { above: 2, below: 1 };
        assert_eq!(Result::Success, set_overscan(state, &req));
        assert_eq!(Result::Success, unsafe { update(state, terminal) });

        let mut it: RowIterator = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe {
            row_iterator_new(Some(alloc), &mut it)
        });
        assert_eq!(Result::Success, unsafe {
            get(
                state,
                RenderStateData::RowIterator,
                &mut it as *mut _ as *mut c_void,
            )
        });
        assert_eq!(2 + 5 + 1, unsafe { (*it).rows.len() });

        // Read one row, then clean the state under the live iterator.
        assert!(unsafe { row_iterator_next(it) });
        let mut dirty = false;
        assert_eq!(Result::Success, unsafe {
            row_get(
                it,
                RenderStateRowData::Dirty,
                &mut dirty as *mut _ as *mut c_void,
            )
        });
        assert!(dirty, "the first update is a full redraw");
        assert_eq!(Result::Success, unsafe { clean(state) });

        // The iterator observes the cleaning, reads every remaining row, and
        // writes a row's dirty flag back through the sub-slice.
        let mut vy: i32 = 0;
        assert_eq!(Result::Success, unsafe {
            row_get(
                it,
                RenderStateRowData::Dirty,
                &mut dirty as *mut _ as *mut c_void,
            )
        });
        assert!(!dirty);
        assert_eq!(Result::Success, unsafe {
            row_get(
                it,
                RenderStateRowData::ViewportY,
                &mut vy as *mut _ as *mut c_void,
            )
        });
        assert_eq!(-2, vy);
        let set_dirty = true;
        assert_eq!(Result::Success, unsafe {
            row_set(
                it,
                RowOption::Dirty,
                &set_dirty as *const bool as *const c_void,
            )
        });
        let mut seen = 1;
        while unsafe { row_iterator_next(it) } {
            let mut id = RowId::default();
            assert_eq!(Result::Success, unsafe {
                row_get(it, RenderStateRowData::Id, &mut id as *mut _ as *mut c_void)
            });
            assert!(id != RowId::default());
            seen += 1;
        }
        assert_eq!(8, seen);
        // The write through the iterator landed on the first captured row.
        let range = unsafe { (*state).state.row_data_range() };
        assert!(unsafe { &(*state).state.row_data }[range.start].dirty);

        // A second clean and a second pass over the same iterator.
        assert_eq!(Result::Success, unsafe { clean(state) });
        unsafe { (*state).state.dirty = RenderDirty::Full };
        let mut y: size::CellCountInt = 0;
        let mut again = 0;
        assert_eq!(Result::Success, unsafe {
            get(
                state,
                RenderStateData::RowIterator,
                &mut it as *mut _ as *mut c_void,
            )
        });
        while unsafe { row_iterator_next_dirty(it, &mut y) } {
            again += 1;
        }
        assert_eq!(8, again);

        unsafe { row_iterator_free(it) };
        unsafe { free(state) };
        unsafe { terminal_c::free(terminal) };
    });
}

// R13 (UD369), the C half: after an update fails with a raised overscan
// request, the row iterator stays inside `row_data`. The reference slices the
// iterator with `rowDataRange()` and indexes out of bounds.
#[test]
fn ud369_row_iterator_after_failed_overscan_update_stays_in_bounds() {
    use rustty_vt::alloc::testing::{Config, FailingAllocator};

    let failing = FailingAllocator::over_heap(Config::default());
    let calloc = failing.allocator();
    let mut terminal: terminal_c::Terminal = std::ptr::null_mut();
    assert_eq!(Result::Success, unsafe {
        terminal_c::new(std::ptr::null(), &mut terminal, 10, 10)
    });
    for i in 0..50 {
        vtw(terminal, format!("{i}\r\n").as_bytes());
    }
    test_scroll_viewport(terminal, rustty_vt::terminal::ScrollViewport::Delta(-5));

    let mut state: RenderState = std::ptr::null_mut();
    assert_eq!(Result::Success, unsafe { new(Some(&calloc), &mut state) });
    assert_eq!(Result::Success, unsafe { update(state, terminal) });

    // The next allocation is the `row_data` growth; refuse it.
    let req = Overscan {
        above: 30,
        below: 30,
    };
    assert_eq!(Result::Success, set_overscan(state, &req));
    failing.fail_index.set(failing.alloc_index.get());
    assert_eq!(Result::OutOfMemory, unsafe { update(state, terminal) });
    failing.fail_index.set(usize::MAX);

    let mut out = Overscan::default();
    assert_eq!(
        Result::Success,
        get_overscan(state, RenderStateData::Overscan, &mut out)
    );
    assert_eq!(Overscan::default(), out, "nothing is reported captured");

    // Reading the rows after the failure stays in bounds (here: no rows).
    let mut it: RowIterator = std::ptr::null_mut();
    assert_eq!(Result::Success, unsafe { row_iterator_new(None, &mut it) });
    assert_eq!(Result::Success, unsafe {
        get(
            state,
            RenderStateData::RowIterator,
            &mut it as *mut _ as *mut c_void,
        )
    });
    let len = unsafe { (*state).state.row_data.len() };
    assert!(unsafe { (*it).rows.len() } <= len);
    // The clamped range is empty: `viewport_start()` (30) is past the 10
    // entries, so nothing of the stale layout is visited.
    assert_eq!(0, unsafe { (*it).rows.len() });
    while unsafe { row_iterator_next(it) } {
        let mut raw: row::CRow = 0;
        assert_eq!(Result::Success, unsafe {
            row_get(
                it,
                RenderStateRowData::Raw,
                &mut raw as *mut _ as *mut c_void,
            )
        });
    }
    unsafe { row_iterator_free(it) };

    // The retry captures the request.
    assert_eq!(Result::Success, unsafe { update(state, terminal) });
    assert_eq!(
        Result::Success,
        get_overscan(state, RenderStateData::Overscan, &mut out)
    );
    assert_eq!(
        Overscan {
            above: 30,
            below: 5
        },
        out
    );

    unsafe { free(state) };
    unsafe { terminal_c::free(terminal) };
    assert_eq!(failing.allocated_bytes.get(), failing.freed_bytes.get());
}
