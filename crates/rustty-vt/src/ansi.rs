//! rustty-vt ansi
//!
//! PORTS: ghostty src/terminal/ansi.zig
//!
//! PORT STATUS: done

// PORT NOTE: the reference imports `const lib = @import("lib.zig");` for
// `lib.Enum`. For the `.zig` target `lib.Enum(...)` is a plain enum with the
// keys in declaration order; ported as a Rust enum.

/// C0 (7-bit) control characters from ANSI.
///
/// This is not complete, control characters are only added to this
/// as the terminal emulator handles them.
// PORT NOTE: `enum(u7)` with a `_` (non-exhaustive) tag. Rust cannot mix
// explicit discriminants with a data-carrying `Other(_)` arm, so the named
// values are documented inline and an `int()`/`from_int()` pair carries the
// discriminant mapping (the reference backing type is u7). The `Other(u8)` arm
// preserves the non-exhaustive semantics noted by the reference below.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum C0 {
    /// Null
    NUL, // = 0x00
    /// Start of heading
    SOH, // = 0x01
    /// Start of text
    STX, // = 0x02
    /// Enquiry
    ENQ, // = 0x05
    /// Bell
    BEL, // = 0x07
    /// Backspace
    BS, // = 0x08
    // Horizontal tab
    HT, // = 0x09
    /// Line feed
    LF, // = 0x0A
    /// Vertical Tab
    VT, // = 0x0B
    /// Form feed
    FF, // = 0x0C
    /// Carriage return
    CR, // = 0x0D
    /// Shift out
    SO, // = 0x0E
    /// Shift in
    SI, // = 0x0F

    // Non-exhaustive so that @intToEnum never fails since the inputs are
    // user-generated.
    Other(u8),
}

impl C0 {
    /// The u7 value of this control character (`@intFromEnum`).
    pub fn int(self) -> u8 {
        match self {
            C0::NUL => 0x00,
            C0::SOH => 0x01,
            C0::STX => 0x02,
            C0::ENQ => 0x05,
            C0::BEL => 0x07,
            C0::BS => 0x08,
            C0::HT => 0x09,
            C0::LF => 0x0A,
            C0::VT => 0x0B,
            C0::FF => 0x0C,
            C0::CR => 0x0D,
            C0::SO => 0x0E,
            C0::SI => 0x0F,
            C0::Other(v) => v,
        }
    }

    /// Construct from a u7 value (`@intToEnum`, never fails).
    pub fn from_int(v: u8) -> C0 {
        match v {
            0x00 => C0::NUL,
            0x01 => C0::SOH,
            0x02 => C0::STX,
            0x05 => C0::ENQ,
            0x07 => C0::BEL,
            0x08 => C0::BS,
            0x09 => C0::HT,
            0x0A => C0::LF,
            0x0B => C0::VT,
            0x0C => C0::FF,
            0x0D => C0::CR,
            0x0E => C0::SO,
            0x0F => C0::SI,
            _ => C0::Other(v),
        }
    }
}

/// The SGR rendition aspects that can be set, sometimes known as attributes.
/// The value corresponds to the parameter value for the SGR command (ESC [ m).
// PORT NOTE: `enum(u16)` with a `_` (non-exhaustive) tag; same `int()`/`from_int()`
// treatment as C0 above.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RenditionAspect {
    Default,   // = 0
    Bold,      // = 1
    DefaultFg, // = 39
    DefaultBg, // = 49

    // Non-exhaustive so that @intToEnum never fails since the inputs are
    // user-generated.
    Other(u16),
}

impl RenditionAspect {
    pub fn int(self) -> u16 {
        match self {
            RenditionAspect::Default => 0,
            RenditionAspect::Bold => 1,
            RenditionAspect::DefaultFg => 39,
            RenditionAspect::DefaultBg => 49,
            RenditionAspect::Other(v) => v,
        }
    }

    pub fn from_int(v: u16) -> RenditionAspect {
        match v {
            0 => RenditionAspect::Default,
            1 => RenditionAspect::Bold,
            39 => RenditionAspect::DefaultFg,
            49 => RenditionAspect::DefaultBg,
            _ => RenditionAspect::Other(v),
        }
    }
}

/// Possible cursor styles (ESC [ q)
// PORT NOTE: `lib.Enum(lib.target, &.{...})`; plain enum for the `.zig` target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CursorStyle {
    Default,
    BlinkingBlock,
    SteadyBlock,
    BlinkingUnderline,
    SteadyUnderline,
    BlinkingBar,
    SteadyBar,
}

/// The status line type for DECSSDT.
// PORT NOTE: `enum(u16)` with a `_` (non-exhaustive) tag; same `int()`/`from_int()`
// treatment as above.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatusLineType {
    None,         // = 0
    Indicator,    // = 1
    HostWritable, // = 2

    // Non-exhaustive so that @intToEnum never fails for unsupported values.
    Other(u16),
}

impl StatusLineType {
    pub fn int(self) -> u16 {
        match self {
            StatusLineType::None => 0,
            StatusLineType::Indicator => 1,
            StatusLineType::HostWritable => 2,
            StatusLineType::Other(v) => v,
        }
    }

    pub fn from_int(v: u16) -> StatusLineType {
        match v {
            0 => StatusLineType::None,
            1 => StatusLineType::Indicator,
            2 => StatusLineType::HostWritable,
            _ => StatusLineType::Other(v),
        }
    }
}

/// The display to target for status updates (DECSASD).
// PORT NOTE: `lib.Enum(lib.target, &.{...})`; plain enum for the `.zig` target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatusDisplay {
    Main,
    StatusLine,
}

/// The possible modify key formats to ESC[>{a};{b}m
/// Note: this is not complete, we should add more as we support more
// PORT NOTE: `lib.Enum(lib.target, &.{...})`; plain enum for the `.zig` target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModifyKeyFormat {
    Legacy,
    CursorKeys,
    FunctionKeys,
    OtherKeysNone,
    OtherKeysNumericExcept,
    OtherKeysNumeric,
}

/// The protection modes that can be set for the terminal. See DECSCA and
/// ESC V, W.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProtectedMode {
    Off,
    Iso, // ESC V, W
    Dec, // CSI Ps " q
}
