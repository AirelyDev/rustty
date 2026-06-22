//! rustty-vt size_report
//!
//! PORTS: ghostty src/terminal/size_report.zig
//!
//! PORT STATUS: done

// PORT NOTE: the reference imports are
//   const std = @import("std");
//   const lib = @import("lib.zig");
//   const CellCountInt = @import("size.zig").CellCountInt;
// `lib.Enum` / `lib.Struct` for the `.zig` target are a plain enum / the original
// struct; ported directly. `CellCountInt` is the hoisted size type from
// rustty-render-types (A1).
//
// PORT NOTE: the reference `encode` takes a `*std.Io.Writer` and returns
// `std.Io.Writer.Error!void`. We port it to take any `core::fmt::Write` sink and
// return `core::fmt::Result`, which models the same "append formatted bytes, may
// fail to write" contract (matching the pattern used elsewhere in this crate,
// e.g. device_attributes).

use core::fmt::Write;

use crate::size::CellCountInt;

/// Output formats for terminal size reports written to the PTY.
// PORT NOTE: `Style = lib.Enum(lib.target, &.{...})`. For the `.zig` target this
// is a plain enum with the keys in order; ported as a Rust enum.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Style {
    // In-band size reports (mode 2048)
    Mode2048,
    // XTWINOPS: report text area size in pixels
    Csi14T,
    // XTWINOPS: report cell size in pixels
    Csi16T,
    // XTWINOPS: report text area size in characters
    Csi18T,
}

/// Runtime size values used to encode terminal size reports.
// PORT NOTE: `Size = lib.Struct(lib.target, struct {...})`. For the `.zig` target
// this is the original struct unchanged; ported as a Rust struct.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Size {
    /// Terminal row count in cells.
    pub rows: CellCountInt,

    /// Terminal column count in cells.
    pub columns: CellCountInt,

    /// Width of a single terminal cell in pixels.
    pub cell_width: u32,

    /// Height of a single terminal cell in pixels.
    pub cell_height: u32,
}

fn width_pixels(s: Size) -> u64 {
    (s.columns as u64) * (s.cell_width as u64)
}

fn height_pixels(s: Size) -> u64 {
    (s.rows as u64) * (s.cell_height as u64)
}

/// Encode a terminal size report sequence.
pub fn encode<W: Write>(writer: &mut W, style: Style, size: Size) -> core::fmt::Result {
    match style {
        Style::Mode2048 => write!(
            writer,
            "\x1B[48;{};{};{};{}t",
            size.rows,
            size.columns,
            height_pixels(size),
            width_pixels(size),
        ),

        Style::Csi14T => write!(
            writer,
            "\x1b[4;{};{}t",
            height_pixels(size),
            width_pixels(size),
        ),

        Style::Csi16T => write!(writer, "\x1b[6;{};{}t", size.cell_height, size.cell_width,),

        Style::Csi18T => write!(writer, "\x1b[8;{};{}t", size.rows, size.columns,),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_size() -> Size {
        Size {
            rows: 24,
            columns: 80,
            cell_width: 9,
            cell_height: 18,
        }
    }

    #[test]
    fn encode_mode_2048() {
        let mut buf = String::new();
        encode(&mut buf, Style::Mode2048, test_size()).unwrap();

        assert_eq!("\x1B[48;24;80;432;720t", buf);
    }

    #[test]
    fn encode_csi_14_t() {
        let mut buf = String::new();
        encode(&mut buf, Style::Csi14T, test_size()).unwrap();

        assert_eq!("\x1b[4;432;720t", buf);
    }

    #[test]
    fn encode_csi_16_t() {
        let mut buf = String::new();
        encode(&mut buf, Style::Csi16T, test_size()).unwrap();

        assert_eq!("\x1b[6;18;9t", buf);
    }

    #[test]
    fn encode_csi_18_t() {
        let mut buf = String::new();
        encode(&mut buf, Style::Csi18T, test_size()).unwrap();

        assert_eq!("\x1b[8;24;80t", buf);
    }

    #[test]
    fn encode_max_values_for_all_fields() {
        // const max_size: Size = .{
        //     .rows = std.math.maxInt(@FieldType(Size, "rows")),
        //     .columns = std.math.maxInt(@FieldType(Size, "columns")),
        //     .cell_width = std.math.maxInt(@FieldType(Size, "cell_width")),
        //     .cell_height = std.math.maxInt(@FieldType(Size, "cell_height")),
        // };
        let max_size = Size {
            rows: CellCountInt::MAX,
            columns: CellCountInt::MAX,
            cell_width: u32::MAX,
            cell_height: u32::MAX,
        };

        // const Case = struct { style: Style, expected: []const u8 };
        struct Case {
            style: Style,
            expected: &'static str,
        }

        let cases = [
            Case {
                style: Style::Mode2048,
                expected: "\x1B[48;65535;65535;281470681677825;281470681677825t",
            },
            Case {
                style: Style::Csi14T,
                expected: "\x1b[4;281470681677825;281470681677825t",
            },
            Case {
                style: Style::Csi16T,
                expected: "\x1b[6;4294967295;4294967295t",
            },
            Case {
                style: Style::Csi18T,
                expected: "\x1b[8;65535;65535t",
            },
        ];

        for case in cases {
            let mut buf = String::new();
            encode(&mut buf, case.style, max_size).unwrap();
            assert_eq!(case.expected, buf);
        }
    }
}
