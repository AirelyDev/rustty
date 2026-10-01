//! PORTS: ghostty src/terminal/c/terminal.zig — tests: selection
//! options/data, `grid_ref`, and `point_from_grid_ref`.

use super::*;

// test "set and get selection"
#[test]
fn set_and_get_selection() {
    with_test_allocator(|alloc| {
        // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);

        vtw(t, b"Hello");

        // var start_ref: grid_ref_c.CGridRef = .{};
        let mut start_ref = CGridRef::default();
        assert_eq!(Result::Success, unsafe {
            grid_ref(t, pt_active(0, 0), &mut start_ref)
        });

        let mut end_ref = CGridRef::default();
        assert_eq!(Result::Success, unsafe {
            grid_ref(t, pt_active(4, 0), &mut end_ref)
        });

        let mut out = crate::selection::CSelection::default();
        assert_eq!(Result::NoValue, unsafe {
            get(
                t,
                TerminalData::Selection,
                &mut out as *mut _ as *mut c_void,
            )
        });

        let sel = crate::selection::CSelection {
            start: start_ref,
            end: end_ref,
            rectangle: true,
            ..crate::selection::CSelection::default()
        };
        assert_eq!(Result::Success, unsafe {
            set(t, Option_::Selection, &sel as *const _ as *const c_void)
        });
        assert!(unsafe {
            (*(*(*t).terminal).screens.active)
                .selection
                .unwrap()
                .tracked()
        });

        assert_eq!(Result::Success, unsafe {
            get(
                t,
                TerminalData::Selection,
                &mut out as *mut _ as *mut c_void,
            )
        });
        assert!(out.start.to_pin().unwrap().eql(start_ref.to_pin().unwrap()));
        assert!(out.end.to_pin().unwrap().eql(end_ref.to_pin().unwrap()));
        assert!(out.rectangle);

        // set(t, .selection, null)
        assert_eq!(Result::Success, unsafe {
            set(t, Option_::Selection, std::ptr::null())
        });
        assert!(unsafe { (*(*(*t).terminal).screens.active).selection.is_none() });
        assert_eq!(Result::NoValue, unsafe {
            get(
                t,
                TerminalData::Selection,
                &mut out as *mut _ as *mut c_void,
            )
        });
        unsafe { free(t) };
    });
}

// test "selection derivation helpers"
#[test]
fn selection_derivation_helpers() {
    with_test_allocator(|alloc| {
        // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);

        vtw(t, b"  Hello  \r\nWorld");

        let mut out = crate::selection::CSelection::default();

        let mut word_ref = CGridRef::default();
        assert_eq!(Result::Success, unsafe {
            grid_ref(t, pt_active(3, 0), &mut word_ref)
        });

        let mut empty_ref = CGridRef::default();
        assert_eq!(Result::Success, unsafe {
            grid_ref(t, pt_active(20, 0), &mut empty_ref)
        });

        let mut line_ref = CGridRef::default();
        assert_eq!(Result::Success, unsafe {
            grid_ref(t, pt_active(0, 0), &mut line_ref)
        });

        let mut word_opts = crate::selection::SelectWordOptions {
            size: size_of::<crate::selection::SelectWordOptions>(),
            ref_: word_ref,
            boundary_codepoints: std::ptr::null(),
            boundary_codepoints_len: 0,
        };
        assert_eq!(Result::Success, unsafe {
            crate::selection::word(t, &word_opts, &mut out)
        });
        assert_eq!(2u16, out.start.to_pin().unwrap().x);
        assert_eq!(6u16, out.end.to_pin().unwrap().x);

        word_opts.ref_ = empty_ref;
        assert_eq!(Result::NoValue, unsafe {
            crate::selection::word(t, &word_opts, &mut out)
        });

        let mut between_start_ref = CGridRef::default();
        assert_eq!(Result::Success, unsafe {
            grid_ref(t, pt_active(20, 1), &mut between_start_ref)
        });

        let mut between_end_ref = CGridRef::default();
        assert_eq!(Result::Success, unsafe {
            grid_ref(t, pt_active(0, 1), &mut between_end_ref)
        });

        let word_between_opts = crate::selection::SelectWordBetweenOptions {
            size: size_of::<crate::selection::SelectWordBetweenOptions>(),
            start: between_start_ref,
            end: between_end_ref,
            boundary_codepoints: std::ptr::null(),
            boundary_codepoints_len: 0,
        };
        assert_eq!(Result::Success, unsafe {
            crate::selection::word_between(t, &word_between_opts, &mut out)
        });
        assert_eq!(0u16, out.start.to_pin().unwrap().x);
        assert_eq!(1u16, out.start.to_pin().unwrap().y);
        assert_eq!(4u16, out.end.to_pin().unwrap().x);
        assert_eq!(1u16, out.end.to_pin().unwrap().y);

        let mut line_opts = crate::selection::SelectLineOptions {
            size: size_of::<crate::selection::SelectLineOptions>(),
            ref_: line_ref,
            whitespace: std::ptr::null(),
            whitespace_len: 0,
            semantic_prompt_boundary: false,
        };
        assert_eq!(Result::Success, unsafe {
            crate::selection::line(t, &line_opts, &mut out)
        });
        assert_eq!(2u16, out.start.to_pin().unwrap().x);
        assert_eq!(6u16, out.end.to_pin().unwrap().x);

        assert_eq!(Result::Success, unsafe {
            crate::selection::all(t, &mut out)
        });
        assert_eq!(2u16, out.start.to_pin().unwrap().x);
        assert_eq!(0u16, out.start.to_pin().unwrap().y);
        assert_eq!(4u16, out.end.to_pin().unwrap().x);
        assert_eq!(1u16, out.end.to_pin().unwrap().y);

        assert_eq!(Result::NoValue, unsafe {
            crate::selection::output(t, line_ref, &mut out)
        });

        line_opts.size = size_of::<usize>() - 1;
        assert_eq!(Result::InvalidValue, unsafe {
            crate::selection::line(t, &line_opts, &mut out)
        });
        assert_eq!(Result::InvalidValue, unsafe {
            crate::selection::word(t, std::ptr::null(), &mut out)
        });
        assert_eq!(Result::InvalidValue, unsafe {
            crate::selection::word(t, &word_opts, std::ptr::null_mut())
        });
        assert_eq!(Result::InvalidValue, unsafe {
            crate::selection::word_between(t, std::ptr::null(), &mut out)
        });
        assert_eq!(Result::InvalidValue, unsafe {
            crate::selection::word_between(t, &word_between_opts, std::ptr::null_mut())
        });
        unsafe { free(t) };
    });
}

// test "selection_adjust mutates snapshot end"
#[test]
fn selection_adjust_mutates_snapshot_end() {
    with_test_allocator(|alloc| {
        // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);

        vtw(t, b"Hello");

        let mut start_ref = CGridRef::default();
        assert_eq!(Result::Success, unsafe {
            grid_ref(t, pt_active(0, 0), &mut start_ref)
        });

        let mut end_ref = CGridRef::default();
        assert_eq!(Result::Success, unsafe {
            grid_ref(t, pt_active(1, 0), &mut end_ref)
        });

        let mut sel = crate::selection::CSelection {
            start: start_ref,
            end: end_ref,
            ..crate::selection::CSelection::default()
        };
        assert_eq!(Result::Success, unsafe {
            crate::selection::adjust(t, &mut sel, crate::selection::Adjustment::Right)
        });
        assert_eq!(0u16, sel.start.to_pin().unwrap().x);
        assert_eq!(2u16, sel.end.to_pin().unwrap().x);

        assert_eq!(Result::Success, unsafe {
            crate::selection::adjust(t, &mut sel, crate::selection::Adjustment::Left)
        });
        assert_eq!(0u16, sel.start.to_pin().unwrap().x);
        assert_eq!(1u16, sel.end.to_pin().unwrap().x);

        sel = crate::selection::CSelection {
            start: end_ref,
            end: start_ref,
            ..crate::selection::CSelection::default()
        };
        assert_eq!(Result::Success, unsafe {
            crate::selection::adjust(t, &mut sel, crate::selection::Adjustment::Right)
        });
        assert_eq!(1u16, sel.start.to_pin().unwrap().x);
        assert_eq!(1u16, sel.end.to_pin().unwrap().x);
        unsafe { free(t) };
    });
}

// test "selection_order and selection_ordered"
#[test]
fn selection_order_and_selection_ordered() {
    with_test_allocator(|alloc| {
        // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);

        vtw(t, b"Hello\r\nWorld");

        let mut start_ref = CGridRef::default();
        assert_eq!(Result::Success, unsafe {
            grid_ref(t, pt_active(3, 0), &mut start_ref)
        });

        let mut end_ref = CGridRef::default();
        assert_eq!(Result::Success, unsafe {
            grid_ref(t, pt_active(1, 1), &mut end_ref)
        });

        let sel = crate::selection::CSelection {
            start: start_ref,
            end: end_ref,
            rectangle: true,
            ..crate::selection::CSelection::default()
        };

        let mut order = crate::selection::Order::Forward;
        assert_eq!(Result::Success, unsafe {
            crate::selection::order(t, &sel, &mut order)
        });
        assert_eq!(crate::selection::Order::MirroredForward, order);

        let mut out = crate::selection::CSelection::default();
        assert_eq!(Result::Success, unsafe {
            crate::selection::ordered(t, &sel, crate::selection::Order::Forward, &mut out)
        });
        assert_eq!(1u16, out.start.to_pin().unwrap().x);
        assert_eq!(0u16, out.start.to_pin().unwrap().y);
        assert_eq!(3u16, out.end.to_pin().unwrap().x);
        assert_eq!(1u16, out.end.to_pin().unwrap().y);
        assert!(out.rectangle);

        assert_eq!(Result::Success, unsafe {
            crate::selection::ordered(t, &sel, crate::selection::Order::Reverse, &mut out)
        });
        assert_eq!(3u16, out.start.to_pin().unwrap().x);
        assert_eq!(1u16, out.start.to_pin().unwrap().y);
        assert_eq!(1u16, out.end.to_pin().unwrap().x);
        assert_eq!(0u16, out.end.to_pin().unwrap().y);
        assert!(out.rectangle);
        unsafe { free(t) };
    });
}

// test "selection_contains"
#[test]
fn selection_contains_test() {
    with_test_allocator(|alloc| {
        // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);

        vtw(t, b"Hello\r\nWorld");

        let mut start_ref = CGridRef::default();
        assert_eq!(Result::Success, unsafe {
            grid_ref(t, pt_active(3, 0), &mut start_ref)
        });

        let mut end_ref = CGridRef::default();
        assert_eq!(Result::Success, unsafe {
            grid_ref(t, pt_active(1, 1), &mut end_ref)
        });

        let linear = crate::selection::CSelection {
            start: start_ref,
            end: end_ref,
            ..crate::selection::CSelection::default()
        };

        let mut contains = false;
        assert_eq!(Result::Success, unsafe {
            crate::selection::contains(t, &linear, pt_active(4, 0), &mut contains)
        });
        assert!(contains);

        assert_eq!(Result::Success, unsafe {
            crate::selection::contains(t, &linear, pt_active(2, 0), &mut contains)
        });
        assert!(!contains);

        let rectangle = crate::selection::CSelection {
            start: start_ref,
            end: end_ref,
            rectangle: true,
            ..crate::selection::CSelection::default()
        };

        assert_eq!(Result::Success, unsafe {
            crate::selection::contains(t, &rectangle, pt_active(2, 0), &mut contains)
        });
        assert!(contains);
        unsafe { free(t) };
    });
}

// test "selection_equal"
#[test]
fn selection_equal_test() {
    with_test_allocator(|alloc| {
        // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);
        // try testing.expectEqual(Result.success, new(&test_allocator, &other_t, .{ ... }));
        let (new_result, other_t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);

        vtw(t, b"Hello");
        vtw(other_t, b"Hello");

        let mut start_ref = CGridRef::default();
        assert_eq!(Result::Success, unsafe {
            grid_ref(t, pt_active(0, 0), &mut start_ref)
        });

        let mut end_ref = CGridRef::default();
        assert_eq!(Result::Success, unsafe {
            grid_ref(t, pt_active(1, 0), &mut end_ref)
        });

        let mut other_end_ref = CGridRef::default();
        assert_eq!(Result::Success, unsafe {
            grid_ref(t, pt_active(2, 0), &mut other_end_ref)
        });

        let mut cross_terminal_ref = CGridRef::default();
        assert_eq!(Result::Success, unsafe {
            grid_ref(other_t, pt_active(1, 0), &mut cross_terminal_ref)
        });

        let sel = crate::selection::CSelection {
            start: start_ref,
            end: end_ref,
            ..crate::selection::CSelection::default()
        };
        let equal_sel = crate::selection::CSelection {
            start: start_ref,
            end: end_ref,
            ..crate::selection::CSelection::default()
        };
        let different_endpoint = crate::selection::CSelection {
            start: start_ref,
            end: other_end_ref,
            ..crate::selection::CSelection::default()
        };
        let different_rectangle = crate::selection::CSelection {
            start: start_ref,
            end: end_ref,
            rectangle: true,
            ..crate::selection::CSelection::default()
        };
        let cross_terminal = crate::selection::CSelection {
            start: start_ref,
            end: cross_terminal_ref,
            ..crate::selection::CSelection::default()
        };

        let mut equal = false;
        assert_eq!(Result::Success, unsafe {
            crate::selection::equal(t, &sel, &equal_sel, &mut equal)
        });
        assert!(equal);

        assert_eq!(Result::Success, unsafe {
            crate::selection::equal(t, &sel, &different_endpoint, &mut equal)
        });
        assert!(!equal);

        assert_eq!(Result::Success, unsafe {
            crate::selection::equal(t, &sel, &different_rectangle, &mut equal)
        });
        assert!(!equal);

        assert_eq!(Result::Success, unsafe {
            crate::selection::equal(t, &sel, &cross_terminal, &mut equal)
        });
        assert!(!equal);
        assert_eq!(Result::InvalidValue, unsafe {
            crate::selection::equal(t, &sel, &equal_sel, std::ptr::null_mut())
        });
        unsafe { free(t) };
        unsafe { free(other_t) };
    });
}

// test "selection_order invalid values"
#[test]
fn selection_order_invalid_values() {
    with_test_allocator(|alloc| {
        // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);

        let mut order = crate::selection::Order::Forward;
        assert_eq!(Result::InvalidValue, unsafe {
            crate::selection::order(std::ptr::null_mut(), std::ptr::null(), &mut order)
        });
        assert_eq!(Result::InvalidValue, unsafe {
            crate::selection::order(t, std::ptr::null(), &mut order)
        });
        unsafe { free(t) };
    });
}

// test "grid_ref"
#[test]
fn grid_ref_test() {
    with_test_allocator(|alloc| {
        // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);

        vtw(t, b"Hello");

        let mut out_ref = CGridRef::default();
        assert_eq!(Result::Success, unsafe {
            grid_ref(t, pt_active(0, 0), &mut out_ref)
        });

        // Extract cell from grid ref and verify it contains 'H'
        let mut out_cell: crate::cell::CCell = 0;
        assert_eq!(Result::Success, unsafe {
            crate::grid_ref::grid_ref_cell(&out_ref, &mut out_cell)
        });

        let mut cp: u32 = 0;
        assert_eq!(Result::Success, unsafe {
            crate::cell::get(
                out_cell,
                crate::cell::CellData::Codepoint,
                &mut cp as *mut _ as *mut c_void,
            )
        });
        assert_eq!(b'H' as u32, cp);
        unsafe { free(t) };
    });
}

// test "grid_ref null terminal"
#[test]
fn grid_ref_null_terminal() {
    let mut out_ref = CGridRef::default();
    assert_eq!(Result::InvalidValue, unsafe {
        grid_ref(std::ptr::null_mut(), pt_active(0, 0), &mut out_ref)
    });
}

// test "point_from_grid_ref roundtrip active"
#[test]
fn point_from_grid_ref_roundtrip_active() {
    with_test_allocator(|alloc| {
        // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);

        vtw(t, b"Hello");

        // Get a grid ref at (2, 0) in active coords
        let mut ref_ = CGridRef::default();
        assert_eq!(Result::Success, unsafe {
            grid_ref(t, pt_active(2, 0), &mut ref_)
        });

        // Convert back to active coords
        let mut coord = Coordinate { x: 0, y: 0 };
        assert_eq!(Result::Success, unsafe {
            point_from_grid_ref(t, &ref_, Tag::Active, &mut coord)
        });
        assert_eq!(2 as size::CellCountInt, coord.x);
        assert_eq!(0u32, coord.y);
        unsafe { free(t) };
    });
}

// test "point_from_grid_ref roundtrip viewport"
#[test]
fn point_from_grid_ref_roundtrip_viewport() {
    with_test_allocator(|alloc| {
        // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);

        vtw(t, b"Hello");

        let mut ref_ = CGridRef::default();
        assert_eq!(Result::Success, unsafe {
            grid_ref(t, pt_viewport(0, 0), &mut ref_)
        });

        let mut coord = Coordinate { x: 0, y: 0 };
        assert_eq!(Result::Success, unsafe {
            point_from_grid_ref(t, &ref_, Tag::Viewport, &mut coord)
        });
        assert_eq!(0 as size::CellCountInt, coord.x);
        assert_eq!(0u32, coord.y);
        unsafe { free(t) };
    });
}

// test "point_from_grid_ref history ref to active returns no_value"
#[test]
fn point_from_grid_ref_history_ref_to_active_returns_no_value() {
    with_test_allocator(|alloc| {
        // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
        let (new_result, t) = new_t(alloc, 80, 4);
        assert_eq!(Result::Success, new_result);

        // Write enough lines to push content into scrollback
        for _ in 0..10 {
            vtw(t, b"line\n");
        }

        // Get a ref to the first line (now in scrollback)
        let mut ref_ = CGridRef::default();
        assert_eq!(Result::Success, unsafe {
            grid_ref(t, pt_screen(0, 0), &mut ref_)
        });

        // Should succeed for screen coords
        let mut coord = Coordinate { x: 0, y: 0 };
        assert_eq!(Result::Success, unsafe {
            point_from_grid_ref(t, &ref_, Tag::Screen, &mut coord)
        });
        assert_eq!(0u32, coord.y);

        // Should fail for active coords (it's in scrollback)
        assert_eq!(Result::NoValue, unsafe {
            point_from_grid_ref(t, &ref_, Tag::Active, &mut coord)
        });
        unsafe { free(t) };
    });
}

// test "point_from_grid_ref null terminal"
#[test]
fn point_from_grid_ref_null_terminal() {
    let ref_ = CGridRef::default();
    assert_eq!(Result::InvalidValue, unsafe {
        point_from_grid_ref(
            std::ptr::null_mut(),
            &ref_,
            Tag::Active,
            std::ptr::null_mut(),
        )
    });
}

// test "point_from_grid_ref null node"
#[test]
fn point_from_grid_ref_null_node() {
    with_test_allocator(|alloc| {
        // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);

        let ref_ = CGridRef::default();
        assert_eq!(Result::InvalidValue, unsafe {
            point_from_grid_ref(t, &ref_, Tag::Active, std::ptr::null_mut())
        });
        unsafe { free(t) };
    });
}
