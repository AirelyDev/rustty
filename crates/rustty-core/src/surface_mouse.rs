//! SurfaceMouse — mouse helper functionality for the core surface.
//!
//! PORTS: ghostty src/surface_mouse.zig
//!
//! PORT STATUS: done

// const std = @import("std");
// const builtin = @import("builtin");
// PORT NOTE: `builtin.target.os.tag.isDarwin()` maps to
// `cfg!(target_vendor = "apple")` (the established convention: vt input/key.rs
// `ctrl_or_super`, rustty-input `ctrl_or_super`).
// const input = @import("input.zig");
// PORT NOTE: the `input` hub's `Key`/`Mods` live in rustty-vt's VT input slice
// (input/key.rs + input/key_mods.rs), not rustty-input (which holds the rest
// of src/input/).
use rustty_vt::input::key::Key;
use rustty_vt::input::key_mods::Mods;
// const terminal = @import("terminal/main.zig");
// const MouseShape = terminal.MouseShape;
use rustty_vt::mouse::Event as MouseEvent;
use rustty_vt::mouse::Shape as MouseShape;

/// SurfaceMouse represents mouse helper functionality for the core surface.
///
/// It's currently small in scope; its purpose is to isolate mouse logic that
/// has gotten a bit complex (e.g. pointer shape handling for key events), but
/// the intention is to grow it later so that we can better test said logic).
// const SurfaceMouse = @This();
// PORT NOTE: the reference file IS the struct (`@This()`); its file-scope fns
// live in the struct namespace, so they port as associated functions below
// (Surface.zig calls `SurfaceMouse.isRectangleSelectState(...)` — the
// namespace-qualified shape). All fields are plain `Copy` data, so the
// by-value `self: SurfaceMouse` receiver ports as a by-value `self`.
#[derive(Clone, Copy, Debug)]
pub struct SurfaceMouse {
    /// For processing key events; the key that was physically pressed on the
    /// keyboard.
    pub physical_key: Key,

    /// The mouse event tracking mode, if any.
    pub mouse_event: MouseEvent,

    /// The current terminal's mouse shape.
    pub mouse_shape: MouseShape,

    /// The last mods state when the last mouse button (whatever it was) was
    /// pressed or release.
    pub mods: Mods,

    /// True if the mouse position is currently over a link.
    pub over_link: bool,

    /// True if the mouse pointer is currently hidden.
    pub hidden: bool,
}

impl SurfaceMouse {
    /// Translates key state to mouse shape (cursor) state, based on a state
    /// machine.
    ///
    /// There are 4 current states:
    ///
    /// * text: starting state, displays a text bar.
    /// * default: default state when in a mouse tracking mode. (e.g. vim, etc).
    ///   Displays an arrow pointer.
    /// * pointer: default state when over a link. Displays a pointing finger.
    /// * crosshair: any above state can transition to this when the rectangle
    ///   select keys are pressed (ctrl/super+alt).
    ///
    /// Additionally, default can transition back to text if one of the shift keys
    /// are pressed during mouse tracking mode.
    ///
    /// Any secondary state transitions back to its default state when the
    /// appropriate keys are released.
    ///
    /// null is returned when the mouse shape does not need changing.
    // pub fn keyToMouseShape(self: SurfaceMouse) ?MouseShape {
    pub fn key_to_mouse_shape(self) -> Option<MouseShape> {
        // Filter for appropriate key events
        // if (!eligibleMouseShapeKeyEvent(self.physical_key)) return null;
        if !Self::eligible_mouse_shape_key_event(self.physical_key) {
            return None;
        }

        // Exceptions: link hover or hidden state overrides any other shape
        // processing and does not change state.
        //
        // TODO: As we unravel mouse state, we can fix this to be more explicit.
        if self.over_link || self.hidden {
            return None;
        }

        // Set our current default state
        // PORT NOTE: `var current_shape_state: MouseShape = undefined;` — Rust
        // has no `undefined`; the if/else below initializes it on both paths
        // exactly as the reference's two assignments do.
        let current_shape_state: MouseShape = if self.mouse_event != MouseEvent::None {
            // In mouse tracking mode, should be default (arrow pointer)
            MouseShape::Default
        } else {
            // Default terminal mode, should be text (text bar)
            MouseShape::Text
        };

        // Transition table.
        //
        // TODO: This could be updated eventually to be a true transition table if
        // we move to a full stateful mouse surface, e.g. very specific inputs
        // transitioning state based on previous state, versus flags like "is the
        // mouse over a link", etc.
        match current_shape_state {
            MouseShape::Default => {
                if Self::is_mouse_mode_override_state(self.mods)
                    && Self::is_rectangle_select_state(self.mods)
                {
                    // Crosshair (rectangle select), only set if we are also
                    // overriding (e.g. shift+ctrl+alt)
                    Some(MouseShape::Crosshair)
                } else if Self::is_mouse_mode_override_state(self.mods) {
                    // Normal override state
                    Some(MouseShape::Text)
                } else {
                    Some(MouseShape::Default)
                }
            }

            MouseShape::Text => {
                if Self::is_rectangle_select_state(self.mods) {
                    // Crosshair (rectangle select)
                    Some(MouseShape::Crosshair)
                } else {
                    Some(MouseShape::Text)
                }
            }

            // Fall back on default state
            // else => unreachable,
            _ => unreachable!(),
        }
    }

    // fn eligibleMouseShapeKeyEvent(physical_key: input.Key) bool {
    fn eligible_mouse_shape_key_event(physical_key: Key) -> bool {
        // return physical_key.ctrlOrSuper() or
        //     physical_key.leftOrRightShift() or
        //     physical_key.leftOrRightAlt();
        physical_key.ctrl_or_super()
            || physical_key.left_or_right_shift()
            || physical_key.left_or_right_alt()
    }

    // fn isMouseModeOverrideState(mods: input.Mods) bool {
    fn is_mouse_mode_override_state(mods: Mods) -> bool {
        // return mods.shift;
        mods.shift
    }

    /// Returns true if our modifiers put us in a state where dragging
    /// should cause a rectangle select.
    // pub fn isRectangleSelectState(mods: input.Mods) bool {
    pub fn is_rectangle_select_state(mods: Mods) -> bool {
        // return if (comptime builtin.target.os.tag.isDarwin())
        //     mods.alt
        // else
        //     mods.ctrlOrSuper() and mods.alt;
        if cfg!(target_vendor = "apple") {
            mods.alt
        } else {
            mods.ctrl_or_super() && mods.alt
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // test "keyToMouseShape" {
    // PORT NOTE: ported as ONE Rust test mirroring the single reference test{}
    // block with its 13 sub-blocks in order (rust >= zig; CORE_BLUEPRINT §7).
    #[test]
    fn key_to_mouse_shape() {
        // const testing = std.testing;

        {
            // No specific key pressed
            let m = SurfaceMouse {
                physical_key: Key::Unidentified,
                mouse_event: MouseEvent::None,
                mouse_shape: MouseShape::Progress,
                mods: Mods::default(),
                over_link: false,
                hidden: false,
            };

            // const got = m.keyToMouseShape();
            let got = m.key_to_mouse_shape();
            // try testing.expect(got == null);
            assert!(got.is_none());
        }

        {
            // Over a link. NOTE: This tests that we don't touch the inbound state,
            // not necessarily if we're over a link.
            let m = SurfaceMouse {
                physical_key: Key::ShiftLeft,
                mouse_event: MouseEvent::None,
                mouse_shape: MouseShape::Progress,
                mods: Mods::default(),
                over_link: true,
                hidden: false,
            };

            let got = m.key_to_mouse_shape();
            assert!(got.is_none());
        }

        {
            // Mouse is currently hidden
            let m = SurfaceMouse {
                physical_key: Key::ShiftLeft,
                mouse_event: MouseEvent::None,
                mouse_shape: MouseShape::Progress,
                mods: Mods::default(),
                over_link: true,
                hidden: true,
            };

            let got = m.key_to_mouse_shape();
            assert!(got.is_none());
        }

        {
            // default, no mods (mouse tracking)
            let m = SurfaceMouse {
                physical_key: Key::ShiftLeft,
                mouse_event: MouseEvent::X10,
                mouse_shape: MouseShape::Default,
                mods: Mods::default(),
                over_link: false,
                hidden: false,
            };

            // const want: MouseShape = .default;
            let want: MouseShape = MouseShape::Default;
            let got = m.key_to_mouse_shape();
            // try testing.expect(want == got);
            assert_eq!(Some(want), got);
        }

        {
            // default -> crosshair (mouse tracking)
            let m = SurfaceMouse {
                physical_key: Key::AltLeft,
                mouse_event: MouseEvent::X10,
                mouse_shape: MouseShape::Default,
                // .mods = .{ .ctrl = true, .super = true, .alt = true, .shift = true },
                mods: Mods {
                    ctrl: true,
                    super_: true,
                    alt: true,
                    shift: true,
                    ..Default::default()
                },
                over_link: false,
                hidden: false,
            };

            let want: MouseShape = MouseShape::Crosshair;
            let got = m.key_to_mouse_shape();
            assert_eq!(Some(want), got);
        }

        {
            // default -> text (mouse tracking)
            let m = SurfaceMouse {
                physical_key: Key::ShiftLeft,
                mouse_event: MouseEvent::X10,
                mouse_shape: MouseShape::Default,
                // .mods = .{ .shift = true },
                mods: Mods {
                    shift: true,
                    ..Default::default()
                },
                over_link: false,
                hidden: false,
            };

            let want: MouseShape = MouseShape::Text;
            let got = m.key_to_mouse_shape();
            assert_eq!(Some(want), got);
        }

        {
            // crosshair -> text (mouse tracking)
            let m = SurfaceMouse {
                physical_key: Key::AltLeft,
                mouse_event: MouseEvent::X10,
                mouse_shape: MouseShape::Crosshair,
                mods: Mods {
                    shift: true,
                    ..Default::default()
                },
                over_link: false,
                hidden: false,
            };

            let want: MouseShape = MouseShape::Text;
            let got = m.key_to_mouse_shape();
            assert_eq!(Some(want), got);
        }

        {
            // crosshair -> default (mouse tracking)
            let m = SurfaceMouse {
                physical_key: Key::AltLeft,
                mouse_event: MouseEvent::X10,
                mouse_shape: MouseShape::Crosshair,
                mods: Mods::default(),
                over_link: false,
                hidden: false,
            };

            let want: MouseShape = MouseShape::Default;
            let got = m.key_to_mouse_shape();
            assert_eq!(Some(want), got);
        }

        {
            // text -> crosshair (mouse tracking)
            let m = SurfaceMouse {
                physical_key: Key::AltLeft,
                mouse_event: MouseEvent::X10,
                mouse_shape: MouseShape::Text,
                mods: Mods {
                    ctrl: true,
                    super_: true,
                    alt: true,
                    shift: true,
                    ..Default::default()
                },
                over_link: false,
                hidden: false,
            };

            let want: MouseShape = MouseShape::Crosshair;
            let got = m.key_to_mouse_shape();
            assert_eq!(Some(want), got);
        }

        {
            // text -> default (mouse tracking)
            let m = SurfaceMouse {
                physical_key: Key::ShiftLeft,
                mouse_event: MouseEvent::X10,
                mouse_shape: MouseShape::Text,
                mods: Mods::default(),
                over_link: false,
                hidden: false,
            };

            let want: MouseShape = MouseShape::Default;
            let got = m.key_to_mouse_shape();
            assert_eq!(Some(want), got);
        }

        {
            // text, no mods (no mouse tracking)
            let m = SurfaceMouse {
                physical_key: Key::ShiftLeft,
                mouse_event: MouseEvent::None,
                mouse_shape: MouseShape::Text,
                mods: Mods::default(),
                over_link: false,
                hidden: false,
            };

            let want: MouseShape = MouseShape::Text;
            let got = m.key_to_mouse_shape();
            assert_eq!(Some(want), got);
        }

        {
            // text -> crosshair (no mouse tracking)
            let m = SurfaceMouse {
                physical_key: Key::AltLeft,
                mouse_event: MouseEvent::None,
                mouse_shape: MouseShape::Text,
                // .mods = .{ .ctrl = true, .super = true, .alt = true },
                mods: Mods {
                    ctrl: true,
                    super_: true,
                    alt: true,
                    ..Default::default()
                },
                over_link: false,
                hidden: false,
            };

            let want: MouseShape = MouseShape::Crosshair;
            let got = m.key_to_mouse_shape();
            assert_eq!(Some(want), got);
        }

        {
            // crosshair -> text (no mouse tracking)
            let m = SurfaceMouse {
                physical_key: Key::AltLeft,
                mouse_event: MouseEvent::None,
                mouse_shape: MouseShape::Crosshair,
                mods: Mods::default(),
                over_link: false,
                hidden: false,
            };

            let want: MouseShape = MouseShape::Text;
            let got = m.key_to_mouse_shape();
            assert_eq!(Some(want), got);
        }
    }
}
