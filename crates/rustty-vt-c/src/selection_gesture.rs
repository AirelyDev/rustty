//! rustty-vt-c selection_gesture
//!
//! PORTS: ghostty src/terminal/c/selection_gesture.zig
//!
//! PORT STATUS: FILLED (Stage B v4). Every type, `pub fn`/helper body, and all
//! `test {}` blocks are ported assertion-for-assertion.

use crate::grid_ref;
use crate::result::Result;
use crate::selection as selection_c;
use crate::terminal as terminal_c;
use rustty_vt::point::Coordinate;
use std::ffi::c_void;
use std::os::raw::c_int;

// const std = @import("std");
// const testing = std.testing;
// const builtin = @import("builtin");
// const lib = @import("../lib.zig");
// const CAllocator = lib.alloc.Allocator;
// const SelectionGesture = @import("../SelectionGesture.zig");
// const selection_codepoints = @import("../selection_codepoints.zig");
// const grid_ref = @import("grid_ref.zig");
// const point = @import("../point.zig");
// const selection_c = @import("selection.zig");
// const terminal_c = @import("terminal.zig");
// const types = @import("types.zig");
// const Result = @import("result.zig").Result;
use allocator_api2::alloc::{Allocator as ApiAllocator, Layout};
use rustty_lib::allocator::Allocator as CAllocator;
use rustty_vt::alloc::{AVec, VtAllocator};
use rustty_vt::selection_codepoints;
use rustty_vt::selection_gesture as sg;
use rustty_vt::selection_gesture::SelectionGesture;

// const log = std.log.scoped(.selection_gesture_c);
//
// PORT NOTE: rustty's scoped-log sink is `sys::log_fn` (the port of
// `terminal/c/sys.zig`'s `logFn`), which is silent until a C caller installs a
// log callback via `rustty_sys_set(RUSTTY_SYS_OPT_LOG, ...)`. `LOG_SCOPE` is
// the `@tagName` of the reference's scope, which `log_fn` forwards to the
// callback as the scope text.
pub(crate) const LOG_SCOPE: &str = "selection_gesture_c";

// PORT NOTE (types.zig deferred): the reference imports `types.SurfacePosition`
// and `types.Codepoints` from `c/types.zig`, which has NOT yet landed (it
// registers ~29 C struct layouts and is gated to the sub-batch that lands them).
// These two `extern struct`s are defined locally here so render/selection_gesture
// can compile + test; when `types.zig` lands they move to the central `types`
// module and these become re-exports. Layout is byte-identical to the reference.
//
// /// C: GhosttySurfacePosition
// pub const SurfacePosition = extern struct { x: f64, y: f64 };
#[repr(C)]
#[derive(Clone, Copy)]
pub struct SurfacePosition {
    pub x: f64,
    pub y: f64,
}

// /// C: GhosttyCodepoints
// pub const Codepoints = extern struct { ptr: ?[*]const u32 = null, len: usize = 0 };
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Codepoints {
    pub ptr: *const u32,
    pub len: usize,
}

/// C: GhosttySelectionGesture -> RusttySelectionGesture (R2)
// pub const Gesture = ?*GestureWrapper;
pub type Gesture = *mut GestureWrapper;

/// C: GhosttySelectionGestureEvent -> RusttySelectionGestureEvent (R2)
// pub const Event = ?*EventWrapper;
pub type Event = *mut EventWrapper;

// const GestureWrapper = struct {
//     alloc: std.mem.Allocator,
//     gesture: SelectionGesture = .init,
// };
pub struct GestureWrapper {
    // PORT NOTE (parity tail S7-8): the reference `alloc: std.mem.Allocator` is
    // the C-resolved allocator, held as the runtime handle
    // (`lib.alloc.default(alloc_)`). The wrapper storage goes through it; the
    // gesture's pin tracking goes through the terminal's own allocator, as in
    // the reference.
    pub alloc: VtAllocator,
    pub gesture: SelectionGesture,
}

// PORT NOTE: the `event` field is a `union(EventType)`; in Rust it is the tagged
// enum `EventPayload` below (the discriminant is `EventType`). The
// `SelectionGesture.Press`/`Drag`/`AutoscrollTick`/`DeepPress` borrow
// `word_boundary_codepoints` and `Press` borrows `behaviors`, so each payload
// carries a `'static`-ish borrow into the event-owned storage. Rust cannot
// express the self-referential borrow with a real lifetime, so the payloads use
// raw slice pointers rebuilt from the event's owned storage on each access (see
// `EventPayload`).
//
// const EventWrapper = struct {
//     alloc: std.mem.Allocator,
//     event: union(EventType) {
//         press: SelectionGesture.Press,
//         release: SelectionGesture.Release,
//         drag: SelectionGesture.Drag,
//         autoscroll_tick: SelectionGesture.AutoscrollTick,
//         deep_press: SelectionGesture.DeepPress,
//     },
//
//     // Validation sidecar for required event fields that don't have safe
//     // sentinels in the real SelectionGesture payloads. For example, PageList.Pin
//     // contains a non-null node pointer and Geometry has no meaningful zero
//     // value. Keep these as one-bit flags so dispatch can reject incomplete C
//     // events instead of using undefined placeholder data.
//     event_validation: packed struct {
//         press_pin_set: bool = false,
//         drag_pin_set: bool = false,
//         drag_geometry_set: bool = false,
//         autoscroll_tick_viewport_set: bool = false,
//         autoscroll_tick_geometry_set: bool = false,
//     } = .{},
//
//     // Backing storage for Press/Drag/AutoscrollTick.word_boundary_codepoints.
//     // The C API receives codepoints as borrowed uint32_t values, but
//     // SelectionGesture stores a []const u21 slice. We copy/convert into
//     // event-owned storage so the real payload can safely point at it until the
//     // event is changed or freed.
//     word_boundary_codepoints: ?[]u21 = null,
//
//     // Backing storage for Press.behaviors. The C API sets behaviors as a value
//     // struct, but SelectionGesture.Press stores a pointer to a [3]Behavior.
//     // Keep the array on the event wrapper so the Press payload can point at a
//     // stable location for the lifetime of the event.
//     behaviors: [3]Behavior = SelectionGesture.default_behaviors,
//     ...
// };
pub struct EventWrapper {
    // PORT NOTE (parity tail S7-8): the C-resolved allocator as the runtime
    // handle; the wrapper storage and the codepoint copy go through it.
    pub alloc: VtAllocator,
    pub event: EventPayload,

    // Validation sidecar for required event fields that don't have safe
    // sentinels in the real SelectionGesture payloads. For example, PageList.Pin
    // contains a non-null node pointer and Geometry has no meaningful zero
    // value. Keep these as one-bit flags so dispatch can reject incomplete C
    // events instead of using undefined placeholder data.
    pub event_validation: EventValidation,

    // Backing storage for Press/Drag/AutoscrollTick.word_boundary_codepoints.
    // The C API receives codepoints as borrowed uint32_t values, but
    // SelectionGesture stores a []const u21 slice. We copy/convert into
    // event-owned storage so the real payload can safely point at it until the
    // event is changed or freed.
    //
    // PORT NOTE: `?[]u21` becomes an owned `Option<AVec<u32>>` allocated
    // through the event's `alloc` (parity tail S7-8; `@sizeOf(u21)` is 4, so
    // `u32` elements make the reference's `len * 4`-byte, 4-aligned requests).
    // An empty slice (`&.{}`, or a zero-length `alloc`) is an empty list that
    // owns no allocation, which is why the reference skips the free when
    // `cps.len == 0`.
    pub word_boundary_codepoints: Option<AVec<u32>>,

    // Backing storage for Press.behaviors. The C API sets behaviors as a value
    // struct, but SelectionGesture.Press stores a pointer to a [3]Behavior.
    // Keep the array on the event wrapper so the Press payload can point at a
    // stable location for the lifetime of the event.
    pub behaviors: [sg::Behavior; 3],
}

// PORT NOTE: the reference `packed struct { ... bool = false }` validation
// sidecar; modeled as a plain struct of bools (the bit-packing is unobservable).
#[derive(Clone, Copy, Default)]
pub struct EventValidation {
    pub press_pin_set: bool,
    pub drag_pin_set: bool,
    pub drag_geometry_set: bool,
    pub autoscroll_tick_viewport_set: bool,
    pub autoscroll_tick_geometry_set: bool,
}

// PORT NOTE: the `union(EventType)` payload. The reference `SelectionGesture`
// payloads borrow `word_boundary_codepoints` (and `Press.behaviors`); rather
// than store the borrowing payloads (impossible self-reference in Rust), we
// store the *plain field values* and rebuild the borrowing
// `SelectionGesture.{Press,Drag,...}` from the event's owned storage when
// dispatching in `handle_event`. This keeps the C-visible semantics (set fields
// then dispatch) identical.
pub enum EventPayload {
    Press(PressFields),
    Release(sg::Release),
    Drag(DragFields),
    AutoscrollTick(AutoscrollTickFields),
    DeepPress(DeepPressFields),
}

// PORT NOTE: the non-borrowing fields of `SelectionGesture.Press`. The borrowed
// `word_boundary_codepoints`/`behaviors` live on the `EventWrapper`.
#[derive(Clone, Copy)]
pub struct PressFields {
    pub time: Option<sg::Time>,
    pub pin: rustty_vt::pagelist::Pin,
    pub xpos: f64,
    pub ypos: f64,
    pub max_distance: f64,
    pub repeat_interval: u64,
}

#[derive(Clone, Copy)]
pub struct DragFields {
    pub pin: rustty_vt::pagelist::Pin,
    pub xpos: f64,
    pub ypos: f64,
    pub rectangle: bool,
    pub geometry: sg::Geometry,
}

#[derive(Clone, Copy)]
pub struct AutoscrollTickFields {
    pub viewport: rustty_vt::point::Coordinate,
    pub xpos: f64,
    pub ypos: f64,
    pub rectangle: bool,
    pub geometry: sg::Geometry,
}

#[derive(Clone, Copy, Default)]
pub struct DeepPressFields {}

impl EventWrapper {
    // fn init(self: *EventWrapper, event_type: EventType) void {
    //     self.event = switch (event_type) {
    //         .press => .{ .press = self.defaultPress() },
    //         .release => .{ .release = self.defaultRelease() },
    //         .drag => .{ .drag = self.defaultDrag() },
    //         .autoscroll_tick => .{ .autoscroll_tick = self.defaultAutoscrollTick() },
    //         .deep_press => .{ .deep_press = self.defaultDeepPress() },
    //     };
    // }
    fn init(&mut self, event_type: EventType) {
        self.event = match event_type {
            EventType::Press => EventPayload::Press(Self::default_press()),
            EventType::Release => EventPayload::Release(Self::default_release()),
            EventType::Drag => EventPayload::Drag(Self::default_drag()),
            EventType::AutoscrollTick => {
                EventPayload::AutoscrollTick(Self::default_autoscroll_tick())
            }
            EventType::DeepPress => EventPayload::DeepPress(Self::default_deep_press()),
        };
    }

    // fn defaultPress(self: *EventWrapper) SelectionGesture.Press {
    //     return .{
    //         .time = null,
    //         .pin = undefined,
    //         .xpos = 0,
    //         .ypos = 0,
    //         .max_distance = 0,
    //         .repeat_interval = 0,
    //         .word_boundary_codepoints = &selection_codepoints.default_word_boundaries,
    //         .behaviors = &self.behaviors,
    //     };
    // }
    //
    // PORT NOTE: `.pin = undefined` becomes a zeroed `Pin` placeholder; the
    // `event_validation.press_pin_set` flag (false until `.ref` is set) is what
    // gates dispatch, so the placeholder is never read. `word_boundary_codepoints`
    // and `behaviors` are rebuilt at dispatch from the event's owned storage.
    fn default_press() -> PressFields {
        PressFields {
            time: None,
            pin: zeroed_pin(),
            xpos: 0.0,
            ypos: 0.0,
            max_distance: 0.0,
            repeat_interval: 0,
        }
    }

    // fn defaultRelease(self: *EventWrapper) SelectionGesture.Release {
    //     _ = self;
    //     return .{ .pin = null };
    // }
    fn default_release() -> sg::Release {
        sg::Release { pin: None }
    }

    // fn defaultDrag(self: *EventWrapper) SelectionGesture.Drag {
    //     _ = self;
    //     return .{
    //         .pin = undefined,
    //         .xpos = 0,
    //         .ypos = 0,
    //         .rectangle = false,
    //         .word_boundary_codepoints = &selection_codepoints.default_word_boundaries,
    //         .geometry = undefined,
    //     };
    // }
    fn default_drag() -> DragFields {
        DragFields {
            pin: zeroed_pin(),
            xpos: 0.0,
            ypos: 0.0,
            rectangle: false,
            geometry: zeroed_geometry(),
        }
    }

    // fn defaultAutoscrollTick(self: *EventWrapper) SelectionGesture.AutoscrollTick {
    //     _ = self;
    //     return .{
    //         .viewport = undefined,
    //         .xpos = 0,
    //         .ypos = 0,
    //         .rectangle = false,
    //         .word_boundary_codepoints = &selection_codepoints.default_word_boundaries,
    //         .geometry = undefined,
    //     };
    // }
    fn default_autoscroll_tick() -> AutoscrollTickFields {
        AutoscrollTickFields {
            viewport: rustty_vt::point::Coordinate { x: 0, y: 0 },
            xpos: 0.0,
            ypos: 0.0,
            rectangle: false,
            geometry: zeroed_geometry(),
        }
    }

    // fn defaultDeepPress(self: *EventWrapper) SelectionGesture.DeepPress {
    //     _ = self;
    //     return .{ .word_boundary_codepoints = &selection_codepoints.default_word_boundaries };
    // }
    fn default_deep_press() -> DeepPressFields {
        DeepPressFields {}
    }

    // fn deinit(self: *EventWrapper) void {
    //     if (self.word_boundary_codepoints) |cps| {
    //         if (cps.len > 0) self.alloc.free(cps);
    //     }
    // }
    //
    // PORT NOTE: the list is bound to `self.alloc`; dropping it is
    // `self.alloc.free(cps)` (an empty list owns nothing and frees nothing).
    fn deinit(&mut self) {
        if let Some(cps) = self.word_boundary_codepoints.take() {
            // if (cps.len > 0) self.alloc.free(cps);
            self.alloc.debug_assert_same(*cps.allocator());
            drop(cps);
        }
    }

    // PORT NOTE: rebuilds the borrowing `SelectionGesture.Press` from the stored
    // `PressFields` + the event's owned `behaviors`/codepoints storage. The
    // codepoint slice is the event-owned `word_boundary_codepoints` if set, else
    // the module default.
    fn press_payload(&self, p: &PressFields) -> sg::Press<'_> {
        sg::Press {
            time: p.time,
            pin: p.pin,
            xpos: p.xpos,
            ypos: p.ypos,
            max_distance: p.max_distance,
            repeat_interval: p.repeat_interval,
            word_boundary_codepoints: self.word_boundaries(),
            behaviors: &self.behaviors,
        }
    }

    fn drag_payload(&self, d: &DragFields) -> sg::Drag<'_> {
        sg::Drag {
            pin: d.pin,
            xpos: d.xpos,
            ypos: d.ypos,
            rectangle: d.rectangle,
            word_boundary_codepoints: self.word_boundaries(),
            geometry: d.geometry,
        }
    }

    fn autoscroll_tick_payload(&self, t: &AutoscrollTickFields) -> sg::AutoscrollTick<'_> {
        sg::AutoscrollTick {
            viewport: t.viewport,
            xpos: t.xpos,
            ypos: t.ypos,
            rectangle: t.rectangle,
            word_boundary_codepoints: self.word_boundaries(),
            geometry: t.geometry,
        }
    }

    fn deep_press_payload(&self, _d: &DeepPressFields) -> sg::DeepPress<'_> {
        sg::DeepPress {
            word_boundary_codepoints: self.word_boundaries(),
        }
    }

    fn word_boundaries(&self) -> &[u32] {
        match &self.word_boundary_codepoints {
            Some(cps) => cps.as_slice(),
            None => &selection_codepoints::DEFAULT_WORD_BOUNDARIES,
        }
    }
}

// PORT NOTE: `.pin = undefined` placeholder. `Pin` carries a non-null node ptr in
// real use; the placeholder is never read (gated by `event_validation`).
fn zeroed_pin() -> rustty_vt::pagelist::Pin {
    // SAFETY: a zeroed `Pin` is an inert placeholder that is never dereferenced;
    // dispatch is gated by `event_validation.*_pin_set` before any read.
    unsafe { std::mem::zeroed() }
}

// PORT NOTE: `.geometry = undefined` placeholder; never read (gated by
// `event_validation.*_geometry_set`).
fn zeroed_geometry() -> sg::Geometry {
    sg::Geometry {
        columns: 0,
        cell_width: 0,
        padding_left: 0,
        screen_height: 0,
    }
}

/// C: GhosttySelectionGestureBehavior -> RusttySelectionGestureBehavior (R2)
// pub const Behavior = SelectionGesture.Behavior;
//
// PORT NOTE: rustty-vt's `SelectionGesture.Behavior` is a plain (non-`repr(C)`)
// enum (the `.zig`-target `lib.Enum`). The C ABI needs `enum(c_int)`, so vt-c
// defines this `#[repr(C)]` projection with the reference key order and converts
// at the boundary. The reference's direct alias is preserved in behavior.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Behavior {
    Cell = 0,
    Word = 1,
    Line = 2,
    Output = 3,
}

// The C-ABI storage width the published header declares
// (`include/rustty/vt/selection.h`:
// `RUSTTY_SELECTION_GESTURE_BEHAVIOR_MAX_VALUE = RUSTTY_ENUM_MAX_VALUE`), which
// is what makes reading a C-supplied behavior field as a raw `c_int` ABI-neutral.
const _: () = assert!(std::mem::size_of::<Behavior>() == std::mem::size_of::<c_int>());

impl Behavior {
    /// Narrow an inbound C integer to a `Behavior`, mirroring
    /// `std.enums.fromInt`; `None` outside the declared range.
    ///
    /// D11 idiom (see `render::Data::from_c_int`): a `#[repr(C)]` enum
    /// materialized from out-of-range C memory is UB in every build mode, so
    /// C-supplied behavior fields are range-checked before conversion. This IS
    /// the reference's `validBehavior` check (`selection_gesture.zig:745-748`).
    pub fn from_c_int(v: c_int) -> Option<Behavior> {
        match v {
            0 => Some(Behavior::Cell),
            1 => Some(Behavior::Word),
            2 => Some(Behavior::Line),
            3 => Some(Behavior::Output),
            _ => None,
        }
    }

    pub fn from_zig(b: sg::Behavior) -> Behavior {
        match b {
            sg::Behavior::Cell => Behavior::Cell,
            sg::Behavior::Word => Behavior::Word,
            sg::Behavior::Line => Behavior::Line,
            sg::Behavior::Output => Behavior::Output,
        }
    }

    pub fn to_zig(self) -> sg::Behavior {
        match self {
            Behavior::Cell => sg::Behavior::Cell,
            Behavior::Word => sg::Behavior::Word,
            Behavior::Line => sg::Behavior::Line,
            Behavior::Output => sg::Behavior::Output,
        }
    }
}

/// C: GhosttySelectionGestureAutoscroll -> RusttySelectionGestureAutoscroll (R2)
// pub const Autoscroll = SelectionGesture.Autoscroll;
//
// PORT NOTE: as for `Behavior`, a `#[repr(C)]` projection of the non-`repr(C)`
// rustty-vt `Autoscroll`.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Autoscroll {
    None = 0,
    Up = 1,
    Down = 2,
}

impl Autoscroll {
    pub fn from_zig(a: sg::Autoscroll) -> Autoscroll {
        match a {
            sg::Autoscroll::None => Autoscroll::None,
            sg::Autoscroll::Up => Autoscroll::Up,
            sg::Autoscroll::Down => Autoscroll::Down,
        }
    }
}

/// C: GhosttySelectionGestureBehaviors -> RusttySelectionGestureBehaviors (R2)
// pub const Behaviors = extern struct {
//     single_click: Behavior,
//     double_click: Behavior,
//     triple_click: Behavior,
// };
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Behaviors {
    pub single_click: Behavior,
    pub double_click: Behavior,
    pub triple_click: Behavior,
}

// PORT NOTE (W66-BEHAVIOR-RAW-01): the reference reads a caller-supplied
// `Behaviors` as three `Behavior`s and range-checks each with `validBehavior`
// (`selection_gesture.zig:495-499`). In Rust the fields of `Behaviors` are a
// 4-variant `#[repr(C)]` enum, but the header declares them as a plain C enum,
// so a C caller can store ANY int in them: reading the caller's buffer as
// `*const Behaviors` would materialize an invalid discriminant (UB in every
// build mode) and would sail past the range check. The inbound read therefore
// uses this layout-identical raw view, and each field goes through
// `Behavior::from_c_int` — exactly the reference's per-field guard.
#[repr(C)]
#[derive(Clone, Copy)]
struct BehaviorsRaw {
    single_click: c_int,
    double_click: c_int,
    triple_click: c_int,
}

const _: () = assert!(std::mem::size_of::<BehaviorsRaw>() == std::mem::size_of::<Behaviors>());
const _: () = assert!(std::mem::align_of::<BehaviorsRaw>() == std::mem::align_of::<Behaviors>());

/// C: GhosttySelectionGestureData -> RusttySelectionGestureData (R2)
// pub const Data = enum(c_int) { click_count = 0, dragged = 1, autoscroll = 2, behavior = 3, anchor = 4 };
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Data {
    ClickCount = 0,
    Dragged = 1,
    Autoscroll = 2,
    Behavior = 3,
    Anchor = 4,
}

// The C-ABI storage width the published header declares
// (`include/rustty/vt/selection.h`: `RUSTTY_SELECTION_GESTURE_DATA_MAX_VALUE =
// RUSTTY_ENUM_MAX_VALUE`), which is what makes reading a `get_multi` key array
// element as a raw `c_int` ABI-neutral.
const _: () = assert!(std::mem::size_of::<Data>() == std::mem::size_of::<c_int>());

impl Data {
    /// Narrow an inbound C integer to a `Data`, mirroring `std.enums.fromInt`;
    /// `None` outside the declared range.
    // PORT NOTE: this is the reference's
    // `std.enums.fromInt(Data, @intFromEnum(data))` guard from `get`
    // (`selection_gesture.zig:361-373`); `Data` is a contiguous `#[repr(C)]`
    // enum here, so the range check IS `fromInt`.
    pub fn from_c_int(v: c_int) -> Option<Data> {
        // contiguous 0..=4
        if (0..=4).contains(&v) {
            // SAFETY: `v` is a valid `Data` discriminant (range-checked above).
            Some(unsafe { std::mem::transmute::<c_int, Data>(v) })
        } else {
            None
        }
    }
}

// Output type expected for querying the data of the given kind.
// PORT NOTE: `pub fn OutType(comptime self: Data) type`:
//     .click_count => u8,
//     .dragged => bool,
//     .autoscroll => Autoscroll,
//     .behavior => Behavior,
//     .anchor => grid_ref.CGridRef,
// In Rust the OutType mapping is preserved as the per-arm pointer cast in
// `get_typed`.

/// C: GhosttySelectionGestureEventType -> RusttySelectionGestureEventType (R2)
// pub const EventType = enum(c_int) { press = 0, release = 1, drag = 2, autoscroll_tick = 3, deep_press = 4 };
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventType {
    Press = 0,
    Release = 1,
    Drag = 2,
    AutoscrollTick = 3,
    DeepPress = 4,
}

/// C: GhosttySelectionGestureEventOption -> RusttySelectionGestureEventOption (R2)
// pub const EventOption = enum(c_int) { ref = 0, position = 1, repeat_distance = 2, time_ns = 3, repeat_interval_ns = 4, word_boundary_codepoints = 5, behaviors = 6, rectangle = 7, geometry = 8, viewport = 9 };
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventOption {
    Ref = 0,
    Position = 1,
    RepeatDistance = 2,
    TimeNs = 3,
    RepeatIntervalNs = 4,
    WordBoundaryCodepoints = 5,
    Behaviors = 6,
    Rectangle = 7,
    Geometry = 8,
    Viewport = 9,
}

// Input type expected for setting the option.
// PORT NOTE: `pub fn Type(comptime self: EventOption) type`:
//     .ref => grid_ref.CGridRef,
//     .position => types.SurfacePosition,
//     .repeat_distance => f64,
//     .time_ns => u64,
//     .repeat_interval_ns => u64,
//     .word_boundary_codepoints => types.Codepoints,
//     .behaviors => Behaviors,
//     .rectangle => bool,
//     .geometry => Geometry,
//     .viewport => point.Coordinate,
// In Rust the Type mapping is preserved as the per-arm pointer cast in the
// per-payload setter helpers.

/// C: GhosttySelectionGestureGeometry -> RusttySelectionGestureGeometry (R2)
// pub const Geometry = extern struct {
//     columns: u32,
//     cell_width: u32,
//     padding_left: u32,
//     screen_height: u32,
//     ...
// };
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Geometry {
    pub columns: u32,
    pub cell_width: u32,
    pub padding_left: u32,
    pub screen_height: u32,
}

impl Geometry {
    // fn toZig(self: Geometry) ?SelectionGesture.Drag.Geometry {
    //     if (self.columns == 0) return null;
    //     if (self.cell_width == 0) return null;
    //     if (self.screen_height == 0) return null;
    //     return .{ .columns, .cell_width, .padding_left, .screen_height };
    // }
    fn to_zig(self) -> Option<sg::Geometry> {
        if self.columns == 0 {
            return None;
        }
        if self.cell_width == 0 {
            return None;
        }
        if self.screen_height == 0 {
            return None;
        }
        Some(sg::Geometry {
            columns: self.columns,
            cell_width: self.cell_width,
            padding_left: self.padding_left,
            screen_height: self.screen_height,
        })
    }
}

// pub fn new(alloc_: ?*const CAllocator, out_gesture: ?*Gesture) ... Result {
//     const out = out_gesture orelse return .invalid_value;
//     const alloc = lib.alloc.default(alloc_);
//     const gesture = alloc.create(GestureWrapper) catch { out.* = null; return .out_of_memory; };
//     gesture.* = .{ .alloc = alloc };
//     out.* = gesture;
//     return .success;
// }
/// # Safety
/// `alloc_`, if non-null, must point to a valid `CAllocator`; `out_gesture`, if
/// non-null, must be a valid writable pointer to a `Gesture`.
pub unsafe fn new(alloc_: Option<&CAllocator>, out_gesture: *mut Gesture) -> Result {
    // const out = out_gesture orelse return .invalid_value;
    if out_gesture.is_null() {
        return Result::InvalidValue;
    }
    // const alloc = lib.alloc.default(alloc_);
    // SAFETY: a non-NULL `alloc_` is a valid `RusttyAllocator` that outlives
    // the gesture (the reference's contract, N260).
    let alloc = unsafe { rustty_vt::alloc::default(alloc_) };
    // const gesture = alloc.create(GestureWrapper) catch { out.* = null; return .out_of_memory; };
    // PORT NOTE (W12-01): through the C-resolved allocator; a rejecting caller
    // allocator yields `.out_of_memory` + null out-param like the reference.
    let gesture: *mut GestureWrapper =
        match ApiAllocator::allocate(&alloc, Layout::new::<GestureWrapper>()) {
            Ok(ptr) => ptr.cast().as_ptr(),
            Err(_) => {
                // SAFETY: `out_gesture` is a non-null valid writable `Gesture`.
                unsafe { *out_gesture = std::ptr::null_mut() };
                return Result::OutOfMemory;
            }
        };
    // gesture.* = .{ .alloc = alloc };  (SelectionGesture.init via field default)
    // SAFETY: `gesture` is valid uninitialized `GestureWrapper` storage.
    unsafe {
        std::ptr::write(
            gesture,
            GestureWrapper {
                alloc,
                gesture: SelectionGesture::INIT,
            },
        )
    };
    // out.* = gesture;
    // SAFETY: `out_gesture` is a non-null valid writable `Gesture`.
    unsafe { *out_gesture = gesture };
    Result::Success
}

// pub fn event_new(alloc_: ?*const CAllocator, out_event: ?*Event, event_type: EventType) ... Result {
//     const out = out_event orelse return .invalid_value;
//     _ = std.meta.intToEnum(EventType, @intFromEnum(event_type)) catch return .invalid_value;
//     const alloc = lib.alloc.default(alloc_);
//     const event = alloc.create(EventWrapper) catch { out.* = null; return .out_of_memory; };
//     event.* = .{ .alloc = alloc, .event = undefined };
//     event.init(event_type);
//     out.* = event;
//     return .success;
// }
//
// PORT NOTE: the `intToEnum(EventType, ...)` guard on the raw `c_int` lives in
// the `extern "C"` wrapper in `lib.rs`; by the time `event_new` is reached the
// `event_type` enum is already validated.
/// # Safety
/// `alloc_`, if non-null, must point to a valid `CAllocator`; `out_event`, if
/// non-null, must be a valid writable pointer to an `Event`.
pub unsafe fn event_new(
    alloc_: Option<&CAllocator>,
    out_event: *mut Event,
    event_type: EventType,
) -> Result {
    // const out = out_event orelse return .invalid_value;
    if out_event.is_null() {
        return Result::InvalidValue;
    }
    // const alloc = lib.alloc.default(alloc_);
    // SAFETY: a non-NULL `alloc_` is a valid `RusttyAllocator` that outlives
    // the event (the reference's contract, N260).
    let alloc = unsafe { rustty_vt::alloc::default(alloc_) };
    // const event = alloc.create(EventWrapper) catch { out.* = null; return .out_of_memory; };
    // PORT NOTE (W12-01): through the C-resolved allocator; a rejecting caller
    // allocator yields `.out_of_memory` + null out-param like the reference.
    let event: *mut EventWrapper =
        match ApiAllocator::allocate(&alloc, Layout::new::<EventWrapper>()) {
            Ok(ptr) => ptr.cast().as_ptr(),
            Err(_) => {
                // SAFETY: `out_event` is a non-null valid writable `Event`.
                unsafe { *out_event = std::ptr::null_mut() };
                return Result::OutOfMemory;
            }
        };
    // event.* = .{ .alloc = alloc, .event = undefined };  (other fields via defaults)
    // SAFETY: `event` is valid uninitialized `EventWrapper` storage.
    unsafe {
        std::ptr::write(
            event,
            EventWrapper {
                alloc,
                // PORT NOTE: `.event = undefined`, immediately overwritten by `init`. The
                // placeholder is the `Release{ pin: None }` variant (never observed).
                event: EventPayload::Release(sg::Release { pin: None }),
                event_validation: EventValidation::default(),
                word_boundary_codepoints: None,
                // .behaviors = SelectionGesture.default_behaviors
                behaviors: sg::DEFAULT_BEHAVIORS,
            },
        )
    };
    // event.init(event_type);
    // SAFETY: `event` was just initialized; narrow `&mut` for the init call.
    unsafe { (*event).init(event_type) };
    // out.* = event;
    // SAFETY: `out_event` is a non-null valid writable `Event`.
    unsafe { *out_event = event };
    Result::Success
}

// pub fn free(gesture_: Gesture, terminal: terminal_c.Terminal) ... void {
//     const wrapper = gesture_ orelse return;
//     if (terminal_c.zigTerminal(terminal)) |t| wrapper.gesture.deinit(t);
//     const alloc = wrapper.alloc;
//     alloc.destroy(wrapper);
// }
/// # Safety
/// `gesture_`, if non-null, must be a handle returned by `new`; `terminal`, if
/// non-null, must be a live terminal handle.
pub unsafe fn free(gesture_: Gesture, terminal: terminal_c::Terminal) {
    // const wrapper = gesture_ orelse return;
    if gesture_.is_null() {
        return;
    }
    // PORT NOTE (W12-01): move the wrapper value out of the caller-allocator
    // storage, deinit, then return the STORAGE through the resolved allocator
    // (`alloc.destroy(wrapper)` — the caller free callback fires).
    // SAFETY: `gesture_` is a live `new`-returned wrapper (caller contract);
    // after the read the storage is treated as uninitialized and freed once.
    let mut wrapper = unsafe { std::ptr::read(gesture_) };
    // if (terminal_c.zigTerminal(terminal)) |t| wrapper.gesture.deinit(t);
    // SAFETY: `terminal` is null or a live wrapper; `zig_terminal` raw-ptr reads.
    let t = unsafe { terminal_c::zig_terminal(terminal) };
    if !t.is_null() {
        // SAFETY: `t` is a live `ZigTerminal`; `deinit` needs `&mut`. The gesture
        // and terminal are independent allocations, so the borrows don't alias.
        let t_ref = unsafe { &mut *t };
        wrapper.gesture.deinit(t_ref);
    }
    // const alloc = wrapper.alloc; alloc.destroy(wrapper);
    let alloc = wrapper.alloc;
    drop(wrapper);
    // SAFETY: `gesture_` was allocated by `new` through this same allocator
    // with this layout.
    unsafe {
        ApiAllocator::deallocate(
            &alloc,
            std::ptr::NonNull::new_unchecked(gesture_.cast()),
            Layout::new::<GestureWrapper>(),
        )
    };
}

// pub fn event_free(event_: Event) ... void {
//     const event = event_ orelse return;
//     event.deinit();
//     const alloc = event.alloc;
//     alloc.destroy(event);
// }
/// # Safety
/// `event_`, if non-null, must be a handle returned by `event_new`.
pub unsafe fn event_free(event_: Event) {
    // const event = event_ orelse return;
    if event_.is_null() {
        return;
    }
    // PORT NOTE (W12-01): move the wrapper value out of the caller-allocator
    // storage, deinit (which frees the codepoint copy), then return the
    // STORAGE through the resolved allocator (`alloc.destroy(event)` — the
    // caller free callback fires).
    // SAFETY: `event_` is a live `event_new`-returned wrapper; after the read
    // the storage is treated as uninitialized and freed exactly once.
    let mut event = unsafe { std::ptr::read(event_) };
    // event.deinit();
    event.deinit();
    // const alloc = event.alloc; alloc.destroy(event);
    let alloc = event.alloc;
    drop(event);
    // SAFETY: `event_` was allocated by `event_new` through this same
    // allocator with this layout.
    unsafe {
        ApiAllocator::deallocate(
            &alloc,
            std::ptr::NonNull::new_unchecked(event_.cast()),
            Layout::new::<EventWrapper>(),
        )
    };
}

// pub fn reset(gesture_: Gesture, terminal: terminal_c.Terminal) ... void {
//     const wrapper = gesture_ orelse return;
//     const t = terminal_c.zigTerminal(terminal) orelse return;
//     wrapper.gesture.reset(t);
// }
/// # Safety
/// `gesture_`/`terminal`, if non-null, must be live handles.
pub unsafe fn reset(gesture_: Gesture, terminal: terminal_c::Terminal) {
    // const wrapper = gesture_ orelse return;
    if gesture_.is_null() {
        return;
    }
    // SAFETY: `gesture_` is a live wrapper (caller contract).
    let wrapper = unsafe { &mut *gesture_ };
    // const t = terminal_c.zigTerminal(terminal) orelse return;
    // SAFETY: `terminal` is null or a live wrapper; `zig_terminal` raw-ptr reads.
    let t = unsafe { terminal_c::zig_terminal(terminal) };
    if t.is_null() {
        return;
    }
    // wrapper.gesture.reset(t);
    // SAFETY: independent allocations — no aliasing of the two `&mut`s.
    let t_ref = unsafe { &mut *t };
    wrapper.gesture.reset(t_ref);
}

// pub fn handle_event(gesture_, terminal, event_, out_selection) ... Result {
//     const wrapper = gesture_ orelse return .invalid_value;
//     const t = terminal_c.zigTerminal(terminal) orelse return .invalid_value;
//     const event_wrapper = event_ orelse return .invalid_value;
//     return switch (event_wrapper.event) { ... };
// }
/// # Safety
/// `gesture_`/`terminal`/`event_`, if non-null, must be live handles;
/// `out_selection`, if non-null, must be a valid writable pointer to a
/// `CSelection`.
pub unsafe fn handle_event(
    gesture_: Gesture,
    terminal: terminal_c::Terminal,
    event_: Event,
    out_selection: *mut selection_c::CSelection,
) -> Result {
    // const wrapper = gesture_ orelse return .invalid_value;
    if gesture_.is_null() {
        return Result::InvalidValue;
    }
    // SAFETY: `gesture_` is a live wrapper (caller contract).
    let wrapper = unsafe { &mut *gesture_ };
    // const t = terminal_c.zigTerminal(terminal) orelse return .invalid_value;
    // SAFETY: `terminal` is null or live; `zig_terminal` raw-ptr reads.
    let t = unsafe { terminal_c::zig_terminal(terminal) };
    if t.is_null() {
        return Result::InvalidValue;
    }
    // SAFETY: independent allocations — gesture/terminal/event don't alias.
    let t_ref = unsafe { &mut *t };
    // const event_wrapper = event_ orelse return .invalid_value;
    if event_.is_null() {
        return Result::InvalidValue;
    }
    // SAFETY: `event_` is a live `event_new`-returned wrapper.
    let event_wrapper = unsafe { &*event_ };

    // return switch (event_wrapper.event) { ... };
    match &event_wrapper.event {
        // .press => |press| {
        //     if (!event_wrapper.event_validation.press_pin_set) return .invalid_value;
        //     const sel = wrapper.gesture.press(t, press) catch return .out_of_memory;
        //     if (out_selection) |out| out.* = selection_c.CSelection.fromZig(sel orelse return .no_value);
        //     else if (sel == null) return .no_value;
        //     return .success;
        // },
        EventPayload::Press(press) => {
            if !event_wrapper.event_validation.press_pin_set {
                return Result::InvalidValue;
            }
            let payload = event_wrapper.press_payload(press);
            let sel = match wrapper.gesture.press(t_ref, payload) {
                Ok(sel) => sel,
                Err(_oom) => return Result::OutOfMemory,
            };
            // SAFETY: `out_selection` validity is the caller's contract.
            unsafe { handle_selection_result(sel, out_selection) }
        }
        // .release => |release| { wrapper.gesture.release(t, release); return .no_value; },
        EventPayload::Release(release) => {
            // PORT NOTE: `Release` is not `Copy` (`pin: Option<Pin>`); reconstruct
            // it by value to match the reference's by-value `release` argument.
            wrapper
                .gesture
                .release(t_ref, sg::Release { pin: release.pin });
            Result::NoValue
        }
        // .drag => |drag| {
        //     if (!event_wrapper.event_validation.drag_pin_set) return .invalid_value;
        //     if (!event_wrapper.event_validation.drag_geometry_set) return .invalid_value;
        //     const sel = wrapper.gesture.drag(t, drag);
        //     ...
        // },
        EventPayload::Drag(drag) => {
            if !event_wrapper.event_validation.drag_pin_set {
                return Result::InvalidValue;
            }
            if !event_wrapper.event_validation.drag_geometry_set {
                return Result::InvalidValue;
            }
            let payload = event_wrapper.drag_payload(drag);
            let sel = wrapper.gesture.drag(t_ref, payload);
            // SAFETY: `out_selection` validity is the caller's contract.
            unsafe { handle_selection_result(sel, out_selection) }
        }
        // .autoscroll_tick => |tick| {
        //     if (!event_wrapper.event_validation.autoscroll_tick_viewport_set) return .invalid_value;
        //     if (!event_wrapper.event_validation.autoscroll_tick_geometry_set) return .invalid_value;
        //     const sel = wrapper.gesture.autoscrollTick(t, tick);
        //     ...
        // },
        EventPayload::AutoscrollTick(tick) => {
            if !event_wrapper.event_validation.autoscroll_tick_viewport_set {
                return Result::InvalidValue;
            }
            if !event_wrapper.event_validation.autoscroll_tick_geometry_set {
                return Result::InvalidValue;
            }
            let payload = event_wrapper.autoscroll_tick_payload(tick);
            let sel = wrapper.gesture.autoscroll_tick(t_ref, payload);
            // SAFETY: `out_selection` validity is the caller's contract.
            unsafe { handle_selection_result(sel, out_selection) }
        }
        // .deep_press => |deep_press| {
        //     const sel = wrapper.gesture.deepPress(t, deep_press);
        //     ...
        // },
        EventPayload::DeepPress(deep_press) => {
            let payload = event_wrapper.deep_press_payload(deep_press);
            let sel = wrapper.gesture.deep_press(t_ref, payload);
            // SAFETY: `out_selection` validity is the caller's contract.
            unsafe { handle_selection_result(sel, out_selection) }
        }
    }
}

// PORT NOTE: the shared tail of each `handle_event` arm (press/drag/autoscroll/
// deep_press all share it verbatim):
//     if (out_selection) |out| out.* = selection_c.CSelection.fromZig(sel orelse return .no_value);
//     else if (sel == null) return .no_value;
//     return .success;
/// # Safety
/// `out_selection`, if non-null, must be a valid writable `CSelection`.
unsafe fn handle_selection_result(
    sel: Option<rustty_vt::selection::Selection>,
    out_selection: *mut selection_c::CSelection,
) -> Result {
    // if (out_selection) |out| out.* = selection_c.CSelection.fromZig(sel orelse return .no_value);
    if !out_selection.is_null() {
        let s = match sel {
            Some(s) => s,
            None => return Result::NoValue,
        };
        // SAFETY: `out_selection` is a non-null valid writable `CSelection`.
        unsafe { *out_selection = selection_c::CSelection::from_zig(s) };
    } else if sel.is_none() {
        // else if (sel == null) return .no_value;
        return Result::NoValue;
    }
    // return .success;
    Result::Success
}

// pub fn event_set(event_: Event, option: EventOption, value: ?*const anyopaque) ... Result {
//     if (comptime std.debug.runtime_safety) { _ = intToEnum(EventOption, ...) catch { log.warn(...); return .invalid_value; } }
//     return switch (option) {
//         inline else => |comptime_option| eventSetTyped(event_, comptime_option, if (value) |ptr| @ptrCast(@alignCast(ptr)) else null),
//     };
// }
//
// PORT NOTE: the `intToEnum(EventOption, ...)` guard lives in the `extern "C"`
// wrapper in `lib.rs`.
/// # Safety
/// `value`, if non-null, must point to a value of the type required by `option`.
pub unsafe fn event_set(event_: Event, option: EventOption, value: *const c_void) -> Result {
    // SAFETY: `value` validity/type is the caller's contract per `option`.
    unsafe { event_set_typed(event_, option, value) }
}

// pub fn get(gesture_, terminal, data, out) ... Result {
//     if (comptime std.debug.runtime_safety) { _ = intToEnum(Data, ...) catch { log.warn(...); return .invalid_value; } }
//     const out_ptr = out orelse return .invalid_value;
//     return switch (data) { inline else => |comptime_data| getTyped(gesture_, terminal, comptime_data, @ptrCast(@alignCast(out_ptr))) };
// }
//
// PORT NOTE (W66-GETMULTI-RAW-01): the `intToEnum(Data, ...)` guard + its
// `log.warn` live in `get_checked` below, on the raw `c_int`, so that
// `get_multi` reaches them PER ARRAY ELEMENT — exactly as the reference's
// `get_multi` reaches the guard by calling `get`. (`get` itself keeps the typed
// `Data`, which by construction cannot be out of range.)
/// # Safety
/// `out` must point to writable storage of the type required by `data`.
pub unsafe fn get(
    gesture_: Gesture,
    terminal: terminal_c::Terminal,
    data: Data,
    out: *mut c_void,
) -> Result {
    // const out_ptr = out orelse return .invalid_value;
    if out.is_null() {
        return Result::InvalidValue;
    }
    // SAFETY: `out` validity/type is the caller's contract per `data`.
    unsafe { get_typed(gesture_, terminal, data, out) }
}

// The raw-`c_int` half of `get`: the reference's
// `if (comptime std.debug.runtime_safety) { _ = intToEnum(Data, ...) catch {
//     log.warn("selection_gesture_get invalid data value={d}", ...);
//     return .invalid_value; } }`
// (`selection_gesture.zig:361-373`). Every C entry point that carries a
// caller-supplied data selector — the `rustty_selection_gesture_get` wrapper
// AND each element of `get_multi`'s key array — goes through here.
/// # Safety
/// `out` must point to writable storage of the type required by `data`.
pub unsafe fn get_checked(
    gesture_: Gesture,
    terminal: terminal_c::Terminal,
    data: c_int,
    out: *mut c_void,
) -> Result {
    let Some(d) = Data::from_c_int(data) else {
        // log.warn("selection_gesture_get invalid data value={d}", .{...});
        crate::sys::log_fn(
            crate::sys::StdLevel::Warn,
            LOG_SCOPE,
            format_args!("selection_gesture_get invalid data value={data}"),
        );
        return Result::InvalidValue;
    };
    // SAFETY: `out` validity/type is the caller's contract per `data`.
    unsafe { get(gesture_, terminal, d, out) }
}

// pub fn get_multi(gesture_, terminal, count, keys, values, out_written) ... Result { ... }
/// # Safety
/// `keys`/`values`/`out_written` validity is the caller's contract.
// PORT NOTE (W66-GETMULTI-RAW-01): `keys` is C memory typed
// `const RusttySelectionGestureData *` in the published header; each element is
// read as a raw `c_int` (ABI-identical — `Data` is `#[repr(C)]`, asserted
// int-sized above) and range-checked by `get_checked`, mirroring the
// reference's per-element `get` guard (first failing index reported through
// `out_written`). Reading an element straight into a `Data` would materialize
// an out-of-range discriminant, which is UB in Rust in EVERY build mode.
pub unsafe fn get_multi(
    gesture_: Gesture,
    terminal: terminal_c::Terminal,
    count: usize,
    keys: *const c_int,
    values: *mut *mut c_void,
    out_written: *mut usize,
) -> Result {
    // const k = keys orelse return .invalid_value;
    if keys.is_null() {
        return Result::InvalidValue;
    }
    // const v = values orelse return .invalid_value;
    if values.is_null() {
        return Result::InvalidValue;
    }

    for i in 0..count {
        // SAFETY: caller guarantees `keys`/`values` have `count` elements.
        let key = unsafe { *keys.add(i) };
        let value = unsafe { *values.add(i) };
        // const result = get(gesture_, terminal, k[i], v[i]);
        let result = unsafe { get_checked(gesture_, terminal, key, value) };
        if result != Result::Success {
            if !out_written.is_null() {
                // SAFETY: caller-provided out pointer, checked non-null.
                unsafe { *out_written = i };
            }
            return result;
        }
    }
    if !out_written.is_null() {
        // SAFETY: caller-provided out pointer, checked non-null.
        unsafe { *out_written = count };
    }
    Result::Success
}

// fn getTyped(gesture_, terminal, comptime data, out: *data.OutType()) Result {
//     const wrapper = gesture_ orelse return .invalid_value;
//     const t = terminal_c.zigTerminal(terminal) orelse return .invalid_value;
//     switch (data) { ... }
//     return .success;
// }
/// # Safety
/// `out` must point to writable storage of the type required by `data`.
unsafe fn get_typed(
    gesture_: Gesture,
    terminal: terminal_c::Terminal,
    data: Data,
    out: *mut c_void,
) -> Result {
    // const wrapper = gesture_ orelse return .invalid_value;
    if gesture_.is_null() {
        return Result::InvalidValue;
    }
    // SAFETY: `gesture_` is a live wrapper (caller contract).
    let wrapper = unsafe { &*gesture_ };
    // const t = terminal_c.zigTerminal(terminal) orelse return .invalid_value;
    // SAFETY: `terminal` is null or live; `zig_terminal` raw-ptr reads.
    let t = unsafe { terminal_c::zig_terminal(terminal) };
    if t.is_null() {
        return Result::InvalidValue;
    }

    match data {
        // .click_count => out.* = wrapper.gesture.left_click_count,
        Data::ClickCount => {
            // SAFETY: `out` points to a writable `u8`.
            unsafe { *(out as *mut u8) = wrapper.gesture.left_click_count };
        }
        // .dragged => out.* = wrapper.gesture.left_click_dragged,
        Data::Dragged => {
            // SAFETY: `out` points to a writable `bool`.
            unsafe { *(out as *mut bool) = wrapper.gesture.left_click_dragged };
        }
        // .autoscroll => out.* = wrapper.gesture.left_drag_autoscroll,
        Data::Autoscroll => {
            // SAFETY: `out` points to a writable `Autoscroll`.
            unsafe {
                *(out as *mut Autoscroll) =
                    Autoscroll::from_zig(wrapper.gesture.left_drag_autoscroll)
            };
        }
        // .behavior => out.* = wrapper.gesture.left_click_behavior,
        Data::Behavior => {
            // SAFETY: `out` points to a writable `Behavior`.
            unsafe {
                *(out as *mut Behavior) = Behavior::from_zig(wrapper.gesture.left_click_behavior)
            };
        }
        // .anchor => {
        //     const pin = wrapper.gesture.validatedLeftClickPin(&t.screens) orelse return .no_value;
        //     out.* = .fromPin(pin.*);
        // },
        Data::Anchor => {
            // SAFETY: `t` is a live `ZigTerminal`; raw-ptr read of `screens`.
            let screens = unsafe { &(*t).screens };
            let pin = match wrapper.gesture.validated_left_click_pin(screens) {
                Some(pin) => pin,
                None => return Result::NoValue,
            };
            // out.* = .fromPin(pin.*);
            // SAFETY: `pin` is a valid tracked-pin pointer; `out` is a writable
            // `CGridRef`.
            unsafe { *(out as *mut grid_ref::CGridRef) = grid_ref::CGridRef::from_pin(*pin) };
        }
    }

    Result::Success
}

// fn eventSetTyped(event_, comptime option, value: ?*const option.Type()) Result {
//     const event = event_ orelse return .invalid_value;
//     return switch (event.event) {
//         .press => |*press| pressSetTyped(event, press, option, value),
//         .release => |*release| releaseSetTyped(release, option, value),
//         .drag => |*drag| dragSetTyped(event, drag, option, value),
//         .autoscroll_tick => |*tick| autoscrollTickSetTyped(event, tick, option, value),
//         .deep_press => |*deep_press| deepPressSetTyped(event, deep_press, option, value),
//     };
// }
/// # Safety
/// `value`, if non-null, must point to a value of the type required by `option`.
unsafe fn event_set_typed(event_: Event, option: EventOption, value: *const c_void) -> Result {
    // const event = event_ orelse return .invalid_value;
    if event_.is_null() {
        return Result::InvalidValue;
    }
    // PORT NOTE: the reference matches on `event.event` by `*` reference and the
    // per-payload setters take `event` + the payload field pointer. In Rust the
    // payload and the rest of `EventWrapper` (`behaviors`/codepoints/validation)
    // are mutated together, so the setters take `&mut EventWrapper` and access
    // the active payload variant directly (asserting the variant they were
    // dispatched for). We dispatch on the variant first to choose the setter.
    // SAFETY: `event_` is a live wrapper (caller contract).
    let variant = {
        let event = unsafe { &*event_ };
        std::mem::discriminant(&event.event)
    };
    // SAFETY: `event_` is a live wrapper; single `&mut` for the whole call.
    let event = unsafe { &mut *event_ };
    let _ = variant;
    match &event.event {
        EventPayload::Press(_) => unsafe { press_set_typed(event, option, value) },
        EventPayload::Release(_) => unsafe { release_set_typed(event, option, value) },
        EventPayload::Drag(_) => unsafe { drag_set_typed(event, option, value) },
        EventPayload::AutoscrollTick(_) => unsafe {
            autoscroll_tick_set_typed(event, option, value)
        },
        EventPayload::DeepPress(_) => unsafe { deep_press_set_typed(event, option, value) },
    }
}

// fn pressSetTyped(event, press, comptime option, value: ?*const option.Type()) Result { ... }
/// # Safety
/// `value`, if non-null, must point to a value of the type required by `option`.
unsafe fn press_set_typed(
    event: &mut EventWrapper,
    option: EventOption,
    value: *const c_void,
) -> Result {
    // const v = value orelse { switch (option) { ... clear ... } return .success; };
    if value.is_null() {
        match option {
            // .ref => event.event_validation.press_pin_set = false,
            EventOption::Ref => event.event_validation.press_pin_set = false,
            // .position => { press.xpos = 0; press.ypos = 0; },
            EventOption::Position => {
                if let EventPayload::Press(press) = &mut event.event {
                    press.xpos = 0.0;
                    press.ypos = 0.0;
                }
            }
            // .repeat_distance => press.max_distance = 0,
            EventOption::RepeatDistance => {
                if let EventPayload::Press(press) = &mut event.event {
                    press.max_distance = 0.0;
                }
            }
            // .time_ns => press.time = null,
            EventOption::TimeNs => {
                if let EventPayload::Press(press) = &mut event.event {
                    press.time = None;
                }
            }
            // .repeat_interval_ns => press.repeat_interval = 0,
            EventOption::RepeatIntervalNs => {
                if let EventPayload::Press(press) = &mut event.event {
                    press.repeat_interval = 0;
                }
            }
            // .word_boundary_codepoints => clearWordBoundaryCodepoints(event, &press.word_boundary_codepoints),
            // PORT NOTE: the press payload's borrowed slice is rebuilt at dispatch,
            // so clearing the event-owned storage is the whole operation.
            EventOption::WordBoundaryCodepoints => clear_word_boundary_codepoints(event),
            // .behaviors => { event.behaviors = SelectionGesture.default_behaviors; press.behaviors = &event.behaviors; },
            EventOption::Behaviors => {
                event.behaviors = sg::DEFAULT_BEHAVIORS;
            }
            // .rectangle, .geometry, .viewport => return .invalid_value,
            EventOption::Rectangle | EventOption::Geometry | EventOption::Viewport => {
                return Result::InvalidValue
            }
        }
        return Result::Success;
    }

    match option {
        // .ref => { press.pin = v.toPin() orelse return .invalid_value; event.event_validation.press_pin_set = true; },
        EventOption::Ref => {
            // SAFETY: `value` points to a readable `CGridRef`.
            let v = unsafe { &*(value as *const grid_ref::CGridRef) };
            let pin = match v.to_pin() {
                Some(pin) => pin,
                None => return Result::InvalidValue,
            };
            if let EventPayload::Press(press) = &mut event.event {
                press.pin = pin;
            }
            event.event_validation.press_pin_set = true;
        }
        // .position => { press.xpos = v.x; press.ypos = v.y; },
        EventOption::Position => {
            // SAFETY: `value` points to a readable `SurfacePosition`.
            let v = unsafe { &*(value as *const SurfacePosition) };
            if let EventPayload::Press(press) = &mut event.event {
                press.xpos = v.x;
                press.ypos = v.y;
            }
        }
        // .repeat_distance => press.max_distance = v.*,
        EventOption::RepeatDistance => {
            // SAFETY: `value` points to a readable `f64`.
            let v = unsafe { *(value as *const f64) };
            if let EventPayload::Press(press) = &mut event.event {
                press.max_distance = v;
            }
        }
        // .time_ns => press.time = instantFromNs(v.*),
        EventOption::TimeNs => {
            // SAFETY: `value` points to a readable `u64`.
            let v = unsafe { *(value as *const u64) };
            if let EventPayload::Press(press) = &mut event.event {
                press.time = Some(instant_from_ns(v));
            }
        }
        // .repeat_interval_ns => press.repeat_interval = v.*,
        EventOption::RepeatIntervalNs => {
            // SAFETY: `value` points to a readable `u64`.
            let v = unsafe { *(value as *const u64) };
            if let EventPayload::Press(press) = &mut event.event {
                press.repeat_interval = v;
            }
        }
        // .word_boundary_codepoints => return trySetWordBoundaryCodepoints(event, &press.word_boundary_codepoints, v),
        EventOption::WordBoundaryCodepoints => {
            // SAFETY: `value` points to a readable `Codepoints`.
            let v = unsafe { &*(value as *const Codepoints) };
            return try_set_word_boundary_codepoints(event, v);
        }
        // .behaviors => {
        //     if (!validBehavior(v.single_click) or !validBehavior(v.double_click) or !validBehavior(v.triple_click)) return .invalid_value;
        //     event.behaviors = .{ v.single_click, v.double_click, v.triple_click };
        //     press.behaviors = &event.behaviors;
        // },
        EventOption::Behaviors => {
            // SAFETY: `value` points to a readable `Behaviors`, whose three C
            // enum fields are read as their raw `c_int` storage (see
            // `BehaviorsRaw`) so the range check below can reject out-of-range
            // values without materializing an invalid discriminant.
            let v = unsafe { &*(value as *const BehaviorsRaw) };
            let (Some(single_click), Some(double_click), Some(triple_click)) = (
                valid_behavior(v.single_click),
                valid_behavior(v.double_click),
                valid_behavior(v.triple_click),
            ) else {
                return Result::InvalidValue;
            };
            event.behaviors = [
                single_click.to_zig(),
                double_click.to_zig(),
                triple_click.to_zig(),
            ];
        }
        // .rectangle, .geometry, .viewport => return .invalid_value,
        EventOption::Rectangle | EventOption::Geometry | EventOption::Viewport => {
            return Result::InvalidValue
        }
    }

    Result::Success
}

// fn releaseSetTyped(release, comptime option, value: ?*const option.Type()) Result { ... }
/// # Safety
/// `value`, if non-null, must point to a value of the type required by `option`.
unsafe fn release_set_typed(
    event: &mut EventWrapper,
    option: EventOption,
    value: *const c_void,
) -> Result {
    match option {
        // .ref => {
        //     const v = value orelse { release.pin = null; return .success; };
        //     release.pin = v.toPin() orelse return .invalid_value;
        // },
        EventOption::Ref => {
            if value.is_null() {
                if let EventPayload::Release(release) = &mut event.event {
                    release.pin = None;
                }
                return Result::Success;
            }
            // SAFETY: `value` points to a readable `CGridRef`.
            let v = unsafe { &*(value as *const grid_ref::CGridRef) };
            let pin = match v.to_pin() {
                Some(pin) => pin,
                None => return Result::InvalidValue,
            };
            if let EventPayload::Release(release) = &mut event.event {
                release.pin = Some(pin);
            }
        }
        // .position, .repeat_distance, .time_ns, .repeat_interval_ns,
        // .word_boundary_codepoints, .behaviors, .rectangle, .geometry, .viewport => return .invalid_value,
        EventOption::Position
        | EventOption::RepeatDistance
        | EventOption::TimeNs
        | EventOption::RepeatIntervalNs
        | EventOption::WordBoundaryCodepoints
        | EventOption::Behaviors
        | EventOption::Rectangle
        | EventOption::Geometry
        | EventOption::Viewport => return Result::InvalidValue,
    }

    Result::Success
}

// fn dragSetTyped(event, drag, comptime option, value: ?*const option.Type()) Result { ... }
/// # Safety
/// `value`, if non-null, must point to a value of the type required by `option`.
unsafe fn drag_set_typed(
    event: &mut EventWrapper,
    option: EventOption,
    value: *const c_void,
) -> Result {
    // const v = value orelse { switch (option) { ... clear ... } return .success; };
    if value.is_null() {
        match option {
            // .ref => event.event_validation.drag_pin_set = false,
            EventOption::Ref => event.event_validation.drag_pin_set = false,
            // .position => { drag.xpos = 0; drag.ypos = 0; },
            EventOption::Position => {
                if let EventPayload::Drag(drag) = &mut event.event {
                    drag.xpos = 0.0;
                    drag.ypos = 0.0;
                }
            }
            // .word_boundary_codepoints => clearWordBoundaryCodepoints(event, &drag.word_boundary_codepoints),
            EventOption::WordBoundaryCodepoints => clear_word_boundary_codepoints(event),
            // .rectangle => drag.rectangle = false,
            EventOption::Rectangle => {
                if let EventPayload::Drag(drag) = &mut event.event {
                    drag.rectangle = false;
                }
            }
            // .geometry => event.event_validation.drag_geometry_set = false,
            EventOption::Geometry => event.event_validation.drag_geometry_set = false,
            // .viewport => return .invalid_value,
            EventOption::Viewport => return Result::InvalidValue,
            // .repeat_distance, .time_ns, .repeat_interval_ns, .behaviors => return .invalid_value,
            EventOption::RepeatDistance
            | EventOption::TimeNs
            | EventOption::RepeatIntervalNs
            | EventOption::Behaviors => return Result::InvalidValue,
        }
        return Result::Success;
    }

    match option {
        // .ref => { drag.pin = v.toPin() orelse return .invalid_value; event.event_validation.drag_pin_set = true; },
        EventOption::Ref => {
            // SAFETY: `value` points to a readable `CGridRef`.
            let v = unsafe { &*(value as *const grid_ref::CGridRef) };
            let pin = match v.to_pin() {
                Some(pin) => pin,
                None => return Result::InvalidValue,
            };
            if let EventPayload::Drag(drag) = &mut event.event {
                drag.pin = pin;
            }
            event.event_validation.drag_pin_set = true;
        }
        // .position => { drag.xpos = v.x; drag.ypos = v.y; },
        EventOption::Position => {
            // SAFETY: `value` points to a readable `SurfacePosition`.
            let v = unsafe { &*(value as *const SurfacePosition) };
            if let EventPayload::Drag(drag) = &mut event.event {
                drag.xpos = v.x;
                drag.ypos = v.y;
            }
        }
        // .word_boundary_codepoints => return trySetWordBoundaryCodepoints(event, &drag.word_boundary_codepoints, v),
        EventOption::WordBoundaryCodepoints => {
            // SAFETY: `value` points to a readable `Codepoints`.
            let v = unsafe { &*(value as *const Codepoints) };
            return try_set_word_boundary_codepoints(event, v);
        }
        // .rectangle => drag.rectangle = v.*,
        EventOption::Rectangle => {
            // SAFETY: `value` points to a readable `bool`.
            let v = unsafe { *(value as *const bool) };
            if let EventPayload::Drag(drag) = &mut event.event {
                drag.rectangle = v;
            }
        }
        // .geometry => { drag.geometry = v.toZig() orelse return .invalid_value; event.event_validation.drag_geometry_set = true; },
        EventOption::Geometry => {
            // SAFETY: `value` points to a readable `Geometry`.
            let v = unsafe { &*(value as *const Geometry) };
            let geom = match v.to_zig() {
                Some(geom) => geom,
                None => return Result::InvalidValue,
            };
            if let EventPayload::Drag(drag) = &mut event.event {
                drag.geometry = geom;
            }
            event.event_validation.drag_geometry_set = true;
        }
        // .viewport => return .invalid_value,
        EventOption::Viewport => return Result::InvalidValue,
        // .repeat_distance, .time_ns, .repeat_interval_ns, .behaviors => return .invalid_value,
        EventOption::RepeatDistance
        | EventOption::TimeNs
        | EventOption::RepeatIntervalNs
        | EventOption::Behaviors => return Result::InvalidValue,
    }

    Result::Success
}

// fn autoscrollTickSetTyped(event, tick, comptime option, value: ?*const option.Type()) Result { ... }
/// # Safety
/// `value`, if non-null, must point to a value of the type required by `option`.
unsafe fn autoscroll_tick_set_typed(
    event: &mut EventWrapper,
    option: EventOption,
    value: *const c_void,
) -> Result {
    // const v = value orelse { switch (option) { ... clear ... } return .success; };
    if value.is_null() {
        match option {
            // .viewport => event.event_validation.autoscroll_tick_viewport_set = false,
            EventOption::Viewport => event.event_validation.autoscroll_tick_viewport_set = false,
            // .position => { tick.xpos = 0; tick.ypos = 0; },
            EventOption::Position => {
                if let EventPayload::AutoscrollTick(tick) = &mut event.event {
                    tick.xpos = 0.0;
                    tick.ypos = 0.0;
                }
            }
            // .word_boundary_codepoints => clearWordBoundaryCodepoints(event, &tick.word_boundary_codepoints),
            EventOption::WordBoundaryCodepoints => clear_word_boundary_codepoints(event),
            // .rectangle => tick.rectangle = false,
            EventOption::Rectangle => {
                if let EventPayload::AutoscrollTick(tick) = &mut event.event {
                    tick.rectangle = false;
                }
            }
            // .geometry => event.event_validation.autoscroll_tick_geometry_set = false,
            EventOption::Geometry => event.event_validation.autoscroll_tick_geometry_set = false,
            // .ref, .repeat_distance, .time_ns, .repeat_interval_ns, .behaviors => return .invalid_value,
            EventOption::Ref
            | EventOption::RepeatDistance
            | EventOption::TimeNs
            | EventOption::RepeatIntervalNs
            | EventOption::Behaviors => return Result::InvalidValue,
        }
        return Result::Success;
    }

    match option {
        // .viewport => { tick.viewport = v.*; event.event_validation.autoscroll_tick_viewport_set = true; },
        EventOption::Viewport => {
            // SAFETY: `value` points to a readable `point.Coordinate`.
            let v = unsafe { *(value as *const Coordinate) };
            if let EventPayload::AutoscrollTick(tick) = &mut event.event {
                tick.viewport = v;
            }
            event.event_validation.autoscroll_tick_viewport_set = true;
        }
        // .position => { tick.xpos = v.x; tick.ypos = v.y; },
        EventOption::Position => {
            // SAFETY: `value` points to a readable `SurfacePosition`.
            let v = unsafe { &*(value as *const SurfacePosition) };
            if let EventPayload::AutoscrollTick(tick) = &mut event.event {
                tick.xpos = v.x;
                tick.ypos = v.y;
            }
        }
        // .word_boundary_codepoints => return trySetWordBoundaryCodepoints(event, &tick.word_boundary_codepoints, v),
        EventOption::WordBoundaryCodepoints => {
            // SAFETY: `value` points to a readable `Codepoints`.
            let v = unsafe { &*(value as *const Codepoints) };
            return try_set_word_boundary_codepoints(event, v);
        }
        // .rectangle => tick.rectangle = v.*,
        EventOption::Rectangle => {
            // SAFETY: `value` points to a readable `bool`.
            let v = unsafe { *(value as *const bool) };
            if let EventPayload::AutoscrollTick(tick) = &mut event.event {
                tick.rectangle = v;
            }
        }
        // .geometry => { tick.geometry = v.toZig() orelse return .invalid_value; event.event_validation.autoscroll_tick_geometry_set = true; },
        EventOption::Geometry => {
            // SAFETY: `value` points to a readable `Geometry`.
            let v = unsafe { &*(value as *const Geometry) };
            let geom = match v.to_zig() {
                Some(geom) => geom,
                None => return Result::InvalidValue,
            };
            if let EventPayload::AutoscrollTick(tick) = &mut event.event {
                tick.geometry = geom;
            }
            event.event_validation.autoscroll_tick_geometry_set = true;
        }
        // .ref, .repeat_distance, .time_ns, .repeat_interval_ns, .behaviors => return .invalid_value,
        EventOption::Ref
        | EventOption::RepeatDistance
        | EventOption::TimeNs
        | EventOption::RepeatIntervalNs
        | EventOption::Behaviors => return Result::InvalidValue,
    }

    Result::Success
}

// fn deepPressSetTyped(event, deep_press, comptime option, value: ?*const option.Type()) Result { ... }
/// # Safety
/// `value`, if non-null, must point to a value of the type required by `option`.
unsafe fn deep_press_set_typed(
    event: &mut EventWrapper,
    option: EventOption,
    value: *const c_void,
) -> Result {
    // const v = value orelse { switch (option) { ... clear ... } return .success; };
    if value.is_null() {
        match option {
            // .word_boundary_codepoints => clearWordBoundaryCodepoints(event, &deep_press.word_boundary_codepoints),
            EventOption::WordBoundaryCodepoints => clear_word_boundary_codepoints(event),
            // .ref, .position, .repeat_distance, .time_ns, .repeat_interval_ns,
            // .behaviors, .rectangle, .geometry, .viewport => return .invalid_value,
            EventOption::Ref
            | EventOption::Position
            | EventOption::RepeatDistance
            | EventOption::TimeNs
            | EventOption::RepeatIntervalNs
            | EventOption::Behaviors
            | EventOption::Rectangle
            | EventOption::Geometry
            | EventOption::Viewport => return Result::InvalidValue,
        }
        return Result::Success;
    }

    match option {
        // .word_boundary_codepoints => return trySetWordBoundaryCodepoints(event, &deep_press.word_boundary_codepoints, v),
        EventOption::WordBoundaryCodepoints => {
            // SAFETY: `value` points to a readable `Codepoints`.
            let v = unsafe { &*(value as *const Codepoints) };
            try_set_word_boundary_codepoints(event, v)
        }
        // .ref, .position, .repeat_distance, .time_ns, .repeat_interval_ns,
        // .behaviors, .rectangle, .geometry, .viewport => return .invalid_value,
        EventOption::Ref
        | EventOption::Position
        | EventOption::RepeatDistance
        | EventOption::TimeNs
        | EventOption::RepeatIntervalNs
        | EventOption::Behaviors
        | EventOption::Rectangle
        | EventOption::Geometry
        | EventOption::Viewport => Result::InvalidValue,
    }
}

// fn trySetWordBoundaryCodepoints(event, target: *[]const u21, value: *const types.Codepoints) Result {
//     if (value.len > 0 and value.ptr == null) return .invalid_value;
//     clearWordBoundaryCodepoints(event, target);
//     const ptr = value.ptr orelse {
//         event.word_boundary_codepoints = &.{};
//         target.* = event.word_boundary_codepoints.?;
//         return .success;
//     };
//     const copy = event.alloc.alloc(u21, value.len) catch return .out_of_memory;
//     errdefer event.alloc.free(copy);
//     for (copy, ptr[0..value.len]) |*dst, cp| dst.* = std.math.cast(u21, cp) orelse return .invalid_value;
//     event.word_boundary_codepoints = copy;
//     target.* = copy;
//     return .success;
// }
//
// PORT NOTE: `target` (the payload's borrowed slice) is rebuilt at dispatch from
// `event.word_boundary_codepoints`, so the `target.* = ...` writes are implicit
// here. The `std.math.cast(u21, cp)` (codepoint must fit in 21 bits) is the
// `u32 -> u21` narrowing check.
fn try_set_word_boundary_codepoints(event: &mut EventWrapper, value: &Codepoints) -> Result {
    // if (value.len > 0 and value.ptr == null) return .invalid_value;
    if value.len > 0 && value.ptr.is_null() {
        return Result::InvalidValue;
    }
    // clearWordBoundaryCodepoints(event, target);
    clear_word_boundary_codepoints(event);
    // const ptr = value.ptr orelse { event.word_boundary_codepoints = &.{}; target.* = ...; return .success; };
    if value.ptr.is_null() {
        // PORT NOTE: `&.{}` (empty slice) becomes an empty list that owns no
        // allocation.
        event.word_boundary_codepoints = Some(AVec::new_in(event.alloc));
        return Result::Success;
    }
    // const copy = event.alloc.alloc(u21, value.len) catch return .out_of_memory;
    // PORT NOTE (parity tail S7-8): one exact `value.len * 4`-byte request
    // through the event's allocator; a zero `len` makes no call (Zig's
    // `alloc(u21, 0)` returns an empty slice without calling the vtable).
    let mut copy: AVec<u32> = AVec::new_in(event.alloc);
    if copy.try_reserve_exact(value.len).is_err() {
        return Result::OutOfMemory;
    }
    // errdefer event.alloc.free(copy);
    // for (copy, ptr[0..value.len]) |*dst, cp| dst.* = std.math.cast(u21, cp) orelse return .invalid_value;
    // SAFETY: `value.ptr` is non-null with `value.len` readable `u32`s (caller).
    let src = unsafe { std::slice::from_raw_parts(value.ptr, value.len) };
    for &cp in src {
        // std.math.cast(u21, cp): reject codepoints that don't fit in 21 bits.
        if cp > 0x1F_FFFF {
            // PORT NOTE: reference defect (R13, UD314). The reference returns
            // `Result`, not an error union, so `return .invalid_value` does
            // not run the `errdefer` and the reference leaks `copy` (reachable
            // from the C API with any codepoint above U+1FFFFF). Returning
            // here drops `copy`, which frees it through `event.alloc`.
            return Result::InvalidValue;
        }
        // Capacity for `value.len` elements was reserved above: no growth.
        copy.push(cp);
    }
    // event.word_boundary_codepoints = copy; target.* = copy;
    event.word_boundary_codepoints = Some(copy);
    Result::Success
}

// fn clearWordBoundaryCodepoints(event: *EventWrapper, target: *[]const u21) void {
//     if (event.word_boundary_codepoints) |cps| { if (cps.len > 0) event.alloc.free(cps); }
//     event.word_boundary_codepoints = null;
//     target.* = &selection_codepoints.default_word_boundaries;
// }
//
// PORT NOTE: `target.* = &default_word_boundaries` is implicit — the payload's
// borrowed slice falls back to the module default when
// `event.word_boundary_codepoints` is `None` (see `EventWrapper::word_boundaries`).
fn clear_word_boundary_codepoints(event: &mut EventWrapper) {
    // if (event.word_boundary_codepoints) |cps| { if (cps.len > 0) event.alloc.free(cps); }
    // event.word_boundary_codepoints = null;
    let _ = event.word_boundary_codepoints.take();
}

// fn instantFromNs(ns: u64) SelectionGesture.Time {
//     if (comptime builtin.target.cpu.arch == .wasm32 and builtin.target.os.tag == .freestanding) return ns;
//     return switch (builtin.os.tag) {
//         .windows, .uefi, .wasi => .{ .timestamp = ns },
//         else => .{ .timestamp = .{ .sec = @intCast(ns / std.time.ns_per_s), .nsec = @intCast(ns % std.time.ns_per_s) } },
//     };
// }
//
// PORT NOTE: `SelectionGesture.Time = std.time.Instant` (rustty target is never
// freestanding wasm). `std::time::Instant` is opaque and has no public ns
// constructor, so the faithful analogue anchors a process-lifetime base instant
// (captured once) and offsets it by `ns`. `time_since` only ever computes a
// delta between two `instantFromNs` values, so the choice of base cancels and
// the observable behavior (the ns delta) is identical to the reference's
// `timestamp`-based instants.
fn instant_from_ns(ns: u64) -> sg::Time {
    use std::sync::OnceLock;
    static BASE: OnceLock<std::time::Instant> = OnceLock::new();
    let base = *BASE.get_or_init(std::time::Instant::now);
    base + std::time::Duration::from_nanos(ns)
}

// fn validBehavior(behavior: Behavior) bool {
//     _ = std.meta.intToEnum(Behavior, @intFromEnum(behavior)) catch return false;
//     return true;
// }
//
// PORT NOTE (W66-BEHAVIOR-RAW-01): the C `Behavior` is a plain C enum in the
// published header, so the value a C caller stores is an arbitrary `c_int` —
// NOT already a valid discriminant. The guard therefore takes the raw integer
// and narrows it, returning the converted `Behavior` on success and `None`
// (the reference's `false`) outside the declared range.
fn valid_behavior(behavior: c_int) -> Option<Behavior> {
    Behavior::from_c_int(behavior)
}

#[cfg(test)]
mod tests;
