//! rustty-vt selection_gesture
//!
//! PORTS: ghostty src/terminal/SelectionGesture.zig
//!
//! SelectionGesture manages gesture-based terminal text selection for one
//! pointer stream: press, drag, release, autoscroll, and pressure/deep-press
//! selection.
//!
//! This type owns only the state required to interpret a gesture. It does not
//! modify the terminal selection directly, except for scrolling the viewport
//! during `autoscrollTick`. The caller feeds platform events into this type and
//! applies the returned `Selection` to the active screen when appropriate.
//!
//! A typical single-click drag flow looks like this:
//!
//! ```zig
//! const selection = try gesture.press(terminal, .{ ... });
//! try terminal.screens.active.select(selection);
//! if (gesture.drag(terminal, .{ ... })) |selection| {
//!     try terminal.screens.active.select(selection);
//! }
//! gesture.release(terminal, .{ ... });
//! ```
//!
//! Double- and triple-click gestures use the same event flow. Repeated presses
//! inside `Press.repeat_interval` and within `Press.max_distance` increment the
//! internal click count up to three. `Press.behaviors` maps single-, double-,
//! and triple-clicks to behavior. By default, a single press returns null to
//! clear any existing selection, a double-click returns a word selection, and a
//! triple-click returns a line selection. Drags use the behavior selected by the
//! corresponding press. A new press that is too late, too far away, or on
//! another active screen starts a new single-click gesture.
//!
//! # Resetting and lifetime
//!
//! `release` ends the active drag/autoscroll phase but intentionally preserves
//! enough state for a subsequent press to become a double- or triple-click.
//! Call `reset` when the gesture is cancelled rather than released normally, or
//! when another subsystem takes ownership of pointer input. Examples include
//! enabling mouse reporting for an application, losing pointer/button state,
//! destroying the surface, switching to a mode that must not continue text
//! selection, or otherwise abandoning the current click sequence. Call `deinit`
//! once before discarding the gesture object so any tracked click pin is
//! released.
//!
//! # Terminal and screen changes
//!
//! The initial press pin is tracked in the active screen's `PageList`, so normal
//! terminal output and viewport scrolling can move rows without making the
//! gesture immediately stale. Selection results are computed against the current
//! terminal contents at the time of each call. For example, a double-click drag
//! selects word boundaries from the screen as it exists during `drag`, not from a
//! snapshot captured at `press`.
//!
//! The tracked pin is tied to both a `ScreenSet.Key` and that screen's
//! generation. If the active screen changes, or a screen is removed/recycled,
//! `validatedLeftClickPin` returns null and drag-style operations stop producing
//! selections. `autoscrollTick` treats this as cancellation and calls `reset` so
//! callers can stop their timers. This avoids exposing pins from inactive or
//! freed screens, but it does not make a historical snapshot of terminal data.
//!
//! # Concurrency
//!
//! SelectionGesture is not concurrency safe. It has mutable gesture state and
//! mutates/tracks pins inside the terminal page list without taking locks. The
//! caller must serialize all calls that touch the same gesture and terminal,
//! typically by holding the same terminal/renderer mutex used for other screen
//! mutations. Do not call `press`, `drag`, `release`, `reset`, `deinit`, or
//! `autoscrollTick` concurrently with each other or with unrelated terminal
//! mutations unless the caller provides that synchronization.

// const SelectionGesture = @This();

// const std = @import("std");
// const builtin = @import("builtin");
// const assert = std.debug.assert;
// const testing = std.testing;
// const Allocator = std.mem.Allocator;
// const lib = @import("lib.zig");
// const PageList = @import("PageList.zig");
// const Pin = PageList.Pin;
// const Screen = @import("Screen.zig");
// const ScreenSet = @import("ScreenSet.zig");
// const Selection = @import("Selection.zig");
// const Terminal = @import("Terminal.zig");
// const point = @import("point.zig");
use crate::pagelist::Pin;
use crate::point;
use crate::screen::{Screen, SelectLine};
use crate::screen_set::{Key, ScreenSet};
use crate::selection::Selection;
use crate::terminal::{ScrollViewport, Terminal};

// const freestanding_wasm = builtin.target.cpu.arch == .wasm32 and
//     builtin.target.os.tag == .freestanding;

/// Monotonic timestamp type for click-repeat detection.
///
/// Freestanding wasm cannot reference std.time.Instant because Zig's stdlib
/// Instant type depends on POSIX timespec for that target, so represent the C
/// API nanosecond timestamp directly as a u64 there.
// PORT NOTE: `Time = if (freestanding_wasm) u64 else std.time.Instant`. The
// Rustty target is never freestanding wasm, so `Time` is the Rust monotonic
// clock `std::time::Instant`, the faithful analogue of `std.time.Instant`.
pub type Time = std::time::Instant;

/// The direction that selection dragging should autoscroll the viewport.
/// This is derived from the most recent drag position relative to the
/// surface bounds and reset whenever there is no active drag gesture.
///
/// When autoscroll is non-none, the caller should setup a timer
/// to periodically call autoscrollTick. The timer interval is up to the
/// caller but reasonable defaults are approximately every 15 milliseconds.
///
/// This is used to implement selection above/below the viewport that
/// wants to drag the viewport.
// PORT NOTE: `Autoscroll = lib.Enum(lib.target, &.{...})`; plain enum for the
// `.zig` target with the keys in order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Autoscroll {
    None,
    Up,
    Down,
}

/// The selection behavior for a click and subsequent drag.
// PORT NOTE: `Behavior = lib.Enum(lib.target, &.{...})`; plain enum for the
// `.zig` target with the keys in order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Behavior {
    // Cell-granular drag selection. Press returns null to clear selection.
    Cell,

    // Word selection on press and word-granular drag selection.
    Word,

    // Line selection on press and line-granular drag selection.
    Line,

    // Semantic command output selection on press and drag.
    Output,
}

/// Standard terminal selection behavior for single-, double-, and triple-clicks.
///
/// A single click uses cell behavior, which returns null on press so callers can
/// clear any existing selection and then drag by cell. A double-click selects and
/// drags by word. A triple-click selects and drags by line.
pub const DEFAULT_BEHAVIORS: [Behavior; 3] = [Behavior::Cell, Behavior::Word, Behavior::Line];

/// Distance from the top or bottom surface edge, in pixels, where dragging
/// should request autoscroll. This preserves the historical 1px buffer used
/// so fullscreen-edge drags can still trigger autoscroll.
const AUTOSCROLL_BUFFER: f64 = 1.0;

pub struct SelectionGesture {
    /// The tracked pin of the initial left click along with the screen
    /// that the pin is part of.
    pub left_click_pin: Option<*mut Pin>,
    pub left_click_screen: Key,
    pub left_click_screen_generation: usize,

    /// The count of clicks to count double and triple clicks and so on.
    /// The left click time was the last time the left click was done, if the
    /// caller could provide one. If this is null then we only support single clicks.
    pub left_click_count: u8,
    pub left_click_time: Option<Time>,

    /// The selection behavior chosen for the active left-click gesture.
    pub left_click_behavior: Behavior,

    /// The starting xpos/ypos of the left click. Note that if scrolling occurs,
    /// these will point to different cells, but the xpos/ypos will stay
    /// stable during scrolling relative to the surface.
    pub left_click_xpos: f64,
    pub left_click_ypos: f64,

    /// True once the active left-click gesture has moved away from the initially
    /// pressed cell. This is reset on every press that starts or continues a
    /// multi-click sequence, and is left available for callers to inspect while
    /// handling the corresponding release.
    pub left_click_dragged: bool,

    /// The current autoscroll state for the active left-click drag gesture.
    pub left_drag_autoscroll: Autoscroll,
}

impl SelectionGesture {
    // PORT NOTE: `pub const init: SelectionGesture = .{...}` becomes an associated
    // const so callers can write `SelectionGesture::INIT`.
    pub const INIT: SelectionGesture = SelectionGesture {
        left_click_pin: None,
        left_click_count: 0,
        left_click_time: None,
        left_click_behavior: Behavior::Cell,
        left_click_screen: Key::Primary,
        left_click_screen_generation: 0,
        left_click_xpos: 0.0,
        left_click_ypos: 0.0,
        left_click_dragged: false,
        left_drag_autoscroll: Autoscroll::None,
    };

    pub fn deinit(&mut self, t: &mut Terminal) {
        // Grab our pagelist that is associated with the pin. If it doesn't
        // exist anymore then our tracked pin is already free.
        let pin = match self.left_click_pin {
            Some(pin) => pin,
            None => return,
        };
        if t.screens.generation(self.left_click_screen) != self.left_click_screen_generation {
            return;
        }
        let screen = match t.screens.get(self.left_click_screen) {
            Some(screen) => screen,
            None => return,
        };
        unsafe { (*screen).pages.untrack_pin(pin) };
    }

    /// Reset any active gesture state and untrack the tracked click pin.
    ///
    /// Use this for cancellation/abandonment, not for the ordinary left-button
    /// release path. `release` deliberately keeps the last press time/count so a
    /// following press can become a double- or triple-click; `reset` clears that
    /// sequence and makes the next press a fresh single click.
    ///
    /// Examples of reset-worthy events are: mouse reporting taking over, pointer
    /// capture being lost, a surface/window being torn down, or another interaction
    /// mode deciding that text selection must stop immediately. If the active screen
    /// was already removed or recycled, this safely drops the stale reference without
    /// trying to untrack a pin from the wrong screen generation.
    pub fn reset(&mut self, t: &mut Terminal) {
        self.left_click_count = 0;
        self.left_click_time = None;
        self.left_click_behavior = Behavior::Cell;
        self.left_click_dragged = false;
        self.left_drag_autoscroll = Autoscroll::None;
        self.untrack_pin(t);
    }

    /// Return the tracked left-click pin only if it still belongs to the current
    /// active screen instance.
    ///
    /// This validates both the screen key and generation so a pin from a removed,
    /// recycled, or inactive screen is never exposed to callers. A null result means
    /// callers should treat the in-progress gesture as temporarily or permanently
    /// unable to produce a selection. For a normal drag this usually means "do
    /// nothing for this event"; for autoscroll it is treated as cancellation because
    /// a timer should not continue firing for a gesture that no longer has a valid
    /// anchor.
    pub fn validated_left_click_pin(&self, screens: &ScreenSet) -> Option<*mut Pin> {
        let pin = self.left_click_pin?;
        if self.left_click_screen != screens.active_key {
            return None;
        }
        if screens.generation(self.left_click_screen) != self.left_click_screen_generation {
            return None;
        }
        screens.get(self.left_click_screen)?;
        Some(pin)
    }

    /// Record a press event and return the standard selection for this click.
    ///
    /// If this press continues the existing click sequence, the click count is
    /// incremented up to three and the original anchor pin is kept. Otherwise, the
    /// previous gesture state is cleared and this press becomes the new anchor.
    /// The returned selection is untracked and represents the standard terminal
    /// click behavior for the resulting click count. The caller is responsible for
    /// applying it to the screen, usually with `Screen.select`, and for arranging
    /// any copy-on-select behavior.
    ///
    /// Examples:
    ///
    /// * first press: `left_click_count == 1`, defaults to cell behavior;
    /// * second nearby press within the repeat interval: `left_click_count == 2`,
    ///   defaults to word behavior;
    /// * third nearby press within the repeat interval: `left_click_count == 3`,
    ///   defaults to line behavior;
    /// * press after the interval, too far away, or after a screen generation
    ///   change: starts over at `left_click_count == 1` and returns null.
    pub fn press(
        &mut self,
        t: &mut Terminal,
        p: Press<'_>,
    ) -> Result<Option<Selection>, crate::pagelist::OutOfMemory> {
        if self.left_click_count > 0 {
            // PORT NOTE: faithful to the reference `if (self.pressRepeat(...)) {
            // ... } else |err| switch (err) { error.PressRequiresReset => {} }`.
            #[allow(clippy::single_match)]
            match self.press_repeat(t, p) {
                Ok(()) => {
                    // Successful repeat.
                    return Ok(self.press_selection(t.screens.active, p));
                }
                Err(PressRequiresReset) => {}
            }
        }

        // Initial click or the repeat failed for some reason such as
        // the subsequent click being too far away.
        self.press_initial(t, p)?;
        Ok(self.press_selection(t.screens.active, p))
    }

    /// Record a drag event and return the current untracked drag selection.
    ///
    /// The returned selection is untracked and represents the best selection for the
    /// terminal contents at the time of this call. The caller is responsible for
    /// applying it to the screen, usually with `Screen.select`, and for arranging any
    /// copy-on-select behavior. A null result means either there is no active
    /// selection gesture, the original press is no longer valid for the active
    /// screen, or the drag has not crossed the threshold required to select a cell.
    ///
    /// This method also updates `left_click_dragged` and `left_drag_autoscroll`.
    /// If `left_drag_autoscroll` becomes `.up` or `.down`, the caller should start or
    /// keep a timer that calls `autoscrollTick` while the button remains pressed. If
    /// it becomes `.none`, the caller should stop that timer.
    ///
    /// Normal terminal output and viewport movement between drag events are allowed:
    /// the tracked press pin follows the page list, and the drag pin is used only
    /// synchronously. Content-sensitive selections such as word and line selection
    /// are recalculated from the current active screen every time.
    pub fn drag(&mut self, t: &mut Terminal, d: Drag<'_>) -> Option<Selection> {
        // If we aren't currently clicked then we don't do any dragging
        // behavior.
        if self.left_click_count == 0 {
            assert!(self.left_drag_autoscroll == Autoscroll::None);
            return None;
        }

        // Get our click pin. We get a validated pin because if our
        // screen changed out from under us then we aren't actually
        // clicking anymore.
        let click_pin = self.validated_left_click_pin(&t.screens)?;
        if !d.pin.eql(unsafe { *click_pin }) {
            self.left_click_dragged = true;
        }

        // Determine if we should autoscroll. If our drag position is above
        // the top, we go up. If its below the bottom we go down. Easy.
        let max_y: f64 = d.geometry.screen_height as f64;
        self.left_drag_autoscroll = if d.ypos <= AUTOSCROLL_BUFFER {
            Autoscroll::Up
        } else if d.ypos > max_y - AUTOSCROLL_BUFFER {
            Autoscroll::Down
        } else {
            Autoscroll::None
        };

        let selection = match self.left_click_behavior {
            Behavior::Cell => drag_selection(
                unsafe { *click_pin },
                d.pin,
                f64::max(0.0, self.left_click_xpos) as u32,
                f64::max(0.0, d.xpos) as u32,
                d.rectangle,
                d.geometry,
            ),

            Behavior::Word => drag_selection_word(
                t.screens.active,
                unsafe { *click_pin },
                d.pin,
                d.word_boundary_codepoints,
            ),

            Behavior::Line => drag_selection_line(t.screens.active, unsafe { *click_pin }, d.pin),

            Behavior::Output => {
                drag_selection_output(t.screens.active, unsafe { *click_pin }, d.pin)
            }
        };

        // Same-cell cell selections can still become real selections when the drag
        // crosses the within-cell threshold. Treat those as drags so callers don't
        // also process click-only actions such as opening links.
        if self.left_click_behavior == Behavior::Cell && selection.is_some() {
            self.left_click_dragged = true;
        }

        selection
    }

    /// Record a selection autoscroll tick for the active left-click drag gesture.
    ///
    /// This scrolls the viewport in the active autoscroll direction and then
    /// continues the drag at the provided viewport position. The viewport position
    /// is resolved to a pin after scrolling so the drag applies to the row now under
    /// the pointer.
    ///
    /// This always scrolls the viewport by exactly one row in the current
    /// autoscroll direction. If you want to scroll by more, increase your
    /// tick rate.
    ///
    /// If the original press pin no longer belongs to the active screen, this calls
    /// `reset` and returns null. That is a signal for the caller to stop its
    /// autoscroll timer and leave any existing terminal selection alone unless some
    /// other event says otherwise.
    pub fn autoscroll_tick(
        &mut self,
        t: &mut Terminal,
        tick: AutoscrollTick<'_>,
    ) -> Option<Selection> {
        if self.left_click_count == 0 {
            assert!(self.left_drag_autoscroll == Autoscroll::None);
            return None;
        }

        let delta: isize = match self.left_drag_autoscroll {
            Autoscroll::None => return None,
            Autoscroll::Up => -1,
            Autoscroll::Down => 1,
        };

        // If our click pin no longer belongs to the active screen, the gesture is
        // no longer valid. Stop it so callers can stop their autoscroll timer
        // without clearing the current selection as if this were a real drag.
        if self.validated_left_click_pin(&t.screens).is_none() {
            self.reset(t);
            return None;
        }

        t.scroll_viewport(ScrollViewport::Delta(delta));

        let pin = unsafe {
            (*t.screens.active)
                .pages
                .pin(point::Point::Viewport(tick.viewport))
        }?;
        self.drag(
            t,
            Drag {
                pin,
                xpos: tick.xpos,
                ypos: tick.ypos,
                rectangle: tick.rectangle,
                word_boundary_codepoints: tick.word_boundary_codepoints,
                geometry: tick.geometry,
            },
        )
    }

    /// Record a deep press event for the active left-click gesture.
    ///
    /// A deep press is a force/pressure activation while the primary pointer is
    /// already down. Ghostty treats it like the platform text-selection affordance:
    /// select the word under the original press, then consume the gesture so
    /// further cursor movement while the button remains pressed does not drag or
    /// autoscroll the selection.
    ///
    /// After a successful deep press, the click sequence is cleared and the tracked
    /// pin is untracked. The returned selection should be applied by the caller. A
    /// null result means there was no valid active left-click anchor, commonly
    /// because the screen changed or the gesture had already been cancelled.
    pub fn deep_press(&mut self, t: &mut Terminal, p: DeepPress<'_>) -> Option<Selection> {
        let click_pin = self.validated_left_click_pin(&t.screens)?;
        let sel =
            unsafe { (*t.screens.active).select_word(*click_pin, p.word_boundary_codepoints) };

        self.left_click_count = 0;
        self.left_click_time = None;
        self.left_click_behavior = Behavior::Cell;
        self.left_click_dragged = true;
        self.left_drag_autoscroll = Autoscroll::None;
        self.untrack_pin(t);

        sel
    }

    /// Record a release event for the active left-click gesture.
    ///
    /// This stops autoscroll and updates `left_click_dragged`, but it does not clear
    /// the click count or time. Keeping that state is what lets the next nearby press
    /// become a double- or triple-click. Call `reset` instead if the release should
    /// cancel the click sequence entirely.
    ///
    /// Pass the release pin when the pointer position maps to a valid terminal cell.
    /// If it does not, pass null; the gesture then conservatively records that the
    /// pointer moved away from the original pressed cell. This is useful for callers
    /// that use `left_click_dragged` after release to decide whether a click should
    /// activate links or other hit targets.
    pub fn release(&mut self, t: &mut Terminal, r: Release) {
        if self.left_click_count == 0 {
            assert!(self.left_drag_autoscroll == Autoscroll::None);
            return;
        }

        if let Some(release_pin) = r.pin {
            if let Some(click_pin) = self.validated_left_click_pin(&t.screens) {
                if !release_pin.eql(unsafe { *click_pin }) {
                    self.left_click_dragged = true;
                }
            } else {
                // If the original anchor is no longer valid, conservatively treat
                // this as a drag/cancelled click so callers don't perform click-only
                // actions on a different or recycled screen.
                self.left_click_dragged = true;
            }
        } else {
            self.left_click_dragged = true;
        }
        self.left_drag_autoscroll = Autoscroll::None;
    }

    fn press_initial(
        &mut self,
        t: &mut Terminal,
        p: Press<'_>,
    ) -> Result<(), crate::pagelist::OutOfMemory> {
        // Setup our pin first, reusing our existing pin if we can.
        if let Some(pin) = self.left_click_pin {
            if cfg!(debug_assertions) {
                assert!(self.left_click_screen == t.screens.active_key);
                assert!(
                    self.left_click_screen_generation == t.screens.generation(t.screens.active_key)
                );
            }
            unsafe { *pin = p.pin };
        } else {
            let screens: &ScreenSet = &t.screens;
            self.left_click_pin = Some(unsafe { (*screens.active).pages.track_pin(p.pin) }?);
            // errdefer comptime unreachable;
            self.left_click_screen = screens.active_key;
            self.left_click_screen_generation = screens.generation(screens.active_key);
        }
        // errdefer comptime unreachable;
        self.left_click_count = 1;
        self.left_click_behavior = p.behaviors[0];
        self.left_click_xpos = p.xpos;
        self.left_click_ypos = p.ypos;
        self.left_click_time = p.time;
        self.left_click_dragged = false;
        self.left_drag_autoscroll = Autoscroll::None;
        Ok(())
    }

    fn press_repeat(&mut self, t: &mut Terminal, p: Press<'_>) -> Result<(), PressRequiresReset> {
        // PORT NOTE: the reference `errdefer` resets click state + untracks the
        // pin on any error return below. Rust has no `errdefer`, so each early
        // `Err` path runs this cleanup explicitly via `self.press_repeat_errdefer`.
        macro_rules! fail {
            () => {{
                self.press_repeat_errdefer(t);
                return Err(PressRequiresReset);
            }};
        }

        // If too much time has passed then we always reset.
        let time = match p.time {
            Some(time) => time,
            None => fail!(),
        };
        {
            let prev_time = match self.left_click_time {
                Some(prev_time) => prev_time,
                None => fail!(),
            };
            // const since = timeSince(time, prev_time) orelse
            //     return error.PressRequiresReset;
            // PORT NOTE (parity 8f1c2fe95): backwards timestamps are failed
            // repeats; the next press becomes a new single-click anchor.
            let since = match time_since(time, prev_time) {
                Some(since) => since,
                None => fail!(),
            };
            if since > p.repeat_interval {
                fail!();
            }
        }

        // If the click is too far away from the initial click we can't continue.
        let distance = f64::sqrt(
            (p.xpos - self.left_click_xpos).powf(2.0) + (p.ypos - self.left_click_ypos).powf(2.0),
        );
        if distance > p.max_distance {
            fail!();
        }

        // If our prior click was on another screen then free and reset. "Another screen"
        // doesn't just mean alt vs primary, it could mean an alt screen that was
        // recycled since we free tracked pins on recycle.
        let screens: &ScreenSet = &t.screens;
        if self.left_click_screen != screens.active_key
            || screens.generation(self.left_click_screen) != self.left_click_screen_generation
        {
            // The error return will trigger the top-level errdefer which
            // will reset our pin.
            fail!();
        }

        self.left_click_time = Some(time);
        self.left_click_dragged = false;
        self.left_drag_autoscroll = Autoscroll::None;
        self.left_click_count = std::cmp::min(
            self.left_click_count + 1,
            3, // We only support triple clicks max
        );
        self.left_click_behavior = p.behaviors[self.left_click_count as usize - 1];
        Ok(())
    }

    // PORT NOTE: the cleanup body of the reference `errdefer` in `pressRepeat`.
    fn press_repeat_errdefer(&mut self, t: &mut Terminal) {
        self.left_click_count = 0;
        self.left_click_behavior = Behavior::Cell;
        self.untrack_pin(t);
    }

    fn press_selection(&self, screen: *mut Screen, p: Press<'_>) -> Option<Selection> {
        match self.left_click_behavior {
            Behavior::Cell => None,
            Behavior::Word => unsafe { (*screen).select_word(p.pin, p.word_boundary_codepoints) },
            Behavior::Line => unsafe {
                // PORT NOTE: the reference `selectLine(.{ .pin = p.pin })` relies
                // on the Zig `SelectLine` field defaults: `whitespace =
                // &selection_codepoints.default_line_whitespace` and
                // `semantic_prompt_boundary = true`. The port has no struct field
                // defaults, so those defaults are written out explicitly here.
                (*screen).select_line(SelectLine {
                    pin: p.pin,
                    whitespace: Some(&crate::selection_codepoints::DEFAULT_LINE_WHITESPACE),
                    semantic_prompt_boundary: true,
                })
            },
            Behavior::Output => unsafe { (*screen).select_output(p.pin) },
        }
    }

    fn untrack_pin(&mut self, t: &mut Terminal) {
        // Can't untrack unless we have a pin.
        let pin = match self.left_click_pin {
            Some(pin) => pin,
            None => return,
        };
        self.left_click_pin = None;

        // If the generation changed our pin is already invalid.
        let screens: &ScreenSet = &t.screens;
        if screens.generation(self.left_click_screen) != self.left_click_screen_generation {
            return;
        }

        // If we can't get a screen then its already freed.
        let screen = match screens.get(self.left_click_screen) {
            Some(screen) => screen,
            None => return,
        };
        unsafe { (*screen).pages.untrack_pin(pin) };
    }
}

pub struct Press<'a> {
    /// The time when the press event occurred. Prefer a monotonic timer;
    /// backwards timestamps reset the repeat sequence.
    /// This can be null if you're on a system that doesn't support
    /// time for some reason. In that case, we only support single clicks.
    pub time: Option<Time>,

    /// The cell where the click was.
    ///
    /// `press` stores a tracked copy of this pin. The caller does not need to
    /// keep `p.pin` alive after the call returns, but the pin must belong to the
    /// terminal's active screen when passed in.
    pub pin: Pin,

    /// The x/y value of the click relative to the surface with (0,0) being
    /// top-left. This is used for distance detection for multi-clicks so
    /// double/triple clicks too far away from each other will reset the click
    /// count as well more accurate drag behaviors.
    pub xpos: f64,
    pub ypos: f64,

    /// Maximum distance a click can be from the original click to register
    /// as a repeat. If uncertain, set this to cell width.
    pub max_distance: f64,

    /// The maximum interval in nanoseconds that a press is considered
    /// a repeat e.g. to record double/triple clicks.
    pub repeat_interval: u64,

    /// The codepoints that delimit words for double-click selection.
    pub word_boundary_codepoints: &'a [u32],

    /// Selection behaviors for single-, double-, and triple-clicks.
    pub behaviors: &'a [Behavior; 3],
}

// PORT NOTE: `Press` is `Copy` so the reference's by-value pass + later field
// reuse (`return self.pressSelection(..., p)` after `pressInitial(..., p)`)
// works without re-borrowing.
impl Copy for Press<'_> {}
impl Clone for Press<'_> {
    fn clone(&self) -> Self {
        *self
    }
}

pub struct Drag<'a> {
    /// The cell where the current drag position is. This is used
    /// synchronously to calculate the selection and is not tracked.
    pub pin: Pin,

    /// The x/y value of the drag relative to the surface with (0,0) being
    /// top-left.
    pub xpos: f64,
    pub ypos: f64,

    /// True if the current drag should produce a rectangular selection.
    pub rectangle: bool,

    /// The codepoints that delimit words for double-click drag selection.
    pub word_boundary_codepoints: &'a [u32],

    /// Geometry required for selection threshold and autoscroll calculations.
    pub geometry: Geometry,
}

/// Display geometry needed to translate surface-relative pointer positions
/// into selection behavior.
#[derive(Clone, Copy)]
pub struct Geometry {
    /// The number of columns in the rendered terminal grid.
    pub columns: u32,

    /// The width of one terminal cell in surface pixels.
    pub cell_width: u32,

    /// The left padding before the terminal grid begins, in surface pixels.
    pub padding_left: u32,

    /// The height of the rendered terminal surface in surface pixels.
    pub screen_height: u32,
}

pub struct AutoscrollTick<'a> {
    /// The viewport cell where the current drag position is. This is resolved
    /// after the viewport is scrolled so the selection tracks the newly visible
    /// row under the pointer.
    pub viewport: point::Coordinate,

    /// The x/y value of the drag relative to the surface with (0,0) being
    /// top-left.
    pub xpos: f64,
    pub ypos: f64,

    /// True if the current drag should produce a rectangular selection.
    pub rectangle: bool,

    /// The codepoints that delimit words for double-click drag selection.
    pub word_boundary_codepoints: &'a [u32],

    /// Geometry required for selection threshold and autoscroll calculations.
    pub geometry: Geometry,
}

/// A pressure-based activation during an existing left-click gesture.
///
/// This is the terminal gesture model for platform features such as macOS
/// force click / deep click on pressure-sensitive trackpads. It is not a
/// distinct mouse button and it is not part of the normal single/double/triple
/// click count sequence; it can only occur after a left press is already
/// active.
pub struct DeepPress<'a> {
    /// The codepoints that delimit words for the word selection produced by
    /// the deep press.
    pub word_boundary_codepoints: &'a [u32],
}

pub struct Release {
    /// The cell where the release occurred, if the release position mapped to
    /// a valid cell. This is used synchronously to update gesture state and is
    /// not tracked.
    pub pin: Option<Pin>,
}

// PORT NOTE: the reference `error{PressRequiresReset}` becomes a zero-field
// error type carried by `Result<(), PressRequiresReset>`.
struct PressRequiresReset;

// PORT NOTE: `timeSince(time, prev_time)` returns `time.since(prev_time)` in
// nanoseconds. `std::time::Instant::duration_since` is the faithful analogue;
// the freestanding-wasm `time -| prev_time` branch is unreachable for this
// target (see `Time`).
// PORT NOTE (parity 8f1c2fe95): backwards timestamps return None (`null`),
// mirroring `if (time.order(prev_time) == .lt) return null;`. (Rust's
// `duration_since` saturates to zero rather than panicking, but a zero
// elapsed time would wrongly count as a valid fast repeat.)
fn time_since(time: Time, prev_time: Time) -> Option<u64> {
    if time < prev_time {
        return None;
    }
    Some(time.duration_since(prev_time).as_nanos() as u64)
}

/// Calculates the appropriate selection given pins and pixel x positions for
/// the click point and the drag point, as well as selection mode and geometry.
fn drag_selection(
    click_pin: Pin,
    drag_pin: Pin,
    click_x: u32,
    drag_x: u32,
    rectangle_selection: bool,
    geometry: Geometry,
) -> Option<Selection> {
    // Explanation:
    //
    // # Normal selections
    //
    // ## Left-to-right selections
    // - The clicked cell is included if it was clicked to the left of its
    //   threshold point and the drag location is right of the threshold point.
    // - The cell under the cursor (the "drag cell") is included if the drag
    //   location is right of its threshold point.
    //
    // ## Right-to-left selections
    // - The clicked cell is included if it was clicked to the right of its
    //   threshold point and the drag location is left of the threshold point.
    // - The cell under the cursor (the "drag cell") is included if the drag
    //   location is left of its threshold point.
    //
    // # Rectangular selections
    //
    // Rectangular selections are handled similarly, except that
    // entire columns are considered rather than individual cells.

    // We only include cells in the selection if the threshold point lies
    // between the start and end points of the selection. A threshold of
    // 60% of the cell width was chosen empirically because it felt good.
    let threshold_point: u32 = f64::round(geometry.cell_width as f64 * 0.6) as u32;

    // We use this to clamp the pixel positions below.
    let max_x = geometry.columns * geometry.cell_width - 1;

    // We need to know how far across in the cell the drag pos is, so
    // we subtract the padding and then take it modulo the cell width.
    let drag_x_frac =
        std::cmp::min(max_x, drag_x.saturating_sub(geometry.padding_left)) % geometry.cell_width;

    // We figure out the fractional part of the click x position similarly.
    let click_x_frac =
        std::cmp::min(max_x, click_x.saturating_sub(geometry.padding_left)) % geometry.cell_width;

    // Whether the click pin and drag pin are equal.
    let same_pin = drag_pin.eql(click_pin);

    // Whether or not the end point of our selection is before the start point.
    let end_before_start = 'ebs: {
        if same_pin {
            break 'ebs drag_x_frac < click_x_frac;
        }

        // Special handling for rectangular selections, we only use x position.
        if rectangle_selection {
            break 'ebs match drag_pin.x.cmp(&click_pin.x) {
                std::cmp::Ordering::Equal => drag_x_frac < click_x_frac,
                std::cmp::Ordering::Less => true,
                std::cmp::Ordering::Greater => false,
            };
        }

        break 'ebs drag_pin.before(click_pin);
    };

    // Whether or not the click pin cell
    // should be included in the selection.
    let include_click_cell = if end_before_start {
        click_x_frac >= threshold_point
    } else {
        click_x_frac < threshold_point
    };

    // Whether or not the drag pin cell
    // should be included in the selection.
    let include_drag_cell = if end_before_start {
        drag_x_frac < threshold_point
    } else {
        drag_x_frac >= threshold_point
    };

    // If the click cell should be included in the selection then it's the
    // start, otherwise we get the previous or next cell to it depending on
    // the type and direction of the selection.
    let start_pin = if include_click_cell {
        click_pin
    } else if end_before_start {
        if rectangle_selection {
            click_pin.left_clamp(1)
        } else {
            click_pin.left_wrap(1).unwrap_or(click_pin)
        }
    } else if rectangle_selection {
        click_pin.right_clamp(1)
    } else {
        click_pin.right_wrap(1).unwrap_or(click_pin)
    };

    // Likewise for the end pin with the drag cell.
    let end_pin = if include_drag_cell {
        drag_pin
    } else if end_before_start {
        if rectangle_selection {
            drag_pin.right_clamp(1)
        } else {
            drag_pin.right_wrap(1).unwrap_or(drag_pin)
        }
    } else if rectangle_selection {
        drag_pin.left_clamp(1)
    } else {
        drag_pin.left_wrap(1).unwrap_or(drag_pin)
    };

    // If the click cell is the same as the drag cell and the click cell
    // shouldn't be included, or if the cells are adjacent such that the
    // start or end pin becomes the other cell, and that cell should not
    // be included, then we have no selection, so we set it to null.
    //
    // If in rectangular selection mode, we compare columns as well.
    //
    // TODO(qwerasd): this can/should probably be refactored, it's a bit
    //                repetitive and does excess work in rectangle mode.
    if (!include_click_cell && same_pin)
        || (!include_click_cell && rectangle_selection && click_pin.x == drag_pin.x)
        || (!include_click_cell && end_pin.eql(click_pin))
        || (!include_click_cell && rectangle_selection && end_pin.x == click_pin.x)
        || (!include_drag_cell && start_pin.eql(drag_pin))
        || (!include_drag_cell && rectangle_selection && start_pin.x == drag_pin.x)
    {
        return None;
    }

    // TODO: Clamp selection to the screen area, don't
    //       let it extend past the last written row.

    Some(Selection::init(start_pin, end_pin, rectangle_selection))
}

/// Calculates the appropriate word-wise selection for a double-click drag.
fn drag_selection_word(
    screen: *mut Screen,
    click_pin: Pin,
    drag_pin: Pin,
    boundary_codepoints: &[u32],
) -> Option<Selection> {
    // Get the word closest to our starting click.
    let word_start =
        unsafe { (*screen).select_word_between(click_pin, drag_pin, boundary_codepoints) }?;

    // Get the word closest to our current point.
    let word_current =
        unsafe { (*screen).select_word_between(drag_pin, click_pin, boundary_codepoints) }?;

    // If our current mouse position is before the starting position,
    // then the selection start is the word nearest our current position.
    if drag_pin.before(click_pin) {
        Some(Selection::init(
            word_current.start(),
            word_start.end(),
            false,
        ))
    } else {
        Some(Selection::init(
            word_start.start(),
            word_current.end(),
            false,
        ))
    }
}

/// Calculates the appropriate line-wise selection for a triple-click drag.
fn drag_selection_line(screen: *mut Screen, click_pin: Pin, drag_pin: Pin) -> Option<Selection> {
    // Get the line selection under our current drag point. If there isn't a
    // line, do nothing.
    // PORT NOTE: the reference `selectLine(.{ .pin = ... })` relies on the Zig
    // `SelectLine` field defaults `whitespace =
    // &selection_codepoints.default_line_whitespace` and
    // `semantic_prompt_boundary = true`, written out explicitly here.
    let line = unsafe {
        (*screen).select_line(SelectLine {
            pin: drag_pin,
            whitespace: Some(&crate::selection_codepoints::DEFAULT_LINE_WHITESPACE),
            semantic_prompt_boundary: true,
        })
    }?;

    // Get the selection under our click point. We first try to trim
    // whitespace if we've selected a word. But if no word exists then
    // we select the blank line.
    let sel_ = unsafe {
        (*screen).select_line(SelectLine {
            pin: click_pin,
            whitespace: Some(&crate::selection_codepoints::DEFAULT_LINE_WHITESPACE),
            semantic_prompt_boundary: true,
        })
    }
    .or_else(|| unsafe {
        (*screen).select_line(SelectLine {
            pin: click_pin,
            whitespace: None,
            semantic_prompt_boundary: true,
        })
    });

    let mut sel = sel_?;
    if drag_pin.before(click_pin) {
        unsafe { *sel.start_ptr() = line.start() };
    } else {
        unsafe { *sel.end_ptr() = line.end() };
    }
    Some(sel)
}

/// Calculates the appropriate semantic-output-wise selection for an output
/// drag. This expands from the output block under the click point to the output
/// block under the current drag point. If the drag point is not output, keep the
/// original output selection.
fn drag_selection_output(screen: *mut Screen, click_pin: Pin, drag_pin: Pin) -> Option<Selection> {
    let mut sel = unsafe { (*screen).select_output(click_pin) }?;
    let current = match unsafe { (*screen).select_output(drag_pin) } {
        Some(current) => current,
        None => return Some(sel),
    };

    if drag_pin.before(click_pin) {
        unsafe { *sel.start_ptr() = current.start() };
    } else {
        unsafe { *sel.end_ptr() = current.end() };
    }
    Some(sel)
}

#[cfg(test)]
// PORT NOTE: the reference tests use `testing.expectEqual(false, x)` /
// `expectEqual(true, x)`; keep the literal-bool comparison faithful.
#[allow(clippy::bool_assert_comparison)]
mod tests {
    use super::*;
    use crate::screen::{Screen, SemanticContentArg};
    use crate::selection::Bounds;
    use crate::terminal::Options;
    use allocator_api2::alloc::Global as Allocator;

    // PORT NOTE: the reference uses inline `Terminal.init(testing.allocator, .{
    // .cols, .rows })` / `.{ ..., .max_scrollback }` literals. `Options` has more
    // fields in the port; these helpers mirror the Terminal test helpers in
    // terminal.rs.
    fn opts(cols: crate::size::CellCountInt, rows: crate::size::CellCountInt) -> Options {
        Options {
            cols,
            rows,
            max_scrollback: 10_000,
            colors: crate::terminal::Colors {
                background: crate::color::DynamicRGB::UNSET,
                foreground: crate::color::DynamicRGB::UNSET,
                cursor: crate::color::DynamicRGB::UNSET,
                palette: crate::color::DynamicPalette::default_palette(),
            },
            default_modes: crate::modes::ModePacked::default(),
            kitty_image_storage_limit: 320 * 1000 * 1000,
            kitty_image_loading_limits: crate::kitty::graphics_image::Limits::DIRECT,
        }
    }

    fn opts_sb(
        cols: crate::size::CellCountInt,
        rows: crate::size::CellCountInt,
        max_scrollback: usize,
    ) -> Options {
        Options {
            max_scrollback,
            ..opts(cols, rows)
        }
    }

    fn screen_opts(
        cols: crate::size::CellCountInt,
        rows: crate::size::CellCountInt,
        max_scrollback: usize,
    ) -> crate::screen::Options {
        crate::screen::Options {
            cols,
            rows,
            max_scrollback,
            kitty_image_storage_limit: 0,
            kitty_image_loading_limits: crate::kitty::graphics_image::Limits::DIRECT,
        }
    }

    // PORT NOTE: the reference asserts selections with `expectEqualDeep`. The
    // port's `Selection` compares via `Selection::eql` (faithful for untracked
    // selections from the same screen; see selection.rs's `expect_sel_eq`).
    // Deep-compare analog of the reference's `testing.expectEqualDeep(expected,
    // sel)`. Returns the comparison instead of asserting so the ASSERTION
    // itself lives in each test body (assertion-for-assertion with the
    // reference; G2 counts source-level asserts per test).
    fn sel_eql(expected: Selection, got: Selection) -> bool {
        expected.eql(got)
    }

    fn test_press(
        t: &mut Terminal,
        x: u16,
        y: u32,
        time: Option<std::time::Instant>,
    ) -> Press<'static> {
        Press {
            time,
            pin: unsafe {
                (*t.screens.active)
                    .pages
                    .pin(point::Point::Active(point::Coordinate { x, y }))
                    .unwrap()
            },
            xpos: x as f64,
            ypos: y as f64,
            max_distance: 1.0,
            repeat_interval: u64::MAX,
            word_boundary_codepoints: &[],
            behaviors: &DEFAULT_BEHAVIORS,
        }
    }

    fn test_drag(t: &mut Terminal, x: u16, y: u32, xpos: f64, ypos: f64) -> Drag<'static> {
        Drag {
            pin: unsafe {
                (*t.screens.active)
                    .pages
                    .pin(point::Point::Active(point::Coordinate { x, y }))
                    .unwrap()
            },
            xpos,
            ypos,
            rectangle: false,
            word_boundary_codepoints: &[],
            geometry: Geometry {
                columns: 5,
                cell_width: 10,
                padding_left: 0,
                screen_height: 100,
            },
        }
    }

    fn test_autoscroll_tick(
        viewport: point::Coordinate,
        xpos: f64,
        ypos: f64,
    ) -> AutoscrollTick<'static> {
        AutoscrollTick {
            viewport,
            xpos,
            ypos,
            rectangle: false,
            word_boundary_codepoints: &[],
            geometry: Geometry {
                columns: 5,
                cell_width: 10,
                padding_left: 0,
                screen_height: 100,
            },
        }
    }

    fn test_pin(t: &mut Terminal, x: u16, y: u32) -> Pin {
        unsafe {
            (*t.screens.active)
                .pages
                .pin(point::Point::Active(point::Coordinate { x, y }))
                .unwrap()
        }
    }

    /// Utility function for the unit tests for drag selection logic.
    ///
    /// Tests a click and drag on a 10x5 cell grid, x positions are given in
    /// fractional cells, e.g. 3.1 would be 10% through the cell at x = 3.
    ///
    /// NOTE: The geometry tested with has 10px wide cells, meaning only one digit
    ///       after the decimal place has any meaning, e.g. 3.14 is equal to 3.1.
    ///
    /// The provided start_x/y and end_x/y are the expected start and end points
    /// of the resulting selection.
    #[allow(clippy::too_many_arguments)]
    fn test_drag_selection(
        click_x: f64,
        click_y: u32,
        drag_x: f64,
        drag_y: u32,
        start_x: u16,
        start_y: u32,
        end_x: u16,
        end_y: u32,
        rect: bool,
    ) {
        // Our screen size is 10x5 cells that are
        // 10x20 px, with 5px padding on all sides.
        let geometry: Geometry = Geometry {
            columns: 10,
            cell_width: 10,
            padding_left: 5,
            screen_height: 110,
        };
        let mut screen = Screen::init(Allocator, screen_opts(10, 5, 0)).unwrap();

        let click_pin = screen
            .pages
            .pin(point::Point::Viewport(point::Coordinate {
                x: click_x.floor() as u16,
                y: click_y,
            }))
            .unwrap();
        let drag_pin = screen
            .pages
            .pin(point::Point::Viewport(point::Coordinate {
                x: drag_x.floor() as u16,
                y: drag_y,
            }))
            .unwrap();

        let cell_width_f64: f64 = geometry.cell_width as f64;
        let click_x_pos: u32 = (click_x * cell_width_f64).floor() as u32 + geometry.padding_left;
        let drag_x_pos: u32 = (drag_x * cell_width_f64).floor() as u32 + geometry.padding_left;

        let start_pin = screen
            .pages
            .pin(point::Point::Viewport(point::Coordinate {
                x: start_x,
                y: start_y,
            }))
            .unwrap();
        let end_pin = screen
            .pages
            .pin(point::Point::Viewport(point::Coordinate {
                x: end_x,
                y: end_y,
            }))
            .unwrap();

        let expected = Selection {
            bounds: Bounds::Untracked {
                start: start_pin,
                end: end_pin,
            },
            rectangle: rect,
        };
        assert!(sel_eql(
            expected,
            drag_selection(click_pin, drag_pin, click_x_pos, drag_x_pos, rect, geometry).unwrap(),
        ));

        screen.deinit();
    }

    /// Like `test_drag_selection` but checks that the resulting selection is null.
    ///
    /// See `test_drag_selection` for more details.
    fn test_drag_selection_is_null(
        click_x: f64,
        click_y: u32,
        drag_x: f64,
        drag_y: u32,
        rect: bool,
    ) {
        // Our screen size is 10x5 cells that are
        // 10x20 px, with 5px padding on all sides.
        let geometry: Geometry = Geometry {
            columns: 10,
            cell_width: 10,
            padding_left: 5,
            screen_height: 110,
        };
        let mut screen = Screen::init(Allocator, screen_opts(10, 5, 0)).unwrap();

        let click_pin = screen
            .pages
            .pin(point::Point::Viewport(point::Coordinate {
                x: click_x.floor() as u16,
                y: click_y,
            }))
            .unwrap();
        let drag_pin = screen
            .pages
            .pin(point::Point::Viewport(point::Coordinate {
                x: drag_x.floor() as u16,
                y: drag_y,
            }))
            .unwrap();

        let cell_width_f64: f64 = geometry.cell_width as f64;
        let click_x_pos: u32 = (click_x * cell_width_f64).floor() as u32 + geometry.padding_left;
        let drag_x_pos: u32 = (drag_x * cell_width_f64).floor() as u32 + geometry.padding_left;

        assert!(
            drag_selection(click_pin, drag_pin, click_x_pos, drag_x_pos, rect, geometry,).is_none()
        );

        screen.deinit();
    }

    // test "SelectionGesture drag selection logic"
    #[test]
    fn drag_selection_logic() {
        // We disable format to make these easier to
        // read by pairing sets of coordinates per line.
        // zig fmt: off

        // -- LTR
        // single cell selection
        test_drag_selection(
            3.0, 3, // click
            3.9, 3, // drag
            3, 3, // expected start
            3, 3,     // expected end
            false, // regular selection
        );
        // including click and drag pin cells
        test_drag_selection(
            3.0, 3, // click
            5.9, 3, // drag
            3, 3, // expected start
            5, 3,     // expected end
            false, // regular selection
        );
        // including click pin cell but not drag pin cell
        test_drag_selection(
            3.0, 3, // click
            5.0, 3, // drag
            3, 3, // expected start
            4, 3,     // expected end
            false, // regular selection
        );
        // including drag pin cell but not click pin cell
        test_drag_selection(
            3.9, 3, // click
            5.9, 3, // drag
            4, 3, // expected start
            5, 3,     // expected end
            false, // regular selection
        );
        // including neither click nor drag pin cells
        test_drag_selection(
            3.9, 3, // click
            5.0, 3, // drag
            4, 3, // expected start
            4, 3,     // expected end
            false, // regular selection
        );
        // empty selection (single cell on only left half)
        test_drag_selection_is_null(
            3.0, 3, // click
            3.1, 3,     // drag
            false, // regular selection
        );
        // empty selection (single cell on only right half)
        test_drag_selection_is_null(
            3.8, 3, // click
            3.9, 3,     // drag
            false, // regular selection
        );
        // empty selection (between two cells, not crossing threshold)
        test_drag_selection_is_null(
            3.9, 3, // click
            4.0, 3,     // drag
            false, // regular selection
        );

        // -- RTL
        // single cell selection
        test_drag_selection(
            3.9, 3, // click
            3.0, 3, // drag
            3, 3, // expected start
            3, 3,     // expected end
            false, // regular selection
        );
        // including click and drag pin cells
        test_drag_selection(
            5.9, 3, // click
            3.0, 3, // drag
            5, 3, // expected start
            3, 3,     // expected end
            false, // regular selection
        );
        // including click pin cell but not drag pin cell
        test_drag_selection(
            5.9, 3, // click
            3.9, 3, // drag
            5, 3, // expected start
            4, 3,     // expected end
            false, // regular selection
        );
        // including drag pin cell but not click pin cell
        test_drag_selection(
            5.0, 3, // click
            3.0, 3, // drag
            4, 3, // expected start
            3, 3,     // expected end
            false, // regular selection
        );
        // including neither click nor drag pin cells
        test_drag_selection(
            5.0, 3, // click
            3.9, 3, // drag
            4, 3, // expected start
            4, 3,     // expected end
            false, // regular selection
        );
        // empty selection (single cell on only left half)
        test_drag_selection_is_null(
            3.1, 3, // click
            3.0, 3,     // drag
            false, // regular selection
        );
        // empty selection (single cell on only right half)
        test_drag_selection_is_null(
            3.9, 3, // click
            3.8, 3,     // drag
            false, // regular selection
        );
        // empty selection (between two cells, not crossing threshold)
        test_drag_selection_is_null(
            4.0, 3, // click
            3.9, 3,     // drag
            false, // regular selection
        );

        // -- Wrapping
        // LTR, wrap excluded cells
        test_drag_selection(
            9.9, 2, // click
            0.0, 4, // drag
            0, 3, // expected start
            9, 3,     // expected end
            false, // regular selection
        );
        // RTL, wrap excluded cells
        test_drag_selection(
            0.0, 4, // click
            9.9, 2, // drag
            9, 3, // expected start
            0, 3,     // expected end
            false, // regular selection
        );
    }

    // test "SelectionGesture rectangle drag selection logic"
    #[test]
    fn rectangle_drag_selection_logic() {
        // We disable format to make these easier to
        // read by pairing sets of coordinates per line.
        // zig fmt: off

        // -- LTR
        // single column selection
        test_drag_selection(
            3.0, 2, // click
            3.9, 4, // drag
            3, 2, // expected start
            3, 4,    // expected end
            true, //rectangle selection
        );
        // including click and drag pin columns
        test_drag_selection(
            3.0, 2, // click
            5.9, 4, // drag
            3, 2, // expected start
            5, 4,    // expected end
            true, //rectangle selection
        );
        // including click pin column but not drag pin column
        test_drag_selection(
            3.0, 2, // click
            5.0, 4, // drag
            3, 2, // expected start
            4, 4,    // expected end
            true, //rectangle selection
        );
        // including drag pin column but not click pin column
        test_drag_selection(
            3.9, 2, // click
            5.9, 4, // drag
            4, 2, // expected start
            5, 4,    // expected end
            true, //rectangle selection
        );
        // including neither click nor drag pin columns
        test_drag_selection(
            3.9, 2, // click
            5.0, 4, // drag
            4, 2, // expected start
            4, 4,    // expected end
            true, //rectangle selection
        );
        // empty selection (single column on only left half)
        test_drag_selection_is_null(
            3.0, 2, // click
            3.1, 4,    // drag
            true, //rectangle selection
        );
        // empty selection (single column on only right half)
        test_drag_selection_is_null(
            3.8, 2, // click
            3.9, 4,    // drag
            true, //rectangle selection
        );
        // empty selection (between two columns, not crossing threshold)
        test_drag_selection_is_null(
            3.9, 2, // click
            4.0, 4,    // drag
            true, //rectangle selection
        );

        // -- RTL
        // single column selection
        test_drag_selection(
            3.9, 2, // click
            3.0, 4, // drag
            3, 2, // expected start
            3, 4,    // expected end
            true, //rectangle selection
        );
        // including click and drag pin columns
        test_drag_selection(
            5.9, 2, // click
            3.0, 4, // drag
            5, 2, // expected start
            3, 4,    // expected end
            true, //rectangle selection
        );
        // including click pin column but not drag pin column
        test_drag_selection(
            5.9, 2, // click
            3.9, 4, // drag
            5, 2, // expected start
            4, 4,    // expected end
            true, //rectangle selection
        );
        // including drag pin column but not click pin column
        test_drag_selection(
            5.0, 2, // click
            3.0, 4, // drag
            4, 2, // expected start
            3, 4,    // expected end
            true, //rectangle selection
        );
        // including neither click nor drag pin columns
        test_drag_selection(
            5.0, 2, // click
            3.9, 4, // drag
            4, 2, // expected start
            4, 4,    // expected end
            true, //rectangle selection
        );
        // empty selection (single column on only left half)
        test_drag_selection_is_null(
            3.1, 2, // click
            3.0, 4,    // drag
            true, //rectangle selection
        );
        // empty selection (single column on only right half)
        test_drag_selection_is_null(
            3.9, 2, // click
            3.8, 4,    // drag
            true, //rectangle selection
        );
        // empty selection (between two columns, not crossing threshold)
        test_drag_selection_is_null(
            4.0, 2, // click
            3.9, 4,    // drag
            true, //rectangle selection
        );

        // -- Wrapping
        // LTR, do not wrap
        test_drag_selection(
            9.9, 2, // click
            0.0, 4, // drag
            9, 2, // expected start
            0, 4,    // expected end
            true, //rectangle selection
        );
        // RTL, do not wrap
        test_drag_selection(
            0.0, 4, // click
            9.9, 2, // drag
            0, 4, // expected start
            9, 2,    // expected end
            true, //rectangle selection
        );
    }

    // test "SelectionGesture press records initial click"
    #[test]
    fn press_records_initial_click() {
        let mut t = Terminal::init(Allocator, opts(5, 5)).unwrap();

        let mut gesture: SelectionGesture = SelectionGesture::INIT;

        let time = std::time::Instant::now();
        let __ev1 = test_press(&mut t, 1, 2, Some(time));
        let _ = gesture.press(&mut t, __ev1).unwrap();

        assert_eq!(1_u8, gesture.left_click_count);
        assert_eq!(time, gesture.left_click_time.unwrap());
        assert_eq!(1.0_f64, gesture.left_click_xpos);
        assert_eq!(2.0_f64, gesture.left_click_ypos);
        assert_eq!(false, gesture.left_click_dragged);

        gesture.deinit(&mut t);
        t.deinit(Allocator);
    }

    // test "SelectionGesture press returns standard click selections"
    #[test]
    fn press_returns_standard_click_selections() {
        let mut t = Terminal::init(Allocator, opts(20, 5)).unwrap();
        t.print_string(b"alpha beta\none two").unwrap();

        let mut gesture: SelectionGesture = SelectionGesture::INIT;

        let time = std::time::Instant::now();
        let boundary: [u32; 1] = [' ' as u32];
        let mut event = test_press(&mut t, 1, 0, Some(time));
        event.word_boundary_codepoints = &boundary;

        assert!(gesture.press(&mut t, event).unwrap().is_none());

        assert!(sel_eql(
            Selection::init(test_pin(&mut t, 0, 0), test_pin(&mut t, 4, 0), false),
            gesture.press(&mut t, event).unwrap().unwrap(),
        ));

        assert!(sel_eql(
            Selection::init(test_pin(&mut t, 0, 0), test_pin(&mut t, 9, 0), false),
            gesture.press(&mut t, event).unwrap().unwrap(),
        ));

        gesture.deinit(&mut t);
        t.deinit(Allocator);
    }

    // test "SelectionGesture press behaviors choose press and drag behavior"
    #[test]
    fn press_behaviors_choose_press_and_drag_behavior() {
        let mut t = Terminal::init(Allocator, opts(20, 5)).unwrap();
        t.print_string(b"alpha beta\none two\nthree four").unwrap();

        let mut gesture: SelectionGesture = SelectionGesture::INIT;

        let time = std::time::Instant::now();
        let behaviors: [Behavior; 3] = [Behavior::Cell, Behavior::Line, Behavior::Word];
        let boundary: [u32; 1] = [' ' as u32];
        let mut event = test_press(&mut t, 1, 0, Some(time));
        event.behaviors = &behaviors;
        event.word_boundary_codepoints = &boundary;

        let _ = gesture.press(&mut t, event).unwrap();
        assert_eq!(Behavior::Cell, gesture.left_click_behavior);

        let double_click = gesture.press(&mut t, event).unwrap().unwrap();
        assert_eq!(Behavior::Line, gesture.left_click_behavior);
        assert!(sel_eql(
            Selection::init(test_pin(&mut t, 0, 0), test_pin(&mut t, 9, 0), false),
            double_click,
        ));

        let __ev2 = test_drag(&mut t, 2, 2, 20.0, 50.0);
        let line_drag = gesture.drag(&mut t, __ev2).unwrap();
        assert!(sel_eql(
            Selection::init(test_pin(&mut t, 0, 0), test_pin(&mut t, 9, 2), false),
            line_drag,
        ));

        gesture.deinit(&mut t);
        t.deinit(Allocator);
    }

    // test "SelectionGesture output behavior selects and drags semantic output"
    #[test]
    fn output_behavior_selects_and_drags_semantic_output() {
        let mut t = Terminal::init(Allocator, opts(10, 6)).unwrap();

        let screen = t.screens.active;
        unsafe {
            (*screen).cursor_set_semantic_content(SemanticContentArg::Output);
            (*screen).test_write_string(b"out1\n").unwrap();
            (*screen).cursor_set_semantic_content(SemanticContentArg::Prompt(
                crate::osc::semantic_prompt::PromptKind::Initial,
            ));
            (*screen).test_write_string(b"$ ").unwrap();
            (*screen).cursor_set_semantic_content(SemanticContentArg::Input(
                crate::screen::SemanticContentInput::ClearExplicit,
            ));
            (*screen).test_write_string(b"cmd\n").unwrap();
            (*screen).cursor_set_semantic_content(SemanticContentArg::Output);
            (*screen).test_write_string(b"out2").unwrap();
        }

        let mut gesture: SelectionGesture = SelectionGesture::INIT;

        let behaviors: [Behavior; 3] = [Behavior::Output, Behavior::Word, Behavior::Line];
        let mut event = test_press(&mut t, 1, 0, Some(std::time::Instant::now()));
        event.behaviors = &behaviors;

        let press_selection = gesture.press(&mut t, event).unwrap().unwrap();
        assert_eq!(Behavior::Output, gesture.left_click_behavior);
        assert!(sel_eql(
            Selection::init(test_pin(&mut t, 0, 0), test_pin(&mut t, 3, 0), false),
            press_selection,
        ));

        let __ev3 = test_drag(&mut t, 1, 2, 10.0, 50.0);
        let output_drag = gesture.drag(&mut t, __ev3).unwrap();
        assert!(sel_eql(
            Selection::init(test_pin(&mut t, 0, 0), test_pin(&mut t, 3, 2), false),
            output_drag,
        ));

        gesture.deinit(&mut t);
        t.deinit(Allocator);
    }

    // test "SelectionGesture drag returns selection and records autoscroll"
    #[test]
    fn drag_returns_selection_and_records_autoscroll() {
        let mut t = Terminal::init(Allocator, opts(5, 5)).unwrap();

        let mut gesture: SelectionGesture = SelectionGesture::INIT;

        let mut press_event = test_press(&mut t, 1, 1, Some(std::time::Instant::now()));
        press_event.xpos = 10.0;
        let _ = gesture.press(&mut t, press_event).unwrap();

        let __ev4 = test_drag(&mut t, 3, 1, 39.0, 50.0);
        let sel = gesture.drag(&mut t, __ev4).unwrap();
        assert_eq!(Autoscroll::None, gesture.left_drag_autoscroll);
        assert_eq!(true, gesture.left_click_dragged);

        assert!(sel_eql(
            Selection::init(
                unsafe {
                    (*t.screens.active)
                        .pages
                        .pin(point::Point::Active(point::Coordinate { x: 1, y: 1 }))
                        .unwrap()
                },
                unsafe {
                    (*t.screens.active)
                        .pages
                        .pin(point::Point::Active(point::Coordinate { x: 3, y: 1 }))
                        .unwrap()
                },
                false,
            ),
            sel,
        ));

        let __ev5 = test_drag(&mut t, 3, 1, 39.0, 1.0);
        let _ = gesture.drag(&mut t, __ev5);
        assert_eq!(Autoscroll::Up, gesture.left_drag_autoscroll);

        let __ev6 = test_drag(&mut t, 3, 1, 39.0, 100.0);
        let _ = gesture.drag(&mut t, __ev6);
        assert_eq!(Autoscroll::Down, gesture.left_drag_autoscroll);

        gesture.deinit(&mut t);
        t.deinit(Allocator);
    }

    // test "SelectionGesture release clears autoscroll and records drag"
    #[test]
    fn release_clears_autoscroll_and_records_drag() {
        let mut t = Terminal::init(Allocator, opts(5, 5)).unwrap();

        let mut gesture: SelectionGesture = SelectionGesture::INIT;

        let __ev7 = test_press(&mut t, 1, 1, Some(std::time::Instant::now()));
        let _ = gesture.press(&mut t, __ev7).unwrap();
        assert_eq!(false, gesture.left_click_dragged);

        let __ev8 = test_drag(&mut t, 1, 1, 10.0, 1.0);
        let _ = gesture.drag(&mut t, __ev8);
        assert_eq!(Autoscroll::Up, gesture.left_drag_autoscroll);
        assert_eq!(false, gesture.left_click_dragged);

        let p = test_pin(&mut t, 2, 1);
        gesture.release(&mut t, Release { pin: Some(p) });
        assert_eq!(Autoscroll::None, gesture.left_drag_autoscroll);
        assert_eq!(true, gesture.left_click_dragged);

        gesture.deinit(&mut t);
        t.deinit(Allocator);
    }

    // test "SelectionGesture release with invalidated click records drag"
    #[test]
    fn release_with_invalidated_click_records_drag() {
        let mut t = Terminal::init(Allocator, opts(5, 5)).unwrap();

        let mut gesture: SelectionGesture = SelectionGesture::INIT;

        let __ev9 = test_press(&mut t, 1, 1, Some(std::time::Instant::now()));
        let _ = gesture.press(&mut t, __ev9).unwrap();
        assert_eq!(false, gesture.left_click_dragged);

        let cols = t.cols;
        let rows = t.rows;
        let _ = t
            .screens
            .get_init(Allocator, Key::Alternate, screen_opts(cols, rows, 10_000))
            .unwrap();
        t.screens.switch_to(Key::Alternate);

        let p = test_pin(&mut t, 1, 1);
        gesture.release(&mut t, Release { pin: Some(p) });
        assert_eq!(true, gesture.left_click_dragged);
        assert_eq!(Autoscroll::None, gesture.left_drag_autoscroll);

        gesture.deinit(&mut t);
        t.deinit(Allocator);
    }

    // test "SelectionGesture same-cell threshold selection records drag"
    #[test]
    fn same_cell_threshold_selection_records_drag() {
        let mut t = Terminal::init(Allocator, opts(5, 5)).unwrap();

        let mut gesture: SelectionGesture = SelectionGesture::INIT;

        let mut press_event = test_press(&mut t, 1, 1, Some(std::time::Instant::now()));
        press_event.xpos = 10.0;
        let _ = gesture.press(&mut t, press_event).unwrap();
        assert_eq!(false, gesture.left_click_dragged);

        let __ev10 = test_drag(&mut t, 1, 1, 19.0, 50.0);
        let sel = gesture.drag(&mut t, __ev10).unwrap();
        assert_eq!(true, gesture.left_click_dragged);
        assert!(sel_eql(
            Selection::init(test_pin(&mut t, 1, 1), test_pin(&mut t, 1, 1), false),
            sel,
        ));

        gesture.deinit(&mut t);
        t.deinit(Allocator);
    }

    // test "SelectionGesture drag without press returns null"
    #[test]
    fn drag_without_press_returns_null() {
        let mut t = Terminal::init(Allocator, opts(5, 5)).unwrap();

        let mut gesture: SelectionGesture = SelectionGesture::INIT;

        let __ev11 = test_drag(&mut t, 1, 1, 10.0, 50.0);
        assert!(gesture.drag(&mut t, __ev11).is_none());
        assert_eq!(Autoscroll::None, gesture.left_drag_autoscroll);

        gesture.deinit(&mut t);
        t.deinit(Allocator);
    }

    // test "SelectionGesture drag autoscroll edge boundaries"
    #[test]
    fn drag_autoscroll_edge_boundaries() {
        let mut t = Terminal::init(Allocator, opts(5, 5)).unwrap();

        let mut gesture: SelectionGesture = SelectionGesture::INIT;

        let mut press_event = test_press(&mut t, 1, 1, Some(std::time::Instant::now()));
        press_event.xpos = 10.0;
        let _ = gesture.press(&mut t, press_event).unwrap();

        let __ev12 = test_drag(&mut t, 2, 1, 20.0, 1.0);
        let _ = gesture.drag(&mut t, __ev12);
        assert_eq!(Autoscroll::Up, gesture.left_drag_autoscroll);

        let __ev13 = test_drag(&mut t, 2, 1, 20.0, 1.1);
        let _ = gesture.drag(&mut t, __ev13);
        assert_eq!(Autoscroll::None, gesture.left_drag_autoscroll);

        let __ev14 = test_drag(&mut t, 2, 1, 20.0, 99.0);
        let _ = gesture.drag(&mut t, __ev14);
        assert_eq!(Autoscroll::None, gesture.left_drag_autoscroll);

        let __ev15 = test_drag(&mut t, 2, 1, 20.0, 99.1);
        let _ = gesture.drag(&mut t, __ev15);
        assert_eq!(Autoscroll::Down, gesture.left_drag_autoscroll);

        gesture.deinit(&mut t);
        t.deinit(Allocator);
    }

    // test "SelectionGesture autoscroll tick scrolls and continues drag"
    #[test]
    fn autoscroll_tick_scrolls_and_continues_drag() {
        let mut t = Terminal::init(Allocator, opts(5, 5)).unwrap();

        let mut gesture: SelectionGesture = SelectionGesture::INIT;

        let mut press_event = test_press(&mut t, 1, 1, Some(std::time::Instant::now()));
        press_event.xpos = 10.0;
        let _ = gesture.press(&mut t, press_event).unwrap();

        let __ev16 = test_drag(&mut t, 3, 1, 39.0, 100.0);
        let _ = gesture.drag(&mut t, __ev16);
        assert_eq!(Autoscroll::Down, gesture.left_drag_autoscroll);

        let sel = gesture
            .autoscroll_tick(
                &mut t,
                test_autoscroll_tick(point::Coordinate { x: 3, y: 2 }, 39.0, 100.0),
            )
            .unwrap();
        assert_eq!(Autoscroll::Down, gesture.left_drag_autoscroll);
        assert_eq!(true, gesture.left_click_dragged);
        assert!(sel_eql(
            Selection::init(test_pin(&mut t, 1, 1), test_pin(&mut t, 3, 2), false),
            sel,
        ));

        gesture.deinit(&mut t);
        t.deinit(Allocator);
    }

    // test "SelectionGesture autoscroll tick resolves drag pin after scrolling"
    #[test]
    fn autoscroll_tick_resolves_drag_pin_after_scrolling() {
        let mut t = Terminal::init(Allocator, opts_sb(5, 3, 10)).unwrap();
        t.print_string(b"1111\n2222\n3333\n4444\n5555").unwrap();
        t.scroll_viewport(ScrollViewport::Delta(-2));

        let mut gesture: SelectionGesture = SelectionGesture::INIT;

        let mut press_event = test_press(&mut t, 1, 1, Some(std::time::Instant::now()));
        press_event.xpos = 10.0;
        let _ = gesture.press(&mut t, press_event).unwrap();

        let __ev17 = test_drag(&mut t, 3, 2, 39.0, 100.0);
        let _ = gesture.drag(&mut t, __ev17);
        assert_eq!(Autoscroll::Down, gesture.left_drag_autoscroll);

        let viewport: point::Coordinate = point::Coordinate { x: 3, y: 2 };
        let pre_scroll_pin = unsafe {
            (*t.screens.active)
                .pages
                .pin(point::Point::Viewport(viewport))
                .unwrap()
        };
        let sel = gesture
            .autoscroll_tick(&mut t, test_autoscroll_tick(viewport, 39.0, 100.0))
            .unwrap();
        let post_scroll_pin = unsafe {
            (*t.screens.active)
                .pages
                .pin(point::Point::Viewport(viewport))
                .unwrap()
        };

        assert!(!pre_scroll_pin.eql(post_scroll_pin));
        assert!(sel_eql(
            Selection::init(test_pin(&mut t, 1, 1), post_scroll_pin, false),
            sel,
        ));

        gesture.deinit(&mut t);
        t.deinit(Allocator);
    }

    // test "SelectionGesture autoscroll tick stops with invalidated click"
    #[test]
    fn autoscroll_tick_stops_with_invalidated_click() {
        let mut t = Terminal::init(Allocator, opts(5, 5)).unwrap();

        let mut gesture: SelectionGesture = SelectionGesture::INIT;

        let mut press_event = test_press(&mut t, 1, 1, Some(std::time::Instant::now()));
        press_event.xpos = 10.0;
        let _ = gesture.press(&mut t, press_event).unwrap();

        let __ev18 = test_drag(&mut t, 2, 1, 20.0, 1.0);
        let _ = gesture.drag(&mut t, __ev18);
        assert_eq!(Autoscroll::Up, gesture.left_drag_autoscroll);

        let cols = t.cols;
        let rows = t.rows;
        let _ = t
            .screens
            .get_init(Allocator, Key::Alternate, screen_opts(cols, rows, 10_000))
            .unwrap();
        t.screens.switch_to(Key::Alternate);

        assert!(gesture
            .autoscroll_tick(
                &mut t,
                test_autoscroll_tick(point::Coordinate { x: 2, y: 1 }, 20.0, 1.0)
            )
            .is_none());
        assert_eq!(Autoscroll::None, gesture.left_drag_autoscroll);
        assert_eq!(0_u8, gesture.left_click_count);

        gesture.deinit(&mut t);
        t.deinit(Allocator);
    }

    // test "SelectionGesture deep press selects word and consumes drag"
    #[test]
    fn deep_press_selects_word_and_consumes_drag() {
        let mut t = Terminal::init(Allocator, opts(20, 5)).unwrap();
        t.print_string(b"alpha beta").unwrap();

        let mut gesture: SelectionGesture = SelectionGesture::INIT;

        let __ev19 = test_press(&mut t, 1, 0, Some(std::time::Instant::now()));
        let _ = gesture.press(&mut t, __ev19).unwrap();
        let __ev20 = test_drag(&mut t, 1, 0, 10.0, 1.0);
        let _ = gesture.drag(&mut t, __ev20);
        assert_eq!(Autoscroll::Up, gesture.left_drag_autoscroll);

        let boundary: [u32; 1] = [' ' as u32];
        let sel = gesture
            .deep_press(
                &mut t,
                DeepPress {
                    word_boundary_codepoints: &boundary,
                },
            )
            .unwrap();

        assert!(sel_eql(
            Selection::init(test_pin(&mut t, 0, 0), test_pin(&mut t, 4, 0), false),
            sel,
        ));
        assert_eq!(0_u8, gesture.left_click_count);
        assert_eq!(None, gesture.left_click_time);
        assert_eq!(true, gesture.left_click_dragged);
        assert_eq!(Autoscroll::None, gesture.left_drag_autoscroll);
        assert!(gesture.left_click_pin.is_none());

        let __ev21 = test_drag(&mut t, 7, 0, 70.0, 50.0);
        assert!(gesture.drag(&mut t, __ev21).is_none());
        let p = test_pin(&mut t, 7, 0);
        gesture.release(&mut t, Release { pin: Some(p) });
        assert_eq!(true, gesture.left_click_dragged);

        gesture.deinit(&mut t);
        t.deinit(Allocator);
    }

    // test "SelectionGesture drag with invalidated click returns null"
    #[test]
    fn drag_with_invalidated_click_returns_null() {
        let mut t = Terminal::init(Allocator, opts(5, 5)).unwrap();

        let mut gesture: SelectionGesture = SelectionGesture::INIT;

        let mut press_event = test_press(&mut t, 1, 1, Some(std::time::Instant::now()));
        press_event.xpos = 10.0;
        let _ = gesture.press(&mut t, press_event).unwrap();

        let __ev22 = test_drag(&mut t, 2, 1, 20.0, 1.0);
        let _ = gesture.drag(&mut t, __ev22);
        assert_eq!(Autoscroll::Up, gesture.left_drag_autoscroll);

        let cols = t.cols;
        let rows = t.rows;
        let _ = t
            .screens
            .get_init(Allocator, Key::Alternate, screen_opts(cols, rows, 10_000))
            .unwrap();
        t.screens.switch_to(Key::Alternate);

        let __ev23 = test_drag(&mut t, 2, 1, 20.0, 50.0);
        assert!(gesture.drag(&mut t, __ev23).is_none());
        assert_eq!(Autoscroll::Up, gesture.left_drag_autoscroll);

        gesture.deinit(&mut t);
        t.deinit(Allocator);
    }

    // test "SelectionGesture double-click drag selects by word"
    #[test]
    fn double_click_drag_selects_by_word() {
        let mut t = Terminal::init(Allocator, opts(20, 5)).unwrap();
        t.print_string(b"alpha beta gamma").unwrap();

        let mut gesture: SelectionGesture = SelectionGesture::INIT;

        let time = std::time::Instant::now();
        let __ev24 = test_press(&mut t, 1, 0, Some(time));
        let _ = gesture.press(&mut t, __ev24).unwrap();
        let __ev25 = test_press(&mut t, 1, 0, Some(time));
        let _ = gesture.press(&mut t, __ev25).unwrap();

        let boundary: [u32; 1] = [' ' as u32];
        let mut drag_event = test_drag(&mut t, 7, 0, 70.0, 50.0);
        drag_event.word_boundary_codepoints = &boundary;
        let sel = gesture.drag(&mut t, drag_event).unwrap();

        assert!(sel_eql(
            Selection::init(test_pin(&mut t, 0, 0), test_pin(&mut t, 9, 0), false),
            sel,
        ));

        gesture.deinit(&mut t);
        t.deinit(Allocator);
    }

    // test "SelectionGesture double-click drag selects by word backwards"
    #[test]
    fn double_click_drag_selects_by_word_backwards() {
        let mut t = Terminal::init(Allocator, opts(20, 5)).unwrap();
        t.print_string(b"alpha beta gamma").unwrap();

        let mut gesture: SelectionGesture = SelectionGesture::INIT;

        let time = std::time::Instant::now();
        let __ev26 = test_press(&mut t, 7, 0, Some(time));
        let _ = gesture.press(&mut t, __ev26).unwrap();
        let __ev27 = test_press(&mut t, 7, 0, Some(time));
        let _ = gesture.press(&mut t, __ev27).unwrap();

        let boundary: [u32; 1] = [' ' as u32];
        let mut drag_event = test_drag(&mut t, 1, 0, 10.0, 50.0);
        drag_event.word_boundary_codepoints = &boundary;
        let sel = gesture.drag(&mut t, drag_event).unwrap();

        assert!(sel_eql(
            Selection::init(test_pin(&mut t, 0, 0), test_pin(&mut t, 9, 0), false),
            sel,
        ));

        gesture.deinit(&mut t);
        t.deinit(Allocator);
    }

    // test "SelectionGesture double-click drag on empty cell selects nearest word"
    #[test]
    fn double_click_drag_on_empty_cell_selects_nearest_word() {
        let mut t = Terminal::init(Allocator, opts(20, 5)).unwrap();
        t.print_string(b"alpha beta").unwrap();

        let mut gesture: SelectionGesture = SelectionGesture::INIT;

        let time = std::time::Instant::now();
        let __ev28 = test_press(&mut t, 1, 0, Some(time));
        let _ = gesture.press(&mut t, __ev28).unwrap();
        let __ev29 = test_press(&mut t, 1, 0, Some(time));
        let _ = gesture.press(&mut t, __ev29).unwrap();

        let boundary: [u32; 1] = [' ' as u32];
        let mut drag_event = test_drag(&mut t, 15, 0, 150.0, 50.0);
        drag_event.word_boundary_codepoints = &boundary;
        let sel = gesture.drag(&mut t, drag_event).unwrap();

        assert!(sel_eql(
            Selection::init(test_pin(&mut t, 0, 0), test_pin(&mut t, 9, 0), false),
            sel,
        ));

        gesture.deinit(&mut t);
        t.deinit(Allocator);
    }

    // test "SelectionGesture triple-click drag selects by line"
    #[test]
    fn triple_click_drag_selects_by_line() {
        let mut t = Terminal::init(Allocator, opts(20, 5)).unwrap();
        t.print_string(b"alpha beta\none two\nthree four").unwrap();

        let mut gesture: SelectionGesture = SelectionGesture::INIT;

        let time = std::time::Instant::now();
        let __ev30 = test_press(&mut t, 1, 0, Some(time));
        let _ = gesture.press(&mut t, __ev30).unwrap();
        let __ev31 = test_press(&mut t, 1, 0, Some(time));
        let _ = gesture.press(&mut t, __ev31).unwrap();
        let __ev32 = test_press(&mut t, 1, 0, Some(time));
        let _ = gesture.press(&mut t, __ev32).unwrap();

        let __ev33 = test_drag(&mut t, 2, 2, 20.0, 50.0);
        let sel = gesture.drag(&mut t, __ev33).unwrap();

        assert!(sel_eql(
            Selection::init(test_pin(&mut t, 0, 0), test_pin(&mut t, 9, 2), false),
            sel,
        ));

        gesture.deinit(&mut t);
        t.deinit(Allocator);
    }

    // test "SelectionGesture triple-click drag selects by line backwards"
    #[test]
    fn triple_click_drag_selects_by_line_backwards() {
        let mut t = Terminal::init(Allocator, opts(20, 5)).unwrap();
        t.print_string(b"alpha beta\none two\nthree four").unwrap();

        let mut gesture: SelectionGesture = SelectionGesture::INIT;

        let time = std::time::Instant::now();
        let __ev34 = test_press(&mut t, 2, 2, Some(time));
        let _ = gesture.press(&mut t, __ev34).unwrap();
        let __ev35 = test_press(&mut t, 2, 2, Some(time));
        let _ = gesture.press(&mut t, __ev35).unwrap();
        let __ev36 = test_press(&mut t, 2, 2, Some(time));
        let _ = gesture.press(&mut t, __ev36).unwrap();

        let __ev37 = test_drag(&mut t, 1, 0, 10.0, 50.0);
        let sel = gesture.drag(&mut t, __ev37).unwrap();

        assert!(sel_eql(
            Selection::init(test_pin(&mut t, 0, 0), test_pin(&mut t, 9, 2), false),
            sel,
        ));

        gesture.deinit(&mut t);
        t.deinit(Allocator);
    }

    // test "SelectionGesture repeat increments click count"
    #[test]
    fn repeat_increments_click_count() {
        let mut t = Terminal::init(Allocator, opts(5, 5)).unwrap();

        let mut gesture: SelectionGesture = SelectionGesture::INIT;

        let time = std::time::Instant::now();
        let __ev38 = test_press(&mut t, 1, 1, Some(time));
        let _ = gesture.press(&mut t, __ev38).unwrap();
        let __ev39 = test_press(&mut t, 1, 1, Some(time));
        let _ = gesture.press(&mut t, __ev39).unwrap();

        assert_eq!(2_u8, gesture.left_click_count);

        gesture.deinit(&mut t);
        t.deinit(Allocator);
    }

    // test "SelectionGesture repeat clamps at triple click"
    #[test]
    fn repeat_clamps_at_triple_click() {
        let mut t = Terminal::init(Allocator, opts(5, 5)).unwrap();

        let mut gesture: SelectionGesture = SelectionGesture::INIT;

        let time = std::time::Instant::now();
        for _ in 0..4 {
            let __ev40 = test_press(&mut t, 1, 1, Some(time));
            let _ = gesture.press(&mut t, __ev40).unwrap();
        }

        assert_eq!(3_u8, gesture.left_click_count);

        gesture.deinit(&mut t);
        t.deinit(Allocator);
    }

    // test "SelectionGesture null initial time stays single click"
    #[test]
    fn null_initial_time_stays_single_click() {
        let mut t = Terminal::init(Allocator, opts(5, 5)).unwrap();

        let mut gesture: SelectionGesture = SelectionGesture::INIT;

        let __ev41 = test_press(&mut t, 1, 1, None);
        let _ = gesture.press(&mut t, __ev41).unwrap();
        let __ev42 = test_press(&mut t, 1, 1, Some(std::time::Instant::now()));
        let _ = gesture.press(&mut t, __ev42).unwrap();

        assert_eq!(1_u8, gesture.left_click_count);
        assert!(gesture.left_click_time.is_some());

        gesture.deinit(&mut t);
        t.deinit(Allocator);
    }

    // test "SelectionGesture null repeat time stays single click"
    #[test]
    fn null_repeat_time_stays_single_click() {
        let mut t = Terminal::init(Allocator, opts(5, 5)).unwrap();

        let mut gesture: SelectionGesture = SelectionGesture::INIT;

        let __ev43 = test_press(&mut t, 1, 1, Some(std::time::Instant::now()));
        let _ = gesture.press(&mut t, __ev43).unwrap();
        let __ev44 = test_press(&mut t, 1, 1, None);
        let _ = gesture.press(&mut t, __ev44).unwrap();

        assert_eq!(1_u8, gesture.left_click_count);
        assert_eq!(None, gesture.left_click_time);

        gesture.deinit(&mut t);
        t.deinit(Allocator);
    }

    // test "SelectionGesture distant press resets click count"
    #[test]
    fn distant_press_resets_click_count() {
        let mut t = Terminal::init(Allocator, opts(5, 5)).unwrap();

        let mut gesture: SelectionGesture = SelectionGesture::INIT;

        let time = std::time::Instant::now();
        let __ev45 = test_press(&mut t, 1, 1, Some(time));
        let _ = gesture.press(&mut t, __ev45).unwrap();
        let __ev46 = test_press(&mut t, 4, 1, Some(time));
        let _ = gesture.press(&mut t, __ev46).unwrap();

        assert_eq!(1_u8, gesture.left_click_count);
        assert_eq!(4.0_f64, gesture.left_click_xpos);

        gesture.deinit(&mut t);
        t.deinit(Allocator);
    }

    // test "SelectionGesture expired repeat resets click count"
    #[test]
    fn expired_repeat_resets_click_count() {
        let mut t = Terminal::init(Allocator, opts(5, 5)).unwrap();

        let mut gesture: SelectionGesture = SelectionGesture::INIT;

        let mut event = test_press(&mut t, 1, 1, Some(std::time::Instant::now()));
        event.repeat_interval = 0;
        let _ = gesture.press(&mut t, event).unwrap();

        // PORT NOTE: `std.Thread.sleep(std.time.ns_per_ms)`; `ns_per_ms` is
        // 1_000_000 nanoseconds.
        std::thread::sleep(std::time::Duration::from_nanos(1_000_000));
        event.time = Some(std::time::Instant::now());
        let _ = gesture.press(&mut t, event).unwrap();

        assert_eq!(1_u8, gesture.left_click_count);

        gesture.deinit(&mut t);
        t.deinit(Allocator);
    }

    // test "SelectionGesture backwards repeat time resets click count"
    // PORT NOTE (parity 8f1c2fe95): test hunk of the upstream backwards-
    // timestamp fix. The reference builds instants from raw ns via a
    // `testInstant` helper; Rust's `std::time::Instant` is opaque (no raw
    // constructor), so the two ordered instants derive from one `now()` anchor
    // plus Duration offsets, preserving the same earlier/later relationship.
    #[test]
    fn backwards_repeat_time_resets_click_count() {
        let mut t = Terminal::init(Allocator, opts(5, 5)).unwrap();

        let mut gesture: SelectionGesture = SelectionGesture::INIT;

        // const earlier = testInstant(std.time.ns_per_s);
        // const later = testInstant(2 * std.time.ns_per_s);
        let anchor = std::time::Instant::now();
        let earlier = anchor + std::time::Duration::from_secs(1);
        let later = anchor + std::time::Duration::from_secs(2);

        let __ev_later = test_press(&mut t, 1, 1, Some(later));
        let _ = gesture.press(&mut t, __ev_later).unwrap();
        let __ev_earlier = test_press(&mut t, 1, 1, Some(earlier));
        let _ = gesture.press(&mut t, __ev_earlier).unwrap();

        // try testing.expectEqual(@as(u3, 1), gesture.left_click_count);
        assert_eq!(1_u8, gesture.left_click_count);
        // try testing.expectEqual(.eq, gesture.left_click_time.?.order(earlier));
        assert_eq!(Some(earlier), gesture.left_click_time);

        gesture.deinit(&mut t);
        t.deinit(Allocator);
    }

    // test "SelectionGesture screen switch resets click count"
    #[test]
    fn screen_switch_resets_click_count() {
        let mut t = Terminal::init(Allocator, opts(5, 5)).unwrap();

        let mut gesture: SelectionGesture = SelectionGesture::INIT;

        let time = std::time::Instant::now();
        let primary_tracked = unsafe { (*t.screens.active).pages.count_tracked_pins() };
        let __ev47 = test_press(&mut t, 1, 1, Some(time));
        let _ = gesture.press(&mut t, __ev47).unwrap();

        let cols = t.cols;
        let rows = t.rows;
        let _ = t
            .screens
            .get_init(Allocator, Key::Alternate, screen_opts(cols, rows, 10_000))
            .unwrap();
        t.screens.switch_to(Key::Alternate);
        let __ev48 = test_press(&mut t, 1, 1, Some(time));
        let _ = gesture.press(&mut t, __ev48).unwrap();

        assert_eq!(1_u8, gesture.left_click_count);
        assert_eq!(Key::Alternate, gesture.left_click_screen);
        assert_eq!(primary_tracked, unsafe {
            (*t.screens.get(Key::Primary).unwrap())
                .pages
                .count_tracked_pins()
        });

        gesture.deinit(&mut t);
        t.deinit(Allocator);
    }

    // test "SelectionGesture removed screen resets without untracking stale pin"
    #[test]
    fn removed_screen_resets_without_untracking_stale_pin() {
        let mut t = Terminal::init(Allocator, opts(5, 5)).unwrap();

        let mut gesture: SelectionGesture = SelectionGesture::INIT;

        let cols = t.cols;
        let rows = t.rows;
        let _ = t
            .screens
            .get_init(Allocator, Key::Alternate, screen_opts(cols, rows, 10_000))
            .unwrap();
        t.screens.switch_to(Key::Alternate);
        let __ev49 = test_press(&mut t, 1, 1, Some(std::time::Instant::now()));
        let _ = gesture.press(&mut t, __ev49).unwrap();

        t.screens.switch_to(Key::Primary);
        t.screens.remove(Allocator, Key::Alternate);
        let __ev50 = test_press(&mut t, 1, 1, Some(std::time::Instant::now()));
        let _ = gesture.press(&mut t, __ev50).unwrap();

        assert_eq!(1_u8, gesture.left_click_count);
        assert_eq!(Key::Primary, gesture.left_click_screen);

        gesture.deinit(&mut t);
        t.deinit(Allocator);
    }

    // test "SelectionGesture deinit untracks pin"
    #[test]
    fn deinit_untracks_pin() {
        let mut t = Terminal::init(Allocator, opts(5, 5)).unwrap();

        let mut gesture: SelectionGesture = SelectionGesture::INIT;
        let tracked = unsafe { (*t.screens.active).pages.count_tracked_pins() };
        let __ev51 = test_press(&mut t, 1, 1, Some(std::time::Instant::now()));
        let _ = gesture.press(&mut t, __ev51).unwrap();
        assert_eq!(tracked + 1, unsafe {
            (*t.screens.active).pages.count_tracked_pins()
        });

        gesture.deinit(&mut t);
        assert_eq!(tracked, unsafe {
            (*t.screens.active).pages.count_tracked_pins()
        });

        t.deinit(Allocator);
    }
}
