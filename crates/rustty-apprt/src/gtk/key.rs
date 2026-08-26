//! rustty-apprt::gtk::key
//!
//! PORTS: ghostty `src/apprt/gtk/key.zig` (535 loc) — the GTK↔core key
//! translation helpers. This is the pure (mostly display-free) translation
//! layer the surface's `keyEvent` (`class/surface.zig:1240-1465`) leans on:
//! the GDK↔`input.Key` keymap and the trigger-formatting helpers used by the
//! menu / keybinding UI.
//!
//! # Type mapping
//!
//! The reference's `input.Key` / `input.Mods` / `input.Action` live in rustty's
//! carved input crates: [`rustty_vt::input::key`] (`Key`, `Action`),
//! [`rustty_vt::input::key_mods`] (`Mods`, `Side`, `ModSide`), and the
//! `Binding.Trigger` in [`rustty_input::binding`]. GDK keyvals are `gdk4::Key`
//! (a `u32` newtype); the reference's `gdk.KEY_*` comptime constants map to the
//! `gdk4::Key::*` associated consts.
//!
//! # Divergences
//!
//! * `accelFromTrigger` / `xdgShortcutFromTrigger` take a scratch `buf` and
//!   return a NUL-terminated `[:0]const u8` in the reference (zero-alloc for the
//!   GTK C consumers). Here they return an owned [`String`]; the GTK-facing
//!   NUL-termination lands with the menu-accelerator consumer (a later tier).
//! * `labelFromTrigger` writes into a `std.Io.Writer`; here it appends to a
//!   caller-owned `&mut String` and returns the same `bool`.

use glib::translate::{FromGlib, IntoGlib};
use gtk4::prelude::*;

use rustty_input::binding::{Trigger, TriggerKey};
use rustty_input::keycodes;
use rustty_vt::input::key::{Action, Key};
use rustty_vt::input::key_mods::{ModSide, Mods};

use crate::gtk::winproto;

/// PORTS: `key.zig:11-30` `accelFromTrigger`. Returns a GTK accelerator string
/// from a trigger (e.g. `"<Ctrl><Shift>a"`), or `None` when the trigger cannot
/// be formatted (a `catch_all` key). See the module doc for the `buf` → owned
/// `String` divergence.
#[must_use]
pub fn accel_from_trigger(trigger: Trigger) -> Option<String> {
    let mut out = String::new();

    // Modifiers (`key.zig:18-21`).
    if trigger.mods.shift {
        out.push_str("<Shift>");
    }
    if trigger.mods.ctrl {
        out.push_str("<Ctrl>");
    }
    if trigger.mods.alt {
        out.push_str("<Alt>");
    }
    if trigger.mods.super_ {
        out.push_str("<Super>");
    }

    // Write our key (`key.zig:24`).
    if !write_trigger_key(&mut out, trigger) {
        return None;
    }

    Some(out)
}

/// PORTS: `key.zig:34-58` `xdgShortcutFromTrigger`. Returns an XDG-compliant
/// shortcuts string from a trigger (spec:
/// <https://specifications.freedesktop.org/shortcuts-spec/latest/>), or `None`
/// for a `catch_all` key.
#[must_use]
pub fn xdg_shortcut_from_trigger(trigger: Trigger) -> Option<String> {
    let mut out = String::new();

    // Modifiers (`key.zig:41-44`).
    if trigger.mods.shift {
        out.push_str("SHIFT+");
    }
    if trigger.mods.ctrl {
        out.push_str("CTRL+");
    }
    if trigger.mods.alt {
        out.push_str("ALT+");
    }
    if trigger.mods.super_ {
        out.push_str("LOGO+");
    }

    // Write our key (`key.zig:47-52`). NOTE: while the spec specifies only
    // libxkbcommon keysyms, GTK's keysyms work as they are identical to X11's.
    if !write_trigger_key(&mut out, trigger) {
        return None;
    }

    Some(out)
}

/// PORTS: `key.zig:60-67` `keysymFromTrigger`. Returns the keysym (identical to
/// a GDK keyval) for a trigger key, or `None` if the trigger has no keysym
/// representation (a `catch_all`).
///
/// This is the trigger→wire conversion the `vicinae_hotkey_v1` bind request needs
/// ([`winproto::wayland`]'s `Hotkeys`); unlike [`write_trigger_key`] it produces
/// the NUMBER rather than the keysym NAME, and unlike that function it never fails
/// for a `unicode` trigger — `gdk_unicode_to_keyval` always yields a keyval
/// (falling back to the `0x01000000 | codepoint` Unicode-keysym encoding for
/// codepoints with no legacy keysym).
///
/// PORT NOTE: the reference returns `?c_uint`; a GDK keyval is a `guint32`, so the
/// port returns `Option<u32>`.
#[must_use]
pub fn keysym_from_trigger(trigger: Trigger) -> Option<u32> {
    match trigger.key {
        // `.physical => |k| keyvalFromKey(k)` (`key.zig:63`).
        TriggerKey::Physical(k) => keyval_from_key(k).map(IntoGlib::into_glib),
        // `.unicode => |cp| gdk.unicodeToKeyval(cp)` (`key.zig:64`).
        TriggerKey::Unicode(cp) => Some(gdk4::unicode_to_keyval(cp)),
        // `.catch_all => null` (`key.zig:65`).
        TriggerKey::CatchAll => None,
    }
}

/// PORTS: `key.zig:60-82` `writeTriggerKey`. Appends the key portion of a
/// trigger in GDK-keysym-name form; returns `false` for a `catch_all` key or an
/// unmappable physical/unicode key.
fn write_trigger_key(out: &mut String, trigger: Trigger) -> bool {
    match trigger.key {
        TriggerKey::Physical(k) => {
            let Some(keyval) = keyval_from_key(k) else {
                return false;
            };
            let Some(name) = keyval.name() else {
                return false;
            };
            out.push_str(name.as_str());
        }

        TriggerKey::Unicode(cp) => {
            // SAFETY: `gdk4::Key` is a transparent `u32` keyval newtype; every
            // `u32` is a valid keyval per GDK (no validation beyond `from_name`).
            let key = unsafe { gdk4::Key::from_glib(cp) };
            if let Some(name) = key.name() {
                out.push_str(name.as_str());
            } else if let Some(c) = char::from_u32(cp) {
                out.push(c);
            }
        }

        TriggerKey::CatchAll => return false,
    }

    true
}

/// PORTS: `key.zig:84-93` `translateMods`. Maps a GDK modifier bitset to an
/// `input.Mods`. `caps_lock` depends on the X settings but we just assume caps
/// lock (`key.zig:90`); `num_lock` / `sides` are left at their defaults (filled
/// by [`event_mods`]).
#[must_use]
pub fn translate_mods(state: gdk4::ModifierType) -> Mods {
    Mods {
        shift: state.contains(gdk4::ModifierType::SHIFT_MASK),
        ctrl: state.contains(gdk4::ModifierType::CONTROL_MASK),
        alt: state.contains(gdk4::ModifierType::ALT_MASK),
        super_: state.contains(gdk4::ModifierType::SUPER_MASK),
        // Lock is dependent on the X settings but we just assume caps lock.
        caps_lock: state.contains(gdk4::ModifierType::LOCK_MASK),
        ..Mods::default()
    }
}

/// PORTS: `gdk.keyvalToUnicode(keyval)` (`class/surface.zig:1352`). Returns the
/// Unicode codepoint for a GDK keyval, or 0 if it has no Unicode mapping.
#[must_use]
pub fn keyval_to_unicode(keyval: u32) -> u32 {
    // SAFETY: `gdk4::Key` is a transparent `u32` keyval newtype; every `u32` is a
    // valid keyval per GDK (no validation beyond `from_name`).
    unsafe { gdk4::Key::from_glib(keyval) }
        .to_unicode()
        .map_or(0, |c| c as u32)
}

/// PORTS: `key.zig:97-139` `keyvalUnicodeUnshifted`. Gets the unshifted unicode
/// value of the keyval — used by the Kitty keyboard protocol. Consults the
/// active keyboard layout (`event.layout()`) and returns the level-0 mapping for
/// the physical `keycode`, or 0 if none.
#[must_use]
pub fn keyval_unicode_unshifted(
    widget: &gtk4::Widget,
    event: &gdk4::KeyEvent,
    keycode: u32,
) -> u32 {
    let display = widget.display();

    // The currently active keyboard layout group (`key.zig:106`).
    let layout = event.layout();

    // All possible keyboard mappings for this keycode (`key.zig:110-116`). A
    // keycode is the physical key pressed.
    let Some(entries) = display.map_keycode(keycode) else {
        return 0;
    };

    for (key, keyval) in entries {
        if key.group() == layout as i32 && key.level() == 0 {
            return keyval.to_unicode().map_or(0, |c| c as u32);
        }
    }

    0
}

/// PORTS: `key.zig:144-209` `eventMods`. Returns the mods for a key event.
/// This requires a lot of context because the `GdkEvent` doesn't contain enough
/// on its own — the windowing-protocol backend (`app_winproto`) supplies the
/// low-level modifier read, `num_lock` comes from the device, and the physical
/// key determines the sided modifiers.
///
/// PORT NOTE: `app_winproto: *winproto.App` is a BORROW_FIELD (`key.zig:149`) —
/// the app's per-application windowing state, borrowed for the read.
#[must_use]
pub fn event_mods(
    event: &gdk4::Event,
    physical_key: Key,
    gtk_mods: gdk4::ModifierType,
    action: Action,
    app_winproto: &winproto::App,
) -> Mods {
    let device = event.device();

    let mut mods = app_winproto.event_mods(device.as_ref(), gtk_mods);
    mods.num_lock = device
        .as_ref()
        .is_some_and(gdk4::prelude::DeviceExt::is_num_locked);

    // We use the physical key to determine sided modifiers. As far as I can tell
    // there's no other way to reliably determine this.
    //
    // We also set the main modifier to true if either side is true, since on
    // both X11/Wayland, GTK doesn't set the main modifier if only the modifier
    // key is pressed, but our core logic relies on it (`key.zig:159-206`).
    match physical_key {
        Key::ShiftLeft => {
            mods.shift = action != Action::Release;
            mods.sides.shift = ModSide::Left;
        }
        Key::ShiftRight => {
            mods.shift = action != Action::Release;
            mods.sides.shift = ModSide::Right;
        }
        Key::ControlLeft => {
            mods.ctrl = action != Action::Release;
            mods.sides.ctrl = ModSide::Left;
        }
        Key::ControlRight => {
            mods.ctrl = action != Action::Release;
            mods.sides.ctrl = ModSide::Right;
        }
        Key::AltLeft => {
            mods.alt = action != Action::Release;
            mods.sides.alt = ModSide::Left;
        }
        Key::AltRight => {
            mods.alt = action != Action::Release;
            mods.sides.alt = ModSide::Right;
        }
        Key::MetaLeft => {
            mods.super_ = action != Action::Release;
            mods.sides.super_ = ModSide::Left;
        }
        Key::MetaRight => {
            mods.super_ = action != Action::Release;
            mods.sides.super_ = ModSide::Right;
        }
        _ => {}
    }

    mods
}

/// PORTS: `key.zig:212-218` `keyFromKeyval`. Returns an input key from a keyval
/// or `None` if we don't have a mapping.
#[must_use]
pub fn key_from_keyval(keyval: u32) -> Option<Key> {
    for (kv, key) in KEYMAP {
        if kv.into_glib() == keyval {
            return Some(*key);
        }
    }
    None
}

/// PORTS: `key.zig:221-234` `keyvalFromKey`. Returns a keyval from an input key
/// or `None` if we don't have a mapping.
#[must_use]
fn keyval_from_key(key: Key) -> Option<gdk4::Key> {
    for (kv, k) in KEYMAP {
        if *k == key {
            return Some(*kv);
        }
    }
    None
}

/// PORTS: `key.zig:240-252` `labelFromTrigger`. Converts a trigger to a
/// human-readable label for display in UI (e.g. `"Ctrl+Shift+A"`), using
/// GTK-accelerator-style formatting. Appends to `out`; returns `false` when the
/// trigger cannot be formatted (a `catch_all`).
pub fn label_from_trigger(out: &mut String, trigger: Trigger) -> bool {
    // Modifiers first, using human-readable format (`key.zig:245-248`).
    if trigger.mods.super_ {
        out.push_str("Super+");
    }
    if trigger.mods.ctrl {
        out.push_str("Ctrl+");
    }
    if trigger.mods.alt {
        out.push_str("Alt+");
    }
    if trigger.mods.shift {
        out.push_str("Shift+");
    }

    // Write the key (`key.zig:251`).
    write_trigger_key_label(out, trigger)
}

/// PORTS: `key.zig:255-298` `writeTriggerKeyLabel`. Appends the key portion of a
/// trigger in human-readable form, capitalizing the first letter for nicer
/// display. Returns `false` for a `catch_all`.
fn write_trigger_key_label(out: &mut String, trigger: Trigger) -> bool {
    match trigger.key {
        TriggerKey::Physical(k) => {
            let Some(keyval) = keyval_from_key(k) else {
                return false;
            };
            let Some(name) = keyval.name() else {
                return false;
            };
            push_capitalized(out, name.as_str());
        }

        TriggerKey::Unicode(cp) => {
            // SAFETY: see `write_trigger_key` — every `u32` is a valid keyval.
            let key = unsafe { gdk4::Key::from_glib(cp) };
            if let Some(name) = key.name() {
                push_capitalized(out, name.as_str());
            } else if let Some(c) = char::from_u32(cp) {
                out.push(c);
            }
        }

        TriggerKey::CatchAll => return false,
    }

    true
}

/// Appends `name` to `out`, capitalizing an ASCII-lowercase first letter — the
/// reference's inline capitalization (`key.zig:265-272,280-287`).
fn push_capitalized(out: &mut String, name: &str) {
    let mut chars = name.chars();
    if let Some(first) = chars.next() {
        if first.is_ascii_lowercase() {
            out.push(first.to_ascii_uppercase());
        } else {
            out.push(first);
        }
        out.push_str(chars.as_str());
    }
}

/// PORTS: `key.zig:388-535` the `keymap` table (`RawEntry = struct { c_uint,
/// input.Key }`). Maps GDK keyvals (`gdk4::Key::*`) to rustty's
/// [`Key`] enum. Preserved 1:1 with the reference's ordering.
#[rustfmt::skip]
static KEYMAP: &[(gdk4::Key, Key)] = &[
    (gdk4::Key::a, Key::KeyA),
    (gdk4::Key::b, Key::KeyB),
    (gdk4::Key::c, Key::KeyC),
    (gdk4::Key::d, Key::KeyD),
    (gdk4::Key::e, Key::KeyE),
    (gdk4::Key::f, Key::KeyF),
    (gdk4::Key::g, Key::KeyG),
    (gdk4::Key::h, Key::KeyH),
    (gdk4::Key::i, Key::KeyI),
    (gdk4::Key::j, Key::KeyJ),
    (gdk4::Key::k, Key::KeyK),
    (gdk4::Key::l, Key::KeyL),
    (gdk4::Key::m, Key::KeyM),
    (gdk4::Key::n, Key::KeyN),
    (gdk4::Key::o, Key::KeyO),
    (gdk4::Key::p, Key::KeyP),
    (gdk4::Key::q, Key::KeyQ),
    (gdk4::Key::r, Key::KeyR),
    (gdk4::Key::s, Key::KeyS),
    (gdk4::Key::t, Key::KeyT),
    (gdk4::Key::u, Key::KeyU),
    (gdk4::Key::v, Key::KeyV),
    (gdk4::Key::w, Key::KeyW),
    (gdk4::Key::x, Key::KeyX),
    (gdk4::Key::y, Key::KeyY),
    (gdk4::Key::z, Key::KeyZ),

    (gdk4::Key::_0, Key::Digit0),
    (gdk4::Key::_1, Key::Digit1),
    (gdk4::Key::_2, Key::Digit2),
    (gdk4::Key::_3, Key::Digit3),
    (gdk4::Key::_4, Key::Digit4),
    (gdk4::Key::_5, Key::Digit5),
    (gdk4::Key::_6, Key::Digit6),
    (gdk4::Key::_7, Key::Digit7),
    (gdk4::Key::_8, Key::Digit8),
    (gdk4::Key::_9, Key::Digit9),

    (gdk4::Key::semicolon, Key::Semicolon),
    (gdk4::Key::space, Key::Space),
    (gdk4::Key::apostrophe, Key::Quote),
    (gdk4::Key::comma, Key::Comma),
    (gdk4::Key::grave, Key::Backquote),
    (gdk4::Key::period, Key::Period),
    (gdk4::Key::slash, Key::Slash),
    (gdk4::Key::minus, Key::Minus),
    (gdk4::Key::equal, Key::Equal),
    (gdk4::Key::bracketleft, Key::BracketLeft),
    (gdk4::Key::bracketright, Key::BracketRight),
    (gdk4::Key::backslash, Key::Backslash),

    (gdk4::Key::Up, Key::ArrowUp),
    (gdk4::Key::Down, Key::ArrowDown),
    (gdk4::Key::Right, Key::ArrowRight),
    (gdk4::Key::Left, Key::ArrowLeft),
    (gdk4::Key::Home, Key::Home),
    (gdk4::Key::End, Key::End),
    (gdk4::Key::Insert, Key::Insert),
    (gdk4::Key::Delete, Key::Delete),
    (gdk4::Key::Caps_Lock, Key::CapsLock),
    (gdk4::Key::Scroll_Lock, Key::ScrollLock),
    (gdk4::Key::Num_Lock, Key::NumLock),
    (gdk4::Key::Page_Up, Key::PageUp),
    (gdk4::Key::Page_Down, Key::PageDown),
    (gdk4::Key::Escape, Key::Escape),
    (gdk4::Key::Return, Key::Enter),
    (gdk4::Key::Tab, Key::Tab),
    (gdk4::Key::BackSpace, Key::Backspace),
    (gdk4::Key::Print, Key::PrintScreen),
    (gdk4::Key::Pause, Key::Pause),

    (gdk4::Key::F1, Key::F1),
    (gdk4::Key::F2, Key::F2),
    (gdk4::Key::F3, Key::F3),
    (gdk4::Key::F4, Key::F4),
    (gdk4::Key::F5, Key::F5),
    (gdk4::Key::F6, Key::F6),
    (gdk4::Key::F7, Key::F7),
    (gdk4::Key::F8, Key::F8),
    (gdk4::Key::F9, Key::F9),
    (gdk4::Key::F10, Key::F10),
    (gdk4::Key::F11, Key::F11),
    (gdk4::Key::F12, Key::F12),
    (gdk4::Key::F13, Key::F13),
    (gdk4::Key::F14, Key::F14),
    (gdk4::Key::F15, Key::F15),
    (gdk4::Key::F16, Key::F16),
    (gdk4::Key::F17, Key::F17),
    (gdk4::Key::F18, Key::F18),
    (gdk4::Key::F19, Key::F19),
    (gdk4::Key::F20, Key::F20),
    (gdk4::Key::F21, Key::F21),
    (gdk4::Key::F22, Key::F22),
    (gdk4::Key::F23, Key::F23),
    (gdk4::Key::F24, Key::F24),
    (gdk4::Key::F25, Key::F25),

    (gdk4::Key::KP_0, Key::Numpad0),
    (gdk4::Key::KP_1, Key::Numpad1),
    (gdk4::Key::KP_2, Key::Numpad2),
    (gdk4::Key::KP_3, Key::Numpad3),
    (gdk4::Key::KP_4, Key::Numpad4),
    (gdk4::Key::KP_5, Key::Numpad5),
    (gdk4::Key::KP_6, Key::Numpad6),
    (gdk4::Key::KP_7, Key::Numpad7),
    (gdk4::Key::KP_8, Key::Numpad8),
    (gdk4::Key::KP_9, Key::Numpad9),
    (gdk4::Key::KP_Decimal, Key::NumpadDecimal),
    (gdk4::Key::KP_Divide, Key::NumpadDivide),
    (gdk4::Key::KP_Multiply, Key::NumpadMultiply),
    (gdk4::Key::KP_Subtract, Key::NumpadSubtract),
    (gdk4::Key::KP_Add, Key::NumpadAdd),
    (gdk4::Key::KP_Enter, Key::NumpadEnter),
    (gdk4::Key::KP_Equal, Key::NumpadEqual),

    (gdk4::Key::KP_Separator, Key::NumpadSeparator),
    (gdk4::Key::KP_Left, Key::NumpadLeft),
    (gdk4::Key::KP_Right, Key::NumpadRight),
    (gdk4::Key::KP_Up, Key::NumpadUp),
    (gdk4::Key::KP_Down, Key::NumpadDown),
    (gdk4::Key::KP_Page_Up, Key::NumpadPageUp),
    (gdk4::Key::KP_Page_Down, Key::NumpadPageDown),
    (gdk4::Key::KP_Home, Key::NumpadHome),
    (gdk4::Key::KP_End, Key::NumpadEnd),
    (gdk4::Key::KP_Insert, Key::NumpadInsert),
    (gdk4::Key::KP_Delete, Key::NumpadDelete),
    (gdk4::Key::KP_Begin, Key::NumpadBegin),

    (gdk4::Key::Copy, Key::Copy),
    (gdk4::Key::Cut, Key::Cut),
    (gdk4::Key::Paste, Key::Paste),

    (gdk4::Key::Shift_L, Key::ShiftLeft),
    (gdk4::Key::Control_L, Key::ControlLeft),
    (gdk4::Key::Alt_L, Key::AltLeft),
    (gdk4::Key::Super_L, Key::MetaLeft),
    (gdk4::Key::Shift_R, Key::ShiftRight),
    (gdk4::Key::Control_R, Key::ControlRight),
    (gdk4::Key::Alt_R, Key::AltRight),
    (gdk4::Key::Super_R, Key::MetaRight),

    // TODO: media keys
];

/// PORTS: the physical-key resolution in `keyEvent` (`class/surface.zig`).
/// Consults the W3C keycode table ([`keycodes::entries`]) for the native
/// `keycode`, then applies [`remap_key`] (`ef01d5fda`: the former inline
/// remap block). Extracted here (display-free) so it is unit-testable and
/// shared.
#[must_use]
pub fn physical_key(keycode: u32, keyval: u32, is_modifier: bool) -> Key {
    // The W3C physical key for this native keycode, else `.unidentified`
    // (`surface.zig:1362-1364`).
    let w3c_key = keycodes::entries()
        .iter()
        .find(|entry| entry.native == keycode)
        .map_or(Key::Unidentified, |entry| entry.key);

    // break :keycode gtk_key.remapKey(w3c_key, keyval, key_event.isModifier() != 0);
    remap_key(w3c_key, keyval, is_modifier)
}

/// Returns the logical key after applying any eligible XKB remapping.
///
/// Modifier keyvals that Rustty doesn't represent must not fall back to the
/// physical key. XKB can turn any physical key into a modifier, and using the
/// original key in that case would encode an unrelated key press.
//
// pub fn remapKey(
//     physical_key: input.Key,
//     keyval: c_uint,
//     is_modifier: bool,
// ) input.Key {
#[must_use]
pub fn remap_key(physical_key: Key, keyval: u32, is_modifier: bool) -> Key {
    // if (keyFromKeyval(keyval)) |remapped| {
    //     if (physical_key.shouldBeRemappable() or remapped.shouldBeRemappable())
    //         return remapped;
    // }
    if let Some(remapped) = key_from_keyval(keyval) {
        if physical_key.should_be_remappable() || remapped.should_be_remappable() {
            return remapped;
        }
    }

    // if (is_modifier or isModifierKeyval(keyval)) return .unidentified;
    // return physical_key;
    if is_modifier || is_modifier_keyval(keyval) {
        return Key::Unidentified;
    }
    physical_key
}

/// Returns whether a keyval has modifier semantics that GDK may not expose
/// through `gdk_key_event_is_modifier`.
///
/// In particular, XKB can report ISO level and group modifiers as keyvals
/// such as `Mode_switch` while GDK reports that the event is not a modifier.
//
// fn isModifierKeyval(keyval: c_uint) bool {
fn is_modifier_keyval(keyval: u32) -> bool {
    // return switch (keyval) { gdk.KEY_Shift_L, ... => true, else => false };
    //
    // (gdk4-rs: the `gdk4::Key` constants; compared by keyval value.)
    let key = unsafe { gdk4::Key::from_glib(keyval) };
    matches!(
        key,
        gdk4::Key::Shift_L
            | gdk4::Key::Shift_R
            | gdk4::Key::Shift_Lock
            | gdk4::Key::Control_L
            | gdk4::Key::Control_R
            | gdk4::Key::Caps_Lock
            | gdk4::Key::Meta_L
            | gdk4::Key::Meta_R
            | gdk4::Key::Alt_L
            | gdk4::Key::Alt_R
            | gdk4::Key::Super_L
            | gdk4::Key::Super_R
            | gdk4::Key::Hyper_L
            | gdk4::Key::Hyper_R
            | gdk4::Key::Num_Lock
            | gdk4::Key::Mode_switch
            | gdk4::Key::ISO_Lock
            | gdk4::Key::ISO_Level2_Latch
            | gdk4::Key::ISO_Level3_Shift
            | gdk4::Key::ISO_Level3_Latch
            | gdk4::Key::ISO_Level3_Lock
            | gdk4::Key::ISO_Level5_Shift
            | gdk4::Key::ISO_Level5_Latch
            | gdk4::Key::ISO_Level5_Lock
            | gdk4::Key::ISO_Group_Latch
            | gdk4::Key::ISO_Group_Lock
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustty_vt::input::key_mods::Mods;

    fn trigger(mods: Mods, key: TriggerKey) -> Trigger {
        Trigger { key, mods }
    }

    // test "remap key" (`ef01d5fda`). Display-free.
    #[test]
    fn key_remap_key() {
        // XKB remaps between non-writing-system keys are honored.
        // try testing.expectEqual(input.Key.escape, remapKey(.caps_lock, gdk.KEY_Escape, false));
        assert_eq!(
            Key::Escape,
            remap_key(Key::CapsLock, gdk4::Key::Escape.into_glib() as u32, false),
        );

        // Writing-system keys retain their physical identity.
        // try testing.expectEqual(input.Key.key_a, remapKey(.key_a, gdk.KEY_b, false));
        assert_eq!(
            Key::KeyA,
            remap_key(Key::KeyA, gdk4::Key::b.into_glib() as u32, false),
        );

        // GDK doesn't identify Mode_switch as a modifier, but it must not fall
        // back to the physical key.
        // try testing.expectEqual(input.Key.unidentified,
        //     remapKey(.delete, gdk.KEY_Mode_switch, false));
        assert_eq!(
            Key::Unidentified,
            remap_key(
                Key::Delete,
                gdk4::Key::Mode_switch.into_glib() as u32,
                false
            ),
        );

        // GDK's modifier flag also covers custom modifier actions whose keyval
        // Rustty doesn't represent.
        // try testing.expectEqual(input.Key.unidentified,
        //     remapKey(.delete, gdk.KEY_VoidSymbol, true));
        assert_eq!(
            Key::Unidentified,
            remap_key(Key::Delete, gdk4::Key::VoidSymbol.into_glib() as u32, true),
        );
    }

    /// PORTS: `key.zig:300-313` `test "accelFromTrigger"`. Display-free (pure GDK
    /// keyval-name lookups).
    #[test]
    fn key_accel_from_trigger() {
        let super_only = Mods {
            super_: true,
            ..Mods::default()
        };
        assert_eq!(
            accel_from_trigger(trigger(super_only, TriggerKey::Unicode('q' as u32))).as_deref(),
            Some("<Super>q")
        );

        let all = Mods {
            ctrl: true,
            alt: true,
            super_: true,
            shift: true,
            ..Mods::default()
        };
        assert_eq!(
            accel_from_trigger(trigger(all, TriggerKey::Unicode(92))).as_deref(),
            Some("<Shift><Ctrl><Alt><Super>backslash")
        );
    }

    /// PORTS: `key.zig:315-328` `test "xdgShortcutFromTrigger"`. Display-free.
    #[test]
    fn key_xdg_shortcut_from_trigger() {
        let super_only = Mods {
            super_: true,
            ..Mods::default()
        };
        assert_eq!(
            xdg_shortcut_from_trigger(trigger(super_only, TriggerKey::Unicode('q' as u32)))
                .as_deref(),
            Some("LOGO+q")
        );

        let all = Mods {
            ctrl: true,
            alt: true,
            super_: true,
            shift: true,
            ..Mods::default()
        };
        assert_eq!(
            xdg_shortcut_from_trigger(trigger(all, TriggerKey::Unicode(92))).as_deref(),
            Some("SHIFT+CTRL+ALT+LOGO+backslash")
        );
    }

    /// PORTS: `key.zig:340-360` `test "keysymFromTrigger"` verbatim. Display-free
    /// (`gdk_unicode_to_keyval` and the static keymap need no GDK display).
    #[test]
    fn key_keysym_from_trigger() {
        // Unicode keys use the keysym numbering (latin-1 maps directly).
        assert_eq!(
            keysym_from_trigger(trigger(
                Mods {
                    super_: true,
                    ..Mods::default()
                },
                TriggerKey::Unicode('q' as u32),
            )),
            Some('q' as u32)
        );

        // Physical keys map through our keymap.
        assert_eq!(
            keysym_from_trigger(trigger(Mods::default(), TriggerKey::Physical(Key::KeyA))),
            Some(gdk4::Key::a.into_glib())
        );

        // catch_all has no keysym representation.
        assert_eq!(
            keysym_from_trigger(trigger(Mods::default(), TriggerKey::CatchAll)),
            None
        );
    }

    /// PORTS: `key.zig:330-386` `test "labelFromTrigger"`. Display-free.
    #[test]
    fn key_label_from_trigger() {
        // Simple unicode key with modifier.
        let mut buf = String::new();
        assert!(label_from_trigger(
            &mut buf,
            trigger(
                Mods {
                    super_: true,
                    ..Mods::default()
                },
                TriggerKey::Unicode('q' as u32),
            ),
        ));
        assert_eq!(buf, "Super+Q");

        // Multiple modifiers.
        let mut buf = String::new();
        assert!(label_from_trigger(
            &mut buf,
            trigger(
                Mods {
                    ctrl: true,
                    alt: true,
                    super_: true,
                    shift: true,
                    ..Mods::default()
                },
                TriggerKey::Unicode(92),
            ),
        ));
        assert_eq!(buf, "Super+Ctrl+Alt+Shift+Backslash");

        // Physical key.
        let mut buf = String::new();
        assert!(label_from_trigger(
            &mut buf,
            trigger(
                Mods {
                    ctrl: true,
                    ..Mods::default()
                },
                TriggerKey::Physical(Key::KeyA),
            ),
        ));
        assert_eq!(buf, "Ctrl+A");

        // No modifiers.
        let mut buf = String::new();
        assert!(label_from_trigger(
            &mut buf,
            trigger(Mods::default(), TriggerKey::Physical(Key::Escape)),
        ));
        assert_eq!(buf, "Escape");

        // catch_all returns false.
        let mut buf = String::new();
        assert!(!label_from_trigger(
            &mut buf,
            trigger(Mods::default(), TriggerKey::CatchAll),
        ));
    }

    /// The keymap round-trips: every entry's keyval resolves back to its key.
    #[test]
    fn keymap_round_trips() {
        for (kv, key) in KEYMAP {
            assert_eq!(key_from_keyval(kv.into_glib()), Some(*key));
        }
    }
}
