//! rustty-vt input/mouse
//!
//! PORTS: ghostty src/input/mouse.zig
//!
//! PORT STATUS: done

// PORT NOTE: the reference imports `const std = @import("std");` only for its
// `std.testing` usage inside the `ScrollMods` test; no runtime std use, so there
// is no import to mirror here.

/// C: RusttyMouseAction (R2) — checked by the A5 enum-discriminant arm.
/// The type of action associated with a mouse event. This is different
/// from ButtonState because button state is simply the current state
/// of a mouse button but an action is something that triggers via
/// an GUI event and supports more.
// PORT NOTE: `enum(c_int)` — backed by C `int` for the embedding API. Mirrored
// with an explicit `#[repr(C)]` enum.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Press,
    Release,
    Motion,
}

/// The state of a mouse button.
///
/// This is backed by a c_int so we can use this as-is for our embedding API.
///
/// IMPORTANT: Any changes here update include/rustty-embed.h (renamed from
/// the reference's include/ghostty.h)
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonState {
    Release,
    Press,
}

/// C: RusttyMouseButton (R2) — checked by the A5 enum-discriminant arm.
/// Possible mouse buttons. We only track up to 11 because that's the maximum
/// button input that terminal mouse tracking handles without becoming
/// ambiguous.
///
/// Its a bit silly to name numbers like this but given its a restricted
/// set, it feels better than passing around raw numeric literals.
///
/// This is backed by a c_int so we can use this as-is for our embedding API.
///
/// IMPORTANT: Any changes here update include/rustty-embed.h (renamed from
/// the reference's include/ghostty.h)
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    Unknown = 0,
    Left = 1,
    Right = 2,
    Middle = 3,
    Four = 4,
    Five = 5,
    Six = 6,
    Seven = 7,
    Eight = 8,
    Nine = 9,
    Ten = 10,
    Eleven = 11,
}

impl Button {
    /// The maximum value in this enum. This can be used to create a densely
    /// packed array, for example.
    // PORT NOTE: the reference computes this at comptime by iterating the enum
    // fields and taking the largest `field.value`. The largest tag here is
    // `eleven = 11`, so the result is a constant `11`.
    pub const MAX: i32 = {
        let mut cur = 0;
        let values = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11];
        let mut i = 0;
        while i < values.len() {
            if values[i] > cur {
                cur = values[i];
            }
            i += 1;
        }
        cur
    };
}

/// The "momentum" of a mouse scroll event. This matches the macOS events
/// because it is the only reliable source right now of momentum events.
/// This is used to handle "inertial scrolling" (i.e. flicking).
///
/// https://developer.apple.com/documentation/appkit/nseventphase
// PORT NOTE: `enum(u3)` — a 3-bit backing integer. The tag values 0..=6 all fit
// in the `u8` repr; `ScrollMods` reproduces the 3-bit packing manually.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Momentum {
    #[default]
    None = 0,
    Began = 1,
    Stationary = 2,
    Changed = 3,
    Ended = 4,
    Cancelled = 5,
    MayBegin = 6,
}

/// The pressure stage of a pressure-sensitive input device.
///
/// This currently only supports the stages that macOS supports.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PressureStage {
    /// The input device is unpressed.
    None = 0,

    /// The input device is pressed a normal amount. On macOS trackpads,
    /// this is after a "click".
    Normal = 1,

    /// The input device is pressed a deep amount. On macOS trackpads,
    /// this is after a "force click".
    Deep = 2,
}

/// The bitmask for mods for scroll events.
///
/// PORT NOTE: `packed struct(u8)`. The field order maps to bit positions:
/// `precision` is bit 0, `momentum` occupies bits 1..=3 (a `u3` Momentum),
/// and bits 4..=7 are padding. `int()` reproduces that layout bit-for-bit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct ScrollMods {
    /// True if this is a high-precision scroll event. For example, Apple
    /// devices such as Magic Mouse, trackpads, etc. are high-precision
    /// and send very detailed scroll events.
    pub precision: bool,

    /// The momentum phase (if available, supported) of the scroll event.
    /// This is used to handle "inertial scrolling" (i.e. flicking).
    pub momentum: Momentum,
    // _padding: u4 = 0,
}

impl ScrollMods {
    /// PORT NOTE: `@bitCast(ScrollMods)` to its `u8` backing integer. `precision`
    /// is the low bit and `momentum` occupies bits 1..=3.
    pub fn int(self) -> u8 {
        (self.precision as u8) | ((self.momentum as u8) << 1)
    }

    /// PORT NOTE: `@bitCast` from the `u8` backing integer back to the packed
    /// `ScrollMods` struct. Reproduces the bit layout from `int()` (`precision`
    /// at bit 0, `momentum` occupying the `u3` at bits 1..=3, `_padding` at bits
    /// 4..=7). Mirrors `key_mods::Mods::from_int` / `Keys::from_int` / `Side::from_int`
    /// — the port idiom for the reference `@bitCast(backing_int) -> packed struct`.
    /// Used by the embedded CAPI `ghostty_surface_mouse_scroll` export (E8) to
    /// reinterpret the wire `scroll_mods` bits the host passes.
    pub fn from_int(v: u8) -> ScrollMods {
        ScrollMods {
            precision: (v & 0b0000_0001) != 0,
            // The `u3` `Momentum` field only names values 0..=6; a valid `ScrollMods`
            // (from `int()` or a real platform scroll event) never sets the seventh
            // (`7`) representation. The reference `@bitCast` to the exhaustive
            // `enum(u3)` treats that value as illegal behavior (ReleaseSafe trap /
            // ReleaseFast UB), so the Rust-safe analog traps here too.
            momentum: match (v >> 1) & 0b0000_0111 {
                0 => Momentum::None,
                1 => Momentum::Began,
                2 => Momentum::Stationary,
                3 => Momentum::Changed,
                4 => Momentum::Ended,
                5 => Momentum::Cancelled,
                6 => Momentum::MayBegin,
                _ => unreachable!("invalid ScrollMods momentum bit pattern"),
            },
        }
    }
}

// For our own understanding
#[cfg(test)]
#[test]
fn scroll_mods() {
    assert_eq!(ScrollMods::default().int(), 0b0u8);
    assert_eq!(
        ScrollMods {
            precision: true,
            ..Default::default()
        }
        .int(),
        0b0000_0001u8,
    );
}
