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
    /// Translates key state to mouse shape, called during key events. This mainly
    /// handles overrides on key presses depending on whether or not we are in
    /// mouse tracking mode, however it is also responsible for resetting cursor
    /// state on any particular key releases.
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

        // Handle possible overrides depending on mouse tracking state.
        // switch (self.mouse_event != .none) {
        match self.mouse_event != MouseEvent::None {
            true => {
                // In mouse tracking mode
                if Self::is_mouse_mode_override_state(self.mods)
                    && Self::is_rectangle_select_state(self.mods)
                {
                    // Crosshair (rectangle select), only set if we are also
                    // overriding (e.g. shift+ctrl+alt)
                    return Some(MouseShape::Crosshair);
                } else if Self::is_mouse_mode_override_state(self.mods) {
                    // Normal override state
                    return Some(MouseShape::Text);
                }
            }

            false => {
                // Default terminal mode
                if Self::is_rectangle_select_state(self.mods) {
                    // Crosshair (rectangle select)
                    return Some(MouseShape::Crosshair);
                } else if Self::is_mouse_mode_override_state(self.mods) {
                    // Shift shows an I-beam so selection is obvious even when
                    // the application cursor is not text (OSC 22). Release
                    // restores mouse_shape below.
                    return Some(MouseShape::Text);
                }
            }
        }

        // No overrides means we just revert back to the stored terminal mouse
        // shape. Note that this may be different than what has been currently sent
        // to the apprt, so this will force the reset.
        Some(self.mouse_shape)
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
    // block with its 12 sub-blocks in order (rust >= zig; CORE_BLUEPRINT §7).
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
            // no override restores the application shape (mouse tracking)
            let m = SurfaceMouse {
                physical_key: Key::AltLeft,
                mouse_event: MouseEvent::X10,
                mouse_shape: MouseShape::Crosshair,
                mods: Mods::default(),
                over_link: false,
                hidden: false,
            };

            let want: MouseShape = MouseShape::Crosshair;
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
            // no override restores the application shape (no mouse tracking)
            let m = SurfaceMouse {
                physical_key: Key::AltLeft,
                mouse_event: MouseEvent::None,
                mouse_shape: MouseShape::Crosshair,
                mods: Mods::default(),
                over_link: false,
                hidden: false,
            };

            let want: MouseShape = MouseShape::Crosshair;
            let got = m.key_to_mouse_shape();
            assert_eq!(Some(want), got);
        }
    }

    // PORT NOTE: Rust-only (rust >= zig). Upstream's test above has no sub-block
    // for 3a766ccf5 (shift shows the I-beam without mouse tracking) or for the
    // OSC 22 case 5dfb67298 fixes (release restores a non-text application
    // shape). Both are pinned here with an application shape that is neither of
    // the pin's hard-coded fallbacks (`.default`/`.text`).
    #[test]
    fn key_to_mouse_shape_osc22_shape_with_shift() {
        {
            // shift held, no mouse tracking: I-beam over an OSC 22 pointer
            let m = SurfaceMouse {
                physical_key: Key::ShiftLeft,
                mouse_event: MouseEvent::None,
                mouse_shape: MouseShape::Pointer,
                mods: Mods {
                    shift: true,
                    ..Default::default()
                },
                over_link: false,
                hidden: false,
            };

            assert_eq!(Some(MouseShape::Text), m.key_to_mouse_shape());
        }

        {
            // shift released, no mouse tracking: the OSC 22 pointer comes back
            let m = SurfaceMouse {
                physical_key: Key::ShiftLeft,
                mouse_event: MouseEvent::None,
                mouse_shape: MouseShape::Pointer,
                mods: Mods::default(),
                over_link: false,
                hidden: false,
            };

            assert_eq!(Some(MouseShape::Pointer), m.key_to_mouse_shape());
        }

        {
            // shift released, mouse tracking: the OSC 22 pointer comes back
            let m = SurfaceMouse {
                physical_key: Key::ShiftLeft,
                mouse_event: MouseEvent::X10,
                mouse_shape: MouseShape::Pointer,
                mods: Mods::default(),
                over_link: false,
                hidden: false,
            };

            assert_eq!(Some(MouseShape::Pointer), m.key_to_mouse_shape());
        }
    }
}
