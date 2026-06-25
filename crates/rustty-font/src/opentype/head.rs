//! rustty-font opentype head
//!
//! PORTS: ghostty src/font/opentype/head.zig
//!
//! PORT STATUS: done

// PORT NOTE: the reference imports are
//   const std = @import("std");
//   const sfnt = @import("sfnt.zig");
use crate::opentype::sfnt;

/// Font Header Table
///
/// References:
/// - https://learn.microsoft.com/en-us/typography/opentype/spec/head
///
/// Field names are in camelCase to match names in spec.
// PORT NOTE: the reference `Head` is an `extern struct` with `align(1)` fields
// laid out for a big-endian on-disk read (`reader.readStructEndian(Head, .big)`).
// The Rust port keeps the spec camelCase field names verbatim (allowing the
// non-snake-case lint) and parses each big-endian field explicitly in `init`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(non_snake_case)]
pub struct Head {
    /// Major version number of the font header table — set to 1.
    pub majorVersion: sfnt::Uint16,

    /// Minor version number of the font header table — set to 0.
    pub minorVersion: sfnt::Uint16,

    /// Set by font manufacturer.
    pub fontRevision: sfnt::Fixed,

    /// To compute: set it to 0, sum the entire font as uint32, then store
    /// 0xB1B0AFBA - sum. If the font is used as a component in a font
    /// collection file, the value of this field will be invalidated by
    /// changes to the file structure and font table directory, and must
    /// be ignored.
    pub checksumAdjustment: sfnt::Uint32,

    /// Set to 0x5F0F3CF5.
    pub magicNumber: sfnt::Uint32,

    /// Bit 0: Baseline for font at y=0.
    ///
    /// Bit 1: Left sidebearing point at x=0
    ///        (relevant only for TrueType rasterizers)
    ///
    /// Bit 2: Instructions may depend on point size.
    ///
    /// Bit 3: Force ppem to integer values for all internal scaler math; may
    ///        use fractional ppem sizes if this bit is clear. It is strongly
    ///        recommended that this be set in hinted fonts.
    ///
    /// Bit 4: Instructions may alter advance width
    ///        (the advance widths might not scale linearly).
    ///
    /// Bit 5: This bit is not used in OpenType, and should not be set in order
    ///        to ensure compatible behavior on all platforms. If set, it may
    ///        result in different behavior for vertical layout in some
    ///        platforms.
    ///
    ///        (See Apple’s specification for details
    ///        regarding behavior in Apple platforms.)
    ///
    /// Bits 6 – 10: These bits are not used in OpenType and should always be
    ///              cleared.
    ///
    ///              (See Apple’s specification for details
    ///              regarding legacy use in Apple platforms.)
    ///
    /// Bit 11: Font data is “lossless” as a result of having been
    ///         subjected to optimizing transformation and/or compression
    ///         (such as compression mechanisms defined by ISO/IEC 14496-18,
    ///         MicroType® Express, WOFF 2.0, or similar) where the original
    ///         font functionality and features are retained but the binary
    ///         compatibility between input and output font files is not
    ///         guaranteed. As a result of the applied transform, the DSIG
    ///         table may also be invalidated.
    ///
    /// Bit 12: Font converted (produce compatible metrics).
    ///
    /// Bit 13: Font optimized for ClearType®. Note, fonts that rely on embedded
    ///         bitmaps (EBDT) for rendering should not be considered optimized
    ///         for ClearType, and therefore should keep this bit cleared.
    ///
    /// Bit 14: Last Resort font. If set, indicates that the glyphs encoded in
    ///         the 'cmap' subtables are simply generic symbolic representations
    ///         of code point ranges and do not truly represent support for
    ///         those code points. If unset, indicates that the glyphs encoded
    ///         in the 'cmap' subtables represent proper support for those code
    ///         points.
    ///
    /// Bit 15: Reserved, set to 0.
    pub flags: sfnt::Uint16,

    /// Set to a value from 16 to 16384. Any value in this range is valid.
    ///
    /// In fonts that have TrueType outlines, a power of 2 is recommended
    /// as this allows performance optimization in some rasterizers.
    pub unitsPerEm: sfnt::Uint16,

    /// Number of seconds since 12:00 midnight that started
    /// January 1st, 1904, in GMT/UTC time zone.
    pub created: sfnt::Longdatetime,

    /// Number of seconds since 12:00 midnight that started
    /// January 1st, 1904, in GMT/UTC time zone.
    pub modified: sfnt::Longdatetime,

    /// Minimum x coordinate across all glyph bounding boxes.
    pub xMin: sfnt::Int16,

    /// Minimum y coordinate across all glyph bounding boxes.
    pub yMin: sfnt::Int16,

    /// Maximum x coordinate across all glyph bounding boxes.
    pub xMax: sfnt::Int16,

    /// Maximum y coordinate across all glyph bounding boxes.
    pub yMax: sfnt::Int16,

    /// Bit 0: Bold (if set to 1);
    /// Bit 1: Italic (if set to 1)
    /// Bit 2: Underline (if set to 1)
    /// Bit 3: Outline (if set to 1)
    /// Bit 4: Shadow (if set to 1)
    /// Bit 5: Condensed (if set to 1)
    /// Bit 6: Extended (if set to 1)
    /// Bits 7 – 15: Reserved (set to 0).
    pub macStyle: sfnt::Uint16,

    /// Smallest readable size in pixels.
    pub lowestRecPPEM: sfnt::Uint16,

    /// Deprecated (Set to 2).
    /// 0: Fully mixed directional glyphs;
    /// 1: Only strongly left to right;
    /// 2: Like 1 but also contains neutrals;
    /// -1: Only strongly right to left;
    /// -2: Like -1 but also contains neutrals.
    pub fontDirectionHint: sfnt::Int16,

    /// 0 for short offsets (Offset16), 1 for long (Offset32).
    pub indexToLocFormat: sfnt::Int16,

    /// 0 for current format.
    pub glyphDataFormat: sfnt::Int16,
}

/// Error returned from [`Head::init`].
// PORT NOTE: the reference `init` returns `error{EndOfStream}!Head`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InitError {
    /// Ran out of bytes before the full table could be read.
    EndOfStream,
}

impl Head {
    /// Parse the table from raw data.
    // PORT NOTE: the reference reads the whole `extern struct` in one shot via
    //   reader.readStructEndian(Head, .big)
    // off a `fixedBufferStream`. The Rust port reads each big-endian field in
    // declaration order (the struct's memory order), returning `EndOfStream`
    // when the buffer underflows — the same failure the reader produces.
    pub fn init(data: &[u8]) -> Result<Head, InitError> {
        let mut pos: usize = 0;
        Ok(Head {
            majorVersion: read_be_u16(data, &mut pos)?,
            minorVersion: read_be_u16(data, &mut pos)?,
            fontRevision: sfnt::Fixed::from_raw(read_be_i32(data, &mut pos)?),
            checksumAdjustment: read_be_u32(data, &mut pos)?,
            magicNumber: read_be_u32(data, &mut pos)?,
            flags: read_be_u16(data, &mut pos)?,
            unitsPerEm: read_be_u16(data, &mut pos)?,
            created: read_be_i64(data, &mut pos)?,
            modified: read_be_i64(data, &mut pos)?,
            xMin: read_be_i16(data, &mut pos)?,
            yMin: read_be_i16(data, &mut pos)?,
            xMax: read_be_i16(data, &mut pos)?,
            yMax: read_be_i16(data, &mut pos)?,
            macStyle: read_be_u16(data, &mut pos)?,
            lowestRecPPEM: read_be_u16(data, &mut pos)?,
            fontDirectionHint: read_be_i16(data, &mut pos)?,
            indexToLocFormat: read_be_i16(data, &mut pos)?,
            glyphDataFormat: read_be_i16(data, &mut pos)?,
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

/// Read a big-endian `u32` at `*pos`, advancing it.
fn read_be_u32(data: &[u8], pos: &mut usize) -> Result<u32, InitError> {
    let bytes: [u8; 4] = data
        .get(*pos..*pos + 4)
        .ok_or(InitError::EndOfStream)?
        .try_into()
        .unwrap();
    *pos += 4;
    Ok(u32::from_be_bytes(bytes))
}

/// Read a big-endian `i32` at `*pos`, advancing it.
fn read_be_i32(data: &[u8], pos: &mut usize) -> Result<i32, InitError> {
    let bytes: [u8; 4] = data
        .get(*pos..*pos + 4)
        .ok_or(InitError::EndOfStream)?
        .try_into()
        .unwrap();
    *pos += 4;
    Ok(i32::from_be_bytes(bytes))
}

/// Read a big-endian `i64` at `*pos`, advancing it.
fn read_be_i64(data: &[u8], pos: &mut usize) -> Result<i64, InitError> {
    let bytes: [u8; 8] = data
        .get(*pos..*pos + 8)
        .ok_or(InitError::EndOfStream)?
        .try_into()
        .unwrap();
    *pos += 8;
    Ok(i64::from_be_bytes(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    // test "head"
    #[test]
    fn head() {
        let test_font = crate::embedded::JULIA_MONO;

        let font = sfnt::Sfnt::init(test_font).unwrap();

        let table = font.get_table(b"head").unwrap();

        let head = Head::init(table).unwrap();

        // PORT NOTE: `expectEqualDeep` on the whole struct; reproduced as an
        // equality assert on the fully-specified expected `Head`, with the
        // `fontRevision` built from the same float literal as the reference
        // (`sfnt.Fixed.from(0.05499267578125)`).
        assert_eq!(
            Head {
                majorVersion: 1,
                minorVersion: 0,
                fontRevision: sfnt::Fixed::from(0.05499267578125),
                checksumAdjustment: 1007668681,
                magicNumber: 1594834165,
                flags: 7,
                unitsPerEm: 2000,
                created: 3797757830,
                modified: 3797760444,
                xMin: -1000,
                yMin: -1058,
                xMax: 3089,
                yMax: 2400,
                macStyle: 0,
                lowestRecPPEM: 7,
                fontDirectionHint: 2,
                indexToLocFormat: 1,
                glyphDataFormat: 0,
            },
            head,
        );
    }
}
