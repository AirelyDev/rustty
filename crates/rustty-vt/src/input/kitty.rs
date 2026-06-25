//! rustty-vt input/kitty
//!
//! PORTS: ghostty src/input/kitty.zig
//!
//! PORT STATUS: done

// PORT NOTE: the reference imports
//   const key = @import("key.zig");
// `key` provides `key.Key`. `u21` is modeled by `key::u21` (a `u32`).

use super::key;
use super::key::Key;

/// A single entry in the kitty keymap data. There are only ~100 entries
/// so the recommendation is to just use a linear search to find the entry
/// for a given key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Entry {
    pub key: Key,
    pub code: key::u21,
    pub final_: u8,
    pub modifier: bool,
}

/// The full list of entries for the current platform.
///
/// PORT NOTE: the reference `entries` is a comptime-evaluated `[]const Entry`
/// built from `raw_entries` (a list of tuples). Rust has no comptime tuple→struct
/// table builder, so this is built once at runtime by `entries()`; the produced
/// data is identical to the reference comptime value. The reference field name
/// `final` is a Rust keyword, so it is rendered `final_`.
pub fn entries() -> Vec<Entry> {
    let mut result: Vec<Entry> = Vec::with_capacity(RAW_ENTRIES.len());
    for raw in RAW_ENTRIES.iter() {
        result.push(Entry {
            key: raw.0,
            code: raw.1,
            final_: raw.2,
            modifier: raw.3,
        });
    }

    // PORT NOTE: `const final = result; break :entries &final;` — the reference
    // binds then returns the value; preserved verbatim.
    #[allow(clippy::let_and_return)]
    let final_ = result;
    final_
}

/// Raw entry is the tuple form of an entry for easy human management.
/// This should never be used in a real program so it is not pub. For
/// real programs, use `entries` which has properly typed, structured data.
type RawEntry = (Key, key::u21, u8, bool);

/// The raw data for how to map keys to Kitty data. Based on the information:
/// https://sw.kovidgoyal.net/kitty/keyboard-protocol/#functional-key-definitions
/// And the exact table is ported from Foot:
/// https://codeberg.org/dnkl/foot/src/branch/master/kitty-keymap.h
///
/// Note that we currently don't support all the same keysyms as Kitty,
/// but we can add them as we add support.
///
/// These are stored in order of the above Kitty link, so it is easy to
/// find the entry for a given key.
const RAW_ENTRIES: &[RawEntry] = &[
    (Key::Escape, 27, b'u', false),
    (Key::Enter, 13, b'u', false),
    (Key::Tab, 9, b'u', false),
    (Key::Backspace, 127, b'u', false),
    (Key::Insert, 2, b'~', false),
    (Key::Delete, 3, b'~', false),
    (Key::ArrowLeft, 1, b'D', false),
    (Key::ArrowRight, 1, b'C', false),
    (Key::ArrowUp, 1, b'A', false),
    (Key::ArrowDown, 1, b'B', false),
    (Key::PageUp, 5, b'~', false),
    (Key::PageDown, 6, b'~', false),
    (Key::Home, 1, b'H', false),
    (Key::End, 1, b'F', false),
    (Key::CapsLock, 57358, b'u', true),
    (Key::ScrollLock, 57359, b'u', false),
    (Key::NumLock, 57360, b'u', true),
    (Key::PrintScreen, 57361, b'u', false),
    (Key::Pause, 57362, b'u', false),
    (Key::F1, 1, b'P', false),
    (Key::F2, 1, b'Q', false),
    (Key::F3, 13, b'~', false),
    (Key::F4, 1, b'S', false),
    (Key::F5, 15, b'~', false),
    (Key::F6, 17, b'~', false),
    (Key::F7, 18, b'~', false),
    (Key::F8, 19, b'~', false),
    (Key::F9, 20, b'~', false),
    (Key::F10, 21, b'~', false),
    (Key::F11, 23, b'~', false),
    (Key::F12, 24, b'~', false),
    (Key::F13, 57376, b'u', false),
    (Key::F14, 57377, b'u', false),
    (Key::F15, 57378, b'u', false),
    (Key::F16, 57379, b'u', false),
    (Key::F17, 57380, b'u', false),
    (Key::F18, 57381, b'u', false),
    (Key::F19, 57382, b'u', false),
    (Key::F20, 57383, b'u', false),
    (Key::F21, 57384, b'u', false),
    (Key::F22, 57385, b'u', false),
    (Key::F23, 57386, b'u', false),
    (Key::F24, 57387, b'u', false),
    (Key::F25, 57388, b'u', false),
    (Key::Numpad0, 57399, b'u', false),
    (Key::Numpad1, 57400, b'u', false),
    (Key::Numpad2, 57401, b'u', false),
    (Key::Numpad3, 57402, b'u', false),
    (Key::Numpad4, 57403, b'u', false),
    (Key::Numpad5, 57404, b'u', false),
    (Key::Numpad6, 57405, b'u', false),
    (Key::Numpad7, 57406, b'u', false),
    (Key::Numpad8, 57407, b'u', false),
    (Key::Numpad9, 57408, b'u', false),
    (Key::NumpadDecimal, 57409, b'u', false),
    (Key::NumpadDivide, 57410, b'u', false),
    (Key::NumpadMultiply, 57411, b'u', false),
    (Key::NumpadSubtract, 57412, b'u', false),
    (Key::NumpadAdd, 57413, b'u', false),
    (Key::NumpadEnter, 57414, b'u', false),
    (Key::NumpadEqual, 57415, b'u', false),
    (Key::NumpadSeparator, 57416, b'u', false),
    (Key::NumpadLeft, 57417, b'u', false),
    (Key::NumpadRight, 57418, b'u', false),
    (Key::NumpadUp, 57419, b'u', false),
    (Key::NumpadDown, 57420, b'u', false),
    (Key::NumpadPageUp, 57421, b'u', false),
    (Key::NumpadPageDown, 57422, b'u', false),
    (Key::NumpadHome, 57423, b'u', false),
    (Key::NumpadEnd, 57424, b'u', false),
    (Key::NumpadInsert, 57425, b'u', false),
    (Key::NumpadDelete, 57426, b'u', false),
    (Key::NumpadBegin, 57427, b'u', false),
    (Key::ShiftLeft, 57441, b'u', true),
    (Key::ShiftRight, 57447, b'u', true),
    (Key::ControlLeft, 57442, b'u', true),
    (Key::ControlRight, 57448, b'u', true),
    (Key::MetaLeft, 57444, b'u', true),
    (Key::MetaRight, 57450, b'u', true),
    (Key::AltLeft, 57443, b'u', true),
    (Key::AltRight, 57449, b'u', true),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn force_comptime() {
        // To force comptime to test it
        let _ = entries();
    }
}
