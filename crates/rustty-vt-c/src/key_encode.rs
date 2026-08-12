//! rustty-vt-c key_encode
//!
//! PORTS: ghostty src/terminal/c/key_encode.zig (the key-encoder C ABI layer).
//!
//! The rustty-vt `input::key_encode::Options`/`encode` are the source of truth
//! (R3); this module is the FFI projection. The `rustty_key_encoder_*`
//! `extern "C"` wrappers live in `lib.rs` and forward here.

// const std = @import("std");
// const Allocator = std.mem.Allocator;
// const lib = @import("../lib.zig");
// const CAllocator = lib.alloc.Allocator;
// const key_encode = @import("../../input/key_encode.zig");
// const key_event = @import("key_event.zig");
// const KittyFlags = @import("../../terminal/kitty/key.zig").Flags;
// const OptionAsAlt = @import("../../input/config.zig").OptionAsAlt;
// const Result = @import("result.zig").Result;
// const KeyEvent = @import("key_event.zig").Event;
// const Terminal = @import("terminal.zig").Terminal;
// const ZigTerminal = @import("../Terminal.zig");
//
// const log = std.log.scoped(.key_encode);
use crate::key_event::{self, Event as KeyEvent};
use crate::result::Result;
use crate::terminal as terminal_c;
use crate::writer::{CountingWriter, FixedWriter};
use rustty_lib::allocator::Allocator as CAllocator;
use rustty_vt::input::config::OptionAsAlt;
use rustty_vt::input::key_encode;
use rustty_vt::kitty::key::Flags as KittyFlags;
use std::ffi::c_void;
use std::os::raw::c_int;

use crate::cwrap::{create, destroy};

/// Wrapper around key encoding options that tracks the allocator for C API usage.
// PORT NOTE: the reference `KeyEncoderWrapper` stores `opts` plus its own
// `Allocator`. rustty stores the original (possibly null) C `alloc_` (a `Copy`
// C struct; see `cwrap`) so `free` reconstructs the identical allocator.
pub struct KeyEncoderWrapper {
    opts: key_encode::Options,
    alloc: Option<CAllocator>,
}

/// C: RusttyKeyEncoder (R2)
// pub const Encoder = ?*KeyEncoderWrapper;
pub type Encoder = *mut KeyEncoderWrapper;

// pub fn new(alloc_: ?*const CAllocator, result: *Encoder) ... Result {
//     const alloc = lib.alloc.default(alloc_);
//     const ptr = alloc.create(KeyEncoderWrapper) catch return .out_of_memory;
//     ptr.* = .{ .opts = .{}, .alloc = alloc };
//     result.* = ptr;
//     return .success;
// }
//
/// # Safety
/// `alloc_`, if non-null, must point to a valid `CAllocator`; `result` must be
/// writable storage for one `Encoder`.
pub unsafe fn new(alloc_: Option<&CAllocator>, result: *mut Encoder) -> Result {
    // const ptr = alloc.create(KeyEncoderWrapper) catch return .out_of_memory;
    // ptr.* = .{ .opts = .{}, .alloc = alloc };
    let ptr = unsafe {
        create(
            alloc_,
            KeyEncoderWrapper {
                // .opts = .{},  (Options default)
                opts: key_encode::Options::default(),
                alloc: alloc_.copied(),
            },
        )
    };
    if ptr.is_null() {
        return Result::OutOfMemory;
    }
    // result.* = ptr;
    // SAFETY: `result` is writable per the caller contract.
    unsafe { *result = ptr };
    // return .success;
    Result::Success
}

// pub fn free(encoder_: Encoder) ... void {
//     const wrapper = encoder_ orelse return;
//     const alloc = wrapper.alloc;
//     alloc.destroy(wrapper);
// }
//
/// # Safety
/// `encoder_`, if non-null, must be a handle returned by `new`, not yet freed.
pub unsafe fn free(encoder_: Encoder) {
    // const wrapper = encoder_ orelse return;
    if encoder_.is_null() {
        return;
    }
    // const alloc = wrapper.alloc;
    // SAFETY: `encoder_` is a live handle (caller contract).
    let alloc = unsafe { (*encoder_).alloc };
    // alloc.destroy(wrapper);
    // SAFETY: `encoder_` was produced by `create` with the same `alloc_`.
    unsafe { destroy(alloc.as_ref(), encoder_) };
}

/// C: RusttyKeyEncoderOption (R2)
// pub const Option = enum(c_int) {
//     cursor_key_application = 0,
//     keypad_key_application = 1,
//     ignore_keypad_with_numlock = 2,
//     alt_esc_prefix = 3,
//     modify_other_keys_state_2 = 4,
//     kitty_flags = 5,
//     macos_option_as_alt = 6,
//     /// DEC Backarrow Key Mode (DECBKM)
//     /// See https://vt100.net/dec/ek-vt3xx-tp-002.pdf page 170
//     /// If `false` (the default), `backspace` emits 0x7f
//     /// If `true`, `backspace` emits 0x08
//     backarrow_key_mode = 7,
//     pub fn InType(comptime self: Option) type { ... }
// };
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Option_ {
    CursorKeyApplication = 0,
    KeypadKeyApplication = 1,
    IgnoreKeypadWithNumlock = 2,
    AltEscPrefix = 3,
    ModifyOtherKeysState2 = 4,
    KittyFlags = 5,
    MacosOptionAsAlt = 6,
    /// DEC Backarrow Key Mode (DECBKM)
    /// See https://vt100.net/dec/ek-vt3xx-tp-002.pdf page 170
    /// If `false` (the default), `backspace` emits 0x7f
    /// If `true`, `backspace` emits 0x08
    BackarrowKeyMode = 7,
    // PORT NOTE: `InType` is a comptime type map used by `setoptTyped`; the
    // per-option typed reads are inlined into `setopt` below.
}

// pub fn setopt(encoder_: Encoder, option: Option, value: ?*const anyopaque) ... void {
//     if (comptime std.debug.runtime_safety) {
//         _ = std.meta.intToEnum(Option, @intFromEnum(option)) catch {
//             log.warn("setopt invalid option value={d}", .{@intFromEnum(option)});
//             return;
//         };
//     }
//     return switch (option) {
//         inline else => |comptime_option| setoptTyped(
//             encoder_, comptime_option, @ptrCast(@alignCast(value orelse return)),
//         ),
//     };
// }
//
/// # Safety
/// `encoder_`, if non-null, must be a `new` handle; `value`, if non-null, must
/// point to a readable value of the option's in-type.
pub unsafe fn setopt(encoder_: Encoder, option: c_int, value: *const c_void) {
    // if (comptime std.debug.runtime_safety) {
    //     _ = std.meta.intToEnum(Option, ...) catch { log.warn(...); return; };
    // }
    // PORT NOTE: the option-range guard runs on the raw `c_int` crossing the
    // boundary; an out-of-range `option` logs and returns.
    let option = match option {
        0 => Option_::CursorKeyApplication,
        1 => Option_::KeypadKeyApplication,
        2 => Option_::IgnoreKeypadWithNumlock,
        3 => Option_::AltEscPrefix,
        4 => Option_::ModifyOtherKeysState2,
        5 => Option_::KittyFlags,
        6 => Option_::MacosOptionAsAlt,
        7 => Option_::BackarrowKeyMode,
        _ => {
            // log.warn("setopt invalid option value={d}", .{@intFromEnum(option)});
            eprintln!("setopt invalid option value={option}");
            return;
        }
    };

    // inline else => setoptTyped(encoder_, comptime_option, value orelse return)
    if value.is_null() {
        return;
    }
    // SAFETY: `encoder_`/`value` validity is the caller's contract.
    unsafe { setopt_typed(encoder_, option, value) };
}

// fn setoptTyped(encoder_: Encoder, comptime option: Option, value: *const option.InType()) void {
//     const opts = &encoder_.?.opts;
//     switch (option) { ... }
// }
//
/// # Safety
/// `encoder_` must be non-null (`encoder_.?`); `value` must point to a readable
/// value of `option`'s in-type.
unsafe fn setopt_typed(encoder_: Encoder, option: Option_, value: *const c_void) {
    // const opts = &encoder_.?.opts;
    // SAFETY: `encoder_` is non-null per the caller contract (`.?`).
    let opts = unsafe { &mut (*encoder_).opts };
    match option {
        // .cursor_key_application => opts.cursor_key_application = value.*,
        Option_::CursorKeyApplication => {
            opts.cursor_key_application = unsafe { *(value as *const bool) }
        }
        // .keypad_key_application => opts.keypad_key_application = value.*,
        Option_::KeypadKeyApplication => {
            opts.keypad_key_application = unsafe { *(value as *const bool) }
        }
        // .ignore_keypad_with_numlock => opts.ignore_keypad_with_numlock = value.*,
        Option_::IgnoreKeypadWithNumlock => {
            opts.ignore_keypad_with_numlock = unsafe { *(value as *const bool) }
        }
        // .alt_esc_prefix => opts.alt_esc_prefix = value.*,
        Option_::AltEscPrefix => opts.alt_esc_prefix = unsafe { *(value as *const bool) },
        // .modify_other_keys_state_2 => opts.modify_other_keys_state_2 = value.*,
        Option_::ModifyOtherKeysState2 => {
            opts.modify_other_keys_state_2 = unsafe { *(value as *const bool) }
        }
        // .kitty_flags => opts.kitty_flags = flags: {
        //     const bits: u5 = @truncate(value.*);
        //     break :flags @bitCast(bits);
        // },
        Option_::KittyFlags => {
            // PORT NOTE: `@truncate` to `u5` then `@bitCast`. `KittyFlags::from_int`
            // already masks the low 5 bits (its in-source PORT NOTE), reproducing
            // the truncate+bitcast.
            let v: u8 = unsafe { *(value as *const u8) };
            opts.kitty_flags = KittyFlags::from_int(v);
        }
        // .macos_option_as_alt => { ...guard...; opts.macos_option_as_alt = value.*; },
        Option_::MacosOptionAsAlt => {
            // PORT NOTE: the inner `intToEnum(OptionAsAlt, @intFromEnum(value.*))`
            // guard runs on the raw enum int crossing the boundary; an invalid
            // value logs and returns without mutating opts.
            let raw: c_int = unsafe { *(value as *const c_int) };
            let val = match raw {
                0 => OptionAsAlt::r#false,
                1 => OptionAsAlt::r#true,
                2 => OptionAsAlt::left,
                3 => OptionAsAlt::right,
                _ => {
                    // log.warn("setopt invalid OptionAsAlt value={d}", .{@intFromEnum(value.*)});
                    eprintln!("setopt invalid OptionAsAlt value={raw}");
                    return;
                }
            };
            opts.macos_option_as_alt = val;
        }
        // .backarrow_key_mode => opts.backarrow_key_mode = value.*,
        Option_::BackarrowKeyMode => opts.backarrow_key_mode = unsafe { *(value as *const bool) },
    }
}

// pub fn setopt_from_terminal(encoder_: Encoder, terminal_: Terminal) ... void {
//     const wrapper = encoder_ orelse return;
//     const t: *ZigTerminal = (terminal_ orelse return).terminal;
//     wrapper.opts = .fromTerminal(t);
// }
//
/// # Safety
/// `encoder_`/`terminal_`, if non-null, must be live handles.
pub unsafe fn setopt_from_terminal(encoder_: Encoder, terminal_: terminal_c::Terminal) {
    // const wrapper = encoder_ orelse return;
    if encoder_.is_null() {
        return;
    }
    // const t: *ZigTerminal = (terminal_ orelse return).terminal;
    if terminal_.is_null() {
        return;
    }
    // SAFETY: `terminal_` is a live handle (caller contract; non-null checked).
    let t = unsafe { &*terminal_c::zig_terminal(terminal_) };
    // wrapper.opts = .fromTerminal(t);
    // SAFETY: `encoder_` is a live handle (caller contract; non-null checked).
    unsafe { (*encoder_).opts = key_encode::Options::from_terminal(t) };
}

// pub fn encode(encoder_, event_, out_, out_len, out_written) ... Result {
//     var writer: std.Io.Writer = .fixed(if (out_) |out| out[0..out_len] else &.{});
//     key_encode.encode(&writer, event_.?.event, encoder_.?.opts) catch |err| switch (err) {
//         error.WriteFailed => {
//             var discarding: std.Io.Writer.Discarding = .init(&.{});
//             key_encode.encode(&discarding.writer, event_.?.event, encoder_.?.opts) catch unreachable;
//             out_written.* = @intCast(discarding.count);
//             return .out_of_space;
//         },
//     };
//     out_written.* = writer.end;
//     return .success;
// }
//
/// # Safety
/// `encoder_`/`event_` must be non-null `new` handles; `out_` (if non-null) must
/// point to `out_len` writable bytes; `out_written` must be writable.
pub unsafe fn encode(
    encoder_: Encoder,
    event_: KeyEvent,
    out_: *mut u8,
    out_len: usize,
    out_written: *mut usize,
) -> Result {
    // Attempt to write to this buffer
    // var writer: std.Io.Writer = .fixed(if (out_) |out| out[0..out_len] else &.{});
    let mut empty: [u8; 0] = [];
    let slice: &mut [u8] = if out_.is_null() {
        &mut empty
    } else {
        // SAFETY: `out_` points to `out_len` writable bytes (caller contract).
        unsafe { std::slice::from_raw_parts_mut(out_, out_len) }
    };
    let mut writer = FixedWriter::new(slice);

    // SAFETY: `event_`/`encoder_` are non-null handles (caller contract `.?`).
    let event = unsafe { key_event::event_ref(event_) };
    let opts = unsafe { (*encoder_).opts };

    // key_encode.encode(&writer, event_.?.event, encoder_.?.opts) catch |err| switch (err) {
    if key_encode::encode(&mut writer, event, opts).is_err() {
        // error.WriteFailed:
        // If we don't have space, use a discarding writer to count
        // how much space we would have needed.
        // var discarding: std.Io.Writer.Discarding = .init(&.{});
        let mut discarding = CountingWriter::new();
        // key_encode.encode(&discarding.writer, event_.?.event, encoder_.?.opts) catch unreachable;
        key_encode::encode(&mut discarding, event, opts).unwrap();
        // Discarding always uses a u64. If we're on 32-bit systems
        // we cast down. We should make this safer in the future.
        // out_written.* = @intCast(discarding.count);
        // SAFETY: `out_written` is writable (caller contract).
        unsafe { *out_written = discarding.count };
        // return .out_of_space;
        return Result::OutOfSpace;
    }

    // out_written.* = writer.end;
    // SAFETY: `out_written` is writable (caller contract).
    unsafe { *out_written = writer.end };
    // return .success;
    Result::Success
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::with_test_allocator;
    use rustty_vt::input::key;

    // test "alloc" {
    #[test]
    fn alloc() {
        with_test_allocator(|calloc| {
            // var e: Encoder = undefined;
            let mut e: Encoder = std::ptr::null_mut();
            // try testing.expectEqual(Result.success, new(&lib.alloc.test_allocator, &e));
            assert_eq!(Result::Success, unsafe { new(Some(calloc), &mut e) });
            // free(e);
            unsafe { free(e) };
        });
    }

    // PORT NOTE: setopt takes the option as the raw `c_int` discriminant (the
    // lib.rs `extern "C"` wrapper guards the range); the tests pass it directly.
    unsafe fn setopt_bool(e: Encoder, option: Option_, v: &bool) {
        unsafe { setopt(e, option as c_int, v as *const bool as *const c_void) };
    }

    // test "setopt bool" {
    #[test]
    fn setopt_bool_test() {
        with_test_allocator(|calloc| {
            let mut e: Encoder = std::ptr::null_mut();
            assert_eq!(Result::Success, unsafe { new(Some(calloc), &mut e) });

            // Test setting bool options
            // const val_true: bool = true;
            let val_true: bool = true;
            // setopt(e, .cursor_key_application, &val_true);
            unsafe { setopt_bool(e, Option_::CursorKeyApplication, &val_true) };
            // try testing.expect(e.?.opts.cursor_key_application);
            assert!(unsafe { (*e).opts.cursor_key_application });

            // const val_false: bool = false;
            let val_false: bool = false;
            // setopt(e, .cursor_key_application, &val_false);
            unsafe { setopt_bool(e, Option_::CursorKeyApplication, &val_false) };
            // try testing.expect(!e.?.opts.cursor_key_application);
            assert!(!unsafe { (*e).opts.cursor_key_application });

            // setopt(e, .keypad_key_application, &val_true);
            unsafe { setopt_bool(e, Option_::KeypadKeyApplication, &val_true) };
            // try testing.expect(e.?.opts.keypad_key_application);
            assert!(unsafe { (*e).opts.keypad_key_application });

            unsafe { free(e) };
        });
    }

    // test "setopt kitty flags" {
    #[test]
    fn setopt_kitty_flags() {
        with_test_allocator(|calloc| {
            let mut e: Encoder = std::ptr::null_mut();
            assert_eq!(Result::Success, unsafe { new(Some(calloc), &mut e) });

            // Test setting kitty flags
            // const flags: KittyFlags = .{ .disambiguate = true, .report_events = true };
            let flags = KittyFlags {
                disambiguate: true,
                report_events: true,
                ..KittyFlags::DISABLED
            };
            // const flags_int: u8 = @intCast(flags.int());
            let flags_int: u8 = flags.int();
            // setopt(e, .kitty_flags, &flags_int);
            unsafe {
                setopt(
                    e,
                    Option_::KittyFlags as c_int,
                    &flags_int as *const u8 as *const c_void,
                )
            };
            // try testing.expect(e.?.opts.kitty_flags.disambiguate);
            assert!(unsafe { (*e).opts.kitty_flags.disambiguate });
            // try testing.expect(e.?.opts.kitty_flags.report_events);
            assert!(unsafe { (*e).opts.kitty_flags.report_events });
            // try testing.expect(!e.?.opts.kitty_flags.report_alternates);
            assert!(!unsafe { (*e).opts.kitty_flags.report_alternates });

            unsafe { free(e) };
        });
    }

    // test "setopt macos option as alt" {
    #[test]
    fn setopt_macos_option_as_alt() {
        with_test_allocator(|calloc| {
            let mut e: Encoder = std::ptr::null_mut();
            assert_eq!(Result::Success, unsafe { new(Some(calloc), &mut e) });

            // Test setting option as alt
            // const opt_left: OptionAsAlt = .left;
            let opt_left: OptionAsAlt = OptionAsAlt::left;
            // setopt(e, .macos_option_as_alt, &opt_left);
            unsafe {
                setopt(
                    e,
                    Option_::MacosOptionAsAlt as c_int,
                    &opt_left as *const OptionAsAlt as *const c_void,
                )
            };
            // try testing.expectEqual(OptionAsAlt.left, e.?.opts.macos_option_as_alt);
            assert_eq!(OptionAsAlt::left, unsafe { (*e).opts.macos_option_as_alt });

            // const opt_true: OptionAsAlt = .true;
            let opt_true: OptionAsAlt = OptionAsAlt::r#true;
            // setopt(e, .macos_option_as_alt, &opt_true);
            unsafe {
                setopt(
                    e,
                    Option_::MacosOptionAsAlt as c_int,
                    &opt_true as *const OptionAsAlt as *const c_void,
                )
            };
            // try testing.expectEqual(OptionAsAlt.true, e.?.opts.macos_option_as_alt);
            assert_eq!(OptionAsAlt::r#true, unsafe {
                (*e).opts.macos_option_as_alt
            });

            unsafe { free(e) };
        });
    }

    // test "setopt_from_terminal" {
    #[test]
    fn setopt_from_terminal_test() {
        with_test_allocator(|calloc| {
            // Create encoder
            let mut e: Encoder = std::ptr::null_mut();
            assert_eq!(Result::Success, unsafe { new(Some(calloc), &mut e) });

            // Create terminal
            let mut t: terminal_c::Terminal = std::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                terminal_c::new(calloc, &mut t, 80, 24)
            });

            // Apply terminal state to encoder
            // setopt_from_terminal(e, t);
            unsafe { setopt_from_terminal(e, t) };

            // Options should reflect defaults from a fresh terminal
            // try testing.expect(!e.?.opts.cursor_key_application);
            assert!(!unsafe { (*e).opts.cursor_key_application });
            // try testing.expect(e.?.opts.alt_esc_prefix);
            assert!(unsafe { (*e).opts.alt_esc_prefix });
            // try testing.expectEqual(KittyFlags.disabled, e.?.opts.kitty_flags);
            assert_eq!(KittyFlags::DISABLED, unsafe { (*e).opts.kitty_flags });
            // try testing.expectEqual(OptionAsAlt.false, e.?.opts.macos_option_as_alt);
            assert_eq!(OptionAsAlt::r#false, unsafe {
                (*e).opts.macos_option_as_alt
            });

            unsafe { terminal_c::free(t) };
            unsafe { free(e) };
        });
    }

    // test "setopt_from_terminal null" {
    #[test]
    fn setopt_from_terminal_null() {
        // Both null should be no-ops
        // setopt_from_terminal(null, null);
        unsafe { setopt_from_terminal(std::ptr::null_mut(), std::ptr::null_mut()) };

        with_test_allocator(|calloc| {
            // Encoder null with valid terminal
            let mut t: terminal_c::Terminal = std::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                terminal_c::new(calloc, &mut t, 80, 24)
            });
            // setopt_from_terminal(null, t);
            unsafe { setopt_from_terminal(std::ptr::null_mut(), t) };

            // Valid encoder with null terminal
            let mut e: Encoder = std::ptr::null_mut();
            assert_eq!(Result::Success, unsafe { new(Some(calloc), &mut e) });
            // setopt_from_terminal(e, null);
            unsafe { setopt_from_terminal(e, std::ptr::null_mut()) };

            unsafe { free(e) };
            unsafe { terminal_c::free(t) };
        });
    }

    // test "encode: kitty ctrl release with ctrl mod set" {
    #[test]
    fn encode_kitty_ctrl_release_with_ctrl_mod_set() {
        with_test_allocator(|calloc| {
            // Create encoder
            let mut encoder: Encoder = std::ptr::null_mut();
            assert_eq!(Result::Success, unsafe { new(Some(calloc), &mut encoder) });

            // Set kitty flags with all features enabled
            {
                // const flags: KittyFlags = .{ ...all true... };
                let flags = KittyFlags {
                    disambiguate: true,
                    report_events: true,
                    report_alternates: true,
                    report_all: true,
                    report_associated: true,
                };
                // const flags_int: u8 = @intCast(flags.int());
                let flags_int: u8 = flags.int();
                // setopt(encoder, .kitty_flags, &flags_int);
                unsafe {
                    setopt(
                        encoder,
                        Option_::KittyFlags as c_int,
                        &flags_int as *const u8 as *const c_void,
                    )
                };
            }

            // Create key event
            let mut event: KeyEvent = std::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                key_event::new(Some(calloc), &mut event)
            });

            // Set event properties: release action, ctrl key, ctrl modifier
            // key_event.set_action(event, .release);
            unsafe { key_event::set_action(event, key::Action::Release) };
            // key_event.set_key(event, .control_left);
            unsafe { key_event::set_key(event, key::Key::ControlLeft) };
            // key_event.set_mods(event, .{ .ctrl = true });
            unsafe {
                key_event::set_mods(
                    event,
                    key::Mods {
                        ctrl: true,
                        ..Default::default()
                    },
                )
            };

            // Encode null should give us the length required
            // var required: usize = 0;
            let mut required: usize = 0;
            // try testing.expectEqual(Result.out_of_space, encode(encoder, event, null, 0, &required));
            assert_eq!(Result::OutOfSpace, unsafe {
                encode(encoder, event, std::ptr::null_mut(), 0, &mut required)
            });

            // Encode the key event
            // var buf: [128]u8 = undefined; var written: usize = 0;
            let mut buf = [0u8; 128];
            let mut written: usize = 0;
            // try testing.expectEqual(Result.success, encode(encoder, event, &buf, buf.len, &written));
            assert_eq!(Result::Success, unsafe {
                encode(encoder, event, buf.as_mut_ptr(), buf.len(), &mut written)
            });
            // try testing.expectEqual(required, written);
            assert_eq!(required, written);

            // Expected: ESC[57442;5:3u (ctrl key code with mods and release event)
            // const actual = buf[0..written];
            let actual = &buf[0..written];
            // try testing.expectEqualStrings("\x1b[57442;5:3u", actual);
            assert_eq!(b"\x1b[57442;5:3u", actual);

            unsafe { key_event::free(event) };
            unsafe { free(encoder) };
        });
    }

    // W81-B01-01 (Rust-only proving test; the reference has none — it inherits
    // the behavior from `std.Io.Writer.fixed`). `key_encode.zig` hands the
    // CALLER buffer to `std.Io.Writer.fixed`, so an encode that does not fit
    // still leaves the fitting PREFIX in that buffer before reporting
    // `out_of_space`. Probed on zig 0.16.0:
    //     buf = "XY"; fixed(buf).writeAll("abc")
    //         => result=error.WriteFailed end=2 buf="ab"
    // rustty's `FixedWriter` used to reject an overflowing write WHOLE, so the
    // C probe saw `text_short_result=-3` with the buffer still "XY". The plain
    // text path writes `event.utf8` in a single `write_all`, so it is the exact
    // shape that manifests.
    #[test]
    fn encode_out_of_space_leaves_the_fitting_prefix_in_the_caller_buffer() {
        with_test_allocator(|calloc| {
            let mut encoder: Encoder = std::ptr::null_mut();
            assert_eq!(Result::Success, unsafe { new(Some(calloc), &mut encoder) });

            let mut event: KeyEvent = std::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                key_event::new(Some(calloc), &mut event)
            });
            unsafe { key_event::set_action(event, key::Action::Press) };
            unsafe { key_event::set_key(event, key::Key::KeyA) };
            let text = b"hello";
            unsafe { key_event::set_utf8(event, text.as_ptr(), text.len()) };

            // The full encoding is the 5 text bytes.
            let mut required: usize = 0;
            assert_eq!(Result::OutOfSpace, unsafe {
                encode(encoder, event, std::ptr::null_mut(), 0, &mut required)
            });
            assert_eq!(text.len(), required);

            // Two bytes of room: `out_of_space` + the required length, AND the
            // prefix that fit is now in the caller buffer (was left as "XY").
            let mut buf = *b"XY";
            let mut written: usize = 0;
            assert_eq!(Result::OutOfSpace, unsafe {
                encode(encoder, event, buf.as_mut_ptr(), buf.len(), &mut written)
            });
            assert_eq!(text.len(), written);
            assert_eq!(b"he", &buf);

            // A buffer that fits is unaffected.
            let mut buf2 = [0u8; 16];
            assert_eq!(Result::Success, unsafe {
                encode(encoder, event, buf2.as_mut_ptr(), buf2.len(), &mut written)
            });
            assert_eq!(text.len(), written);
            assert_eq!(&text[..], &buf2[..written]);

            unsafe { key_event::free(event) };
            unsafe { free(encoder) };
        });
    }

    // W80-B02-01 (Rust-only proving test; the reference has none — the borrow
    // is simply what `event.utf8 = ptr[0..len]` means). The reference encodes
    // from the caller's live slice, so mutating that buffer AFTER `set_utf8`
    // changes what `encode` emits. rustty's wrapper keeps the borrow and
    // refreshes the owned rustty-vt vec from it at use time (`event_ref`), so
    // the encoder reads the same live bytes; the earlier copy-at-`set_utf8`
    // implementation would still have emitted "hi".
    #[test]
    fn encode_reads_the_live_caller_utf8_buffer() {
        with_test_allocator(|calloc| {
            let mut encoder: Encoder = std::ptr::null_mut();
            assert_eq!(Result::Success, unsafe { new(Some(calloc), &mut encoder) });

            let mut event: KeyEvent = std::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                key_event::new(Some(calloc), &mut event)
            });
            unsafe { key_event::set_action(event, key::Action::Press) };
            unsafe { key_event::set_key(event, key::Key::KeyA) };

            let mut text = *b"hi";
            let text_len = text.len();
            // ALIASING NOTE (Stacked Borrows): the event KEEPS this pointer and
            // the encoder reads through it after the mutation below, so the
            // caller-side buffer is reached through a SINGLE provenance root —
            // one raw `*mut u8`, used for both the mutation and the readback.
            // Naming the local again (`text[0] = ...`) would create a fresh
            // borrow whose write invalidates the tag the event stored, making
            // the encoder's read UB under the G6 Miri lane.
            let base: *mut u8 = text.as_mut_ptr();
            unsafe { key_event::set_utf8(event, base.cast_const(), text_len) };

            // Mutate the caller's buffer after the set. (The read below is what
            // the encoder does through the borrow, which the compiler cannot
            // see — assert on the buffer so the store is not "never read".)
            unsafe { *base = b'H' };
            assert_eq!(b"Hi", unsafe {
                std::slice::from_raw_parts(base.cast_const(), text_len)
            });

            let mut buf = [0u8; 16];
            let mut written: usize = 0;
            assert_eq!(Result::Success, unsafe {
                encode(encoder, event, buf.as_mut_ptr(), buf.len(), &mut written)
            });
            assert_eq!(b"Hi", &buf[..written]);

            unsafe { key_event::free(event) };
            unsafe { free(encoder) };
        });
    }
}
