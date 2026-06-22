//! rustty-vt device_attributes
//!
//! PORTS: ghostty src/terminal/device_attributes.zig
//!
//! PORT STATUS: done

// PARITY: `Default` impls mirror the reference's explicit field defaults; keep
// the explicit form documenting each reference default.
#![allow(clippy::derivable_impls)]

// PORT NOTE: the reference imports are
//   const std = @import("std");
//   const testing = std.testing;
//   const lib = @import("lib.zig");
// `lib.Enum` for the `.zig` target is a plain enum. The reference encode methods
// take a `*std.Io.Writer` and return `std.Io.Writer.Error!void`; we port them to
// take any `core::fmt::Write` sink and return `core::fmt::Result`, which models
// the same "append formatted bytes, may fail to write" contract for the
// ASCII-only output produced here.

use core::fmt::Write;

/// The device attribute request type (CSI c).
// PORT NOTE: `lib.Enum(lib.target, &.{...})`; plain enum for the `.zig` target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Req {
    Primary,   // Blank
    Secondary, // >
    Tertiary,  // =
}

/// Response data for all device attribute queries.
#[derive(Clone, Copy, Debug)]
pub struct Attributes {
    /// Reply to CSI c (DA1).
    pub primary: Primary,

    /// Reply to CSI > c (DA2).
    pub secondary: Secondary,

    /// Reply to CSI = c (DA3).
    pub tertiary: Tertiary,
}

impl Default for Attributes {
    // PORT NOTE: reference field defaults are all `= .{}`.
    fn default() -> Self {
        Attributes {
            primary: Primary::default(),
            secondary: Secondary::default(),
            tertiary: Tertiary::default(),
        }
    }
}

impl Attributes {
    /// Encode the response for the given request type into the writer.
    pub fn encode<W: Write>(&self, req: Req, writer: &mut W) -> core::fmt::Result {
        match req {
            Req::Primary => self.primary.encode(writer),
            Req::Secondary => self.secondary.encode(writer),
            Req::Tertiary => self.tertiary.encode(writer),
        }
    }
}

/// Primary device attributes (DA1).
///
/// Response format: CSI ? Pp ; Ps... c
/// where Pp is the conformance level and Ps are feature flags.
#[derive(Clone, Copy, Debug)]
pub struct Primary {
    /// Conformance level sent as the first parameter.
    pub conformance_level: ConformanceLevel,

    /// Optional feature attributes.
    pub features: &'static [Feature],
}

impl Default for Primary {
    // PORT NOTE: reference defaults: `conformance_level = .vt220`,
    // `features = &.{.ansi_color}`.
    fn default() -> Self {
        Primary {
            conformance_level: ConformanceLevel::VT220,
            features: &[Feature::AnsiColor],
        }
    }
}

/// DA1 feature attribute codes.
// PORT NOTE: the reference nests `Feature` inside `Primary`; hoisted to a
// top-level `#[repr(u16)]` enum with a non-exhaustive `_` arm.
#[repr(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Feature {
    Columns132 = 1,
    Printer = 2,
    Regis = 3,
    Sixel = 4,
    SelectiveErase = 6,
    UserDefinedKeys = 8,
    NationalReplacement = 9,
    TechnicalCharacters = 15,
    Locator = 16,
    TerminalState = 17,
    Windowing = 18,
    HorizontalScrolling = 21,
    AnsiColor = 22,
    RectangularEditing = 28,
    AnsiTextLocator = 29,
    Clipboard = 52,
    Other(u16),
}

impl Feature {
    // PORT NOTE: the reference uses `@intFromEnum(feature)`. With a
    // non-exhaustive arm we provide an explicit integer conversion.
    fn int(self) -> u16 {
        match self {
            Feature::Columns132 => 1,
            Feature::Printer => 2,
            Feature::Regis => 3,
            Feature::Sixel => 4,
            Feature::SelectiveErase => 6,
            Feature::UserDefinedKeys => 8,
            Feature::NationalReplacement => 9,
            Feature::TechnicalCharacters => 15,
            Feature::Locator => 16,
            Feature::TerminalState => 17,
            Feature::Windowing => 18,
            Feature::HorizontalScrolling => 21,
            Feature::AnsiColor => 22,
            Feature::RectangularEditing => 28,
            Feature::AnsiTextLocator => 29,
            Feature::Clipboard => 52,
            Feature::Other(v) => v,
        }
    }
}

impl Primary {
    /// Encode the primary DA response into the writer.
    pub fn encode<W: Write>(&self, writer: &mut W) -> core::fmt::Result {
        write!(writer, "\x1b[?{}", self.conformance_level.int())?;
        for feature in self.features {
            write!(writer, ";{}", feature.int())?;
        }
        writer.write_str("c")
    }
}

/// Secondary device attributes (DA2).
///
/// Response format: CSI > Pp ; Pv ; Pc c
#[derive(Clone, Copy, Debug)]
pub struct Secondary {
    /// Terminal type identifier (Pp parameter from secondary DA response).
    pub device_type: DeviceType,

    /// Firmware/patch version number.
    pub firmware_version: u16,

    /// ROM cartridge registration number. Always 0 for emulators.
    pub rom_cartridge: u16,
}

impl Default for Secondary {
    // PORT NOTE: reference defaults: `device_type = .vt220`,
    // `firmware_version = 0`, `rom_cartridge = 0`.
    fn default() -> Self {
        Secondary {
            device_type: DeviceType::VT220,
            firmware_version: 0,
            rom_cartridge: 0,
        }
    }
}

impl Secondary {
    /// Encode the secondary DA response into the writer.
    pub fn encode<W: Write>(&self, writer: &mut W) -> core::fmt::Result {
        write!(
            writer,
            "\x1b[>{};{};{}c",
            self.device_type.int(),
            self.firmware_version,
            self.rom_cartridge,
        )
    }
}

/// Tertiary device attributes (DA3).
///
/// Response format: DCS ! | D...D ST
/// where D...D is the unit ID as hex digits (DECRPTUI).
#[derive(Clone, Copy, Debug)]
pub struct Tertiary {
    /// Unit ID (DECRPTUI). Encoded as 8 uppercase hex digits.
    /// Meaningless for emulators nowadays. The actual DEC manuals
    /// appear to split this into two 16-bit fields but since there
    /// is no practical usage I know if I'm simplifying this.
    pub unit_id: u32,
}

impl Default for Tertiary {
    // PORT NOTE: reference default: `unit_id = 0`.
    fn default() -> Self {
        Tertiary { unit_id: 0 }
    }
}

impl Tertiary {
    /// Encode the tertiary DA response into the writer.
    pub fn encode<W: Write>(&self, writer: &mut W) -> core::fmt::Result {
        // PORT NOTE: the reference format `"\x1bP!|{X:0>8}\x1b\\"` is uppercase
        // hex, zero-padded to width 8; the Rust equivalent is `{:08X}`.
        write!(writer, "\x1bP!|{:08X}\x1b\\", self.unit_id)
    }
}

/// Conformance level reported as the first parameter (Pp) in the
/// primary device attributes (DA1) response.
// PORT NOTE: `enum(u16)` with a `_` (non-exhaustive) tag plus a set of
// `pub const` aliases. Rust enum variants cannot share discriminants, so the
// aliases (vt101, vt220, ...) are ported as associated constants below.
#[repr(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConformanceLevel {
    // VT100-series have per-model values.
    VT100 = 1,
    VT132 = 4,
    VT102 = 6,
    VT131 = 7,
    VT125 = 12,

    // VT200+ use 60 + decTerminalID/100.
    /// Level 2 conformance (VT200 series, e.g. VT220, VT240).
    Level2 = 62,
    /// Level 3 conformance (VT300 series, e.g. VT320, VT340).
    Level3 = 63,
    /// Level 4 conformance (VT400 series, e.g. VT420).
    Level4 = 64,
    /// Level 5 conformance (VT500 series, e.g. VT510, VT520, VT525).
    Level5 = 65,

    Other(u16),
}

impl ConformanceLevel {
    pub const VT101: ConformanceLevel = ConformanceLevel::VT100;
    pub const VT220: ConformanceLevel = ConformanceLevel::Level2;
    pub const VT240: ConformanceLevel = ConformanceLevel::Level2;
    pub const VT320: ConformanceLevel = ConformanceLevel::Level3;
    pub const VT340: ConformanceLevel = ConformanceLevel::Level3;
    pub const VT420: ConformanceLevel = ConformanceLevel::Level4;
    pub const VT510: ConformanceLevel = ConformanceLevel::Level5;
    pub const VT520: ConformanceLevel = ConformanceLevel::Level5;
    pub const VT525: ConformanceLevel = ConformanceLevel::Level5;

    // PORT NOTE: `@intFromEnum` with a non-exhaustive arm.
    fn int(self) -> u16 {
        match self {
            ConformanceLevel::VT100 => 1,
            ConformanceLevel::VT132 => 4,
            ConformanceLevel::VT102 => 6,
            ConformanceLevel::VT131 => 7,
            ConformanceLevel::VT125 => 12,
            ConformanceLevel::Level2 => 62,
            ConformanceLevel::Level3 => 63,
            ConformanceLevel::Level4 => 64,
            ConformanceLevel::Level5 => 65,
            ConformanceLevel::Other(v) => v,
        }
    }
}

/// Terminal type identifier reported as the Pp parameter in the
/// secondary device attributes (DA2) response. Values correspond
/// to the decTerminalID resource in xterm.
#[repr(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeviceType {
    VT100 = 0,
    VT220 = 1,
    VT240 = 2,
    VT330 = 18,
    VT340 = 19,
    VT320 = 24,
    VT382 = 32,
    VT420 = 41,
    VT510 = 61,
    VT520 = 64,
    VT525 = 65,
    Other(u16),
}

impl DeviceType {
    // PORT NOTE: `@intFromEnum` with a non-exhaustive arm.
    fn int(self) -> u16 {
        match self {
            DeviceType::VT100 => 0,
            DeviceType::VT220 => 1,
            DeviceType::VT240 => 2,
            DeviceType::VT330 => 18,
            DeviceType::VT340 => 19,
            DeviceType::VT320 => 24,
            DeviceType::VT382 => 32,
            DeviceType::VT420 => 41,
            DeviceType::VT510 => 61,
            DeviceType::VT520 => 64,
            DeviceType::VT525 => 65,
            DeviceType::Other(v) => v,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primary_default() {
        let mut buf = String::new();
        Primary::default().encode(&mut buf).unwrap();
        assert_eq!("\x1b[?62;22c", buf);
    }

    #[test]
    fn primary_with_clipboard() {
        let mut buf = String::new();
        Primary {
            features: &[Feature::AnsiColor, Feature::Clipboard],
            ..Primary::default()
        }
        .encode(&mut buf)
        .unwrap();
        assert_eq!("\x1b[?62;22;52c", buf);
    }

    #[test]
    fn primary_with_multiple_features() {
        let mut buf = String::new();
        Primary {
            conformance_level: ConformanceLevel::VT420,
            features: &[
                Feature::Columns132,
                Feature::SelectiveErase,
                Feature::AnsiColor,
            ],
        }
        .encode(&mut buf)
        .unwrap();
        assert_eq!("\x1b[?64;1;6;22c", buf);
    }

    #[test]
    fn primary_no_features() {
        let mut buf = String::new();
        Primary {
            conformance_level: ConformanceLevel::VT100,
            features: &[],
        }
        .encode(&mut buf)
        .unwrap();
        assert_eq!("\x1b[?1c", buf);
    }

    #[test]
    fn secondary_default() {
        let mut buf = String::new();
        Secondary::default().encode(&mut buf).unwrap();
        assert_eq!("\x1b[>1;0;0c", buf);
    }

    #[test]
    fn tertiary_default() {
        let mut buf = String::new();
        Tertiary::default().encode(&mut buf).unwrap();
        assert_eq!("\x1bP!|00000000\x1b\\", buf);
    }

    #[test]
    fn tertiary_custom_unit_id() {
        let mut buf = String::new();
        Tertiary {
            unit_id: 0xAABBCCDD,
        }
        .encode(&mut buf)
        .unwrap();
        assert_eq!("\x1bP!|AABBCCDD\x1b\\", buf);
    }
}
