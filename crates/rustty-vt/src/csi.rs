//! rustty-vt csi
//!
//! PORTS: ghostty src/terminal/csi.zig
//!
//! PORT STATUS: done

// PORT NOTE: the reference imports `const lib = @import("lib.zig");` for
// `lib.Enum`. For the `.zig` target `lib.Enum(...)` is a plain enum with the
// keys in declaration order; ported as a Rust enum.

/// Modes for the ED CSI command.
// PORT NOTE: `enum(u8)`; ported as `#[repr(u8)]` to match the backing type.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EraseDisplay {
    Below = 0,
    Above = 1,
    Complete = 2,
    Scrollback = 3,

    /// This is an extension added by Kitty to move the viewport into the
    /// scrollback and then erase the display.
    ScrollComplete = 22,
}

/// Modes for the EL CSI command.
// PORT NOTE: `enum(u8)` with a `_` (non-exhaustive) tag. Ported as `#[repr(u8)]`
// with an explicit `Other(u8)` arm preserving the unknown-value semantics; see
// the trailing comment from the reference below.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EraseLine {
    Right = 0,
    Left = 1,
    Complete = 2,
    RightUnlessPendingWrap = 4,

    // Non-exhaustive so that @intToEnum never fails since the inputs are
    // user-generated.
    Other(u8),
}

/// Modes for the TBC (tab clear) command.
// PORT NOTE: `enum(u8)` with a `_` (non-exhaustive) tag; same treatment as
// EraseLine above.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TabClear {
    Current = 0,
    All = 3,

    // Non-exhaustive so that @intToEnum never fails since the inputs are
    // user-generated.
    Other(u8),
}

/// Style formats for terminal size reports.
// PORT NOTE: `SizeReportStyle = lib.Enum(lib.target, &.{...})`. For the `.zig`
// target this is a plain enum with the keys in order; ported as a Rust enum.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SizeReportStyle {
    // XTWINOPS
    Csi14T,
    Csi16T,
    Csi18T,
    Csi21T,
}

/// XTWINOPS CSI 22/23
pub struct TitlePushPop {
    pub op: Op,
    pub index: u16,
}

// pub const Op = enum { push, pop };
// PORT NOTE: the reference nests `Op` inside `TitlePushPop`; Rust has no nested
// enum types so it is hoisted to a top-level enum with the same variants.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    Push,
    Pop,
}
