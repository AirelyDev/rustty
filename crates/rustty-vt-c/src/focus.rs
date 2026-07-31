//! rustty-vt-c focus
//!
//! PORTS: ghostty src/terminal/c/focus.zig
//!
//! PORT STATUS: done

// PORT NOTE: the reference imports are
//   const std = @import("std");
//   const lib = @import("../lib.zig");
//   const terminal_focus = @import("../focus.zig");
//   const Result = @import("result.zig").Result;
// `terminal_focus` is rustty-vt's `focus` module; `Result` is the sibling
// `result` module's `Result`.

use crate::result::Result;
use crate::writer::{CountingWriter, FixedWriter};
use rustty_vt::focus as terminal_focus;

/// C: GhosttyFocusEvent -> RusttyFocusEvent (R2)
// PORT NOTE: `terminal_focus.Event` is `lib.Enum(lib.target, &.{ "gained",
// "lost" })`. For the C-ABI target (`lib.target == .c`) this is a `c_int` enum
// with `gained = 0, lost = 1`; rustty-vt's `focus::Event` is the `.zig`-target
// plain enum (not `#[repr(C)]`). To keep the FFI boundary C-ABI-safe AND name a
// stable C type, vt-c declares the `.c`-target projection here and converts it to
// the rustty-vt `Event` (the source of truth, R3).
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FocusEvent {
    Gained = 0,
    Lost = 1,
}

// D11 (P3-W6 T2). `size_of` proves the ABI is unchanged by taking the value as
// a `c_int` in the exported signature: this enum IS a C `int` enum.
const _: () =
    assert!(std::mem::size_of::<FocusEvent>() == std::mem::size_of::<std::os::raw::c_int>());

impl FocusEvent {
    /// Narrow an inbound C integer to a `FocusEvent`, mirroring
    /// `std.meta.intToEnum`. Returns `None` for any value outside the declared
    /// range.
    ///
    /// D11 (P3-W6 T2): `rustty_focus_encode` takes this value as a plain
    /// `c_int` and narrows it HERE, rather than declaring `FocusEvent` as the C
    /// parameter type. The difference is not stylistic: a `#[repr(C)]` enum
    /// parameter is materialized by the callee's PROLOGUE, so an out-of-range
    /// tag is UB before any defensive code of ours can run — there is no
    /// "check it first". And the value is reachable: `include/rustty/vt/focus.h`
    /// itself declares `RUSTTY_FOCUS_EVENT_MAX_VALUE`, i.e. the published header
    /// documents an out-of-range enumerator a conforming C caller may pass.
    /// Same reasoning as the `ReportState` PORT NOTE at `modes.rs:19`.
    pub fn from_c_int(v: std::os::raw::c_int) -> Option<FocusEvent> {
        match v {
            0 => Some(FocusEvent::Gained),
            1 => Some(FocusEvent::Lost),
            _ => None,
        }
    }

    /// Project the C-ABI focus event onto the rustty-vt source-of-truth `Event`.
    pub fn to_vt(self) -> terminal_focus::Event {
        match self {
            FocusEvent::Gained => terminal_focus::Event::Gained,
            FocusEvent::Lost => terminal_focus::Event::Lost,
        }
    }
}

/// PORT NOTE: `pub fn encode(event, out_, out_len, out_written) callconv(...)
/// Result`. The C calling convention is the `extern "C"` wrapper in `lib.rs`
/// (`rustty_focus_encode`). `out_` is the optional caller buffer (NULL ==> a
/// zero-length buffer, exactly as `if (out_) |out| out[0..out_len] else &.{}`);
/// `out_written` is a non-null out-pointer for the byte count.
///
/// # Safety
/// `out_`, if non-null, must point to `out_len` writable bytes; `out_written`
/// must be a valid, writable pointer to `usize`.
pub unsafe fn encode(
    event: terminal_focus::Event,
    out_: *mut u8,
    out_len: usize,
    out_written: *mut usize,
) -> Result {
    // var writer: std.Io.Writer = .fixed(if (out_) |out| out[0..out_len] else &.{});
    let mut empty: [u8; 0] = [];
    // SAFETY: `out_`/`out_len` describe a valid writable region when non-null;
    // `out_written` is a valid writable pointer (caller contract).
    let slice: &mut [u8] = if out_.is_null() {
        &mut empty
    } else {
        unsafe { std::slice::from_raw_parts_mut(out_, out_len) }
    };
    let mut writer = FixedWriter::new(slice);
    // terminal_focus.encode(&writer, event) catch |err| switch (err) {
    //     error.WriteFailed => { ... return .out_of_space; },
    // };
    if terminal_focus::encode(&mut writer, event).is_err() {
        // error.WriteFailed:
        // var discarding: std.Io.Writer.Discarding = .init(&.{});
        let mut discarding = CountingWriter::new();
        // terminal_focus.encode(&discarding.writer, event) catch unreachable;
        terminal_focus::encode(&mut discarding, event).unwrap();
        // out_written.* = @intCast(discarding.count);
        unsafe { *out_written = discarding.count };
        // return .out_of_space;
        return Result::OutOfSpace;
    }

    // out_written.* = writer.end;
    unsafe { *out_written = writer.end };
    // return .success;
    Result::Success
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_focus_gained() {
        // var buf: [terminal_focus.max_encode_size]u8 = undefined;
        let mut buf = [0u8; terminal_focus::MAX_ENCODE_SIZE];
        // var written: usize = 0;
        let mut written: usize = 0;
        // const result = encode(.gained, &buf, buf.len, &written);
        let result = unsafe {
            encode(
                terminal_focus::Event::Gained,
                buf.as_mut_ptr(),
                buf.len(),
                &mut written,
            )
        };
        // try std.testing.expectEqual(.success, result);
        assert_eq!(Result::Success, result);
        // try std.testing.expectEqualStrings("\x1B[I", buf[0..written]);
        assert_eq!(b"\x1B[I", &buf[0..written]);
    }

    #[test]
    fn encode_focus_lost() {
        let mut buf = [0u8; terminal_focus::MAX_ENCODE_SIZE];
        let mut written: usize = 0;
        // const result = encode(.lost, &buf, buf.len, &written);
        let result = unsafe {
            encode(
                terminal_focus::Event::Lost,
                buf.as_mut_ptr(),
                buf.len(),
                &mut written,
            )
        };
        assert_eq!(Result::Success, result);
        // try std.testing.expectEqualStrings("\x1B[O", buf[0..written]);
        assert_eq!(b"\x1B[O", &buf[0..written]);
    }

    #[test]
    fn encode_with_insufficient_buffer() {
        // var buf: [1]u8 = undefined;
        let mut buf = [0u8; 1];
        let mut written: usize = 0;
        let result = unsafe {
            encode(
                terminal_focus::Event::Gained,
                buf.as_mut_ptr(),
                buf.len(),
                &mut written,
            )
        };
        // try std.testing.expectEqual(.out_of_space, result);
        assert_eq!(Result::OutOfSpace, result);
        // try std.testing.expectEqual(terminal_focus.max_encode_size, written);
        assert_eq!(terminal_focus::MAX_ENCODE_SIZE, written);
    }

    #[test]
    fn encode_with_null_buffer() {
        let mut written: usize = 0;
        // const result = encode(.gained, null, 0, &written);
        let result = unsafe {
            encode(
                terminal_focus::Event::Gained,
                std::ptr::null_mut(),
                0,
                &mut written,
            )
        };
        assert_eq!(Result::OutOfSpace, result);
        assert_eq!(terminal_focus::MAX_ENCODE_SIZE, written);
    }
}
