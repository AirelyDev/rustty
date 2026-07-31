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
fn press_xpos(event: Event) -> f64 {
    match &unsafe { &*event }.event {
        EventPayload::Press(p) => p.xpos,
        _ => unreachable!(),
    }
}

fn press_ypos(event: Event) -> f64 {
    match &unsafe { &*event }.event {
        EventPayload::Press(p) => p.ypos,
        _ => unreachable!(),
    }
}

fn press_max_distance(event: Event) -> f64 {
    match &unsafe { &*event }.event {
        EventPayload::Press(p) => p.max_distance,
        _ => unreachable!(),
    }
}

fn press_word_boundaries(event: Event) -> Vec<u32> {
    let ev = unsafe { &*event };
    match &ev.event {
        EventPayload::Press(_) => ev.word_boundaries().to_vec(),
        _ => unreachable!(),
    }
}

fn press_behaviors(event: Event) -> [sg::Behavior; 3] {
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
                Data::ClickCount,
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
                Data::Dragged,
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
                Data::Autoscroll,
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
                Data::Behavior,
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
                Data::Anchor,
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
            Data::ClickCount,
            Data::Dragged,
            Data::Autoscroll,
            Data::Behavior,
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
                keys.as_ptr(),
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
        let keys = [Data::ClickCount, Data::Anchor, Data::Dragged];
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
                keys.as_ptr(),
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
        let mut event: Event = std::ptr::null_mut();
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
        let mut event: Event = std::ptr::null_mut();
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
        let mut event: Event = std::ptr::null_mut();
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

        let mut press_event: Event = std::ptr::null_mut();
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

        let mut press_event: Event = std::ptr::null_mut();
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

        let mut press_event: Event = std::ptr::null_mut();
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

        let mut press_event: Event = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe {
            event_new(Some(alloc), &mut press_event, EventType::Press)
        });

        let mut release_event: Event = std::ptr::null_mut();
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
                Data::Dragged,
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

        let mut press_event: Event = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe {
            event_new(Some(alloc), &mut press_event, EventType::Press)
        });

        let mut release_event: Event = std::ptr::null_mut();
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
                Data::Dragged,
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

        let mut press_event: Event = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe {
            event_new(Some(alloc), &mut press_event, EventType::Press)
        });

        let mut drag_event: Event = std::ptr::null_mut();
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
                Data::Dragged,
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

        let mut drag_event: Event = std::ptr::null_mut();
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

        let mut press_event: Event = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe {
            event_new(Some(alloc), &mut press_event, EventType::Press)
        });

        let mut drag_event: Event = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe {
            event_new(Some(alloc), &mut drag_event, EventType::Drag)
        });

        let mut tick_event: Event = std::ptr::null_mut();
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
                Data::Autoscroll,
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

        let mut tick_event: Event = std::ptr::null_mut();
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

        let mut press_event: Event = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe {
            event_new(Some(alloc), &mut press_event, EventType::Press)
        });

        let mut deep_press_event: Event = std::ptr::null_mut();
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
                Data::Dragged,
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

        let mut deep_press_event: Event = std::ptr::null_mut();
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
