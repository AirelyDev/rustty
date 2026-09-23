// Keymap is responsible for translating keyboard inputs into localized chars.
//
// PORT NOTE: the reference's file-level comment uses `///` doc markers attached
// to the file's `const Keymap = @This();` declaration. In Rust this header
// documents no immediate item (the `Keymap` struct is gated/declared below), so
// the markers are normalized to `//` line comments; the text is verbatim.
//
// For example, the physical key "S" on a US-layout keyboard might mean "O"
// in Dvorak. On international keyboard layouts, it may require multiple
// keystrokes to produce a single character that is otherwise a single
// keystroke on a US-layout keyboard.
//
// This information is critical to know for many reasons. For keybindings,
// if a user configures "ctrl+o" to do something, it should work with the
// physical "ctrl+S" key on a Dvorak keyboard and so on.
//
// This is currently only implemented for macOS.
//
// PORTS: ghostty src/input/KeymapDarwin.zig
//
// PORT STATUS: done
//
// PORT NOTE: the reference file IS the `Keymap` struct via
//   const Keymap = @This();
// and imports
//   const std = @import("std");
//   const builtin = @import("builtin");
//   const Allocator = std.mem.Allocator;
//   const macos = @import("macos");
//   const codes = @import("keycodes.zig").entries;
//   const Mods = @import("key.zig").Mods;
// `macos.foundation.{CFRelease,Data,String}` map to CoreFoundation `extern "C"`
// declarations (R3: a binding to the SAME framework). `codes` maps to the ported
// `crate::keycodes::entries()`. `Mods` maps to `rustty_vt::input::key_mods::Mods`.
// `builtin.mode == .Debug` maps to `cfg!(debug_assertions)`.
//
// The Carbon/CoreFoundation FFI below is macOS-only (the reference is too — see
// the doc comment). On non-macOS the externs are absent and the struct's
// platform methods are gated `#[cfg(target_os = "macos")]`, mirroring the Zig
// file only being compiled into the macOS build.

use rustty_vt::input::key_mods::Mods;

/// PORT NOTE: the reference declares the keymap's fields at file scope (the file
/// IS the `Keymap` struct). Here they are the fields of an explicit `Keymap`
/// struct. Both fields are macOS opaque pointers.
#[cfg(target_os = "macos")]
pub struct Keymap {
    /// The current input source that is selected for the keyboard. This can
    /// and does change whenever the user selects a new keyboard layout. This
    /// change doesn't happen automatically; the user of this struct has to
    /// detect it and then call `reload` to update the keymap.
    source: *mut TISInputSource,

    /// The keyboard layout for the current input source.
    ///
    /// This doesn't need to be freed because its owned by the InputSource.
    unicode_layout: *const UCKeyboardLayout,
}

/// PORT NOTE: `Keymap.Error = error{ GetInputSourceFailed, TranslateFailed }`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    GetInputSourceFailed,
    TranslateFailed,
    /// PORT NOTE: `sourceId` returns `Allocator.Error![]const u8`; the only
    /// `Allocator.Error` value is `OutOfMemory`, used here as the failure tag
    /// when the CoreFoundation lookups return null.
    OutOfMemory,
}

/// The state that has to be passed in with each call to translate.
/// The contents of this are meant to mostly be opaque and can change
/// for platform-specific reasons.
#[derive(Clone, Copy, Debug, Default)]
pub struct State {
    pub dead_key: u32,
}

/// The result of a translation. The result of a translation can be multiple
/// states. For example, if the user types a dead key, the result will be
/// "composing" since they're still in the process of composing a full
/// character.
//
// PORT NOTE: the reference field defaults are
//   text: []const u8 = "", composing: bool = false, mods: Mods = .{}
// which are exactly the Rust-derived defaults (`Vec::new()`, `false`,
// `Mods::default()`), so `Default` is derived.
#[derive(Default)]
pub struct Translation {
    /// The translation result. If this is a dead key state, then this will
    /// be pre-edit text that can be displayed but will ultimately be replaced.
    pub text: Vec<u8>,

    /// Whether the text is still composing, i.e. this is a dead key state.
    pub composing: bool,

    /// The mods that were consumed to produce this translation
    pub mods: Mods,
}

#[cfg(target_os = "macos")]
impl Keymap {
    pub fn init() -> Result<Keymap, Error> {
        // var keymap: Keymap = .{ .source = undefined, .unicode_layout = undefined };
        let mut keymap = Keymap {
            source: std::ptr::null_mut(),
            unicode_layout: std::ptr::null(),
        };
        keymap.reinit()?;
        Ok(keymap)
    }

    pub fn deinit(&self) {
        unsafe { CFRelease(self.source as *mut std::ffi::c_void) };
    }

    /// Reload the keymap. This must be called if the user changes their
    /// keyboard layout.
    //
    // PORT NOTE: reference defect (R13), UD69. The reference releases
    // `self.source` FIRST and then `reinit`s; if `reinit` fails, the keymap keeps
    // either the released pointer (a null `TISCopy…` returns before the store),
    // which `deinit` then releases a second time, or a new source whose
    // `unicode_layout` still points into the released one. The port reinits
    // first and releases the old source only once the new one is committed, so
    // a failed reload leaves the keymap on its previous, still-owned layout and
    // `deinit` releases exactly what the keymap holds. Reachable when the OS
    // hands back no current layout, or one without `uchr` data, after the user
    // switches input source. The error still propagates, as in the reference.
    pub fn reload(&mut self) -> Result<(), Error> {
        self.reload_with(
            || unsafe { TISCopyCurrentKeyboardLayoutInputSource() },
            |source| unsafe { TISGetInputSourceProperty(source, kTISPropertyUnicodeKeyLayoutData) },
        )
    }

    /// [`reload`](Self::reload) with the two TIS calls passed in, so a test can
    /// make either return null. Production passes the real calls.
    fn reload_with(
        &mut self,
        copy_source: impl FnOnce() -> *mut TISInputSource,
        layout_data: impl FnOnce(*mut TISInputSource) -> *mut std::ffi::c_void,
    ) -> Result<(), Error> {
        let old = self.source;
        self.reinit_with(copy_source, layout_data)?;
        unsafe { CFRelease(old as *mut std::ffi::c_void) };
        Ok(())
    }

    /// Get the input source ID for the current keyboard layout. The input
    /// source ID is a unique identifier for the keyboard layout which is uniquely
    /// defined by Apple.
    ///
    /// This is macOS-only. Other platforms don't have an equivalent of this
    /// so this isn't expected to be generally implemented.
    pub fn source_id<'a>(&self, buf: &'a mut [u8]) -> Result<&'a [u8], Error> {
        // Get the raw CFStringRef
        let id_raw = unsafe { TISGetInputSourceProperty(self.source, kTISPropertyInputSourceID) };
        if id_raw.is_null() {
            return Err(Error::OutOfMemory);
        }

        // Convert the CFStringRef to a C string into our buffer.
        // const id: *CFString = @ptrCast(id_raw);
        // return id.cstring(buf, .utf8) orelse error.OutOfMemory;
        let id = id_raw as *const CFString;
        cf_string_cstring(id, buf).ok_or(Error::OutOfMemory)
    }

    /// Reinit reinitializes the keymap. It assumes that all the memory associated
    /// with the keymap is already freed.
    //
    // PORT NOTE: reference defect (R13), UD69. The reference stores the +1
    // `TISCopy…` result into `self.source` and then returns
    // `GetInputSourceFailed` if the source has no `kTISPropertyUnicodeKeyLayoutData`,
    // so `init` (which then discards the keymap) leaks the source, and `reload`
    // is left holding a source whose layout was never read. The port copies into
    // a local, releases it on that error path, and commits `source` and
    // `unicode_layout` together only on success; on any error the keymap's
    // fields are untouched. (`reload` now releases the previous source after
    // this succeeds, so the "already freed" precondition above is kept by the
    // caller's ordering rather than before the call.)
    fn reinit(&mut self) -> Result<(), Error> {
        self.reinit_with(
            || unsafe { TISCopyCurrentKeyboardLayoutInputSource() },
            |source| unsafe { TISGetInputSourceProperty(source, kTISPropertyUnicodeKeyLayoutData) },
        )
    }

    /// [`reinit`](Self::reinit) with the two TIS calls passed in (see
    /// [`reload_with`](Self::reload_with)).
    fn reinit_with(
        &mut self,
        copy_source: impl FnOnce() -> *mut TISInputSource,
        layout_data: impl FnOnce(*mut TISInputSource) -> *mut std::ffi::c_void,
    ) -> Result<(), Error> {
        let source = copy_source();
        if source.is_null() {
            return Err(Error::GetInputSourceFailed);
        }

        let unicode_layout = {
            // This returns a CFDataRef
            let data_raw = layout_data(source);
            if data_raw.is_null() {
                unsafe { CFRelease(source as *mut std::ffi::c_void) };
                return Err(Error::GetInputSourceFailed);
            }
            let data = data_raw as *const CFData;

            // The CFDataRef contains a UCKeyboardLayout pointer
            unsafe { cf_data_get_pointer(data) as *const UCKeyboardLayout }
        };
        self.source = source;
        self.unicode_layout = unicode_layout;

        // if (comptime builtin.mode == .Debug) id: { ... }
        if cfg!(debug_assertions) {
            let mut buf: [u8; 256] = [0; 256];
            // const id = self.sourceId(&buf) catch break :id;
            if let Ok(id) = self.source_id(&mut buf) {
                // std.log.debug("keyboard layout={s}", .{id});
                // PORT NOTE: `std.log.debug` -> the established `eprintln!`
                // debug-log shim (see `rustty-os` log helpers).
                eprintln!("[debug] keyboard layout={}", String::from_utf8_lossy(id));
            }
        }

        Ok(())
    }

    /// Translate a single key input into a utf8 sequence.
    pub fn translate(
        &self,
        out: &mut [u8],
        state: &mut State,
        code: u16,
        input_mods: Mods,
    ) -> Result<Translation, Error> {
        // On macOS we strip ctrl because UCKeyTranslate
        // converts to the masked values (i.e. ctrl+c becomes 3)
        // and we don't want that behavior in Rustty ever. This makes
        // this file not a general-purpose keymap implementation.
        let mods: Mods = {
            let mut v = input_mods;
            v.ctrl = false;
            v
        };

        // Get the keycode for the space key, using comptime.
        // PORT NOTE: the reference scans `codes` (comptime) for the "Space"
        // entry's `native` value. Here the same scan runs over the ported
        // `keycodes::entries()`. The reference `else => @compileError(...)` is
        // a build-time guarantee the entry exists; if absent at runtime we
        // surface `TranslateFailed`.
        let code_space: u16 = {
            let entries = crate::keycodes::entries();
            let mut found: Option<u16> = None;
            for entry in entries.iter() {
                if entry.code == b"Space" {
                    found = Some(entry.native as u16);
                    break;
                }
            }
            match found {
                Some(v) => v,
                None => return Err(Error::TranslateFailed), // space code not found
            }
        };

        // Convert our mods from our format to the Carbon API format
        let modifier_state: u32 = CarbonMods {
            alt: mods.alt,
            ctrl: mods.ctrl,
            meta: mods.super_,
            shift: mods.shift,
            caps_lock: mods.caps_lock,
        }
        .uc_key_translate();

        // We use 4 here because the Chromium source code uses 4 and Chrome
        // works pretty well. They have a todo to look into longer sequences
        // but given how mature that software is I think this is fine.
        //
        // From Chromium:
        // Per Apple docs, the buffer length can be up to 255 but is rarely more than 4.
        // https://developer.apple.com/documentation/coreservices/1390584-uckeytranslate
        let mut char: [u16; 4] = [0; 4];
        let mut char_count: std::os::raw::c_ulong = 0;
        if unsafe {
            UCKeyTranslate(
                self.unicode_layout,
                code,
                K_UC_KEY_ACTION_DOWN,
                modifier_state,
                LMGetKbdType().into(),
                K_UC_KEY_TRANSLATE_NO_DEAD_KEYS_BIT,
                &mut state.dead_key,
                char.len() as std::os::raw::c_ulong,
                &mut char_count,
                char.as_mut_ptr(),
            )
        } != 0
        {
            return Err(Error::TranslateFailed);
        }

        // If we got a dead key, then we translate again with "space"
        // in order to get the pre-edit text.
        let composing = if state.dead_key != 0 && char_count == 0 {
            // We need to copy our dead key state so that it isn't modified.
            let mut dead_key_ignore: u32 = state.dead_key;
            if unsafe {
                UCKeyTranslate(
                    self.unicode_layout,
                    code_space,
                    K_UC_KEY_ACTION_DOWN,
                    modifier_state,
                    LMGetKbdType().into(),
                    K_UC_KEY_TRANSLATE_NO_DEAD_KEYS_MASK,
                    &mut dead_key_ignore,
                    char.len() as std::os::raw::c_ulong,
                    &mut char_count,
                    char.as_mut_ptr(),
                )
            } != 0
            {
                return Err(Error::TranslateFailed);
            }
            true
        } else {
            false
        };

        // Convert the utf16 to utf8
        // const len = try std.unicode.utf16LeToUtf8(out, char[0..char_count]);
        let len = utf16_le_to_utf8(out, &char[0..char_count as usize])
            .map_err(|_| Error::TranslateFailed)?;
        Ok(Translation {
            text: out[0..len].to_vec(),
            composing,
            mods,
        })
    }
}

/// Map to the modifiers format used by the UCKeyTranslate function.
/// We use a u32 here because our bit arithmetic is all u32 anyways.
//
// PORT NOTE: the reference is `packed struct(u32)` with explicit bit layout:
//   _padding_start: u8 = 0  (bits 0..=7)
//   meta: bool              (bit 8)
//   shift: bool             (bit 9)
//   caps_lock: bool         (bit 10)
//   alt: bool               (bit 11)
//   ctrl: bool              (bit 12)
//   _padding_end: u19 = 0   (bits 13..=31)
// `@bitCast(self)` to/from `u32` is reproduced by `int()` below, which assembles
// the bits at exactly those positions.
//
// PORT NOTE: the reference file is compiled only into the macOS build, so its
// `CarbonMods` has no other-target existence. This port keeps the type (and the
// `expected_values` test that pins its bit layout) portable so the ported test
// runs everywhere, which means the type is live off Darwin only under `test` —
// hence the cfg. `uc_key_translate` below stays Darwin-only because its sole
// caller, `translate`, is.
#[cfg(any(target_os = "macos", test))]
#[derive(Clone, Copy, Debug, Default)]
struct CarbonMods {
    meta: bool,
    shift: bool,
    caps_lock: bool,
    alt: bool,
    ctrl: bool,
}

#[cfg(any(target_os = "macos", test))]
impl CarbonMods {
    // const int: u32 = @bitCast(self);
    fn int(self) -> u32 {
        ((self.meta as u32) << 8)
            | ((self.shift as u32) << 9)
            | ((self.caps_lock as u32) << 10)
            | ((self.alt as u32) << 11)
            | ((self.ctrl as u32) << 12)
    }

    /// Translate NSEventModifierFlags into the format used by UCKeyTranslate.
    #[cfg(target_os = "macos")]
    fn uc_key_translate(self) -> u32 {
        let int: u32 = self.int();
        (int >> 8) & 0xFF
    }
}

// The documentation for all of these types and functions is in the macOS SDK:
// Carbon.framework/Versions/A/Frameworks/HIToolbox.framework/Versions/A/Headers/TextInputSources.h
//
// PORT NOTE: the reference declares these `extern "c"` symbols and `opaque{}`
// types. They are macOS-only — gated `#[cfg(target_os = "macos")]` and linked
// from the Carbon/CoreFoundation frameworks (R3: SAME libraries).
#[cfg(target_os = "macos")]
#[repr(C)]
pub struct TISInputSource {
    _private: [u8; 0],
}
#[cfg(target_os = "macos")]
#[repr(C)]
pub struct UCKeyboardLayout {
    _private: [u8; 0],
}
#[cfg(target_os = "macos")]
#[repr(C)]
struct CFString {
    _private: [u8; 0],
}
#[cfg(target_os = "macos")]
#[repr(C)]
struct CFData {
    _private: [u8; 0],
}

#[cfg(target_os = "macos")]
#[link(name = "Carbon", kind = "framework")]
extern "C" {
    fn TISCopyCurrentKeyboardLayoutInputSource() -> *mut TISInputSource;
    fn TISGetInputSourceProperty(
        source: *mut TISInputSource,
        key: *const CFString,
    ) -> *mut std::ffi::c_void;
    #[allow(dead_code)]
    fn LMGetKbdLast() -> u8;
    fn LMGetKbdType() -> u8;
    #[allow(clippy::too_many_arguments)]
    fn UCKeyTranslate(
        layout: *const UCKeyboardLayout,
        virtual_key_code: u16,
        key_action: u16,
        modifier_key_state: u32,
        keyboard_type: u32,
        key_translate_options: u32,
        dead_key_state: *mut u32,
        max_string_length: std::os::raw::c_ulong,
        actual_string_length: *mut std::os::raw::c_ulong,
        unicode_string: *mut u16,
    ) -> i32;
    // PORT NOTE: declared by the reference but unused (as in the reference).
    #[allow(dead_code)]
    static kTISPropertyLocalizedName: *const CFString;
    static kTISPropertyUnicodeKeyLayoutData: *const CFString;
    static kTISPropertyInputSourceID: *const CFString;
}

// PORT NOTE: `macos.foundation.CFRelease`, `CFString.cstring`, `CFData.getPointer`
// wrap CoreFoundation primitives. They are declared here directly (the same
// framework). `cstring` is `CFStringGetCString(_, buf, len, kCFStringEncodingUTF8)`;
// `getPointer` is `CFDataGetBytePtr`.
#[cfg(target_os = "macos")]
#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFRelease(cf: *mut std::ffi::c_void);
    fn CFStringGetCString(
        the_string: *const CFString,
        buffer: *mut std::os::raw::c_char,
        buffer_size: std::os::raw::c_long,
        encoding: u32,
    ) -> u8;
    fn CFDataGetBytePtr(the_data: *const CFData) -> *const u8;
}

// kCFStringEncodingUTF8
#[cfg(target_os = "macos")]
const K_CF_STRING_ENCODING_UTF8: u32 = 0x0800_0100;

// PORT NOTE: `String.cstring(buf, .utf8)` returns the bytes written (excluding
// the NUL) or null on failure. CFStringGetCString writes a NUL-terminated C
// string into `buf`; we return the slice up to (not including) the NUL.
#[cfg(target_os = "macos")]
fn cf_string_cstring(s: *const CFString, buf: &mut [u8]) -> Option<&[u8]> {
    let ok = unsafe {
        CFStringGetCString(
            s,
            buf.as_mut_ptr() as *mut std::os::raw::c_char,
            buf.len() as std::os::raw::c_long,
            K_CF_STRING_ENCODING_UTF8,
        )
    };
    if ok == 0 {
        return None;
    }
    // Find the NUL terminator.
    let len = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    Some(&buf[0..len])
}

#[cfg(target_os = "macos")]
unsafe fn cf_data_get_pointer(data: *const CFData) -> *const std::ffi::c_void {
    unsafe { CFDataGetBytePtr(data) as *const std::ffi::c_void }
}

// const kUCKeyActionDown: u16 = 0;
#[cfg(target_os = "macos")]
const K_UC_KEY_ACTION_DOWN: u16 = 0;
// const kUCKeyActionUp: u16 = 1;
#[cfg(target_os = "macos")]
#[allow(dead_code)]
const K_UC_KEY_ACTION_UP: u16 = 1;
// const kUCKeyActionAutoKey: u16 = 2;
#[cfg(target_os = "macos")]
#[allow(dead_code)]
const K_UC_KEY_ACTION_AUTO_KEY: u16 = 2;
// const kUCKeyActionDisplay: u16 = 3;
#[cfg(target_os = "macos")]
#[allow(dead_code)]
const K_UC_KEY_ACTION_DISPLAY: u16 = 3;
// const kUCKeyTranslateNoDeadKeysBit: u32 = 0;
#[cfg(target_os = "macos")]
const K_UC_KEY_TRANSLATE_NO_DEAD_KEYS_BIT: u32 = 0;
// const kUCKeyTranslateNoDeadKeysMask: u32 = 1 << kUCKeyTranslateNoDeadKeysBit;
#[cfg(target_os = "macos")]
const K_UC_KEY_TRANSLATE_NO_DEAD_KEYS_MASK: u32 = 1 << K_UC_KEY_TRANSLATE_NO_DEAD_KEYS_BIT;

// PORT NOTE: `std.unicode.utf16LeToUtf8(out, src)` decodes the little-endian
// UTF-16 codeunits `src` into UTF-8 written to `out`, returning the byte length.
// `char` is a native `[u16; 4]`; on a little-endian target (all Rustty targets)
// the in-memory representation is already little-endian, matching the reference.
#[cfg(target_os = "macos")]
fn utf16_le_to_utf8(out: &mut [u8], src: &[u16]) -> Result<usize, ()> {
    let mut i = 0usize;
    for c in char::decode_utf16(src.iter().copied()) {
        let c = c.map_err(|_| ())?;
        let mut tmp = [0u8; 4];
        let s = c.encode_utf8(&mut tmp);
        let bytes = s.as_bytes();
        if i + bytes.len() > out.len() {
            return Err(());
        }
        out[i..i + bytes.len()].copy_from_slice(bytes);
        i += bytes.len();
    }
    Ok(i)
}

#[cfg(test)]
mod tests {
    use super::*;

    // We got this from dumping the values out of another program since
    // I can't find the header for this anywhere. I find various modifier
    // headers but they do not match this! 🥺
    #[test]
    fn expected_values() {
        // const testing = std.testing;
        // try testing.expectEqual(@as(u32, 0x100), @as(u32, @bitCast(CarbonMods{ .meta = true })));
        assert_eq!(
            0x100u32,
            CarbonMods {
                meta: true,
                ..CarbonMods::default()
            }
            .int()
        );
        // try testing.expectEqual(@as(u32, 0x200), @as(u32, @bitCast(CarbonMods{ .shift = true })));
        assert_eq!(
            0x200u32,
            CarbonMods {
                shift: true,
                ..CarbonMods::default()
            }
            .int()
        );
        // try testing.expectEqual(@as(u32, 0x400), @as(u32, @bitCast(CarbonMods{ .caps_lock = true })));
        assert_eq!(
            0x400u32,
            CarbonMods {
                caps_lock: true,
                ..CarbonMods::default()
            }
            .int()
        );
        // try testing.expectEqual(@as(u32, 0x800), @as(u32, @bitCast(CarbonMods{ .alt = true })));
        assert_eq!(
            0x800u32,
            CarbonMods {
                alt: true,
                ..CarbonMods::default()
            }
            .int()
        );
        // try testing.expectEqual(@as(u32, 0x1000), @as(u32, @bitCast(CarbonMods{ .ctrl = true })));
        assert_eq!(
            0x1000u32,
            CarbonMods {
                ctrl: true,
                ..CarbonMods::default()
            }
            .int()
        );
    }

    // PORT NOTE: the reference's trailing anonymous `test {}` calls `init()` /
    // `deinit()` (the body otherwise is fully commented because it depends on
    // the user-selected keyboard layout). `init` requires live macOS Carbon
    // input-source APIs, so the test is gated `#[cfg(target_os = "macos")]`
    // (the reference file is itself macOS-only). The commented exploratory
    // body is preserved verbatim below.
    #[cfg(target_os = "macos")]
    #[test]
    fn anonymous() {
        let _tis = TIS_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        // var keymap = try init();
        // defer keymap.deinit();
        let keymap = Keymap::init().unwrap();

        // These tests are all commented because they depend on the user-selected
        // keyboard layout...
        //
        // // Single quote ' which is fine on US, but dead on US-International
        // var buf: [4]u8 = undefined;
        // var state: State = .{};
        // {
        //     const result = try keymap.translate(&buf, &state, 0x27, .{});
        //     std.log.warn("map: text={s} dead={}", .{ result.text, result.composing });
        // }
        //
        // // Then type "a" which should combine with the dead key to make á
        // {
        //     const result = try keymap.translate(&buf, &state, 0x00, .{});
        //     std.log.warn("map: text={s} dead={}", .{ result.text, result.composing });
        // }
        //
        // // Shift+1 = ! on US
        // {
        //     const result = try keymap.translate(&buf, &state, 0x12, .{ .shift = true });
        //     std.log.warn("map: text={s} dead={}", .{ result.text, result.composing });
        // }
        //
        // // Scratch space
        // {
        //     const result = try keymap.translate(&buf, &state, 0x00, .{ .ctrl = true });
        //     std.log.warn("map: text={s} dead={}", .{ result.text, result.composing });
        // }

        // PORT NOTE: `defer keymap.deinit();` runs here at scope end.
        keymap.deinit();
    }

    // UD69 (R13) tests. The two TIS calls are replaced through `reinit_with` /
    // `reload_with` (production passes the real calls), so a test can make the
    // copy or the layout lookup return null. The "input sources" are plain
    // CFData objects: on every path these tests take, nothing hands them to a
    // TIS function (the success path's debug `source_id` is never reached), and
    // a fresh CF object's retain count is exact, so a leak (+1) or an extra
    // release (-1) is observable.
    #[cfg(target_os = "macos")]
    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFDataCreate(
            allocator: *const std::ffi::c_void,
            bytes: *const u8,
            length: std::os::raw::c_long,
        ) -> *mut std::ffi::c_void;
        fn CFRetain(cf: *mut std::ffi::c_void) -> *mut std::ffi::c_void;
        fn CFGetRetainCount(cf: *mut std::ffi::c_void) -> std::os::raw::c_long;
    }

    // HIToolbox's TIS calls are not thread-safe: two libtest workers calling
    // `TISCopyCurrentKeyboardLayoutInputSource` at once abort inside
    // HIToolbox (the same finding as `rustty-apprt-c/tests/macos_window_host.rs`).
    // The app calls them from one thread; the tests that make real TIS calls
    // take this lock to keep that contract.
    #[cfg(target_os = "macos")]
    static TIS_SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[cfg(target_os = "macos")]
    fn fake_source() -> *mut TISInputSource {
        let bytes = [1u8, 2, 3, 4];
        let obj = unsafe { CFDataCreate(std::ptr::null(), bytes.as_ptr(), 4) };
        assert!(!obj.is_null());
        obj.cast()
    }

    #[cfg(target_os = "macos")]
    fn retain_count(obj: *mut TISInputSource) -> std::os::raw::c_long {
        unsafe { CFGetRetainCount(obj.cast()) }
    }

    /// `init`'s leak: the copy succeeds (+1), the layout lookup fails. The copy
    /// must be released and the keymap fields left untouched.
    #[cfg(target_os = "macos")]
    #[test]
    fn a_source_without_layout_data_is_released_on_the_error_path() {
        let s = fake_source();
        let before = retain_count(s);
        let mut looked_up = false;
        let mut keymap = Keymap {
            source: std::ptr::null_mut(),
            unicode_layout: std::ptr::null(),
        };
        let got = keymap.reinit_with(
            || unsafe { CFRetain(s.cast()) }.cast(),
            |src| {
                assert_eq!(src, s);
                looked_up = true;
                std::ptr::null_mut()
            },
        );
        assert_eq!(Err(Error::GetInputSourceFailed), got);
        assert!(looked_up, "the layout lookup ran on the copied source");
        assert_eq!(
            before,
            retain_count(s),
            "the +1 copy was released, not leaked"
        );
        assert!(keymap.source.is_null() && keymap.unicode_layout.is_null());
        unsafe { CFRelease(s.cast()) };
    }

    /// `reload`'s double release: the new copy returns null. The keymap must
    /// keep its old source, NOT released, so `deinit` releases it exactly once.
    #[cfg(target_os = "macos")]
    #[test]
    fn a_failed_reload_keeps_the_old_source_owned() {
        let old = fake_source();
        // Our own +1, so the object outlives the keymap's release and its count
        // can still be read afterwards.
        unsafe { CFRetain(old.cast()) };
        let layout = 0x1000 as *const UCKeyboardLayout;
        let mut keymap = Keymap {
            source: old,
            unicode_layout: layout,
        };
        let before = retain_count(old);
        let mut copied = false;

        let got = keymap.reload_with(
            || {
                copied = true;
                std::ptr::null_mut()
            },
            |_| unreachable!("no source to look up"),
        );
        assert_eq!(Err(Error::GetInputSourceFailed), got);
        assert!(copied, "the reload reached the copy");
        assert_eq!(old, keymap.source, "the keymap keeps its source, not null");
        assert_eq!(layout, keymap.unicode_layout);
        assert_eq!(before, retain_count(old), "the old source was not released");

        keymap.deinit();
        assert_eq!(before - 1, retain_count(old), "deinit releases it once");
        unsafe { CFRelease(old.cast()) };
    }

    /// `reload` where the new source has no layout data: the new copy is
    /// released, and the keymap keeps the old source and layout.
    #[cfg(target_os = "macos")]
    #[test]
    fn a_reload_to_a_source_without_layout_data_keeps_the_old_layout() {
        let old = fake_source();
        unsafe { CFRetain(old.cast()) };
        let new = fake_source();
        let layout = 0x1000 as *const UCKeyboardLayout;
        let mut keymap = Keymap {
            source: old,
            unicode_layout: layout,
        };
        let (old_before, new_before) = (retain_count(old), retain_count(new));

        let got = keymap.reload_with(
            || unsafe { CFRetain(new.cast()) }.cast(),
            |_| std::ptr::null_mut(),
        );
        assert_eq!(Err(Error::GetInputSourceFailed), got);
        assert_eq!((old, layout), (keymap.source, keymap.unicode_layout));
        assert_eq!(
            old_before,
            retain_count(old),
            "the old source was not released"
        );
        assert_eq!(new_before, retain_count(new), "the new copy was released");

        keymap.deinit();
        unsafe {
            CFRelease(old.cast());
            CFRelease(new.cast());
        }
    }

    /// A successful reload through the real TIS calls still commits a source
    /// and releases the previous one (the reference's behaviour on success).
    #[cfg(target_os = "macos")]
    #[test]
    fn a_successful_reload_replaces_the_source() {
        let _tis = TIS_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let old = fake_source();
        unsafe { CFRetain(old.cast()) };
        let mut keymap = Keymap {
            source: old,
            unicode_layout: std::ptr::null(),
        };
        let before = retain_count(old);
        keymap.reload().expect("the current keyboard layout loads");
        assert_ne!(old, keymap.source);
        assert!(!keymap.unicode_layout.is_null());
        assert_eq!(
            before - 1,
            retain_count(old),
            "the previous source was released"
        );
        keymap.deinit();
        unsafe { CFRelease(old.cast()) };
    }
}
