//! rustty-input input/keycodes
//!
//! PORTS: ghostty src/input/keycodes.zig
//!
//! PORT STATUS: done

// Based on the Chromium source. The Chromium source code is governed by a
// BSD-style license that can be found in the LICENSE file.
// https://source.chromium.org/chromium/chromium/src/+/main:ui/events/keycodes/dom/dom_code_data.inc
//
// PORT NOTE: the reference imports
//   const std = @import("std");
//   const builtin = @import("builtin");
//   const Key = @import("key.zig").Key;
// `builtin.os.tag` maps to `cfg!(target_os = ...)`; `Key` maps to the ported
// `rustty_vt::input::key::Key`. `std.StaticStringMap` (`code_to_key`) maps to a
// linear-scan over a static table (the reference comment notes this map "isn't
// meant to be used at runtime").

use rustty_vt::input::key::Key;

/// Entry contains the USB code, native keycode, and W3C dom code for
/// the current platform.
//
// PORT NOTE: field order matches the reference. `code` is a static byte-slice
// (`[]const u8`) -> `&'static [u8]`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Entry {
    pub key: Key,    // input key enum
    pub usb: u32,    // USB HID usage code
    pub native: u32, // Native keycode
    // W3C DOM code, static memory
    pub code: &'static [u8],
}

/// The codes for the table from the Chromium data set. These are ALL the
/// codes, not just the ones that are supported by the current platform.
/// These are `pub` but you shouldn't use this because it uses way more
/// memory than is necessary.
///
/// The format is: usb, evdev, xkb, win, mac, code
//
// PORT NOTE: the reference `RawEntry` is a 6-tuple struct
// `struct { u32, u32, u32, u32, u32, []const u8 }`. Modeled as a Rust tuple
// struct preserving field order (usb, evdev, xkb, win, mac, code).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RawEntry(
    pub u32,
    pub u32,
    pub u32,
    pub u32,
    pub u32,
    pub &'static [u8],
);

/// A map from code to key. This isn't meant to be used at runtime
/// (though it could), so it isn't exported. It used to build the
/// key value for Entry.
//
// PORT NOTE: `std.StaticStringMap(Key).initComptime(...)` — a comptime string
// map. Modeled as a static slice of (code, Key) pairs scanned linearly by
// `code_to_key_get`. The set of pairs and their order is preserved verbatim.
const CODE_TO_KEY: &[(&[u8], Key)] = &[
    (b"KeyA", Key::KeyA),
    (b"KeyB", Key::KeyB),
    (b"KeyC", Key::KeyC),
    (b"KeyD", Key::KeyD),
    (b"KeyE", Key::KeyE),
    (b"KeyF", Key::KeyF),
    (b"KeyG", Key::KeyG),
    (b"KeyH", Key::KeyH),
    (b"KeyI", Key::KeyI),
    (b"KeyJ", Key::KeyJ),
    (b"KeyK", Key::KeyK),
    (b"KeyL", Key::KeyL),
    (b"KeyM", Key::KeyM),
    (b"KeyN", Key::KeyN),
    (b"KeyO", Key::KeyO),
    (b"KeyP", Key::KeyP),
    (b"KeyQ", Key::KeyQ),
    (b"KeyR", Key::KeyR),
    (b"KeyS", Key::KeyS),
    (b"KeyT", Key::KeyT),
    (b"KeyU", Key::KeyU),
    (b"KeyV", Key::KeyV),
    (b"KeyW", Key::KeyW),
    (b"KeyX", Key::KeyX),
    (b"KeyY", Key::KeyY),
    (b"KeyZ", Key::KeyZ),
    (b"Digit1", Key::Digit1),
    (b"Digit2", Key::Digit2),
    (b"Digit3", Key::Digit3),
    (b"Digit4", Key::Digit4),
    (b"Digit5", Key::Digit5),
    (b"Digit6", Key::Digit6),
    (b"Digit7", Key::Digit7),
    (b"Digit8", Key::Digit8),
    (b"Digit9", Key::Digit9),
    (b"Digit0", Key::Digit0),
    (b"Enter", Key::Enter),
    (b"Escape", Key::Escape),
    (b"Backspace", Key::Backspace),
    (b"Tab", Key::Tab),
    (b"Space", Key::Space),
    (b"Minus", Key::Minus),
    (b"Equal", Key::Equal),
    (b"BracketLeft", Key::BracketLeft),
    (b"BracketRight", Key::BracketRight),
    (b"Backslash", Key::Backslash),
    (b"Semicolon", Key::Semicolon),
    (b"Quote", Key::Quote),
    (b"Backquote", Key::Backquote),
    (b"Comma", Key::Comma),
    (b"Period", Key::Period),
    (b"Slash", Key::Slash),
    (b"CapsLock", Key::CapsLock),
    (b"F1", Key::F1),
    (b"F2", Key::F2),
    (b"F3", Key::F3),
    (b"F4", Key::F4),
    (b"F5", Key::F5),
    (b"F6", Key::F6),
    (b"F7", Key::F7),
    (b"F8", Key::F8),
    (b"F9", Key::F9),
    (b"F10", Key::F10),
    (b"F11", Key::F11),
    (b"F12", Key::F12),
    (b"F13", Key::F13),
    (b"F14", Key::F14),
    (b"F15", Key::F15),
    (b"F16", Key::F16),
    (b"F17", Key::F17),
    (b"F18", Key::F18),
    (b"F19", Key::F19),
    (b"F20", Key::F20),
    (b"F21", Key::F21),
    (b"F22", Key::F22),
    (b"F23", Key::F23),
    (b"F24", Key::F24),
    (b"PrintScreen", Key::PrintScreen),
    (b"ScrollLock", Key::ScrollLock),
    (b"Pause", Key::Pause),
    (b"Insert", Key::Insert),
    (b"Home", Key::Home),
    (b"PageUp", Key::PageUp),
    (b"Delete", Key::Delete),
    (b"End", Key::End),
    (b"Copy", Key::Copy),
    (b"Cut", Key::Cut),
    (b"Paste", Key::Paste),
    (b"PageDown", Key::PageDown),
    (b"ArrowRight", Key::ArrowRight),
    (b"ArrowLeft", Key::ArrowLeft),
    (b"ArrowDown", Key::ArrowDown),
    (b"ArrowUp", Key::ArrowUp),
    (b"NumLock", Key::NumLock),
    (b"NumpadDivide", Key::NumpadDivide),
    (b"NumpadMultiply", Key::NumpadMultiply),
    (b"NumpadSubtract", Key::NumpadSubtract),
    (b"NumpadAdd", Key::NumpadAdd),
    (b"NumpadEnter", Key::NumpadEnter),
    (b"Numpad1", Key::Numpad1),
    (b"Numpad2", Key::Numpad2),
    (b"Numpad3", Key::Numpad3),
    (b"Numpad4", Key::Numpad4),
    (b"Numpad5", Key::Numpad5),
    (b"Numpad6", Key::Numpad6),
    (b"Numpad7", Key::Numpad7),
    (b"Numpad8", Key::Numpad8),
    (b"Numpad9", Key::Numpad9),
    (b"Numpad0", Key::Numpad0),
    (b"NumpadDecimal", Key::NumpadDecimal),
    (b"NumpadEqual", Key::NumpadEqual),
    (b"ContextMenu", Key::ContextMenu),
    (b"ControlLeft", Key::ControlLeft),
    (b"ShiftLeft", Key::ShiftLeft),
    (b"AltLeft", Key::AltLeft),
    (b"MetaLeft", Key::MetaLeft),
    (b"ControlRight", Key::ControlRight),
    (b"ShiftRight", Key::ShiftRight),
    (b"AltRight", Key::AltRight),
    (b"MetaRight", Key::MetaRight),
];

// PORT NOTE: `code_to_key.get(code)` — the StaticStringMap lookup.
fn code_to_key_get(code: &[u8]) -> Option<Key> {
    let mut i = 0;
    while i < CODE_TO_KEY.len() {
        if CODE_TO_KEY[i].0 == code {
            return Some(CODE_TO_KEY[i].1);
        }
        i += 1;
    }
    None
}

// PORT NOTE: `builtin.os.tag` selects which tuple index is the "native" code.
// The reference maps ios/macos -> 4 (mac), windows -> 3 (win),
// freebsd/linux -> 2 (xkb), else -> @compileError("unsupported platform").
// The tuple indices here are 0-based fields of `RawEntry`:
//   .0 usb, .1 evdev, .2 xkb, .3 win, .4 mac, .5 code
fn native_of(raw: &RawEntry) -> u32 {
    #[cfg(any(target_os = "ios", target_os = "macos"))]
    {
        raw.4 // mac
    }
    #[cfg(target_os = "windows")]
    {
        raw.3 // win
    }
    #[cfg(any(target_os = "freebsd", target_os = "linux"))]
    {
        raw.2 // xkb
    }
    #[cfg(not(any(
        target_os = "ios",
        target_os = "macos",
        target_os = "windows",
        target_os = "freebsd",
        target_os = "linux",
    )))]
    {
        compile_error!("unsupported platform")
    }
}

/// The full list of entries for the current platform.
//
// PORT NOTE: the reference builds `entries` at comptime by reflecting over
// `raw_entries` and selecting the platform-native keycode. Rust cannot build a
// `&'static [Entry]` from a runtime loop, so `entries()` returns an owned
// `Vec<Entry>` constructed once on demand; the construction is byte-for-byte
// equivalent to the comptime table.
pub fn entries() -> Vec<Entry> {
    let raw = raw_entries();
    let mut result: Vec<Entry> = Vec::with_capacity(raw.len());
    for r in raw.iter() {
        result.push(Entry {
            key: code_to_key_get(r.5).unwrap_or(Key::Unidentified),
            usb: r.0,
            code: r.5,
            native: native_of(r),
        });
    }
    result
}

/// All of the full entries. This is marked pub but it should NOT be referenced
/// directly because it contains too much data for normal usage. Use `entries`
/// instead which contains just the relevant data for the target platform.
pub fn raw_entries() -> &'static [RawEntry] {
    //   USB     evdev    XKB     Win     Mac   Code
    RAW_ENTRIES
}

// PORT NOTE: `#[allow(clippy::mixed_case_hex_literals)]` — the entry
// `0x0c019C` is reproduced verbatim from the reference (which uses an uppercase
// `C` in that one literal). R1 byte-for-byte fidelity over lint uniformity.
#[rustfmt::skip]
#[allow(clippy::mixed_case_hex_literals)]
static RAW_ENTRIES: &[RawEntry] = &[
    //   USB     evdev    XKB     Win     Mac   Code
    RawEntry( 0x000000, 0x0000, 0x0000, 0x0000, 0xffff, b"" ),

    // =========================================
    // Non-USB codes
    // =========================================

    //   USB     evdev    XKB     Win     Mac   Code
    RawEntry( 0x000010, 0x0000, 0x0000, 0x0000, 0xffff, b"Hyper" ),
    RawEntry( 0x000011, 0x0000, 0x0000, 0x0000, 0xffff, b"Super" ),
    RawEntry( 0x000012, 0x0000, 0x0000, 0x0000, 0xffff, b"Fn" ),
    RawEntry( 0x000013, 0x0000, 0x0000, 0x0000, 0xffff, b"FnLock" ),
    RawEntry( 0x000014, 0x0000, 0x0000, 0x0000, 0xffff, b"Suspend" ),
    RawEntry( 0x000015, 0x0000, 0x0000, 0x0000, 0xffff, b"Resume" ),
    RawEntry( 0x000016, 0x0000, 0x0000, 0x0000, 0xffff, b"Turbo" ),

    // =========================================
    // USB Usage Page 0x01: Generic Desktop Page
    // =========================================

    // Sleep could be encoded as USB#0c0032, but there's no corresponding WakeUp
    // in the 0x0c USB page.
    //   USB     evdev    XKB     Win     Mac
    RawEntry( 0x010082, 0x008e, 0x0096, 0xe05f, 0xffff, b"Sleep" ),
    RawEntry( 0x010083, 0x008f, 0x0097, 0xe063, 0xffff, b"WakeUp" ),
    RawEntry( 0x0100a9, 0x00f8, 0x0100, 0x0000, 0xffff, b"" ),
    RawEntry( 0x0100b5, 0x00e3, 0x00eb, 0x0000, 0xffff, b"DisplayToggleIntExt" ),

    // =========================================
    // USB Usage Page 0x07: Keyboard/Keypad Page
    // =========================================

    // TODO(garykac):
    // XKB#005c ISO Level3 Shift (AltGr)
    // XKB#005e <>||
    // XKB#006d Linefeed
    // XKB#008a SunProps cf. USB#0700a3 CrSel/Props
    // XKB#008e SunOpen
    // Mac#003f kVK_Function
    // Mac#000a kVK_ISO_Section (ISO keyboards only)
    // Mac#0066 kVK_JIS_Eisu (USB#07008a Henkan?)

    //   USB     evdev    XKB     Win     Mac
    RawEntry( 0x070000, 0x0000, 0x0000, 0x0000, 0xffff, b"" ),
    RawEntry( 0x070001, 0x0000, 0x0000, 0x00ff, 0xffff, b"" ),
    RawEntry( 0x070002, 0x0000, 0x0000, 0x00fc, 0xffff, b"" ),
    RawEntry( 0x070003, 0x0000, 0x0000, 0x0000, 0xffff, b"" ),
    RawEntry( 0x070004, 0x001e, 0x0026, 0x001e, 0x0000, b"KeyA" ),
    RawEntry( 0x070005, 0x0030, 0x0038, 0x0030, 0x000b, b"KeyB" ),
    RawEntry( 0x070006, 0x002e, 0x0036, 0x002e, 0x0008, b"KeyC" ),
    RawEntry( 0x070007, 0x0020, 0x0028, 0x0020, 0x0002, b"KeyD" ),

    RawEntry( 0x070008, 0x0012, 0x001a, 0x0012, 0x000e, b"KeyE" ),
    RawEntry( 0x070009, 0x0021, 0x0029, 0x0021, 0x0003, b"KeyF" ),
    RawEntry( 0x07000a, 0x0022, 0x002a, 0x0022, 0x0005, b"KeyG" ),
    RawEntry( 0x07000b, 0x0023, 0x002b, 0x0023, 0x0004, b"KeyH" ),
    RawEntry( 0x07000c, 0x0017, 0x001f, 0x0017, 0x0022, b"KeyI" ),
    RawEntry( 0x07000d, 0x0024, 0x002c, 0x0024, 0x0026, b"KeyJ" ),
    RawEntry( 0x07000e, 0x0025, 0x002d, 0x0025, 0x0028, b"KeyK" ),
    RawEntry( 0x07000f, 0x0026, 0x002e, 0x0026, 0x0025, b"KeyL" ),

    RawEntry( 0x070010, 0x0032, 0x003a, 0x0032, 0x002e, b"KeyM" ),
    RawEntry( 0x070011, 0x0031, 0x0039, 0x0031, 0x002d, b"KeyN" ),
    RawEntry( 0x070012, 0x0018, 0x0020, 0x0018, 0x001f, b"KeyO" ),
    RawEntry( 0x070013, 0x0019, 0x0021, 0x0019, 0x0023, b"KeyP" ),
    RawEntry( 0x070014, 0x0010, 0x0018, 0x0010, 0x000c, b"KeyQ" ),
    RawEntry( 0x070015, 0x0013, 0x001b, 0x0013, 0x000f, b"KeyR" ),
    RawEntry( 0x070016, 0x001f, 0x0027, 0x001f, 0x0001, b"KeyS" ),
    RawEntry( 0x070017, 0x0014, 0x001c, 0x0014, 0x0011, b"KeyT" ),

    RawEntry( 0x070018, 0x0016, 0x001e, 0x0016, 0x0020, b"KeyU" ),
    RawEntry( 0x070019, 0x002f, 0x0037, 0x002f, 0x0009, b"KeyV" ),
    RawEntry( 0x07001a, 0x0011, 0x0019, 0x0011, 0x000d, b"KeyW" ),
    RawEntry( 0x07001b, 0x002d, 0x0035, 0x002d, 0x0007, b"KeyX" ),
    RawEntry( 0x07001c, 0x0015, 0x001d, 0x0015, 0x0010, b"KeyY" ),
    RawEntry( 0x07001d, 0x002c, 0x0034, 0x002c, 0x0006, b"KeyZ" ),
    RawEntry( 0x07001e, 0x0002, 0x000a, 0x0002, 0x0012, b"Digit1" ),
    RawEntry( 0x07001f, 0x0003, 0x000b, 0x0003, 0x0013, b"Digit2" ),

    RawEntry( 0x070020, 0x0004, 0x000c, 0x0004, 0x0014, b"Digit3" ),
    RawEntry( 0x070021, 0x0005, 0x000d, 0x0005, 0x0015, b"Digit4" ),
    RawEntry( 0x070022, 0x0006, 0x000e, 0x0006, 0x0017, b"Digit5" ),
    RawEntry( 0x070023, 0x0007, 0x000f, 0x0007, 0x0016, b"Digit6" ),
    RawEntry( 0x070024, 0x0008, 0x0010, 0x0008, 0x001a, b"Digit7" ),
    RawEntry( 0x070025, 0x0009, 0x0011, 0x0009, 0x001c, b"Digit8" ),
    RawEntry( 0x070026, 0x000a, 0x0012, 0x000a, 0x0019, b"Digit9" ),
    RawEntry( 0x070027, 0x000b, 0x0013, 0x000b, 0x001d, b"Digit0" ),

    RawEntry( 0x070028, 0x001c, 0x0024, 0x001c, 0x0024, b"Enter" ),
    RawEntry( 0x070029, 0x0001, 0x0009, 0x0001, 0x0035, b"Escape" ),
    RawEntry( 0x07002a, 0x000e, 0x0016, 0x000e, 0x0033, b"Backspace" ),
    RawEntry( 0x07002b, 0x000f, 0x0017, 0x000f, 0x0030, b"Tab" ),
    RawEntry( 0x07002c, 0x0039, 0x0041, 0x0039, 0x0031, b"Space" ),
    RawEntry( 0x07002d, 0x000c, 0x0014, 0x000c, 0x001b, b"Minus" ),
    RawEntry( 0x07002e, 0x000d, 0x0015, 0x000d, 0x0018, b"Equal" ),
    RawEntry( 0x07002f, 0x001a, 0x0022, 0x001a, 0x0021, b"BracketLeft" ),

    RawEntry( 0x070030, 0x001b, 0x0023, 0x001b, 0x001e, b"BracketRight" ),
    RawEntry( 0x070031, 0x002b, 0x0033, 0x002b, 0x002a, b"Backslash" ),
    // USB#070032 never appears on keyboards that have USB#070031.
    // Platforms use the same scancode as for the two keys.
    // Hence this code can only be generated synthetically
    // (e.g. in a DOM Level 3 KeyboardEvent).
    // The keycap varies on international keyboards:
    //   Dan: '*  Dutch: <>  Ger: #'  UK: #~
    // TODO(garykac): Verify Mac intl keyboard.
    //RawEntry( 0x070032, 0x0000, 0x0000, 0x0000, 0xffff, b""),
    RawEntry( 0x070033, 0x0027, 0x002f, 0x0027, 0x0029, b"Semicolon" ),
    RawEntry( 0x070034, 0x0028, 0x0030, 0x0028, 0x0027, b"Quote" ),
    RawEntry( 0x070035, 0x0029, 0x0031, 0x0029, 0x0032, b"Backquote" ),
    RawEntry( 0x070036, 0x0033, 0x003b, 0x0033, 0x002b, b"Comma" ),
    RawEntry( 0x070037, 0x0034, 0x003c, 0x0034, 0x002f, b"Period" ),

    RawEntry( 0x070038, 0x0035, 0x003d, 0x0035, 0x002c, b"Slash" ),
    // TODO(garykac): CapsLock requires special handling for each platform.
    RawEntry( 0x070039, 0x003a, 0x0042, 0x003a, 0x0039, b"CapsLock" ),
    RawEntry( 0x07003a, 0x003b, 0x0043, 0x003b, 0x007a, b"F1" ),
    RawEntry( 0x07003b, 0x003c, 0x0044, 0x003c, 0x0078, b"F2" ),
    RawEntry( 0x07003c, 0x003d, 0x0045, 0x003d, 0x0063, b"F3" ),
    RawEntry( 0x07003d, 0x003e, 0x0046, 0x003e, 0x0076, b"F4" ),
    RawEntry( 0x07003e, 0x003f, 0x0047, 0x003f, 0x0060, b"F5" ),
    RawEntry( 0x07003f, 0x0040, 0x0048, 0x0040, 0x0061, b"F6" ),

    RawEntry( 0x070040, 0x0041, 0x0049, 0x0041, 0x0062, b"F7" ),
    RawEntry( 0x070041, 0x0042, 0x004a, 0x0042, 0x0064, b"F8" ),
    RawEntry( 0x070042, 0x0043, 0x004b, 0x0043, 0x0065, b"F9" ),
    RawEntry( 0x070043, 0x0044, 0x004c, 0x0044, 0x006d, b"F10" ),
    RawEntry( 0x070044, 0x0057, 0x005f, 0x0057, 0x0067, b"F11" ),
    RawEntry( 0x070045, 0x0058, 0x0060, 0x0058, 0x006f, b"F12" ),
    // PrintScreen is effectively F13 on Mac OS X.
    RawEntry( 0x070046, 0x0063, 0x006b, 0xe037, 0xffff, b"PrintScreen" ),
    RawEntry( 0x070047, 0x0046, 0x004e, 0x0046, 0xffff, b"ScrollLock" ),

    RawEntry( 0x070048, 0x0077, 0x007f, 0x0045, 0xffff, b"Pause" ),
    // USB#0x070049 Insert, labeled "Help/Insert" on Mac -- see note M1 at top.
    RawEntry( 0x070049, 0x006e, 0x0076, 0xe052, 0x0072, b"Insert" ),
    RawEntry( 0x07004a, 0x0066, 0x006e, 0xe047, 0x0073, b"Home" ),
    RawEntry( 0x07004b, 0x0068, 0x0070, 0xe049, 0x0074, b"PageUp" ),
    // Delete (Forward Delete) named DEL because DELETE conflicts with <windows.h>
    RawEntry( 0x07004c, 0x006f, 0x0077, 0xe053, 0x0075, b"Delete" ),
    RawEntry( 0x07004d, 0x006b, 0x0073, 0xe04f, 0x0077, b"End" ),
    RawEntry( 0x07004e, 0x006d, 0x0075, 0xe051, 0x0079, b"PageDown" ),
    RawEntry( 0x07004f, 0x006a, 0x0072, 0xe04d, 0x007c, b"ArrowRight" ),

    RawEntry( 0x070050, 0x0069, 0x0071, 0xe04b, 0x007b, b"ArrowLeft" ),
    RawEntry( 0x070051, 0x006c, 0x0074, 0xe050, 0x007d, b"ArrowDown" ),
    RawEntry( 0x070052, 0x0067, 0x006f, 0xe048, 0x007e, b"ArrowUp" ),
    RawEntry( 0x070053, 0x0045, 0x004d, 0xe045, 0x0047, b"NumLock" ),
    RawEntry( 0x070054, 0x0062, 0x006a, 0xe035, 0x004b, b"NumpadDivide" ),
    RawEntry( 0x070055, 0x0037, 0x003f, 0x0037, 0x0043, b"NumpadMultiply" ),
    RawEntry( 0x070056, 0x004a, 0x0052, 0x004a, 0x004e, b"NumpadSubtract" ),
    RawEntry( 0x070057, 0x004e, 0x0056, 0x004e, 0x0045, b"NumpadAdd" ),

    RawEntry( 0x070058, 0x0060, 0x0068, 0xe01c, 0x004c, b"NumpadEnter" ),
    RawEntry( 0x070059, 0x004f, 0x0057, 0x004f, 0x0053, b"Numpad1" ), // +End
    RawEntry( 0x07005a, 0x0050, 0x0058, 0x0050, 0x0054, b"Numpad2" ), // +Down
    RawEntry( 0x07005b, 0x0051, 0x0059, 0x0051, 0x0055, b"Numpad3" ), // +PageDown
    RawEntry( 0x07005c, 0x004b, 0x0053, 0x004b, 0x0056, b"Numpad4" ), // +Left
    RawEntry( 0x07005d, 0x004c, 0x0054, 0x004c, 0x0057, b"Numpad5" ),
    RawEntry( 0x07005e, 0x004d, 0x0055, 0x004d, 0x0058, b"Numpad6" ), // +Right
    RawEntry( 0x07005f, 0x0047, 0x004f, 0x0047, 0x0059, b"Numpad7" ), // +Home
    RawEntry( 0x070060, 0x0048, 0x0050, 0x0048, 0x005b, b"Numpad8" ), // +Up
    RawEntry( 0x070061, 0x0049, 0x0051, 0x0049, 0x005c, b"Numpad9" ), // +PageUp
    RawEntry( 0x070062, 0x0052, 0x005a, 0x0052, 0x0052, b"Numpad0" ), // +Insert
    RawEntry( 0x070063, 0x0053, 0x005b, 0x0053, 0x0041, b"NumpadDecimal" ), // +Delete
    // USB#070064 is not present on US keyboard.
    // This key is typically located near LeftShift key.
    // The keycap varies on international keyboards:
    //   Dan: <> Dutch: ][ Ger: <> UK: \|
    RawEntry( 0x070064, 0x0056, 0x005e, 0x0056, 0x000a, b"IntlBackslash" ),
    // USB#0x070065 Application Menu (next to RWin key) -- see note L2 at top.
    RawEntry( 0x070065, 0x007f, 0x0087, 0xe05d, 0x006e, b"ContextMenu" ),
    RawEntry( 0x070066, 0x0074, 0x007c, 0xe05e, 0xffff, b"Power" ),
    RawEntry( 0x070067, 0x0075, 0x007d, 0x0059, 0x0051, b"NumpadEqual" ),

    RawEntry( 0x070068, 0x00b7, 0x00bf, 0x0064, 0x0069, b"F13" ),
    RawEntry( 0x070069, 0x00b8, 0x00c0, 0x0065, 0x006b, b"F14" ),
    RawEntry( 0x07006a, 0x00b9, 0x00c1, 0x0066, 0x0071, b"F15" ),
    RawEntry( 0x07006b, 0x00ba, 0x00c2, 0x0067, 0x006a, b"F16" ),
    RawEntry( 0x07006c, 0x00bb, 0x00c3, 0x0068, 0x0040, b"F17" ),
    RawEntry( 0x07006d, 0x00bc, 0x00c4, 0x0069, 0x004f, b"F18" ),
    RawEntry( 0x07006e, 0x00bd, 0x00c5, 0x006a, 0x0050, b"F19" ),
    RawEntry( 0x07006f, 0x00be, 0x00c6, 0x006b, 0x005a, b"F20" ),

    RawEntry( 0x070070, 0x00bf, 0x00c7, 0x006c, 0xffff, b"F21" ),
    RawEntry( 0x070071, 0x00c0, 0x00c8, 0x006d, 0xffff, b"F22" ),
    RawEntry( 0x070072, 0x00c1, 0x00c9, 0x006e, 0xffff, b"F23" ),
    // USB#0x070073 -- see note W1 at top.
    RawEntry( 0x070073, 0x00c2, 0x00ca, 0x0076, 0xffff, b"F24" ),
    RawEntry( 0x070074, 0x0086, 0x008e, 0x0000, 0xffff, b"Open" ),
    // USB#0x070075 Help -- see note M1 at top.
    RawEntry( 0x070075, 0x008a, 0x0092, 0xe03b, 0xffff, b"Help" ),
    // USB#0x070076 Keyboard Menu -- see note L2 at top.
    //RawEntry( 0x070076, 0x0000, 0x0000, 0x0000, 0xffff, b""),
    RawEntry( 0x070077, 0x0084, 0x008c, 0x0000, 0xffff, b"Select" ),

    //RawEntry( 0x070078, 0x0080, 0x0088, 0x0000, 0xffff, b""),
    RawEntry( 0x070079, 0x0081, 0x0089, 0x0000, 0xffff, b"Again" ),
    RawEntry( 0x07007a, 0x0083, 0x008b, 0xe008, 0xffff, b"Undo" ),
    RawEntry( 0x07007b, 0x0089, 0x0091, 0xe017, 0xffff, b"Cut" ),
    RawEntry( 0x07007c, 0x0085, 0x008d, 0xe018, 0xffff, b"Copy" ),
    RawEntry( 0x07007d, 0x0087, 0x008f, 0xe00a, 0xffff, b"Paste" ),
    RawEntry( 0x07007e, 0x0088, 0x0090, 0x0000, 0xffff, b"Find" ),
    RawEntry( 0x07007f, 0x0071, 0x0079, 0xe020, 0x004a, b"AudioVolumeMute" ),

    RawEntry( 0x070080, 0x0073, 0x007b, 0xe030, 0x0048, b"AudioVolumeUp" ),
    RawEntry( 0x070081, 0x0072, 0x007a, 0xe02e, 0x0049, b"AudioVolumeDown" ),
    //RawEntry( 0x070082, 0x0000, 0x0000, 0x0000, 0xffff, b""),
    //RawEntry( 0x070083, 0x0000, 0x0000, 0x0000, 0xffff, b""),
    //RawEntry( 0x070084, 0x0000, 0x0000, 0x0000, 0xffff, b""),
    RawEntry( 0x070085, 0x0079, 0x0081, 0x007e, 0x005f, b"NumpadComma" ),

    // International1
    // USB#070086 is used on AS/400 keyboards. Standard Keypad_= is USB#070067.
    //RawEntry( 0x070086, 0x0000, 0x0000, 0x0000, 0xffff, b""),
    // USB#070087 is used for Brazilian /? and Japanese _ 'ro'.
    RawEntry( 0x070087, 0x0059, 0x0061, 0x0073, 0x005e, b"IntlRo" ),
    // International2
    // USB#070088 is used as Japanese Hiragana/Katakana key.
    RawEntry( 0x070088, 0x005d, 0x0065, 0x0070, 0xffff, b"KanaMode" ),
    // International3
    // USB#070089 is used as Japanese Yen key.
    RawEntry( 0x070089, 0x007c, 0x0084, 0x007d, 0x005d, b"IntlYen" ),
    // International4
    // USB#07008a is used as Japanese Henkan (Convert) key.
    RawEntry( 0x07008a, 0x005c, 0x0064, 0x0079, 0xffff, b"Convert" ),
    // International5
    // USB#07008b is used as Japanese Muhenkan (No-convert) key.
    RawEntry( 0x07008b, 0x005e, 0x0066, 0x007b, 0xffff, b"NonConvert" ),
    //RawEntry( 0x07008c, 0x005f, 0x0067, 0x005c, 0xffff, b""),
    //RawEntry( 0x07008d, 0x0000, 0x0000, 0x0000, 0xffff, b""),
    //RawEntry( 0x07008e, 0x0000, 0x0000, 0x0000, 0xffff, b""),
    //RawEntry( 0x07008f, 0x0000, 0x0000, 0x0000, 0xffff, b""),

    // LANG1
    // USB#070090 is used as Korean Hangul/English toggle key, and as the Kana key
    // on the Apple Japanese keyboard.
    RawEntry( 0x070090, 0x007a, 0x0082, 0x0072, 0x0068, b"Lang1" ),
    // LANG2
    // USB#070091 is used as Korean Hanja conversion key, and as the Eisu key on
    // the Apple Japanese keyboard.
    RawEntry( 0x070091, 0x007b, 0x0083, 0x0071, 0x0066, b"Lang2" ),
    // LANG3
    // USB#070092 is used as Japanese Katakana key.
    RawEntry( 0x070092, 0x005a, 0x0062, 0x0078, 0xffff, b"Lang3" ),
    // LANG4
    // USB#070093 is used as Japanese Hiragana key.
    RawEntry( 0x070093, 0x005b, 0x0063, 0x0077, 0xffff, b"Lang4" ),
    // LANG5
    // USB#070094 is used as Japanese Zenkaku/Hankaku (Fullwidth/halfwidth) key.
    // Not mapped on Windows -- see note W1 at top.
    RawEntry( 0x070094, 0x0055, 0x005d, 0x0000, 0xffff, b"Lang5" ),
    //RawEntry( 0x070095, 0x0000, 0x0000, 0x0000, 0xffff, b""),
    //RawEntry( 0x070096, 0x0000, 0x0000, 0x0000, 0xffff, b""),
    //RawEntry( 0x070097, 0x0000, 0x0000, 0x0000, 0xffff, b""),
    //RawEntry( 0x070098, 0x0000, 0x0000, 0x0000, 0xffff, b""),

    //RawEntry( 0x070099, 0x0000, 0x0000, 0x0000, 0xffff, b""),
    //RawEntry( 0x07009a, 0x0000, 0x0000, 0x0000, 0xffff, b""),
    RawEntry( 0x07009b, 0x0000, 0x0000, 0x0000, 0xffff, b"Abort" ),
    // USB#0x07009c Keyboard Clear -- see note L1 at top.
    //RawEntry( 0x07009c, 0x0000, 0x0000, 0x0000, 0xffff, b""),
    //RawEntry( 0x07009d, 0x0000, 0x0000, 0x0000, 0xffff, b""),
    //RawEntry( 0x07009e, 0x0000, 0x0000, 0x0000, 0xffff, b""),
    //RawEntry( 0x07009f, 0x0000, 0x0000, 0x0000, 0xffff, b""),

    //RawEntry( 0x0700a0, 0x0000, 0x0000, 0x0000, 0xffff, b""),
    //RawEntry( 0x0700a1, 0x0000, 0x0000, 0x0000, 0xffff, b""),
    //RawEntry( 0x0700a2, 0x0000, 0x0000, 0x0000, 0xffff, b""),
    // USB#0x0700a3 Props -- see note L2 at top.
    RawEntry( 0x0700a3, 0x0000, 0x0000, 0x0000, 0xffff, b"Props" ),
    //RawEntry( 0x0700a4, 0x0000, 0x0000, 0x0000, 0xffff, b""),

    //RawEntry( 0x0700b0, 0x0000, 0x0000, 0x0000, 0xffff, b""),
    //RawEntry( 0x0700b1, 0x0000, 0x0000, 0x0000, 0xffff, b""),
    //RawEntry( 0x0700b2, 0x0000, 0x0000, 0x0000, 0xffff, b""),
    //RawEntry( 0x0700b3, 0x0000, 0x0000, 0x0000, 0xffff, b""),
    //RawEntry( 0x0700b4, 0x0000, 0x0000, 0x0000, 0xffff, b""),
    //RawEntry( 0x0700b5, 0x0000, 0x0000, 0x0000, 0xffff, b""),
    RawEntry( 0x0700b6, 0x00b3, 0x00bb, 0x0000, 0xffff, b"NumpadParenLeft" ),
    RawEntry( 0x0700b7, 0x00b4, 0x00bc, 0x0000, 0xffff, b"NumpadParenRight" ),

    //RawEntry( 0x0700b8, 0x0000, 0x0000, 0x0000, 0xffff, b""),
    //RawEntry( 0x0700b9, 0x0000, 0x0000, 0x0000, 0xffff, b""),
    //RawEntry( 0x0700ba, 0x0000, 0x0000, 0x0000, 0xffff, b""),
    RawEntry( 0x0700bb, 0x0000, 0x0000, 0x0000, 0xffff, b"NumpadBackspace" ),
    //RawEntry( 0x0700bc, 0x0000, 0x0000, 0x0000, 0xffff, b""),
    //RawEntry( 0x0700bd, 0x0000, 0x0000, 0x0000, 0xffff, b""),
    //RawEntry( 0x0700be, 0x0000, 0x0000, 0x0000, 0xffff, b""),
    //RawEntry( 0x0700bf, 0x0000, 0x0000, 0x0000, 0xffff, b""),

    //RawEntry( 0x0700c0, 0x0000, 0x0000, 0x0000, 0xffff, b""),
    //RawEntry( 0x0700c1, 0x0000, 0x0000, 0x0000, 0xffff, b""),
    //RawEntry( 0x0700c2, 0x0000, 0x0000, 0x0000, 0xffff, b""),
    //RawEntry( 0x0700c3, 0x0000, 0x0000, 0x0000, 0xffff, b""),
    //RawEntry( 0x0700c4, 0x0000, 0x0000, 0x0000, 0xffff, b""),
    //RawEntry( 0x0700c5, 0x0000, 0x0000, 0x0000, 0xffff, b""),
    //RawEntry( 0x0700c6, 0x0000, 0x0000, 0x0000, 0xffff, b""),
    //RawEntry( 0x0700c7, 0x0000, 0x0000, 0x0000, 0xffff, b""),

    //RawEntry( 0x0700c8, 0x0000, 0x0000, 0x0000, 0xffff, b""),
    //RawEntry( 0x0700c9, 0x0000, 0x0000, 0x0000, 0xffff, b""),
    //RawEntry( 0x0700ca, 0x0000, 0x0000, 0x0000, 0xffff, b""),
    //           NUMPAD_DOUBLE_VERTICAL_BAR),  // Keypad_||
    //RawEntry( 0x0700cb, 0x0000, 0x0000, 0x0000, 0xffff, b""),
    //RawEntry( 0x0700cc, 0x0000, 0x0000, 0x0000, 0xffff, b""),
    //RawEntry( 0x0700cd, 0x0000, 0x0000, 0x0000, 0xffff, b""),
    //RawEntry( 0x0700ce, 0x0000, 0x0000, 0x0000, 0xffff, b""),
    //RawEntry( 0x0700cf, 0x0000, 0x0000, 0x0000, 0xffff, b""),

    RawEntry( 0x0700d0, 0x0000, 0x0000, 0x0000, 0xffff, b"NumpadMemoryStore" ),
    RawEntry( 0x0700d1, 0x0000, 0x0000, 0x0000, 0xffff, b"NumpadMemoryRecall" ),
    RawEntry( 0x0700d2, 0x0000, 0x0000, 0x0000, 0xffff, b"NumpadMemoryClear" ),
    RawEntry( 0x0700d3, 0x0000, 0x0000, 0x0000, 0xffff, b"NumpadMemoryAdd" ),
    RawEntry( 0x0700d4, 0x0000, 0x0000, 0x0000, 0xffff, b"NumpadMemorySubtract" ),
    //RawEntry( 0x0700d5, 0x0000, 0x0000, 0x0000, 0xffff, b""),
    //RawEntry( 0x0700d6, 0x0000, 0x0000, 0x0000, 0xffff, b""),
    RawEntry( 0x0700d7, 0x0076, 0x007e, 0x0000, 0xffff, b"" ),
    // USB#0x0700d8 Keypad Clear -- see note L1 at top.
    RawEntry( 0x0700d8, 0x0000, 0x0000, 0x0000, 0xffff, b"NumpadClear" ),
    RawEntry( 0x0700d9, 0x0000, 0x0000, 0x0000, 0xffff, b"NumpadClearEntry" ),
    //RawEntry( 0x0700da, 0x0000, 0x0000, 0x0000, 0xffff, b""),
    //RawEntry( 0x0700db, 0x0000, 0x0000, 0x0000, 0xffff, b""),
    //RawEntry( 0x0700dc, 0x0000, 0x0000, 0x0000, 0xffff, b""),
    //RawEntry( 0x0700dd, 0x0000, 0x0000, 0x0000, 0xffff, b""),

    // USB#0700de - #0700df are reserved.
    RawEntry( 0x0700e0, 0x001d, 0x0025, 0x001d, 0x003b, b"ControlLeft" ),
    RawEntry( 0x0700e1, 0x002a, 0x0032, 0x002a, 0x0038, b"ShiftLeft" ),
    // USB#0700e2: left Alt key (Mac left Option key).
    RawEntry( 0x0700e2, 0x0038, 0x0040, 0x0038, 0x003a, b"AltLeft" ),
    // USB#0700e3: left GUI key, e.g. Windows, Mac Command, ChromeOS Search.
    RawEntry( 0x0700e3, 0x007d, 0x0085, 0xe05b, 0x0037, b"MetaLeft" ),
    RawEntry( 0x0700e4, 0x0061, 0x0069, 0xe01d, 0x003e, b"ControlRight" ),
    RawEntry( 0x0700e5, 0x0036, 0x003e, 0x0036, 0x003c, b"ShiftRight" ),
    // USB#0700e6: right Alt key (Mac right Option key).
    RawEntry( 0x0700e6, 0x0064, 0x006c, 0xe038, 0x003d, b"AltRight" ),
    // USB#0700e7: right GUI key, e.g. Windows, Mac Command, ChromeOS Search.
    RawEntry( 0x0700e7, 0x007e, 0x0086, 0xe05c, 0x0036, b"MetaRight" ),

    // USB#0700e8 - #07ffff are reserved

    // ==================================
    // USB Usage Page 0x0c: Consumer Page
    // ==================================
    // AL = Application Launch
    // AC = Application Control

    // TODO(garykac): Many XF86 keys have multiple scancodes mapping to them.
    // We need to map all of these into a canonical USB scancode without
    // confusing the reverse-lookup - most likely by simply returning the first
    // found match.

    // TODO(garykac): Find appropriate mappings for:
    // Win#e03c Music - USB#0c0193 is AL_AVCapturePlayback
    // Win#e064 Pictures
    // XKB#0080 XF86LaunchA
    // XKB#0099 XF86Send
    // XKB#009b XF86Xfer
    // XKB#009c XF86Launch1
    // XKB#009d XF86Launch2
    // XKB... remaining XF86 keys

    // KEY_BRIGHTNESS* added in Linux 3.16
    // http://www.usb.org/developers/hidpage/HUTRR41.pdf
    //
    // Keyboard backlight/illumination spec update.
    // https://www.usb.org/sites/default/files/hutrr73_-_fn_key_and_keyboard_backlight_brightness_0.pdf
    //   USB     evdev    XKB     Win     Mac   Code
    RawEntry( 0x0c0060, 0x0166, 0x016e, 0x0000, 0xffff, b"" ),
    RawEntry( 0x0c0061, 0x0172, 0x017a, 0x0000, 0xffff, b"" ),
    RawEntry( 0x0c006f, 0x00e1, 0x00e9, 0x0000, 0xffff, b"BrightnessUp" ),
    RawEntry( 0x0c0070, 0x00e0, 0x00e8, 0x0000, 0xffff, b"BrightnessDown" ),
    RawEntry( 0x0c0072, 0x01af, 0x01b7, 0x0000, 0xffff, b"" ),
    RawEntry( 0x0c0073, 0x0250, 0x0258, 0x0000, 0xffff, b"" ),
    RawEntry( 0x0c0074, 0x0251, 0x0259, 0x0000, 0xffff, b"" ),
    RawEntry( 0x0c0075, 0x00f4, 0x00fc, 0x0000, 0xffff, b"" ),
    RawEntry( 0x0c0079, 0x00e6, 0x00ee, 0x0000, 0xffff, b"" ),
    RawEntry( 0x0c007a, 0x00e5, 0x00ed, 0x0000, 0xffff, b"" ),
    RawEntry( 0x0c007c, 0x00e4, 0x00ec, 0x0000, 0xffff, b"" ),
    RawEntry( 0x0c0083, 0x0195, 0x019d, 0x0000, 0xffff, b"" ),
    RawEntry( 0x0c008c, 0x00a9, 0x00b1, 0x0000, 0xffff, b"" ),
    RawEntry( 0x0c008d, 0x016a, 0x0172, 0x0000, 0xffff, b"" ),
    RawEntry( 0x0c0094, 0x00ae, 0x00b6, 0x0000, 0xffff, b"" ),
    RawEntry( 0x0c009c, 0x019a, 0x01a2, 0x0000, 0xffff, b"" ),
    RawEntry( 0x0c009d, 0x019b, 0x01a3, 0x0000, 0xffff, b"" ),

    //              USB     evdev    XKB     Win     Mac
    RawEntry( 0x0c00b0, 0x00cf, 0x00d7, 0x0000, 0xffff, b"MediaPlay" ),
    RawEntry( 0x0c00b1, 0x00c9, 0x00d1, 0x0000, 0xffff, b"MediaPause" ),
    RawEntry( 0x0c00b2, 0x00a7, 0x00af, 0x0000, 0xffff, b"MediaRecord" ),
    RawEntry( 0x0c00b3, 0x00d0, 0x00d8, 0x0000, 0xffff, b"MediaFastForward" ),
    RawEntry( 0x0c00b4, 0x00a8, 0x00b0, 0x0000, 0xffff, b"MediaRewind" ),
    RawEntry( 0x0c00b5, 0x00a3, 0x00ab, 0xe019, 0xffff, b"MediaTrackNext" ),
    RawEntry( 0x0c00b6, 0x00a5, 0x00ad, 0xe010, 0xffff, b"MediaTrackPrevious" ),
    RawEntry( 0x0c00b7, 0x00a6, 0x00ae, 0xe024, 0xffff, b"MediaStop" ),
    RawEntry( 0x0c00b8, 0x00a1, 0x00a9, 0xe02c, 0xffff, b"Eject" ),
    RawEntry( 0x0c00cd, 0x00a4, 0x00ac, 0xe022, 0xffff, b"MediaPlayPause" ),
    RawEntry( 0x0c00cf, 0x0246, 0x024e, 0x0000, 0xffff, b"" ),
    RawEntry( 0x0c00d8, 0x024a, 0x0252, 0x0000, 0xffff, b"" ),
    RawEntry( 0x0c00d9, 0x0249, 0x0251, 0x0000, 0xffff, b"" ),
    RawEntry( 0x0c00e5, 0x00d1, 0x00d9, 0x0000, 0xffff, b"" ),
    //RawEntry( 0x0c00e6, 0x0000, 0x0000, 0x0000, 0xffff, b""),
    //RawEntry( 0x0c0150, 0x0000, 0x0000, 0x0000, 0xffff, b""),
    //RawEntry( 0x0c0151, 0x0000, 0x0000, 0x0000, 0xffff, b""),
    //RawEntry( 0x0c0152, 0x0000, 0x0000, 0x0000, 0xffff, b""),
    //RawEntry( 0x0c0153, 0x0000, 0x0000, 0x0000, 0xffff, b""),
    //RawEntry( 0x0c0154, 0x0000, 0x0000, 0x0000, 0xffff, b""),
    //RawEntry( 0x0c0155, 0x0000, 0x0000, 0x0000, 0xffff, b""),
    // USB#0c0183: AL Consumer Control Configuration
    RawEntry( 0x0c0183, 0x00ab, 0x00b3, 0xe06d, 0xffff, b"MediaSelect" ),
    RawEntry( 0x0c0184, 0x01a5, 0x01ad, 0x0000, 0xffff, b"" ),
    RawEntry( 0x0c0186, 0x01a7, 0x01af, 0x0000, 0xffff, b"" ),
    // USB#0x0c018a AL_EmailReader
    RawEntry( 0x0c018a, 0x009b, 0x00a3, 0xe06c, 0xffff, b"LaunchMail" ),
    // USB#0x0c018d: AL Contacts/Address Book
    RawEntry( 0x0c018d, 0x01ad, 0x01b5, 0x0000, 0xffff, b"" ),
    // USB#0x0c018e: AL Calendar/Schedule
    RawEntry( 0x0c018e, 0x018d, 0x0195, 0x0000, 0xffff, b"" ),
    // USB#0x0c018f AL Task/Project Manager
    //RawEntry( 0x0c018f, 0x0241, 0x0249, 0x0000, 0xffff, b""),
    // USB#0x0c0190: AL Log/Journal/Timecard
    //RawEntry( 0x0c0190, 0x0242, 0x024a, 0x0000, 0xffff, b""),
    // USB#0x0c0192: AL_Calculator
    RawEntry( 0x0c0192, 0x008c, 0x0094, 0xe021, 0xffff, b"LaunchApp2" ),
    // USB#0c0194: My Computer (AL_LocalMachineBrowser)
    RawEntry( 0x0c0194, 0x0090, 0x0098, 0xe06b, 0xffff, b"LaunchApp1" ),
    RawEntry( 0x0c0196, 0x0096, 0x009e, 0x0000, 0xffff, b"" ),
    RawEntry( 0x0c019C, 0x01b1, 0x01b9, 0x0000, 0xffff, b"" ),
    // USB#0x0c019e: AL Terminal Lock/Screensaver
    RawEntry( 0x0c019e, 0x0098, 0x00a0, 0x0000, 0xffff, b"" ),
    // USB#0x0c019f AL Control Panel
    RawEntry( 0x0c019f, 0x0243, 0x024b, 0x0000, 0xffff, b"LaunchControlPanel" ),
    // USB#0x0c01a2: AL Select Task/Application
    RawEntry( 0x0c01a2, 0x0244, 0x024c, 0x0000, 0xffff, b"SelectTask" ),
    // USB#0x0c01a7: AL_Documents
    RawEntry( 0x0c01a7, 0x00eb, 0x00f3, 0x0000, 0xffff, b"" ),
    RawEntry( 0x0c01ab, 0x01b0, 0x01b8, 0x0000, 0xffff, b"" ),
    // USB#0x0c01ae: AL Keyboard Layout
    RawEntry( 0x0c01ae, 0x0176, 0x017e, 0x0000, 0xffff, b"" ),
    RawEntry( 0x0c01b1, 0x0245, 0x024d, 0x0000, 0xffff, b"LaunchScreenSaver" ),
    RawEntry( 0x0c01cb, 0x0247, 0x024f, 0x0000, 0xffff, b"LaunchAssistant" ),
    // USB#0c01b4: Home Directory (AL_FileBrowser) (Explorer)
    //RawEntry( 0x0c01b4, 0x0000, 0x0000, 0x0000, 0xffff, b""),
    // USB#0x0c01b7: AL Audio Browser
    RawEntry( 0x0c01b7, 0x0188, 0x0190, 0x0000, 0xffff, b"" ),
    // USB#0x0c0201: AC New
    RawEntry( 0x0c0201, 0x00b5, 0x00bd, 0x0000, 0xffff, b"" ),
    // USB#0x0c0203: AC Close
    RawEntry( 0x0c0203, 0x00ce, 0x00d6, 0x0000, 0xffff, b"" ),
    // USB#0x0c0207: AC Close
    RawEntry( 0x0c0207, 0x00ea, 0x00f2, 0x0000, 0xffff, b"" ),
    // USB#0x0c0208: AC Print
    RawEntry( 0x0c0208, 0x00d2, 0x00da, 0x0000, 0xffff, b"" ),
    // USB#0x0c0221:  AC_Search
    RawEntry( 0x0c0221, 0x00d9, 0x00e1, 0xe065, 0xffff, b"BrowserSearch" ),
    // USB#0x0c0223:  AC_Home
    RawEntry( 0x0c0223, 0x00ac, 0x00b4, 0xe032, 0xffff, b"BrowserHome" ),
    // USB#0x0c0224:  AC_Back
    RawEntry( 0x0c0224, 0x009e, 0x00a6, 0xe06a, 0xffff, b"BrowserBack" ),
    // USB#0x0c0225:  AC_Forward
    RawEntry( 0x0c0225, 0x009f, 0x00a7, 0xe069, 0xffff, b"BrowserForward" ),
    // USB#0x0c0226:  AC_Stop
    RawEntry( 0x0c0226, 0x0080, 0x0088, 0xe068, 0xffff, b"BrowserStop" ),
    // USB#0x0c0227:  AC_Refresh (Reload)
    RawEntry( 0x0c0227, 0x00ad, 0x00b5, 0xe067, 0xffff, b"BrowserRefresh" ),
    // USB#0x0c022a:  AC_Bookmarks (Favorites)
    RawEntry( 0x0c022a, 0x009c, 0x00a4, 0xe066, 0xffff, b"BrowserFavorites" ),
    RawEntry( 0x0c022d, 0x01a2, 0x01aa, 0x0000, 0xffff, b"" ),
    RawEntry( 0x0c022e, 0x01a3, 0x01ab, 0x0000, 0xffff, b"" ),
    // USB#0x0c0230:  AC Full Screen View
    //RawEntry( 0x0c0230, 0x0000, 0x0000, 0x0000, 0xffff, b""),
    // USB#0x0c0231:  AC Normal View
    //RawEntry( 0x0c0231, 0x0000, 0x0000, 0x0000, 0xffff, b""),
    RawEntry( 0x0c0232, 0x0174, 0x017c, 0x0000, 0xffff, b"ZoomToggle" ),
    // USB#0x0c0279:  AC Redo/Repeat
    RawEntry( 0x0c0279, 0x00b6, 0x00be, 0x0000, 0xffff, b"" ),
    // USB#0x0c0289:  AC_Reply
    RawEntry( 0x0c0289, 0x00e8, 0x00f0, 0x0000, 0xffff, b"MailReply" ),
    // USB#0x0c028b:  AC_ForwardMsg (MailForward)
    RawEntry( 0x0c028b, 0x00e9, 0x00f1, 0x0000, 0xffff, b"MailForward" ),
    // USB#0x0c028c:  AC_Send
    RawEntry( 0x0c028c, 0x00e7, 0x00ef, 0x0000, 0xffff, b"MailSend" ),
    // USB#0x0c029d:  AC Next Keyboard Layout Select
    RawEntry( 0x0c029d, 0x0248, 0x0250, 0x0000, 0xffff, b"KeyboardLayoutSelect" ),
    RawEntry( 0x0c029f, 0x0078, 0x0080, 0x0000, 0xffff, b"ShowAllWindows" ),
    RawEntry( 0x0c02a2, 0x00cc, 0x00d4, 0x0000, 0xffff, b"" ),
    RawEntry( 0x0c02d0, 0x0279, 0x0281, 0x0000, 0xffff, b"" ),
];

#[cfg(test)]
mod tests {
    // test {}
    //
    // PORT NOTE: the reference's trailing `test {}` is an empty test block: its
    // only purpose is to force semantic analysis of the file's comptime
    // declarations (`entries`, `code_to_key`, `raw_entries`). In Rust those are
    // not comptime, so this test references each to ensure they compile and
    // build without error, matching the analysis-forcing intent.
    use super::*;

    #[test]
    fn empty() {
        let _ = entries();
        let _ = raw_entries();
        let _ = code_to_key_get(b"KeyA");
    }
}
