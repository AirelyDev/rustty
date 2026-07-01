//! rustty-font opentype hhea
//!
//! PORTS: ghostty src/font/opentype/hhea.zig
//!
//! PORT STATUS: done

// PORT NOTE: the reference imports are
//   const std = @import("std");
//   const sfnt = @import("sfnt.zig");
use crate::opentype::sfnt;

/// Horizontal Header Table
///
/// References:
/// - https://learn.microsoft.com/en-us/typography/opentype/spec/hhea
///
/// Field names are in camelCase to match names in spec.
// PORT NOTE: the reference `Hhea` is an `extern struct` with `align(1)` fields
// read big-endian in one shot via `reader.readStructEndian(Hhea, .big)`. The Rust
// port keeps the spec camelCase field names verbatim (allowing the non-snake-case
// lint) and parses each big-endian field explicitly in `init`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(non_snake_case)]
pub struct Hhea {
    /// Major version number of the horizontal header table — set to 1.
    pub majorVersion: sfnt::Uint16,

    /// Minor version number of the horizontal header table — set to 0.
    pub minorVersion: sfnt::Uint16,

    /// Typographic ascent—see remarks below.
    pub ascender: sfnt::Fword,

    /// Typographic descent—see remarks below.
    pub descender: sfnt::Fword,

    /// Typographic line gap.
    ///
    /// Negative lineGap values are treated as zero
    /// in some legacy platform implementations.
    pub lineGap: sfnt::Fword,

    /// Maximum advance width value in 'hmtx' table.
    pub advanceWidthMax: sfnt::Ufword,

    /// Minimum left sidebearing value in 'hmtx' table for
    /// glyphs with contours (empty glyphs should be ignored).
    pub minLeftSideBearing: sfnt::Fword,

    /// Minimum right sidebearing value; calculated as
    /// min(aw - (lsb + xMax - xMin)) for glyphs with
    /// contours (empty glyphs should be ignored).
    pub minRightSideBearing: sfnt::Fword,

    /// Max(lsb + (xMax - xMin)).
    pub xMaxExtent: sfnt::Fword,

    /// Used to calculate the slope of the cursor (rise/run); 1 for vertical.
    pub caretSlopeRise: sfnt::Int16,

    /// 0 for vertical.
    pub caretSlopeRun: sfnt::Int16,

    /// The amount by which a slanted highlight on a glyph needs to be shifted
    /// to produce the best appearance. Set to 0 for non-slanted fonts
    pub caretOffset: sfnt::Int16,

    /// set to 0
    pub _reserved0: sfnt::Int16,

    /// set to 0
    pub _reserved1: sfnt::Int16,

    /// set to 0
    pub _reserved2: sfnt::Int16,

    /// set to 0
    pub _reserved3: sfnt::Int16,

    /// 0 for current format.
    pub metricDataFormat: sfnt::Int16,

    /// Number of hMetric entries in 'hmtx' table
    pub numberOfHMetrics: sfnt::Uint16,
}

/// Error returned from [`Hhea::init`].
// PORT NOTE: the reference `init` returns `!Hhea` (only `error{EndOfStream}` is
// reachable from `readStructEndian` off a fixed buffer).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InitError {
    /// Ran out of bytes before the full table could be read.
    EndOfStream,
}

impl Hhea {
    /// Parse the table from raw data.
    // PORT NOTE: `reader.readStructEndian(Hhea, .big)`; reproduced as an explicit
    // big-endian read of each field in declaration order.
    pub fn init(data: &[u8]) -> Result<Hhea, InitError> {
        let mut pos: usize = 0;
        Ok(Hhea {
            majorVersion: read_be_u16(data, &mut pos)?,
            minorVersion: read_be_u16(data, &mut pos)?,
            ascender: read_be_i16(data, &mut pos)?,
            descender: read_be_i16(data, &mut pos)?,
            lineGap: read_be_i16(data, &mut pos)?,
            advanceWidthMax: read_be_u16(data, &mut pos)?,
            minLeftSideBearing: read_be_i16(data, &mut pos)?,
            minRightSideBearing: read_be_i16(data, &mut pos)?,
            xMaxExtent: read_be_i16(data, &mut pos)?,
            caretSlopeRise: read_be_i16(data, &mut pos)?,
            caretSlopeRun: read_be_i16(data, &mut pos)?,
            caretOffset: read_be_i16(data, &mut pos)?,
            _reserved0: read_be_i16(data, &mut pos)?,
            _reserved1: read_be_i16(data, &mut pos)?,
            _reserved2: read_be_i16(data, &mut pos)?,
            _reserved3: read_be_i16(data, &mut pos)?,
            metricDataFormat: read_be_i16(data, &mut pos)?,
            numberOfHMetrics: read_be_u16(data, &mut pos)?,
        })
    }
}

/// Read a big-endian `u16` at `*pos`, advancing it.
fn read_be_u16(data: &[u8], pos: &mut usize) -> Result<u16, InitError> {
    let bytes: [u8; 2] = data
        .get(*pos..*pos + 2)
        .ok_or(InitError::EndOfStream)?
        .try_into()
        .unwrap();
    *pos += 2;
    Ok(u16::from_be_bytes(bytes))
}

/// Read a big-endian `i16` at `*pos`, advancing it.
fn read_be_i16(data: &[u8], pos: &mut usize) -> Result<i16, InitError> {
    let bytes: [u8; 2] = data
        .get(*pos..*pos + 2)
        .ok_or(InitError::EndOfStream)?
        .try_into()
        .unwrap();
    *pos += 2;
    Ok(i16::from_be_bytes(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    // test "hhea"
    #[test]
    fn hhea() {
        let test_font = crate::embedded::JULIA_MONO;

        let font = sfnt::Sfnt::init(test_font).unwrap();

        let table = font.get_table(b"hhea").unwrap();

        let hhea = Hhea::init(table).unwrap();

        assert_eq!(
            Hhea {
                majorVersion: 1,
                minorVersion: 0,
                ascender: 1900,
                descender: -450,
                lineGap: 0,
                advanceWidthMax: 1200,
                minLeftSideBearing: -1000,
                minRightSideBearing: -1889,
                xMaxExtent: 3089,
                caretSlopeRise: 1,
                caretSlopeRun: 0,
                caretOffset: 0,
                _reserved0: 0,
                _reserved1: 0,
                _reserved2: 0,
                _reserved3: 0,
                metricDataFormat: 0,
                numberOfHMetrics: 2,
            },
            hhea,
        );
    }
}
