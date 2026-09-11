//! rustty-font opentype os2
//!
//! PORTS: ghostty src/font/opentype/os2.zig
//!
//! PORT STATUS: done

// PORT NOTE: the reference imports are
//   const std = @import("std");
//   const sfnt = @import("sfnt.zig");
use crate::opentype::sfnt;

/// `fsSelection` bit field.
// PORT NOTE: the reference `FSSelection` is `packed struct(sfnt.uint16)` whose
// fields map, LSB→MSB, onto the 16-bit value. Rust has no bit-precise packed
// structs, so it is a plain struct of `bool` fields with `from_bits`/`to_bits`
// helpers reproducing that exact bit layout (bit 0 = `italic`, ..., bit 9 =
// `oblique`, bits 10..=15 reserved). The reference reads it via
// `readStructEndian(.big)`, i.e. the 2 on-disk bytes as a big-endian `u16`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FSSelection {
    /// Font contains italic or oblique glyphs, otherwise they are upright.
    pub italic: bool,

    /// Glyphs are underscored.
    pub underscore: bool,

    /// Glyphs have their foreground and background reversed.
    pub negative: bool,

    /// Outline (hollow) glyphs, otherwise they are solid.
    pub outlined: bool,

    /// Glyphs are overstruck.
    pub strikeout: bool,

    /// Glyphs are emboldened.
    pub bold: bool,

    /// Glyphs are in the standard weight/style for the font.
    pub regular: bool,

    /// If set, it is strongly recommended that applications use
    /// OS/2.sTypoAscender - OS/2.sTypoDescender + OS/2.sTypoLineGap
    /// as the default line spacing for this font.
    pub use_typo_metrics: bool,

    /// The font has 'name' table strings consistent with a weight/width/slope
    /// family without requiring use of name IDs 21 and 22.
    pub wws: bool,

    /// Font contains oblique glyphs.
    pub oblique: bool,
    // _reserved: u6 = 0,
}

impl FSSelection {
    /// Decode the bit field from its backing `u16` (bit 0 = `italic`).
    pub fn from_bits(v: u16) -> FSSelection {
        FSSelection {
            italic: v & (1 << 0) != 0,
            underscore: v & (1 << 1) != 0,
            negative: v & (1 << 2) != 0,
            outlined: v & (1 << 3) != 0,
            strikeout: v & (1 << 4) != 0,
            bold: v & (1 << 5) != 0,
            regular: v & (1 << 6) != 0,
            use_typo_metrics: v & (1 << 7) != 0,
            wws: v & (1 << 8) != 0,
            oblique: v & (1 << 9) != 0,
        }
    }

    /// Encode the bit field into its backing `u16`.
    pub fn to_bits(self) -> u16 {
        (self.italic as u16)
            | ((self.underscore as u16) << 1)
            | ((self.negative as u16) << 2)
            | ((self.outlined as u16) << 3)
            | ((self.strikeout as u16) << 4)
            | ((self.bold as u16) << 5)
            | ((self.regular as u16) << 6)
            | ((self.use_typo_metrics as u16) << 7)
            | ((self.wws as u16) << 8)
            | ((self.oblique as u16) << 9)
    }
}

// OS/2 and Windows Metrics Table
//
// References:
// - https://learn.microsoft.com/en-us/typography/opentype/spec/os2
//
// Field names are in camelCase to match names in spec.
// PORT NOTE: this doc header sits on the reference's versioned `OS2v5` struct;
// since that versioned struct is collapsed into `OS2::init` in this port (see the
// note below), the header is a plain comment here rather than a `///` doc comment
// orphaned from any item.
//
// PORT NOTE: the reference defines four versioned `extern struct`s — `OS2v5`,
// `OS2v4_3_2`, `OS2v1`, `OS2v0` — that are read whole via `readStructEndian(.big)`
// and then copied field-by-field into the generic `OS2`. Each is a prefix of the
// next (v0 ⊂ v1 ⊂ v4_3_2 ⊂ v5). These pure parse-intermediates are collapsed here
// into `OS2::init`'s single sequential big-endian reader, which reads the shared
// prefix and then the version-gated tail; the produced `OS2` is byte-for-byte the
// same as the reference's copy. Their field layouts are preserved by the generic
// `OS2` below and the read order in `init`.

/// Generic OS/2 table with optional fields
/// for those that don't exist in all versions.
///
/// References:
/// - https://learn.microsoft.com/en-us/typography/opentype/spec/os2
///
/// Field names are in camelCase to match names in spec.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(non_snake_case)]
pub struct OS2 {
    /// The version number for the OS/2 table: 0x0000 to 0x0005.
    ///
    /// https://learn.microsoft.com/en-us/typography/opentype/spec/os2#version
    pub version: u16,
    /// The Average Character Width field specifies the arithmetic average of the escapement (width) of all non-zero width glyphs in the font.
    ///
    /// https://learn.microsoft.com/en-us/typography/opentype/spec/os2/#xavgcharwidth
    pub xAvgCharWidth: i16,
    /// Indicates the visual weight (degree of blackness or thickness of strokes) of the characters in the font. Values from 1 to 1000 are valid.
    ///
    /// https://learn.microsoft.com/en-us/typography/opentype/spec/os2/#usweightclass
    pub usWeightClass: u16,
    /// Indicates a relative change from the normal aspect ratio (width to height ratio) as specified by a font designer for the glyphs in a font.
    ///
    /// https://learn.microsoft.com/en-us/typography/opentype/spec/os2/#uswidthclass
    pub usWidthClass: u16,
    /// Indicates font embedding licensing rights for the font.
    ///
    /// https://learn.microsoft.com/en-us/typography/opentype/spec/os2/#fstype
    pub fsType: u16,
    /// The recommended horizontal size in font design units for subscripts for this font.
    ///
    /// https://learn.microsoft.com/en-us/typography/opentype/spec/os2/#ysubscriptxsize
    pub ySubscriptXSize: i16,
    /// The recommended vertical size in font design units for subscripts for this font.
    ///
    /// https://learn.microsoft.com/en-us/typography/opentype/spec/os2/#ysubscriptysize
    pub ySubscriptYSize: i16,
    /// The recommended horizontal offset in font design units for subscripts for this font.
    ///
    /// https://learn.microsoft.com/en-us/typography/opentype/spec/os2/#ysubscriptxoffset
    pub ySubscriptXOffset: i16,
    /// The recommended vertical offset in font design units from the baseline for subscripts for this font.
    ///
    /// https://learn.microsoft.com/en-us/typography/opentype/spec/os2/#ysubscriptyoffset
    pub ySubscriptYOffset: i16,
    /// The recommended horizontal size in font design units for superscripts for this font.
    ///
    /// https://learn.microsoft.com/en-us/typography/opentype/spec/os2/#ysuperscriptxsize
    pub ySuperscriptXSize: i16,
    /// The recommended vertical size in font design units for superscripts for this font.
    ///
    /// https://learn.microsoft.com/en-us/typography/opentype/spec/os2/#ysuperscriptysize
    pub ySuperscriptYSize: i16,
    /// The recommended horizontal offset in font design units for superscripts for this font.
    ///
    /// https://learn.microsoft.com/en-us/typography/opentype/spec/os2/#ysuperscriptxoffset
    pub ySuperscriptXOffset: i16,
    /// The recommended vertical offset in font design units from the baseline for superscripts for this font.
    ///
    /// https://learn.microsoft.com/en-us/typography/opentype/spec/os2/#ysuperscriptyoffset
    pub ySuperscriptYOffset: i16,
    /// Thickness of the strikeout stroke in font design units. Should be > 0.
    ///
    /// https://learn.microsoft.com/en-us/typography/opentype/spec/os2/#ystrikeoutsize
    pub yStrikeoutSize: i16,
    /// The position of the top of the strikeout stroke relative to the baseline in font design units.
    ///
    /// https://learn.microsoft.com/en-us/typography/opentype/spec/os2/#ystrikeoutposition
    pub yStrikeoutPosition: i16,
    /// This field provides a classification of font-family design.
    ///
    /// https://learn.microsoft.com/en-us/typography/opentype/spec/os2/#sfamilyclass
    pub sFamilyClass: i16,
    /// This 10-byte array of numbers is used to describe the visual characteristics of a given typeface.
    ///
    /// https://learn.microsoft.com/en-us/typography/opentype/spec/os2/#panose
    pub panose: [u8; 10],
    /// https://learn.microsoft.com/en-us/typography/opentype/spec/os2/#ulunicoderange
    pub ulUnicodeRange1: u32,
    /// https://learn.microsoft.com/en-us/typography/opentype/spec/os2/#ulunicoderange
    pub ulUnicodeRange2: u32,
    /// https://learn.microsoft.com/en-us/typography/opentype/spec/os2/#ulunicoderange
    pub ulUnicodeRange3: u32,
    /// https://learn.microsoft.com/en-us/typography/opentype/spec/os2/#ulunicoderange
    pub ulUnicodeRange4: u32,
    /// The four character identifier for the vendor of the given type face.
    ///
    /// https://learn.microsoft.com/en-us/typography/opentype/spec/os2/#achvendid
    pub achVendID: [u8; 4],
    /// Contains information concerning the nature of the font patterns.
    ///
    /// https://learn.microsoft.com/en-us/typography/opentype/spec/os2/#fsselection
    pub fsSelection: FSSelection,
    /// The minimum Unicode index (character code) in this font, according to the 'cmap' subtable for platform ID 3 and platform-specific encoding ID 0 or 1.
    ///
    /// https://learn.microsoft.com/en-us/typography/opentype/spec/os2/#usfirstcharindex
    pub usFirstCharIndex: u16,
    /// The maximum Unicode index (character code) in this font, according to the 'cmap' subtable for platform ID 3 and encoding ID 0 or 1.
    ///
    /// https://learn.microsoft.com/en-us/typography/opentype/spec/os2/#uslastcharindex
    pub usLastCharIndex: u16,
    /// The typographic ascender for this font. This field should be combined with the sTypoDescender and sTypoLineGap values to determine default line spacing.
    ///
    /// https://learn.microsoft.com/en-us/typography/opentype/spec/os2/#stypoascender
    pub sTypoAscender: i16,
    /// The typographic descender for this font. This field should be combined with the sTypoAscender and sTypoLineGap values to determine default line spacing.
    ///
    /// https://learn.microsoft.com/en-us/typography/opentype/spec/os2/#stypodescender
    pub sTypoDescender: i16,
    /// The typographic line gap for this font. This field should be combined with the sTypoAscender and sTypoDescender values to determine default line spacing.
    ///
    /// https://learn.microsoft.com/en-us/typography/opentype/spec/os2/#stypolinegap
    pub sTypoLineGap: i16,
    /// The “Windows ascender” metric. This should be used to specify the height above the baseline for a clipping region.
    ///
    /// https://learn.microsoft.com/en-us/typography/opentype/spec/os2/#uswinascent
    pub usWinAscent: u16,
    /// The “Windows descender” metric. This should be used to specify the vertical extent below the baseline for a clipping region.
    ///
    /// https://learn.microsoft.com/en-us/typography/opentype/spec/os2/#uswindescent
    pub usWinDescent: u16,
    /// https://learn.microsoft.com/en-us/typography/opentype/spec/os2/#ulcodepagerange
    pub ulCodePageRange1: Option<u32>,
    /// https://learn.microsoft.com/en-us/typography/opentype/spec/os2/#ulcodepagerange
    pub ulCodePageRange2: Option<u32>,
    /// This metric specifies the distance between the baseline and the approximate height of non-ascending lowercase letters measured in font design units.
    ///
    /// https://learn.microsoft.com/en-us/typography/opentype/spec/os2/#sxheight
    pub sxHeight: Option<i16>,
    /// This metric specifies the distance between the baseline and the approximate height of uppercase letters measured in font design units.
    ///
    /// https://learn.microsoft.com/en-us/typography/opentype/spec/os2/#scapheight
    pub sCapHeight: Option<i16>,
    /// This is the Unicode code point, in UTF-16 encoding, of a character that can be used for a default glyph if a requested character is not supported in the font.
    ///
    /// https://learn.microsoft.com/en-us/typography/opentype/spec/os2/#usdefaultchar
    pub usDefaultChar: Option<u16>,
    /// This is the Unicode code point, in UTF-16 encoding, of a character that can be used as a default break character.
    ///
    /// https://learn.microsoft.com/en-us/typography/opentype/spec/os2/#usbreakchar
    pub usBreakChar: Option<u16>,
    /// The maximum length of a target glyph context for any feature in this font. For example, a font which has only a pair kerning feature should set this field to 2.
    ///
    /// https://learn.microsoft.com/en-us/typography/opentype/spec/os2/#usmaxcontext
    pub usMaxContext: Option<u16>,
    /// This field is used for fonts with multiple optical styles.
    ///
    /// https://learn.microsoft.com/en-us/typography/opentype/spec/os2/#usloweropticalpointsize
    pub usLowerOpticalPointSize: Option<u16>,
    /// This field is used for fonts with multiple optical styles.
    ///
    /// https://learn.microsoft.com/en-us/typography/opentype/spec/os2/#usupperopticalpointsize
    pub usUpperOpticalPointSize: Option<u16>,
}

/// Error returned from [`OS2::init`].
// PORT NOTE: the reference `init` error set is
// `error{ EndOfStream, OS2VersionNotSupported }`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InitError {
    EndOfStream,
    OS2VersionNotSupported,
}

impl OS2 {
    /// Parse the table from raw data.
    // PORT NOTE: the reference reads `version` (seeking back to 0), then dispatches
    // on it and reads the whole versioned struct. Here we read the shared prefix
    // sequentially (the version is the first field), then read the version-gated
    // tail; unsupported versions return `OS2VersionNotSupported`.
    // PORT NOTE: `#[allow(non_snake_case)]` keeps the spec camelCase names on the
    // local bindings so the `OS2 { .. }` field-shorthand construction matches the
    // (likewise camelCase, spec-faithful) struct fields.
    #[allow(non_snake_case)]
    pub fn init(data: &[u8]) -> Result<OS2, InitError> {
        let mut pos: usize = 0;

        let version = read_be_u16(data, &mut pos)?;

        // Return to the start, cause the version is part of the struct.
        // PORT NOTE: the reference `seekTo(0)` then re-reads `version` as the first
        // field of the versioned struct; here `version` above IS that first field
        // and reading continues in place, so no seek is needed.

        // Only versions 0..=5 are supported.
        //
        // PORT NOTE (S92-F1): the reference `switch (version)` dispatches BEFORE
        // reading any field, so its `else => return error.OS2VersionNotSupported`
        // fires on an unsupported version no matter how short the table is. This
        // check used to sit AFTER every field read, so a short table declaring
        // version > 5 returned `EndOfStream` instead.
        if version > 5 {
            return Err(InitError::OS2VersionNotSupported);
        }

        // Prefix shared by every version (v0).
        let xAvgCharWidth = read_be_i16(data, &mut pos)?;
        let usWeightClass = read_be_u16(data, &mut pos)?;
        let usWidthClass = read_be_u16(data, &mut pos)?;
        let fsType = read_be_u16(data, &mut pos)?;
        let ySubscriptXSize = read_be_i16(data, &mut pos)?;
        let ySubscriptYSize = read_be_i16(data, &mut pos)?;
        let ySubscriptXOffset = read_be_i16(data, &mut pos)?;
        let ySubscriptYOffset = read_be_i16(data, &mut pos)?;
        let ySuperscriptXSize = read_be_i16(data, &mut pos)?;
        let ySuperscriptYSize = read_be_i16(data, &mut pos)?;
        let ySuperscriptXOffset = read_be_i16(data, &mut pos)?;
        let ySuperscriptYOffset = read_be_i16(data, &mut pos)?;
        let yStrikeoutSize = read_be_i16(data, &mut pos)?;
        let yStrikeoutPosition = read_be_i16(data, &mut pos)?;
        let sFamilyClass = read_be_i16(data, &mut pos)?;
        let panose = read_panose(data, &mut pos)?;
        let ulUnicodeRange1 = read_be_u32(data, &mut pos)?;
        let ulUnicodeRange2 = read_be_u32(data, &mut pos)?;
        let ulUnicodeRange3 = read_be_u32(data, &mut pos)?;
        let ulUnicodeRange4 = read_be_u32(data, &mut pos)?;
        let achVendID = read_tag(data, &mut pos)?;
        let fsSelection = FSSelection::from_bits(read_be_u16(data, &mut pos)?);
        let usFirstCharIndex = read_be_u16(data, &mut pos)?;
        let usLastCharIndex = read_be_u16(data, &mut pos)?;
        let sTypoAscender = read_be_i16(data, &mut pos)?;
        let sTypoDescender = read_be_i16(data, &mut pos)?;
        let sTypoLineGap = read_be_i16(data, &mut pos)?;
        let usWinAscent = read_be_u16(data, &mut pos)?;
        let usWinDescent = read_be_u16(data, &mut pos)?;

        // Version-gated tail.
        let (ulCodePageRange1, ulCodePageRange2) = if version >= 1 {
            (
                Some(read_be_u32(data, &mut pos)?),
                Some(read_be_u32(data, &mut pos)?),
            )
        } else {
            (None, None)
        };

        let (sxHeight, sCapHeight, usDefaultChar, usBreakChar, usMaxContext) = if version >= 2 {
            (
                Some(read_be_i16(data, &mut pos)?),
                Some(read_be_i16(data, &mut pos)?),
                Some(read_be_u16(data, &mut pos)?),
                Some(read_be_u16(data, &mut pos)?),
                Some(read_be_u16(data, &mut pos)?),
            )
        } else {
            (None, None, None, None, None)
        };

        let (usLowerOpticalPointSize, usUpperOpticalPointSize) = if version >= 5 {
            (
                Some(read_be_u16(data, &mut pos)?),
                Some(read_be_u16(data, &mut pos)?),
            )
        } else {
            (None, None)
        };

        Ok(OS2 {
            version,
            xAvgCharWidth,
            usWeightClass,
            usWidthClass,
            fsType,
            ySubscriptXSize,
            ySubscriptYSize,
            ySubscriptXOffset,
            ySubscriptYOffset,
            ySuperscriptXSize,
            ySuperscriptYSize,
            ySuperscriptXOffset,
            ySuperscriptYOffset,
            yStrikeoutSize,
            yStrikeoutPosition,
            sFamilyClass,
            panose,
            ulUnicodeRange1,
            ulUnicodeRange2,
            ulUnicodeRange3,
            ulUnicodeRange4,
            achVendID,
            fsSelection,
            usFirstCharIndex,
            usLastCharIndex,
            sTypoAscender,
            sTypoDescender,
            sTypoLineGap,
            usWinAscent,
            usWinDescent,
            ulCodePageRange1,
            ulCodePageRange2,
            sxHeight,
            sCapHeight,
            usDefaultChar,
            usBreakChar,
            usMaxContext,
            usLowerOpticalPointSize,
            usUpperOpticalPointSize,
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

/// Read the 10-byte `panose` array at `*pos`, advancing it.
fn read_panose(data: &[u8], pos: &mut usize) -> Result<[u8; 10], InitError> {
    let bytes: [u8; 10] = data
        .get(*pos..*pos + 10)
        .ok_or(InitError::EndOfStream)?
        .try_into()
        .unwrap();
    *pos += 10;
    Ok(bytes)
}

/// Read a 4-byte [`sfnt::Tag`] at `*pos`, advancing it.
fn read_tag(data: &[u8], pos: &mut usize) -> Result<sfnt::Tag, InitError> {
    let bytes: [u8; 4] = data
        .get(*pos..*pos + 4)
        .ok_or(InitError::EndOfStream)?
        .try_into()
        .unwrap();
    *pos += 4;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// PORT NOTE: no reference analogue. The reference's `switch (version)`
    /// dispatches before any field is read, so an unsupported version is
    /// reported as `OS2VersionNotSupported` regardless of table length. The
    /// port ran the check after every read and returned `EndOfStream` for a
    /// short table (S92-F1).
    #[test]
    fn unsupported_version_is_reported_before_reading_fields() {
        // A two-byte table: nothing but an unsupported version.
        assert_eq!(
            Err(InitError::OS2VersionNotSupported),
            OS2::init(&[0x00, 0x06])
        );
        // And one byte longer than the version, still far too short.
        assert_eq!(
            Err(InitError::OS2VersionNotSupported),
            OS2::init(&[0x00, 0xFF, 0x00])
        );
        // Positive control: a SUPPORTED version that is genuinely truncated
        // still reports `EndOfStream`, so the check above did not simply
        // swallow every short table.
        assert_eq!(Err(InitError::EndOfStream), OS2::init(&[0x00, 0x04, 0x00]));
    }

    // test "OS/2"
    #[test]
    fn os_2() {
        let test_font = crate::embedded::JULIA_MONO;

        let font = sfnt::Sfnt::init(test_font).unwrap();

        let table = font.get_table(b"OS/2").unwrap();

        let os2 = OS2::init(table).unwrap();

        assert_eq!(
            OS2 {
                version: 4,
                xAvgCharWidth: 1200,
                usWeightClass: 400,
                usWidthClass: 5,
                fsType: 0,
                ySubscriptXSize: 1300,
                ySubscriptYSize: 1200,
                ySubscriptXOffset: 0,
                ySubscriptYOffset: 150,
                ySuperscriptXSize: 1300,
                ySuperscriptYSize: 1200,
                ySuperscriptXOffset: 0,
                ySuperscriptYOffset: 700,
                yStrikeoutSize: 100,
                yStrikeoutPosition: 550,
                sFamilyClass: 0,
                panose: [2, 11, 6, 9, 6, 3, 0, 2, 0, 4],
                ulUnicodeRange1: 3843162111,
                ulUnicodeRange2: 3603300351,
                ulUnicodeRange3: 117760229,
                ulUnicodeRange4: 96510060,
                achVendID: *b"corm",
                fsSelection: FSSelection {
                    regular: true,
                    use_typo_metrics: true,
                    ..Default::default()
                },
                usFirstCharIndex: 13,
                usLastCharIndex: 65535,
                sTypoAscender: 1900,
                sTypoDescender: -450,
                sTypoLineGap: 0,
                usWinAscent: 2400,
                usWinDescent: 450,
                ulCodePageRange1: Some(1613234687),
                ulCodePageRange2: Some(0),
                sxHeight: Some(1100),
                sCapHeight: Some(1450),
                usDefaultChar: Some(0),
                usBreakChar: Some(32),
                usMaxContext: Some(126),
                usLowerOpticalPointSize: None,
                usUpperOpticalPointSize: None,
            },
            os2,
        );
    }
}
