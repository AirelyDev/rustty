//! rustty-vt-c size_report
//!
//! PORTS: ghostty src/terminal/c/size_report.zig
//!
//! PORT STATUS: done

// PORT NOTE: the reference imports are
//   const std = @import("std");
//   const lib = @import("../lib.zig");
//   const terminal_size_report = @import("../size_report.zig");
//   const Result = @import("result.zig").Result;
// `terminal_size_report` is rustty-vt's `size_report` module; `Result` is the
// sibling `result` module's `Result`.

use crate::result::Result;
use crate::writer::{CountingWriter, FixedWriter};
use rustty_vt::size_report as terminal_size_report;

/// C: RusttySizeReportStyle (R2)
// PORT NOTE: `pub const Style = terminal_size_report.Style;`. `terminal_size_report.Style`
// is `lib.Enum(lib.target, &.{...})`; for the C-ABI target (`lib.target == .c`)
// it is a `c_int` enum with `mode_2048 = 0, csi_14_t = 1, csi_16_t = 2,
// csi_18_t = 3`. rustty-vt's `Style` is the `.zig`-target plain enum (not
// `#[repr(C)]`). To keep the FFI boundary C-ABI-safe AND name a stable C type,
// vt-c declares the `.c`-target projection here and converts it to the rustty-vt
// `Style` (the source of truth, R3) — exactly as `focus.rs` does for `Event`.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Style {
    Mode2048 = 0,
    Csi14T = 1,
    Csi16T = 2,
    Csi18T = 3,
}

impl Style {
    /// Project the C-ABI style onto the rustty-vt source-of-truth `Style`.
    pub fn to_vt(self) -> terminal_size_report::Style {
        match self {
            Style::Mode2048 => terminal_size_report::Style::Mode2048,
            Style::Csi14T => terminal_size_report::Style::Csi14T,
            Style::Csi16T => terminal_size_report::Style::Csi16T,
            Style::Csi18T => terminal_size_report::Style::Csi18T,
        }
    }
}

/// C: RusttySizeReportSize (R2)
// PORT NOTE: `pub const Size = terminal_size_report.Size;`. `terminal_size_report.Size`
// is `lib.Struct(lib.target, struct {...})`; for the C-ABI target it is an
// `extern struct` with `rows`/`columns` (`CellCountInt` == `u16`) and
// `cell_width`/`cell_height` (`u32`). The vt `Size` is the `.zig`-target plain
// struct (not `#[repr(C)]`), so vt-c declares the `#[repr(C)]` projection here
// and converts it to the source-of-truth `Size` (R3).
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Size {
    /// Terminal row count in cells.
    pub rows: u16,
    /// Terminal column count in cells.
    pub columns: u16,
    /// Width of a single terminal cell in pixels.
    pub cell_width: u32,
    /// Height of a single terminal cell in pixels.
    pub cell_height: u32,
}

impl Size {
    /// Project the C-ABI size onto the rustty-vt source-of-truth `Size`.
    pub fn to_vt(self) -> terminal_size_report::Size {
        terminal_size_report::Size {
            rows: self.rows,
            columns: self.columns,
            cell_width: self.cell_width,
            cell_height: self.cell_height,
        }
    }
}

/// PORT NOTE: `pub fn encode(style, size, out_, out_len, out_written) callconv(...)
/// Result`. The C calling convention is the `extern "C"` wrapper in `lib.rs`
/// (`rustty_size_report_encode`).
///
/// # Safety
/// `out_`, if non-null, must point to `out_len` writable bytes; `out_written`
/// must be a valid, writable pointer to `usize`.
pub unsafe fn encode(
    style: Style,
    size: Size,
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
    // PORT NOTE: project the C-ABI `style`/`size` onto the vt source of truth.
    let vt_style = style.to_vt();
    let vt_size = size.to_vt();
    // terminal_size_report.encode(&writer, style, size) catch |err| switch (err) {
    //     error.WriteFailed => { ... return .out_of_space; },
    // };
    if terminal_size_report::encode(&mut writer, vt_style, vt_size).is_err() {
        // error.WriteFailed:
        // var discarding: std.Io.Writer.Discarding = .init(&.{});
        let mut discarding = CountingWriter::new();
        // terminal_size_report.encode(&discarding.writer, style, size) catch unreachable;
        terminal_size_report::encode(&mut discarding, vt_style, vt_size).unwrap();
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
    fn encode_mode_2048() {
        // var buf: [64]u8 = undefined;
        let mut buf = [0u8; 64];
        // var written: usize = 0;
        let mut written: usize = 0;
        // const result = encode(.mode_2048, .{ .rows = 24, .columns = 80,
        //     .cell_width = 9, .cell_height = 18 }, &buf, buf.len, &written);
        let result = unsafe {
            encode(
                Style::Mode2048,
                Size {
                    rows: 24,
                    columns: 80,
                    cell_width: 9,
                    cell_height: 18,
                },
                buf.as_mut_ptr(),
                buf.len(),
                &mut written,
            )
        };
        // try std.testing.expectEqual(.success, result);
        assert_eq!(Result::Success, result);
        // try std.testing.expectEqualStrings("\x1B[48;24;80;432;720t", buf[0..written]);
        assert_eq!(b"\x1B[48;24;80;432;720t", &buf[0..written]);
    }

    #[test]
    fn encode_csi_14_t() {
        let mut buf = [0u8; 64];
        let mut written: usize = 0;
        // const result = encode(.csi_14_t, .{ ... }, &buf, buf.len, &written);
        let result = unsafe {
            encode(
                Style::Csi14T,
                Size {
                    rows: 24,
                    columns: 80,
                    cell_width: 9,
                    cell_height: 18,
                },
                buf.as_mut_ptr(),
                buf.len(),
                &mut written,
            )
        };
        assert_eq!(Result::Success, result);
        // try std.testing.expectEqualStrings("\x1b[4;432;720t", buf[0..written]);
        assert_eq!(b"\x1b[4;432;720t", &buf[0..written]);
    }

    #[test]
    fn encode_with_insufficient_buffer() {
        // var buf: [1]u8 = undefined;
        let mut buf = [0u8; 1];
        let mut written: usize = 0;
        // const result = encode(.csi_18_t, .{ ... }, &buf, buf.len, &written);
        let result = unsafe {
            encode(
                Style::Csi18T,
                Size {
                    rows: 24,
                    columns: 80,
                    cell_width: 9,
                    cell_height: 18,
                },
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

    #[test]
    fn encode_with_null_buffer() {
        let mut written: usize = 0;
        // const result = encode(.csi_18_t, .{ ... }, null, 0, &written);
        let result = unsafe {
            encode(
                Style::Csi18T,
                Size {
                    rows: 24,
                    columns: 80,
                    cell_width: 9,
                    cell_height: 18,
                },
                std::ptr::null_mut(),
                0,
                &mut written,
            )
        };
        assert_eq!(Result::OutOfSpace, result);
        // try std.testing.expect(written > 0);
        assert!(written > 0);
    }
}
