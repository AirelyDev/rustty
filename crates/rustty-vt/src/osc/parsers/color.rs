//! PORTS: ghostty src/terminal/osc/parsers/color.zig
//!
//! PORT STATUS: stub — public types referenced by the OSC parent `Command`
//! (`Operation`, `List`, `Request`, `Target`, `ColoredTarget`) are declared so
//! the parent compiles; `parse` is a signature-faithful stub. The body and the
//! module's `test {}` blocks land in the fill phase.

use crate::color::Dynamic as DynamicColor;
use crate::color::Special as SpecialColor;
use crate::color::RGB;
use crate::osc::{Command, Parser};

/// The possible operations we support for colors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(non_camel_case_types)]
pub enum Operation {
    osc_4,
    osc_5,
    osc_10,
    osc_11,
    osc_12,
    osc_13,
    osc_14,
    osc_15,
    osc_16,
    osc_17,
    osc_18,
    osc_19,
    osc_104,
    osc_105,
    osc_110,
    osc_111,
    osc_112,
    osc_113,
    osc_114,
    osc_115,
    osc_116,
    osc_117,
    osc_118,
    osc_119,
}

/// A single operation related to the terminal color palette.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Request {
    Set(ColoredTarget),
    Query(Target),
    Reset(Target),
    ResetPalette,
    ResetSpecial,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Palette(u8),
    Special(SpecialColor),
    Dynamic(DynamicColor),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColoredTarget {
    pub target: Target,
    pub color: RGB,
}

// PORT NOTE: `pub const List = std.SegmentedList(Request, 2);` — a segmented
// list with 2 prealloc slots. `std.SegmentedList` is not yet ported; the stub
// models the storage as a `Vec<Request>` carrying the same elements. The
// prealloc-count tuning (2) is a perf detail with no observable behavior. The
// faithful `SegmentedList` replacement lands with the fill phase.
pub type List = Vec<Request>;

/// Parse OSCs 4, 5, 10-19, 104, 110-119
#[allow(clippy::todo, unused_variables)]
pub fn parse<'a>(parser: &'a mut Parser, terminator_ch: Option<u8>) -> Option<&'a mut Command> {
    todo!()
}
