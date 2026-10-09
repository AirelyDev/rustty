//! PORT NOTE: ghostty `src/terminal/c/selection_gesture.zig` `test {}` blocks,
//! ported assertion-for-assertion. `&lib.alloc.test_allocator` becomes the C
//! allocator from `with_test_allocator` (leaks caught by Miri per R6).

use super::*;
use crate::point::{PointC, PointCValue, TagC};
use crate::result::Result;
use crate::test_util::with_test_allocator;
use rustty_vt::point::Coordinate as ZigCoordinate;
use std::os::raw::c_int;

// PORT NOTE: `point.Point.C` literal `.{ .tag = .active, .value = .{ .active =
// .{ .x, .y } } }` -> a `PointC` with the active union member.
fn active_point(x: u16, y: u16) -> PointC {
    PointC {
        tag: TagC::Active as c_int,
        value: PointCValue {
            active: ZigCoordinate { x, y: y as u32 },
        },
    }
}

// PORT NOTE: test accessors for the press payload fields. The reference reads
// `event.?.event.press.<field>` directly off the active union member; in Rust
// the payload is the `EventPayload::Press` variant and the borrowed
// `word_boundary_codepoints`/`behaviors` are rebuilt from event-owned storage.
fn press_xpos(event: SelectionGestureEvent) -> f64 {
    match &unsafe { &*event }.event {
        EventPayload::Press(p) => p.xpos,
        _ => unreachable!(),
    }
}

fn press_ypos(event: SelectionGestureEvent) -> f64 {
    match &unsafe { &*event }.event {
        EventPayload::Press(p) => p.ypos,
        _ => unreachable!(),
    }
}

fn press_max_distance(event: SelectionGestureEvent) -> f64 {
    match &unsafe { &*event }.event {
        EventPayload::Press(p) => p.max_distance,
        _ => unreachable!(),
    }
}

fn press_word_boundaries(event: SelectionGestureEvent) -> Vec<u32> {
    let ev = unsafe { &*event };
    match &ev.event {
        EventPayload::Press(_) => ev.word_boundaries().to_vec(),
        _ => unreachable!(),
    }
}

fn press_behaviors(event: SelectionGestureEvent) -> [sg::Behavior; 3] {
    unsafe { &*event }.behaviors
}

// test "selection gesture lifecycle and get"
#[test]
fn selection_gesture_lifecycle_and_get() {
    with_test_allocator(|alloc| {
        let mut terminal: terminal_c::Terminal = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe {
            terminal_c::new(alloc, &mut terminal, 5, 2)
        });
        // defer terminal_c.free(terminal);

        let mut gesture: Gesture = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe { new(Some(alloc), &mut gesture) });
        // defer free(gesture, terminal);

        // var click_count: u8 = 255;
        let mut click_count: u8 = 255;
        assert_eq!(Result::Success, unsafe {
            get(
                gesture,
                terminal,
                SelectionGestureData::ClickCount,
                &mut click_count as *mut _ as *mut c_void,
            )
        });
        // try testing.expectEqual(@as(u8, 0), click_count);
        assert_eq!(0u8, click_count);

        // var dragged = true;
        let mut dragged = true;
        assert_eq!(Result::Success, unsafe {
            get(
                gesture,
                terminal,
                SelectionGestureData::Dragged,
                &mut dragged as *mut _ as *mut c_void,
            )
        });
        assert!(!dragged);

        // var autoscroll: Autoscroll = .up;
        let mut autoscroll: Autoscroll = Autoscroll::Up;
        assert_eq!(Result::Success, unsafe {
            get(
                gesture,
                terminal,
                SelectionGestureData::Autoscroll,
                &mut autoscroll as *mut _ as *mut c_void,
            )
        });
        // try testing.expectEqual(Autoscroll.none, autoscroll);
        assert_eq!(Autoscroll::None, autoscroll);

        // var behavior: Behavior = .word;
        let mut behavior: Behavior = Behavior::Word;
        assert_eq!(Result::Success, unsafe {
            get(
                gesture,
                terminal,
                SelectionGestureData::Behavior,
                &mut behavior as *mut _ as *mut c_void,
            )
        });
        // try testing.expectEqual(Behavior.cell, behavior);
        assert_eq!(Behavior::Cell, behavior);

        // var anchor: grid_ref.CGridRef = undefined;
        let mut anchor: grid_ref::CGridRef = grid_ref::CGridRef::default();
        // try testing.expectEqual(Result.no_value, get(gesture, terminal, .anchor, &anchor));
        assert_eq!(Result::NoValue, unsafe {
            get(
                gesture,
                terminal,
                SelectionGestureData::Anchor,
                &mut anchor as *mut _ as *mut c_void,
            )
        });

        unsafe { free(gesture, terminal) };
        unsafe { terminal_c::free(terminal) };
    });
}

// test "selection gesture get_multi"
#[test]
fn selection_gesture_get_multi() {
    with_test_allocator(|alloc| {
        let mut terminal: terminal_c::Terminal = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe {
            terminal_c::new(alloc, &mut terminal, 5, 2)
        });

        let mut gesture: Gesture = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe { new(Some(alloc), &mut gesture) });

        // const keys = [_]Data{ .click_count, .dragged, .autoscroll, .behavior };
        let keys = [
            SelectionGestureData::ClickCount,
            SelectionGestureData::Dragged,
            SelectionGestureData::Autoscroll,
            SelectionGestureData::Behavior,
        ];
        let mut click_count: u8 = 255;
        let mut dragged = true;
        let mut autoscroll: Autoscroll = Autoscroll::Up;
        let mut behavior: Behavior = Behavior::Word;
        let mut values = [
            &mut click_count as *mut _ as *mut c_void,
            &mut dragged as *mut _ as *mut c_void,
            &mut autoscroll as *mut _ as *mut c_void,
            &mut behavior as *mut _ as *mut c_void,
        ];
        let mut written: usize = 0;

        assert_eq!(Result::Success, unsafe {
            get_multi(
                gesture,
                terminal,
                keys.len(),
                // PORT NOTE (W66-GETMULTI-RAW-01): the key array stays typed
                // `Data` (the reference's `[_]Data{...}`); `get_multi` reads its
                // elements as raw `c_int`, so the pointer is cast at the call.
                keys.as_ptr().cast(),
                values.as_mut_ptr(),
                &mut written,
            )
        });
        assert_eq!(keys.len(), written);
        assert_eq!(0u8, click_count);
        assert!(!dragged);
        assert_eq!(Autoscroll::None, autoscroll);
        assert_eq!(Behavior::Cell, behavior);

        unsafe { free(gesture, terminal) };
        unsafe { terminal_c::free(terminal) };
    });
}

// test "selection gesture get_multi returns first failing index"
#[test]
fn selection_gesture_get_multi_returns_first_failing_index() {
    with_test_allocator(|alloc| {
        let mut terminal: terminal_c::Terminal = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe {
            terminal_c::new(alloc, &mut terminal, 5, 2)
        });

        let mut gesture: Gesture = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe { new(Some(alloc), &mut gesture) });

        // const keys = [_]Data{ .click_count, .anchor, .dragged };
        let keys = [
            SelectionGestureData::ClickCount,
            SelectionGestureData::Anchor,
            SelectionGestureData::Dragged,
        ];
        let mut click_count: u8 = 255;
        let mut anchor: grid_ref::CGridRef = grid_ref::CGridRef::default();
        let mut dragged = true;
        let mut values = [
            &mut click_count as *mut _ as *mut c_void,
            &mut anchor as *mut _ as *mut c_void,
            &mut dragged as *mut _ as *mut c_void,
        ];
        let mut written: usize = 0;

        // try testing.expectEqual(Result.no_value, get_multi(...));
        assert_eq!(Result::NoValue, unsafe {
            get_multi(
                gesture,
                terminal,
                keys.len(),
                // PORT NOTE (W66-GETMULTI-RAW-01): the key array stays typed
                // `Data` (the reference's `[_]Data{...}`); `get_multi` reads its
                // elements as raw `c_int`, so the pointer is cast at the call.
                keys.as_ptr().cast(),
                values.as_mut_ptr(),
                &mut written,
            )
        });
        // try testing.expectEqual(@as(usize, 1), written);
        assert_eq!(1usize, written);
        assert_eq!(0u8, click_count);
        assert!(dragged);

        unsafe { free(gesture, terminal) };
        unsafe { terminal_c::free(terminal) };
    });
}

// test "selection gesture event set clear and free"
#[test]
fn selection_gesture_event_set_clear_and_free() {
    with_test_allocator(|alloc| {
        let mut event: SelectionGestureEvent = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe {
            event_new(Some(alloc), &mut event, EventType::Press)
        });
        // defer event_free(event);

        // const in_pos: types.SurfacePosition = .{ .x = 12.5, .y = -3.25 };
        let in_pos = SurfacePosition { x: 12.5, y: -3.25 };
        assert_eq!(Result::Success, unsafe {
            event_set(
                event,
                EventOption::Position,
                &in_pos as *const _ as *const c_void,
            )
        });
        // try testing.expectEqual(@as(f64, 12.5), event.?.event.press.xpos);
        assert_eq!(12.5f64, press_xpos(event));
        assert_eq!(-3.25f64, press_ypos(event));

        // try testing.expectEqual(Result.success, event_set(event, .position, null));
        assert_eq!(Result::Success, unsafe {
            event_set(event, EventOption::Position, std::ptr::null())
        });
        assert_eq!(0f64, press_xpos(event));
        assert_eq!(0f64, press_ypos(event));

        // const repeat_distance: f64 = 4.0;
        let repeat_distance: f64 = 4.0;
        assert_eq!(Result::Success, unsafe {
            event_set(
                event,
                EventOption::RepeatDistance,
                &repeat_distance as *const _ as *const c_void,
            )
        });
        // try testing.expectEqual(repeat_distance, event.?.event.press.max_distance);
        assert_eq!(repeat_distance, press_max_distance(event));

        unsafe { event_free(event) };
    });
}

// test "selection gesture event copies clears and frees codepoints"
// PORT NOTE: `values[0] = 'x'` after the `event_set` is a deliberate mutation
// that proves the event copied (did not alias) the input codepoints; the
// subsequent reads observe the original values. The assignment is therefore not
// read by Rust, hence the allow.
#[allow(unused_assignments)]
#[test]
fn selection_gesture_event_copies_clears_and_frees_codepoints() {
    with_test_allocator(|alloc| {
        let mut event: SelectionGestureEvent = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe {
            event_new(Some(alloc), &mut event, EventType::Press)
        });
        // defer event_free(event);

        // var values = [_]u32{ ' ', '\t' };
        let mut values = [' ' as u32, '\t' as u32];
        // const in: types.Codepoints = .{ .ptr = &values, .len = values.len };
        let in_ = Codepoints {
            ptr: values.as_ptr(),
            len: values.len(),
        };
        assert_eq!(Result::Success, unsafe {
            event_set(
                event,
                EventOption::WordBoundaryCodepoints,
                &in_ as *const _ as *const c_void,
            )
        });

        // values[0] = 'x';
        values[0] = 'x' as u32;

        // try testing.expectEqual(@as(usize, 2), event.?.event.press.word_boundary_codepoints.len);
        let wbc = press_word_boundaries(event);
        assert_eq!(2usize, wbc.len());
        // try testing.expectEqual(@as(u21, ' '), event.?.event.press.word_boundary_codepoints[0]);
        assert_eq!(' ' as u32, wbc[0]);
        assert_eq!('\t' as u32, wbc[1]);

        // const invalid: types.Codepoints = .{ .ptr = null, .len = 1 };
        let invalid = Codepoints {
            ptr: std::ptr::null(),
            len: 1,
        };
        assert_eq!(Result::InvalidValue, unsafe {
            event_set(
                event,
                EventOption::WordBoundaryCodepoints,
                &invalid as *const _ as *const c_void,
            )
        });

        // try testing.expectEqual(Result.success, event_set(event, .word_boundary_codepoints, null));
        assert_eq!(Result::Success, unsafe {
            event_set(event, EventOption::WordBoundaryCodepoints, std::ptr::null())
        });
        // try testing.expectEqual(default_word_boundaries.len, event.?.event.press.word_boundary_codepoints.len);
        assert_eq!(
            selection_codepoints::DEFAULT_WORD_BOUNDARIES.len(),
            press_word_boundaries(event).len()
        );

        // const empty: types.Codepoints = .{ .ptr = null, .len = 0 };
        let empty = Codepoints {
            ptr: std::ptr::null(),
            len: 0,
        };
        assert_eq!(Result::Success, unsafe {
            event_set(
                event,
                EventOption::WordBoundaryCodepoints,
                &empty as *const _ as *const c_void,
            )
        });
        // try testing.expectEqual(@as(usize, 0), event.?.event.press.word_boundary_codepoints.len);
        assert_eq!(0usize, press_word_boundaries(event).len());

        unsafe { event_free(event) };
    });
}

// test "selection gesture event behaviors"
#[test]
fn selection_gesture_event_behaviors() {
    with_test_allocator(|alloc| {
        let mut event: SelectionGestureEvent = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe {
            event_new(Some(alloc), &mut event, EventType::Press)
        });
        // defer event_free(event);

        // const in: Behaviors = .{ .single_click = .cell, .double_click = .word, .triple_click = .line };
        let in_ = Behaviors {
            single_click: Behavior::Cell,
            double_click: Behavior::Word,
            triple_click: Behavior::Line,
        };
        assert_eq!(Result::Success, unsafe {
            event_set(
                event,
                EventOption::Behaviors,
                &in_ as *const _ as *const c_void,
            )
        });
        // try testing.expectEqual(Behavior.cell, event.?.event.press.behaviors[0]);
        let behaviors = press_behaviors(event);
        assert_eq!(sg::Behavior::Cell, behaviors[0]);
        assert_eq!(sg::Behavior::Word, behaviors[1]);
        assert_eq!(sg::Behavior::Line, behaviors[2]);

        unsafe { event_free(event) };
    });
}

// test "selection gesture event applies press"
#[test]
fn selection_gesture_event_applies_press() {
    with_test_allocator(|alloc| {
        let mut terminal: terminal_c::Terminal = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe {
            terminal_c::new(alloc, &mut terminal, 5, 2)
        });

        let mut gesture: Gesture = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe { new(Some(alloc), &mut gesture) });

        let mut press_event: SelectionGestureEvent = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe {
            event_new(Some(alloc), &mut press_event, EventType::Press)
        });

        let input = b"abc";
        unsafe { terminal_c::vt_write(terminal, input.as_ptr(), 3) };

        // var ref: grid_ref.CGridRef = undefined;
        let mut ref_: grid_ref::CGridRef = grid_ref::CGridRef::default();
        assert_eq!(Result::Success, unsafe {
            terminal_c::grid_ref(terminal, active_point(1, 0), &mut ref_)
        });
        assert_eq!(Result::Success, unsafe {
            event_set(
                press_event,
                EventOption::Ref,
                &ref_ as *const _ as *const c_void,
            )
        });
        let behaviors = Behaviors {
            single_click: Behavior::Word,
            double_click: Behavior::Word,
            triple_click: Behavior::Line,
        };
        assert_eq!(Result::Success, unsafe {
            event_set(
                press_event,
                EventOption::Behaviors,
                &behaviors as *const _ as *const c_void,
            )
        });

        // var sel: selection_c.CSelection = undefined;
        let mut sel: selection_c::CSelection = unsafe { std::mem::zeroed() };
        assert_eq!(Result::Success, unsafe {
            handle_event(gesture, terminal, press_event, &mut sel)
        });
        // try testing.expectEqual(@as(u16, 0), sel.start.toPin().?.x);
        assert_eq!(0u16, sel.start.to_pin().unwrap().x);
        // try testing.expectEqual(@as(u16, 2), sel.end.toPin().?.x);
        assert_eq!(2u16, sel.end.to_pin().unwrap().x);

        // try testing.expectEqual(Result.success, handle_event(gesture, terminal, press_event, null));
        assert_eq!(Result::Success, unsafe {
            handle_event(gesture, terminal, press_event, std::ptr::null_mut())
        });

        unsafe { event_free(press_event) };
        unsafe { free(gesture, terminal) };
        unsafe { terminal_c::free(terminal) };
    });
}

// test "selection gesture event press requires ref"
#[test]
fn selection_gesture_event_press_requires_ref() {
    with_test_allocator(|alloc| {
        let mut terminal: terminal_c::Terminal = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe {
            terminal_c::new(alloc, &mut terminal, 5, 2)
        });

        let mut gesture: Gesture = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe { new(Some(alloc), &mut gesture) });

        let mut press_event: SelectionGestureEvent = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe {
            event_new(Some(alloc), &mut press_event, EventType::Press)
        });

        // var sel: selection_c.CSelection = undefined;
        let mut sel: selection_c::CSelection = unsafe { std::mem::zeroed() };
        assert_eq!(Result::InvalidValue, unsafe {
            handle_event(gesture, terminal, press_event, &mut sel)
        });

        unsafe { event_free(press_event) };
        unsafe { free(gesture, terminal) };
        unsafe { terminal_c::free(terminal) };
    });
}

// test "selection gesture event null output still reports no selection"
#[test]
fn selection_gesture_event_null_output_still_reports_no_selection() {
    with_test_allocator(|alloc| {
        let mut terminal: terminal_c::Terminal = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe {
            terminal_c::new(alloc, &mut terminal, 5, 2)
        });

        let mut gesture: Gesture = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe { new(Some(alloc), &mut gesture) });

        let mut press_event: SelectionGestureEvent = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe {
            event_new(Some(alloc), &mut press_event, EventType::Press)
        });

        let mut ref_: grid_ref::CGridRef = grid_ref::CGridRef::default();
        assert_eq!(Result::Success, unsafe {
            terminal_c::grid_ref(terminal, active_point(1, 0), &mut ref_)
        });
        assert_eq!(Result::Success, unsafe {
            event_set(
                press_event,
                EventOption::Ref,
                &ref_ as *const _ as *const c_void,
            )
        });

        // try testing.expectEqual(Result.no_value, handle_event(gesture, terminal, press_event, null));
        assert_eq!(Result::NoValue, unsafe {
            handle_event(gesture, terminal, press_event, std::ptr::null_mut())
        });

        unsafe { event_free(press_event) };
        unsafe { free(gesture, terminal) };
        unsafe { terminal_c::free(terminal) };
    });
}

// test "selection gesture event applies release"
#[test]
fn selection_gesture_event_applies_release() {
    with_test_allocator(|alloc| {
        let mut terminal: terminal_c::Terminal = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe {
            terminal_c::new(alloc, &mut terminal, 5, 2)
        });

        let mut gesture: Gesture = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe { new(Some(alloc), &mut gesture) });

        let mut press_event: SelectionGestureEvent = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe {
            event_new(Some(alloc), &mut press_event, EventType::Press)
        });

        let mut release_event: SelectionGestureEvent = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe {
            event_new(Some(alloc), &mut release_event, EventType::Release)
        });

        let mut ref_: grid_ref::CGridRef = grid_ref::CGridRef::default();
        assert_eq!(Result::Success, unsafe {
            terminal_c::grid_ref(terminal, active_point(1, 0), &mut ref_)
        });
        assert_eq!(Result::Success, unsafe {
            event_set(
                press_event,
                EventOption::Ref,
                &ref_ as *const _ as *const c_void,
            )
        });
        assert_eq!(Result::Success, unsafe {
            event_set(
                release_event,
                EventOption::Ref,
                &ref_ as *const _ as *const c_void,
            )
        });

        assert_eq!(Result::NoValue, unsafe {
            handle_event(gesture, terminal, press_event, std::ptr::null_mut())
        });
        assert_eq!(Result::NoValue, unsafe {
            handle_event(gesture, terminal, release_event, std::ptr::null_mut())
        });

        let mut dragged = true;
        assert_eq!(Result::Success, unsafe {
            get(
                gesture,
                terminal,
                SelectionGestureData::Dragged,
                &mut dragged as *mut _ as *mut c_void,
            )
        });
        assert!(!dragged);

        // const pos: types.SurfacePosition = .{ .x = 0, .y = 0 };
        let pos = SurfacePosition { x: 0.0, y: 0.0 };
        // try testing.expectEqual(Result.invalid_value, event_set(release_event, .position, &pos));
        assert_eq!(Result::InvalidValue, unsafe {
            event_set(
                release_event,
                EventOption::Position,
                &pos as *const _ as *const c_void,
            )
        });

        unsafe { event_free(release_event) };
        unsafe { event_free(press_event) };
        unsafe { free(gesture, terminal) };
        unsafe { terminal_c::free(terminal) };
    });
}

// test "selection gesture release without ref marks dragged"
#[test]
fn selection_gesture_release_without_ref_marks_dragged() {
    with_test_allocator(|alloc| {
        let mut terminal: terminal_c::Terminal = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe {
            terminal_c::new(alloc, &mut terminal, 5, 2)
        });

        let mut gesture: Gesture = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe { new(Some(alloc), &mut gesture) });

        let mut press_event: SelectionGestureEvent = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe {
            event_new(Some(alloc), &mut press_event, EventType::Press)
        });

        let mut release_event: SelectionGestureEvent = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe {
            event_new(Some(alloc), &mut release_event, EventType::Release)
        });

        let mut ref_: grid_ref::CGridRef = grid_ref::CGridRef::default();
        assert_eq!(Result::Success, unsafe {
            terminal_c::grid_ref(terminal, active_point(1, 0), &mut ref_)
        });
        assert_eq!(Result::Success, unsafe {
            event_set(
                press_event,
                EventOption::Ref,
                &ref_ as *const _ as *const c_void,
            )
        });

        assert_eq!(Result::NoValue, unsafe {
            handle_event(gesture, terminal, press_event, std::ptr::null_mut())
        });
        assert_eq!(Result::NoValue, unsafe {
            handle_event(gesture, terminal, release_event, std::ptr::null_mut())
        });

        let mut dragged = false;
        assert_eq!(Result::Success, unsafe {
            get(
                gesture,
                terminal,
                SelectionGestureData::Dragged,
                &mut dragged as *mut _ as *mut c_void,
            )
        });
        assert!(dragged);

        unsafe { event_free(release_event) };
        unsafe { event_free(press_event) };
        unsafe { free(gesture, terminal) };
        unsafe { terminal_c::free(terminal) };
    });
}

// test "selection gesture event applies drag"
#[test]
fn selection_gesture_event_applies_drag() {
    with_test_allocator(|alloc| {
        let mut terminal: terminal_c::Terminal = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe {
            terminal_c::new(alloc, &mut terminal, 5, 2)
        });

        let input = b"abcde";
        unsafe { terminal_c::vt_write(terminal, input.as_ptr(), 5) };

        let mut gesture: Gesture = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe { new(Some(alloc), &mut gesture) });

        let mut press_event: SelectionGestureEvent = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe {
            event_new(Some(alloc), &mut press_event, EventType::Press)
        });

        let mut drag_event: SelectionGestureEvent = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe {
            event_new(Some(alloc), &mut drag_event, EventType::Drag)
        });

        let mut press_ref: grid_ref::CGridRef = grid_ref::CGridRef::default();
        assert_eq!(Result::Success, unsafe {
            terminal_c::grid_ref(terminal, active_point(1, 0), &mut press_ref)
        });
        assert_eq!(Result::Success, unsafe {
            event_set(
                press_event,
                EventOption::Ref,
                &press_ref as *const _ as *const c_void,
            )
        });

        let press_pos = SurfacePosition { x: 10.0, y: 10.0 };
        assert_eq!(Result::Success, unsafe {
            event_set(
                press_event,
                EventOption::Position,
                &press_pos as *const _ as *const c_void,
            )
        });
        assert_eq!(Result::NoValue, unsafe {
            handle_event(gesture, terminal, press_event, std::ptr::null_mut())
        });

        let mut drag_ref: grid_ref::CGridRef = grid_ref::CGridRef::default();
        assert_eq!(Result::Success, unsafe {
            terminal_c::grid_ref(terminal, active_point(3, 0), &mut drag_ref)
        });
        assert_eq!(Result::Success, unsafe {
            event_set(
                drag_event,
                EventOption::Ref,
                &drag_ref as *const _ as *const c_void,
            )
        });

        let drag_pos = SurfacePosition { x: 36.0, y: 10.0 };
        assert_eq!(Result::Success, unsafe {
            event_set(
                drag_event,
                EventOption::Position,
                &drag_pos as *const _ as *const c_void,
            )
        });
        let geometry = Geometry {
            columns: 5,
            cell_width: 10,
            padding_left: 0,
            screen_height: 20,
        };
        assert_eq!(Result::Success, unsafe {
            event_set(
                drag_event,
                EventOption::Geometry,
                &geometry as *const _ as *const c_void,
            )
        });

        let mut sel: selection_c::CSelection = unsafe { std::mem::zeroed() };
        assert_eq!(Result::Success, unsafe {
            handle_event(gesture, terminal, drag_event, &mut sel)
        });
        assert_eq!(1u16, sel.start.to_pin().unwrap().x);
        assert_eq!(3u16, sel.end.to_pin().unwrap().x);

        let mut dragged = false;
        assert_eq!(Result::Success, unsafe {
            get(
                gesture,
                terminal,
                SelectionGestureData::Dragged,
                &mut dragged as *mut _ as *mut c_void,
            )
        });
        assert!(dragged);

        unsafe { event_free(drag_event) };
        unsafe { event_free(press_event) };
        unsafe { free(gesture, terminal) };
        unsafe { terminal_c::free(terminal) };
    });
}

// test "selection gesture drag requires ref and geometry"
#[test]
fn selection_gesture_drag_requires_ref_and_geometry() {
    with_test_allocator(|alloc| {
        let mut terminal: terminal_c::Terminal = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe {
            terminal_c::new(alloc, &mut terminal, 5, 2)
        });

        let mut gesture: Gesture = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe { new(Some(alloc), &mut gesture) });

        let mut drag_event: SelectionGestureEvent = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe {
            event_new(Some(alloc), &mut drag_event, EventType::Drag)
        });

        let mut sel: selection_c::CSelection = unsafe { std::mem::zeroed() };
        assert_eq!(Result::InvalidValue, unsafe {
            handle_event(gesture, terminal, drag_event, &mut sel)
        });

        let mut ref_: grid_ref::CGridRef = grid_ref::CGridRef::default();
        assert_eq!(Result::Success, unsafe {
            terminal_c::grid_ref(terminal, active_point(1, 0), &mut ref_)
        });
        assert_eq!(Result::Success, unsafe {
            event_set(
                drag_event,
                EventOption::Ref,
                &ref_ as *const _ as *const c_void,
            )
        });
        assert_eq!(Result::InvalidValue, unsafe {
            handle_event(gesture, terminal, drag_event, &mut sel)
        });

        // const invalid_geometry: Geometry = .{ .columns = 5, .cell_width = 0, .padding_left = 0, .screen_height = 20 };
        let invalid_geometry = Geometry {
            columns: 5,
            cell_width: 0,
            padding_left: 0,
            screen_height: 20,
        };
        assert_eq!(Result::InvalidValue, unsafe {
            event_set(
                drag_event,
                EventOption::Geometry,
                &invalid_geometry as *const _ as *const c_void,
            )
        });

        unsafe { event_free(drag_event) };
        unsafe { free(gesture, terminal) };
        unsafe { terminal_c::free(terminal) };
    });
}

// test "selection gesture event applies autoscroll tick"
#[test]
fn selection_gesture_event_applies_autoscroll_tick() {
    with_test_allocator(|alloc| {
        let mut terminal: terminal_c::Terminal = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe {
            terminal_c::new(alloc, &mut terminal, 5, 2)
        });

        // terminal_c.vt_write(terminal, "abcde\r\nfghij", 12);
        let input = b"abcde\r\nfghij";
        unsafe { terminal_c::vt_write(terminal, input.as_ptr(), 12) };

        let mut gesture: Gesture = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe { new(Some(alloc), &mut gesture) });

        let mut press_event: SelectionGestureEvent = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe {
            event_new(Some(alloc), &mut press_event, EventType::Press)
        });

        let mut drag_event: SelectionGestureEvent = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe {
            event_new(Some(alloc), &mut drag_event, EventType::Drag)
        });

        let mut tick_event: SelectionGestureEvent = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe {
            event_new(Some(alloc), &mut tick_event, EventType::AutoscrollTick)
        });

        let geometry = Geometry {
            columns: 5,
            cell_width: 10,
            padding_left: 0,
            screen_height: 20,
        };

        let mut press_ref: grid_ref::CGridRef = grid_ref::CGridRef::default();
        assert_eq!(Result::Success, unsafe {
            terminal_c::grid_ref(terminal, active_point(1, 0), &mut press_ref)
        });
        assert_eq!(Result::Success, unsafe {
            event_set(
                press_event,
                EventOption::Ref,
                &press_ref as *const _ as *const c_void,
            )
        });
        let press_pos = SurfacePosition { x: 10.0, y: 10.0 };
        assert_eq!(Result::Success, unsafe {
            event_set(
                press_event,
                EventOption::Position,
                &press_pos as *const _ as *const c_void,
            )
        });
        assert_eq!(Result::NoValue, unsafe {
            handle_event(gesture, terminal, press_event, std::ptr::null_mut())
        });

        let mut drag_ref: grid_ref::CGridRef = grid_ref::CGridRef::default();
        assert_eq!(Result::Success, unsafe {
            terminal_c::grid_ref(terminal, active_point(3, 1), &mut drag_ref)
        });
        assert_eq!(Result::Success, unsafe {
            event_set(
                drag_event,
                EventOption::Ref,
                &drag_ref as *const _ as *const c_void,
            )
        });
        let drag_pos = SurfacePosition { x: 36.0, y: 20.0 };
        assert_eq!(Result::Success, unsafe {
            event_set(
                drag_event,
                EventOption::Position,
                &drag_pos as *const _ as *const c_void,
            )
        });
        assert_eq!(Result::Success, unsafe {
            event_set(
                drag_event,
                EventOption::Geometry,
                &geometry as *const _ as *const c_void,
            )
        });
        let mut sel: selection_c::CSelection = unsafe { std::mem::zeroed() };
        assert_eq!(Result::Success, unsafe {
            handle_event(gesture, terminal, drag_event, &mut sel)
        });

        let mut autoscroll: Autoscroll = Autoscroll::None;
        assert_eq!(Result::Success, unsafe {
            get(
                gesture,
                terminal,
                SelectionGestureData::Autoscroll,
                &mut autoscroll as *mut _ as *mut c_void,
            )
        });
        // try testing.expectEqual(Autoscroll.down, autoscroll);
        assert_eq!(Autoscroll::Down, autoscroll);

        // const viewport: point.Coordinate = .{ .x = 3, .y = 1 };
        let viewport = ZigCoordinate { x: 3, y: 1 };
        assert_eq!(Result::Success, unsafe {
            event_set(
                tick_event,
                EventOption::Viewport,
                &viewport as *const _ as *const c_void,
            )
        });
        assert_eq!(Result::Success, unsafe {
            event_set(
                tick_event,
                EventOption::Position,
                &drag_pos as *const _ as *const c_void,
            )
        });
        assert_eq!(Result::Success, unsafe {
            event_set(
                tick_event,
                EventOption::Geometry,
                &geometry as *const _ as *const c_void,
            )
        });

        assert_eq!(Result::Success, unsafe {
            handle_event(gesture, terminal, tick_event, &mut sel)
        });

        unsafe { event_free(tick_event) };
        unsafe { event_free(drag_event) };
        unsafe { event_free(press_event) };
        unsafe { free(gesture, terminal) };
        unsafe { terminal_c::free(terminal) };
    });
}

// test "selection gesture autoscroll tick requires viewport and geometry"
#[test]
fn selection_gesture_autoscroll_tick_requires_viewport_and_geometry() {
    with_test_allocator(|alloc| {
        let mut terminal: terminal_c::Terminal = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe {
            terminal_c::new(alloc, &mut terminal, 5, 2)
        });

        let mut gesture: Gesture = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe { new(Some(alloc), &mut gesture) });

        let mut tick_event: SelectionGestureEvent = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe {
            event_new(Some(alloc), &mut tick_event, EventType::AutoscrollTick)
        });

        let mut sel: selection_c::CSelection = unsafe { std::mem::zeroed() };
        assert_eq!(Result::InvalidValue, unsafe {
            handle_event(gesture, terminal, tick_event, &mut sel)
        });

        let viewport = ZigCoordinate { x: 1, y: 0 };
        assert_eq!(Result::Success, unsafe {
            event_set(
                tick_event,
                EventOption::Viewport,
                &viewport as *const _ as *const c_void,
            )
        });
        assert_eq!(Result::InvalidValue, unsafe {
            handle_event(gesture, terminal, tick_event, &mut sel)
        });

        let mut ref_: grid_ref::CGridRef = grid_ref::CGridRef::default();
        assert_eq!(Result::Success, unsafe {
            terminal_c::grid_ref(terminal, active_point(1, 0), &mut ref_)
        });
        // try testing.expectEqual(Result.invalid_value, event_set(tick_event, .ref, &ref));
        assert_eq!(Result::InvalidValue, unsafe {
            event_set(
                tick_event,
                EventOption::Ref,
                &ref_ as *const _ as *const c_void,
            )
        });

        unsafe { event_free(tick_event) };
        unsafe { free(gesture, terminal) };
        unsafe { terminal_c::free(terminal) };
    });
}

// test "selection gesture event applies deep press"
#[test]
fn selection_gesture_event_applies_deep_press() {
    with_test_allocator(|alloc| {
        let mut terminal: terminal_c::Terminal = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe {
            terminal_c::new(alloc, &mut terminal, 5, 2)
        });

        let input = b"abcde";
        unsafe { terminal_c::vt_write(terminal, input.as_ptr(), 5) };

        let mut gesture: Gesture = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe { new(Some(alloc), &mut gesture) });

        let mut press_event: SelectionGestureEvent = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe {
            event_new(Some(alloc), &mut press_event, EventType::Press)
        });

        let mut deep_press_event: SelectionGestureEvent = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe {
            event_new(Some(alloc), &mut deep_press_event, EventType::DeepPress)
        });

        let mut ref_: grid_ref::CGridRef = grid_ref::CGridRef::default();
        assert_eq!(Result::Success, unsafe {
            terminal_c::grid_ref(terminal, active_point(2, 0), &mut ref_)
        });
        assert_eq!(Result::Success, unsafe {
            event_set(
                press_event,
                EventOption::Ref,
                &ref_ as *const _ as *const c_void,
            )
        });
        assert_eq!(Result::NoValue, unsafe {
            handle_event(gesture, terminal, press_event, std::ptr::null_mut())
        });

        let mut sel: selection_c::CSelection = unsafe { std::mem::zeroed() };
        assert_eq!(Result::Success, unsafe {
            handle_event(gesture, terminal, deep_press_event, &mut sel)
        });
        assert_eq!(0u16, sel.start.to_pin().unwrap().x);
        assert_eq!(4u16, sel.end.to_pin().unwrap().x);

        let mut dragged = false;
        assert_eq!(Result::Success, unsafe {
            get(
                gesture,
                terminal,
                SelectionGestureData::Dragged,
                &mut dragged as *mut _ as *mut c_void,
            )
        });
        assert!(dragged);

        // const pos: types.SurfacePosition = .{ .x = 0, .y = 0 };
        let pos = SurfacePosition { x: 0.0, y: 0.0 };
        // try testing.expectEqual(Result.invalid_value, event_set(deep_press_event, .position, &pos));
        assert_eq!(Result::InvalidValue, unsafe {
            event_set(
                deep_press_event,
                EventOption::Position,
                &pos as *const _ as *const c_void,
            )
        });

        unsafe { event_free(deep_press_event) };
        unsafe { event_free(press_event) };
        unsafe { free(gesture, terminal) };
        unsafe { terminal_c::free(terminal) };
    });
}

// test "selection gesture deep press without active anchor returns no value"
#[test]
fn selection_gesture_deep_press_without_active_anchor_returns_no_value() {
    with_test_allocator(|alloc| {
        let mut terminal: terminal_c::Terminal = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe {
            terminal_c::new(alloc, &mut terminal, 5, 2)
        });

        let mut gesture: Gesture = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe { new(Some(alloc), &mut gesture) });

        let mut deep_press_event: SelectionGestureEvent = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe {
            event_new(Some(alloc), &mut deep_press_event, EventType::DeepPress)
        });

        let mut sel: selection_c::CSelection = unsafe { std::mem::zeroed() };
        assert_eq!(Result::NoValue, unsafe {
            handle_event(gesture, terminal, deep_press_event, &mut sel)
        });

        unsafe { event_free(deep_press_event) };
        unsafe { free(gesture, terminal) };
        unsafe { terminal_c::free(terminal) };
    });
}

// test "selection gesture free null"
#[test]
fn selection_gesture_free_null() {
    // free(null, null);
    unsafe { free(std::ptr::null_mut(), std::ptr::null_mut()) };
}

// test "selection gesture event free null"
#[test]
fn selection_gesture_event_free_null() {
    // event_free(null);
    unsafe { event_free(std::ptr::null_mut()) };
}

// PORT NOTE (W12-01 proving tests, Rust-only — the reference has no unit test
// for this; the contract is `new`/`event_new`'s `alloc.create(...) catch {
// out.* = null; return .out_of_memory; }` + `free`/`event_free`'s
// `alloc.destroy`): the caller-supplied C allocator must OWN the gesture and
// event wrapper lifecycle storage.
#[test]
fn gesture_new_free_uses_caller_allocator() {
    let stats = crate::test_util::CountingCAllocator::new();
    let calloc = stats.c_allocator();
    let mut gesture: Gesture = std::ptr::null_mut();
    assert_eq!(Result::Success, unsafe { new(Some(&calloc), &mut gesture) });
    assert!(!gesture.is_null());
    assert_eq!(1, stats.alloc_calls.get());
    unsafe { free(gesture, std::ptr::null_mut()) };
    assert_eq!(1, stats.free_calls.get());
    assert_eq!(0, stats.live_bytes.get());
}

#[test]
fn gesture_new_rejecting_allocator_out_of_memory() {
    let stats = crate::test_util::CountingCAllocator::new();
    stats.reject.set(true);
    let calloc = stats.c_allocator();
    let mut gesture: Gesture = std::ptr::null_mut();
    assert_eq!(Result::OutOfMemory, unsafe {
        new(Some(&calloc), &mut gesture)
    });
    assert!(gesture.is_null());
    assert_eq!(1, stats.alloc_calls.get());
}

#[test]
fn event_new_free_uses_caller_allocator() {
    let stats = crate::test_util::CountingCAllocator::new();
    let calloc = stats.c_allocator();
    let mut event: SelectionGestureEvent = std::ptr::null_mut();
    assert_eq!(Result::Success, unsafe {
        event_new(Some(&calloc), &mut event, EventType::Press)
    });
    assert!(!event.is_null());
    assert_eq!(1, stats.alloc_calls.get());
    unsafe { event_free(event) };
    assert_eq!(1, stats.free_calls.get());
    assert_eq!(0, stats.live_bytes.get());
}

#[test]
fn event_new_rejecting_allocator_out_of_memory() {
    let stats = crate::test_util::CountingCAllocator::new();
    stats.reject.set(true);
    let calloc = stats.c_allocator();
    let mut event: SelectionGestureEvent = std::ptr::null_mut();
    assert_eq!(Result::OutOfMemory, unsafe {
        event_new(Some(&calloc), &mut event, EventType::Press)
    });
    assert!(event.is_null());
    assert_eq!(1, stats.alloc_calls.get());
}

// W66-BEHAVIOR-RAW-01 (Rust-only proving test; the reference has no equivalent
// test, but this is its `validBehavior` guard —
// ghostty src/terminal/c/selection_gesture.zig:495-499 + :745-748):
// `RusttySelectionGestureBehaviors`'s three fields are PLAIN C enums, so a C
// caller can store any int in them. Every field is range-checked before it is
// converted, and any out-of-range field rejects the whole call with
// `RUSTTY_INVALID_VALUE` — the port must not accept a raw 99 as a behavior.
#[test]
fn event_set_behaviors_rejects_out_of_range_c_values() {
    const C_SUCCESS: c_int = 0;
    const C_INVALID_VALUE: c_int = -2;
    // RUSTTY_SELECTION_GESTURE_EVENT_OPT_BEHAVIORS
    const OPT_BEHAVIORS: c_int = EventOption::Behaviors as c_int;

    with_test_allocator(|alloc| {
        let mut event: SelectionGestureEvent = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe {
            event_new(Some(alloc), &mut event, EventType::Press)
        });

        // The C-side struct as a C caller can actually build it: three ints.
        // Each of the three fields is guarded, so each gets its own rejection.
        for raw in [
            [99 as c_int, 0, 0],
            [0, 99, 0],
            [0, 0, 99],
            // Just past the last declared variant (`.output = 3`), and the
            // header's `RUSTTY_ENUM_MAX_VALUE` sentinel.
            [4, 0, 0],
            [0, 0, c_int::MAX],
            // Negative storage is representable in a C enum too.
            [-1, 0, 0],
        ] {
            assert_eq!(
                C_INVALID_VALUE,
                unsafe {
                    crate::rustty_selection_gesture_event_set(
                        event,
                        OPT_BEHAVIORS,
                        raw.as_ptr() as *const c_void,
                    )
                },
                "raw behaviors {raw:?} must be rejected",
            );
            // The rejection is total: the event keeps the default table.
            assert_eq!(sg::DEFAULT_BEHAVIORS, press_behaviors(event));
        }

        // In-range raw values still apply, unchanged: `.word, .word, .line`.
        let ok: [c_int; 3] = [1, 1, 2];
        assert_eq!(C_SUCCESS, unsafe {
            crate::rustty_selection_gesture_event_set(
                event,
                OPT_BEHAVIORS,
                ok.as_ptr() as *const c_void,
            )
        });
        let behaviors = press_behaviors(event);
        assert_eq!(sg::Behavior::Word, behaviors[0]);
        assert_eq!(sg::Behavior::Word, behaviors[1]);
        assert_eq!(sg::Behavior::Line, behaviors[2]);

        unsafe { event_free(event) };
    });
}

// W66-GETMULTI-RAW-01 (Rust-only proving test, shaped after the reference's
// "get_multi returns first failing index" test): the `keys` array is C memory
// typed `const RusttySelectionGestureData *`, so its elements are
// caller-supplied ints. The reference reaches its per-element
// `std.enums.fromInt(Data, ...)` guard because `get_multi` calls `get`
// (selection_gesture.zig:361-405); the port must classify an out-of-range key
// as `RUSTTY_INVALID_VALUE` — not `RUSTTY_NO_VALUE` — and report the first
// failing index through `out_written`.
#[test]
fn get_multi_rejects_out_of_range_c_keys() {
    // N625: this test reaches `sys::log_fn`; hold the sys guard so its
    // warnings cannot land in another test's log sink.
    let _guard = crate::sys::tests::reset_global();
    const C_INVALID_VALUE: c_int = -2;

    with_test_allocator(|alloc| {
        let mut terminal: terminal_c::Terminal = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe {
            terminal_c::new(alloc, &mut terminal, 5, 2)
        });

        let mut gesture: Gesture = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe { new(Some(alloc), &mut gesture) });

        // [ .click_count, <raw 99> ]: the valid prefix is processed first, then
        // the bad key stops the walk at index 1.
        let keys: [c_int; 2] = [SelectionGestureData::ClickCount as c_int, 99];
        let mut click_count: u8 = 255;
        let mut dragged = true;
        let mut values = [
            &mut click_count as *mut _ as *mut c_void,
            &mut dragged as *mut _ as *mut c_void,
        ];
        let mut written: usize = 99;

        assert_eq!(C_INVALID_VALUE, unsafe {
            crate::rustty_selection_gesture_get_multi(
                gesture,
                terminal,
                keys.len(),
                keys.as_ptr().cast(),
                values.as_mut_ptr(),
                &mut written,
            )
        });
        assert_eq!(1usize, written);
        // The valid prefix still wrote its value.
        assert_eq!(0u8, click_count);
        // The rejected key never wrote through its out pointer.
        assert!(dragged);

        // The scalar entry point classifies the same key identically.
        let mut out: u8 = 255;
        assert_eq!(C_INVALID_VALUE, unsafe {
            crate::rustty_selection_gesture_get(
                gesture,
                terminal,
                99,
                &mut out as *mut _ as *mut c_void,
            )
        });

        unsafe { free(gesture, terminal) };
        unsafe { terminal_c::free(terminal) };
    });
}

// ---------------------------------------------------------------------------
// Parity tail S7-8: the event's codepoint copy goes through the event's
// allocator (`event.alloc.alloc(u21, value.len)`, `event.alloc.free(cps)`),
// with the reference's exact sizes, its OUT_OF_MEMORY arm, and the R13 fix of
// the invalid-codepoint leak (UD314).
// ---------------------------------------------------------------------------

fn set_codepoints(event: SelectionGestureEvent, values: &[u32]) -> Result {
    let in_ = Codepoints {
        ptr: values.as_ptr(),
        len: values.len(),
    };
    unsafe {
        event_set(
            event,
            EventOption::WordBoundaryCodepoints,
            &in_ as *const _ as *const c_void,
        )
    }
}

#[test]
fn event_codepoints_route_through_the_event_allocator() {
    use rustty_vt::alloc::testing::CountingAllocator;
    let counting = CountingAllocator::over_heap();
    let calloc = counting.allocator();
    let wrapper_len = std::mem::size_of::<EventWrapper>();

    let mut event: SelectionGestureEvent = std::ptr::null_mut();
    assert_eq!(Result::Success, unsafe {
        event_new(Some(&calloc), &mut event, EventType::Press)
    });
    // alloc.create(EventWrapper): one call, nothing else.
    assert_eq!(1, counting.alloc_calls.get());
    assert_eq!(wrapper_len, counting.live_bytes.get());

    // alloc(u21, 3): one exact 12-byte, 4-aligned request.
    assert_eq!(
        Result::Success,
        set_codepoints(event, &[' ' as u32, '\t' as u32, ';' as u32])
    );
    assert_eq!(2, counting.alloc_calls.get());
    assert_eq!(Some((12, 2)), counting.last_alloc.get());
    assert_eq!(wrapper_len + 12, counting.live_bytes.get());
    assert_eq!(
        vec![' ' as u32, '\t' as u32, ';' as u32],
        press_word_boundaries(event)
    );

    // Replacing frees the old copy (clearWordBoundaryCodepoints) before the
    // new one is allocated.
    assert_eq!(Result::Success, set_codepoints(event, &['x' as u32]));
    assert_eq!(1, counting.free_calls.get());
    assert_eq!(Some((12, 2)), counting.last_free.get());
    assert_eq!(3, counting.alloc_calls.get());
    assert_eq!(Some((4, 2)), counting.last_alloc.get());
    assert_eq!(wrapper_len + 4, counting.live_bytes.get());

    // A zero-length, non-null slice: `alloc(u21, 0)` makes no call, and the
    // old copy is freed.
    let none: [u32; 0] = [];
    let empty = Codepoints {
        ptr: none.as_ptr(),
        len: 0,
    };
    assert!(!empty.ptr.is_null());
    assert_eq!(Result::Success, unsafe {
        event_set(
            event,
            EventOption::WordBoundaryCodepoints,
            &empty as *const _ as *const c_void,
        )
    });
    assert_eq!(3, counting.alloc_calls.get());
    assert_eq!(2, counting.free_calls.get());
    assert_eq!(0, press_word_boundaries(event).len());

    // The null-value clear allocates nothing.
    assert_eq!(
        Result::Success,
        set_codepoints(event, &['y' as u32, 'z' as u32])
    );
    assert_eq!(4, counting.alloc_calls.get());
    assert_eq!(Result::Success, unsafe {
        event_set(event, EventOption::WordBoundaryCodepoints, std::ptr::null())
    });
    assert_eq!(4, counting.alloc_calls.get());
    assert_eq!(3, counting.free_calls.get());
    assert_eq!(Some((8, 2)), counting.last_free.get());

    // event_free frees a live copy, then the wrapper.
    assert_eq!(Result::Success, set_codepoints(event, &['a' as u32]));
    assert_eq!(5, counting.alloc_calls.get());
    unsafe { event_free(event) };
    assert_eq!(5, counting.free_calls.get());
    assert_eq!(
        Some((
            wrapper_len,
            std::mem::align_of::<EventWrapper>().trailing_zeros() as u8
        )),
        counting.last_free.get()
    );
    assert_eq!(0, counting.live_bytes.get());
    // Nothing reached the vtable's resize/remap.
    assert_eq!(0, counting.resize_calls.get());
    assert_eq!(0, counting.remap_calls.get());
}

#[test]
fn event_codepoints_allocation_failure_is_out_of_memory() {
    use rustty_vt::alloc::testing::{Config, FailingAllocator};
    // The wrapper succeeds; the codepoint copy is the second allocation.
    let failing = FailingAllocator::over_heap(Config {
        fail_index: 1,
        ..Config::default()
    });
    let calloc = failing.allocator();
    let mut event: SelectionGestureEvent = std::ptr::null_mut();
    assert_eq!(Result::Success, unsafe {
        event_new(Some(&calloc), &mut event, EventType::Drag)
    });
    assert_eq!(Result::OutOfMemory, set_codepoints(event, &[' ' as u32]));
    assert!(failing.has_induced_failure.get());
    // The failed set already cleared any previous copy: the payload reads the
    // default boundaries, as the reference's `target.*` does.
    // SAFETY: `event` is live.
    let ev = unsafe { &*event };
    assert!(ev.word_boundary_codepoints.is_none());
    assert_eq!(
        selection_codepoints::DEFAULT_WORD_BOUNDARIES.len(),
        ev.word_boundaries().len()
    );
    unsafe { event_free(event) };
    assert_eq!(1, failing.allocations.get());
    assert_eq!(1, failing.deallocations.get());
    assert_eq!(failing.allocated_bytes.get(), failing.freed_bytes.get());
}

// R13 (UD314): an out-of-range codepoint returns INVALID_VALUE. The reference
// leaks the copy there (`errdefer` does not run for a non-error return); the
// port frees it.
#[test]
fn event_invalid_codepoint_frees_the_copy() {
    use rustty_vt::alloc::testing::CountingAllocator;
    let counting = CountingAllocator::over_heap();
    let calloc = counting.allocator();
    let wrapper_len = std::mem::size_of::<EventWrapper>();
    let mut event: SelectionGestureEvent = std::ptr::null_mut();
    assert_eq!(Result::Success, unsafe {
        event_new(Some(&calloc), &mut event, EventType::Press)
    });
    assert_eq!(Result::Success, set_codepoints(event, &[' ' as u32]));
    assert_eq!(wrapper_len + 4, counting.live_bytes.get());

    // The copy is allocated (2 elements, 8 bytes), then the second codepoint
    // does not fit in 21 bits.
    assert_eq!(
        Result::InvalidValue,
        set_codepoints(event, &['a' as u32, 0x20_0000])
    );
    // The fixed path ran: the copy was allocated...
    assert_eq!(3, counting.alloc_calls.get());
    assert_eq!(Some((8, 2)), counting.last_alloc.get());
    // ...and freed, along with the previous copy.
    assert_eq!(2, counting.free_calls.get());
    assert_eq!(Some((8, 2)), counting.last_free.get());
    assert_eq!(wrapper_len, counting.live_bytes.get());
    // The event falls back to the defaults, as the reference's does.
    assert_eq!(
        selection_codepoints::DEFAULT_WORD_BOUNDARIES.to_vec(),
        press_word_boundaries(event)
    );
    // The boundary itself (U+1FFFFF) is accepted.
    assert_eq!(Result::Success, set_codepoints(event, &[0x1F_FFFF]));
    unsafe { event_free(event) };
    assert_eq!(0, counting.live_bytes.get());
}

#[test]
fn gesture_wrapper_routes_through_the_gesture_allocator() {
    use rustty_vt::alloc::testing::CountingAllocator;
    let counting = CountingAllocator::over_heap();
    let calloc = counting.allocator();
    let mut gesture: Gesture = std::ptr::null_mut();
    assert_eq!(Result::Success, unsafe { new(Some(&calloc), &mut gesture) });
    assert_eq!(1, counting.alloc_calls.get());
    assert_eq!(
        Some((
            std::mem::size_of::<GestureWrapper>(),
            std::mem::align_of::<GestureWrapper>().trailing_zeros() as u8
        )),
        counting.last_alloc.get()
    );
    unsafe { free(gesture, std::ptr::null_mut()) };
    assert_eq!(1, counting.free_calls.get());
    assert_eq!(0, counting.live_bytes.get());
}

// The C-API allocation-failure sweep over the selection-gesture lifecycle: for
// every allocation index, the sequence either succeeds or reports
// OUT_OF_MEMORY at exactly the failing call, never aborts, and returns every
// byte. The lifecycle makes 4 allocations (gesture, event, two copies) and no
// resize or remap (the routing test pins that), so there is no remap loop.
#[test]
fn selection_gesture_allocation_failure_sweep() {
    use rustty_vt::alloc::testing::{Config, FailingAllocator};
    const ALLOCS: usize = 4;
    for fail_index in 0..=ALLOCS {
        let failing = FailingAllocator::over_heap(Config {
            fail_index,
            ..Config::default()
        });
        let calloc = failing.allocator();
        let mut results = Vec::new();
        let mut gesture: Gesture = std::ptr::null_mut();
        results.push(unsafe { new(Some(&calloc), &mut gesture) });
        let mut event: SelectionGestureEvent = std::ptr::null_mut();
        results.push(unsafe { event_new(Some(&calloc), &mut event, EventType::Press) });
        if !event.is_null() {
            results.push(set_codepoints(event, &[' ' as u32, '\t' as u32]));
            results.push(set_codepoints(event, &['x' as u32]));
        }
        unsafe { event_free(event) };
        unsafe { free(gesture, std::ptr::null_mut()) };

        // Per index: each call makes one allocation, and the failing
        // allocator refuses every allocation from `fail_index` on, so every
        // call from `fail_index` on reports OUT_OF_MEMORY and every earlier
        // call succeeds. A failed `event_new` leaves no event to set.
        let expected_calls = if fail_index <= 1 { 2 } else { ALLOCS };
        assert_eq!(expected_calls, results.len(), "fail_index {fail_index}");
        for (i, r) in results.iter().enumerate() {
            let expect = if i >= fail_index {
                Result::OutOfMemory
            } else {
                Result::Success
            };
            assert_eq!(expect, *r, "fail_index {fail_index}, call {i}");
        }
        assert_eq!(fail_index < ALLOCS, failing.has_induced_failure.get());
        assert_eq!(fail_index.min(ALLOCS), failing.allocations.get());
        assert_eq!(failing.allocations.get(), failing.deallocations.get());
        assert_eq!(failing.allocated_bytes.get(), failing.freed_bytes.get());
    }
}
