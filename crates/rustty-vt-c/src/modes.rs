//! rustty-vt-c modes
//!
//! PORTS: ghostty src/terminal/c/modes.zig
//!
//! PORT STATUS: done

// PORT NOTE: the reference imports are
//   const std = @import("std");
//   const lib = @import("../lib.zig");
//   const modes = @import("../modes.zig");
//   const Result = @import("result.zig").Result;
// `modes` is rustty-vt's `modes` module.

use crate::result::Result;
use crate::writer::{CountingWriter, FixedWriter};
use rustty_vt::modes;

/// C: GhosttyModeReportState -> RusttyModeReportState (R2)
// PORT NOTE: the reference is `pub const ReportState = enum(c_int) { _ ... }` —
// a NON-EXHAUSTIVE C enum (any `c_int` value is representable). Rust has no
// non-exhaustive `#[repr(C)]` enum, so it is modeled as a transparent `c_int`
// newtype; `to_zig` reproduces `std.meta.intToEnum(modes.Report.State, ...)`,
// returning `None` (Zig's `catch null`) for values outside the valid range.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReportState(pub std::os::raw::c_int);

impl ReportState {
    // fn toZig(self: ReportState) ?modes.Report.State {
    //     return std.meta.intToEnum(modes.Report.State, @intFromEnum(self)) catch null;
    // }
    fn to_zig(self) -> Option<modes::State> {
        // PORT NOTE: `std.meta.intToEnum` maps the integer to the enum value if it
        // is one of the declared tags, else errors. `modes.Report.State` (rustty
        // `modes::State`) has tags 0..=4; any other value yields `None`.
        match self.0 {
            0 => Some(modes::State::NotRecognized),
            1 => Some(modes::State::Set),
            2 => Some(modes::State::Reset),
            3 => Some(modes::State::PermanentlySet),
            4 => Some(modes::State::PermanentlyReset),
            _ => None,
        }
    }
}

/// PORT NOTE: `pub fn report_encode(tag, state, out_, out_len, out_written)
/// callconv(...) Result`. The C calling convention is the `extern "C"` wrapper
/// in `lib.rs` (`rustty_mode_report_encode`). `tag` is `modes.ModeTag.Backing`
/// (rustty `modes::ModeTagBacking` == `u16`).
///
/// # Safety
/// `out_`, if non-null, must point to `out_len` writable bytes; `out_written`
/// must be a valid, writable pointer to `usize`.
pub unsafe fn report_encode(
    tag: modes::ModeTagBacking,
    state: ReportState,
    out_: *mut u8,
    out_len: usize,
    out_written: *mut usize,
) -> Result {
    // const mode_tag: modes.ModeTag = @bitCast(tag);
    let mode_tag: modes::ModeTag = modes::ModeTag::from_backing(tag);
    // const report: modes.Report = .{
    //     .tag = mode_tag,
    //     .state = state.toZig() orelse return .invalid_value,
    // };
    let report = modes::Report {
        tag: mode_tag,
        state: match state.to_zig() {
            Some(s) => s,
            None => return Result::InvalidValue,
        },
    };

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
    // report.encode(&writer) catch |err| switch (err) {
    //     error.WriteFailed => { ... return .out_of_space; },
    // };
    if report.encode(&mut writer).is_err() {
        // error.WriteFailed:
        // var discarding: std.Io.Writer.Discarding = .init(&.{});
        let mut discarding = CountingWriter::new();
        // report.encode(&discarding.writer) catch unreachable;
        report.encode(&mut discarding).unwrap();
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

    // PORT NOTE: `@enumFromInt(n)` on the non-exhaustive C enum constructs the
    // `ReportState` directly from the integer.
    fn report_state(n: std::os::raw::c_int) -> ReportState {
        ReportState(n)
    }

    #[test]
    fn encode_dec_mode_set() {
        // var buf: [modes.Report.max_size]u8 = undefined;
        let mut buf = [0u8; modes::Report::MAX_SIZE];
        // var written: usize = 0;
        let mut written: usize = 0;
        // const tag: modes.ModeTag.Backing = @bitCast(modes.ModeTag{ .value = 1, .ansi = false });
        let tag: modes::ModeTagBacking = (modes::ModeTag {
            value: 1,
            ansi: false,
        })
        .to_backing();
        // const result = report_encode(tag, @enumFromInt(1), &buf, buf.len, &written);
        let result = unsafe {
            report_encode(
                tag,
                report_state(1),
                buf.as_mut_ptr(),
                buf.len(),
                &mut written,
            )
        };
        // try std.testing.expectEqual(.success, result);
        assert_eq!(Result::Success, result);
        // try std.testing.expectEqualStrings("\x1B[?1;1$y", buf[0..written]);
        assert_eq!(b"\x1B[?1;1$y", &buf[0..written]);
    }

    #[test]
    fn encode_dec_mode_reset() {
        let mut buf = [0u8; modes::Report::MAX_SIZE];
        let mut written: usize = 0;
        let tag: modes::ModeTagBacking = (modes::ModeTag {
            value: 1,
            ansi: false,
        })
        .to_backing();
        // const result = report_encode(tag, @enumFromInt(2), &buf, buf.len, &written);
        let result = unsafe {
            report_encode(
                tag,
                report_state(2),
                buf.as_mut_ptr(),
                buf.len(),
                &mut written,
            )
        };
        assert_eq!(Result::Success, result);
        // try std.testing.expectEqualStrings("\x1B[?1;2$y", buf[0..written]);
        assert_eq!(b"\x1B[?1;2$y", &buf[0..written]);
    }

    #[test]
    fn encode_ansi_mode() {
        let mut buf = [0u8; modes::Report::MAX_SIZE];
        let mut written: usize = 0;
        // const tag: modes.ModeTag.Backing = @bitCast(modes.ModeTag{ .value = 4, .ansi = true });
        let tag: modes::ModeTagBacking = (modes::ModeTag {
            value: 4,
            ansi: true,
        })
        .to_backing();
        // const result = report_encode(tag, @enumFromInt(1), &buf, buf.len, &written);
        let result = unsafe {
            report_encode(
                tag,
                report_state(1),
                buf.as_mut_ptr(),
                buf.len(),
                &mut written,
            )
        };
        assert_eq!(Result::Success, result);
        // try std.testing.expectEqualStrings("\x1B[4;1$y", buf[0..written]);
        assert_eq!(b"\x1B[4;1$y", &buf[0..written]);
    }

    #[test]
    fn encode_not_recognized() {
        let mut buf = [0u8; modes::Report::MAX_SIZE];
        let mut written: usize = 0;
        // const tag: modes.ModeTag.Backing = @bitCast(modes.ModeTag{ .value = 9999, .ansi = false });
        let tag: modes::ModeTagBacking = (modes::ModeTag {
            value: 9999,
            ansi: false,
        })
        .to_backing();
        // const result = report_encode(tag, @enumFromInt(0), &buf, buf.len, &written);
        let result = unsafe {
            report_encode(
                tag,
                report_state(0),
                buf.as_mut_ptr(),
                buf.len(),
                &mut written,
            )
        };
        assert_eq!(Result::Success, result);
        // try std.testing.expectEqualStrings("\x1B[?9999;0$y", buf[0..written]);
        assert_eq!(b"\x1B[?9999;0$y", &buf[0..written]);
    }

    #[test]
    fn encode_with_insufficient_buffer() {
        // var buf: [1]u8 = undefined;
        let mut buf = [0u8; 1];
        let mut written: usize = 0;
        let tag: modes::ModeTagBacking = (modes::ModeTag {
            value: 1,
            ansi: false,
        })
        .to_backing();
        // const result = report_encode(tag, @enumFromInt(1), &buf, buf.len, &written);
        let result = unsafe {
            report_encode(
                tag,
                report_state(1),
                buf.as_mut_ptr(),
                buf.len(),
                &mut written,
            )
        };
        // try std.testing.expectEqual(.out_of_space, result);
        assert_eq!(Result::OutOfSpace, result);
        // try std.testing.expect(written > 1);
        assert!(written > 1);
    }

    // W88-B02-01 (Rust-only proving test; the reference has none — the behavior
    // comes from `std.Io.Writer.fixed`, whose `fixedDrain` fills the available
    // buffer before returning `error.WriteFailed`). A too-small NON-NULL caller
    // buffer must come back holding the fitting PREFIX of the report, across
    // the MULTI-PIECE encoder (each piece lands in order until one overflows).
    #[test]
    fn encode_with_insufficient_buffer_keeps_the_fitting_prefix() {
        let mut buf = [0xAAu8; 3];
        let mut written: usize = 0;
        let tag: modes::ModeTagBacking = (modes::ModeTag {
            value: 1,
            ansi: false,
        })
        .to_backing();
        let result = unsafe {
            report_encode(
                tag,
                report_state(1),
                buf.as_mut_ptr(),
                buf.len(),
                &mut written,
            )
        };
        assert_eq!(Result::OutOfSpace, result);
        assert_eq!(8, written);
        // "\x1B[?1;1$y" truncated to the 3 bytes that fit.
        assert_eq!(b"\x1B[?", &buf);
    }

    #[test]
    fn encode_with_invalid_state() {
        let mut buf = [0u8; modes::Report::MAX_SIZE];
        let mut written: usize = 0;
        let tag: modes::ModeTagBacking = (modes::ModeTag {
            value: 1,
            ansi: false,
        })
        .to_backing();
        // const result = report_encode(tag, @enumFromInt(99), &buf, buf.len, &written);
        let result = unsafe {
            report_encode(
                tag,
                report_state(99),
                buf.as_mut_ptr(),
                buf.len(),
                &mut written,
            )
        };
        // try std.testing.expectEqual(.invalid_value, result);
        assert_eq!(Result::InvalidValue, result);
    }

    #[test]
    fn encode_with_null_buffer() {
        let mut written: usize = 0;
        let tag: modes::ModeTagBacking = (modes::ModeTag {
            value: 1,
            ansi: false,
        })
        .to_backing();
        // const result = report_encode(tag, @enumFromInt(1), null, 0, &written);
        let result =
            unsafe { report_encode(tag, report_state(1), std::ptr::null_mut(), 0, &mut written) };
        assert_eq!(Result::OutOfSpace, result);
        // try std.testing.expect(written > 0);
        assert!(written > 0);
    }
}
