//! rustty-font opentype post
//!
//! PORTS: ghostty src/font/opentype/post.zig
//!
//! PORT STATUS: done

// PORT NOTE: the reference imports are
//   const std = @import("std");
//   const sfnt = @import("sfnt.zig");
use crate::opentype::sfnt;

/// PostScript Table
///
/// This implementation doesn't parse the
/// extra fields in versions 2.0 and 2.5.
///
/// References:
/// - https://learn.microsoft.com/en-us/typography/opentype/spec/post
///
/// Field names are in camelCase to match names in spec.
// PORT NOTE: the reference `Post` is an `extern struct` with `align(1)` fields
// read big-endian in one shot via `reader.readStructEndian(Post, .big)`. The Rust
// port keeps the spec camelCase field names verbatim (allowing the non-snake-case
// lint) and parses each big-endian field explicitly in `init`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(non_snake_case)]
pub struct Post {
    pub version: sfnt::Version16Dot16,

    /// Italic angle in counter-clockwise degrees from the vertical.
    /// Zero for upright text, negative for text that leans to the
    /// right (forward).
    pub italicAngle: sfnt::Fixed,

    /// Suggested y-coordinate of the top of the underline.
    pub underlinePosition: sfnt::Fword,

    /// Suggested values for the underline thickness.
    /// In general, the underline thickness should match the thickness of
    /// the underscore character (U+005F LOW LINE), and should also match
    /// the strikeout thickness, which is specified in the OS/2 table.
    pub underlineThickness: sfnt::Fword,

    /// Set to 0 if the font is proportionally spaced, non-zero if
    /// the font is not proportionally spaced (i.e. monospaced).
    pub isFixedPitch: sfnt::Uint32,

    /// Minimum memory usage when an OpenType font is downloaded.
    pub minMemType42: sfnt::Uint32,

    /// Maximum memory usage when an OpenType font is downloaded.
    pub maxMemType42: sfnt::Uint32,

    /// Minimum memory usage when an OpenType
    /// font is downloaded as a Type 1 font.
    pub minMemType1: sfnt::Uint32,

    /// Maximum memory usage when an OpenType
    /// font is downloaded as a Type 1 font.
    pub maxMemType1: sfnt::Uint32,
}

/// Error returned from [`Post::init`].
// PORT NOTE: the reference `init` returns `error{EndOfStream}!Post`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InitError {
    /// Ran out of bytes before the full table could be read.
    EndOfStream,
}

impl Post {
    /// Parse the table from raw data.
    // PORT NOTE: `reader.readStructEndian(Post, .big)` off a `fixedBufferStream`;
    // reproduced as an explicit big-endian read of each field in declaration order.
    pub fn init(data: &[u8]) -> Result<Post, InitError> {
        let mut pos: usize = 0;
        Ok(Post {
            version: read_be_version16dot16(data, &mut pos)?,
            italicAngle: sfnt::Fixed::from_raw(read_be_i32(data, &mut pos)?),
            underlinePosition: read_be_i16(data, &mut pos)?,
            underlineThickness: read_be_i16(data, &mut pos)?,
            isFixedPitch: read_be_u32(data, &mut pos)?,
            minMemType42: read_be_u32(data, &mut pos)?,
            maxMemType42: read_be_u32(data, &mut pos)?,
            minMemType1: read_be_u32(data, &mut pos)?,
            maxMemType1: read_be_u32(data, &mut pos)?,
        })
    }
}

/// Read a big-endian [`sfnt::Version16Dot16`] at `*pos`, advancing it.
// PORT NOTE: the reference `Version16Dot16` is `packed struct(u32) { minor, major }`
// read big-endian: the 4 on-disk bytes become a big-endian `u32` whose high 16
// bits are `major` and low 16 bits are `minor`.
fn read_be_version16dot16(data: &[u8], pos: &mut usize) -> Result<sfnt::Version16Dot16, InitError> {
    let raw = read_be_u32(data, pos)?;
    Ok(sfnt::Version16Dot16 {
        minor: (raw & 0xFFFF) as u16,
        major: (raw >> 16) as u16,
    })
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

#[cfg(test)]
mod tests {
    use super::*;

    // test "post"
    #[test]
    fn post() {
        let test_font = crate::embedded::JULIA_MONO;

        let font = sfnt::Sfnt::init(test_font).unwrap();

        let table = font.get_table(b"post").unwrap();

        let post = Post::init(table).unwrap();

        assert_eq!(
            Post {
                version: sfnt::Version16Dot16 { minor: 0, major: 2 },
                italicAngle: sfnt::Fixed::from(0.0),
                underlinePosition: -200,
                underlineThickness: 100,
                isFixedPitch: 1,
                minMemType42: 0,
                maxMemType42: 0,
                minMemType1: 0,
                maxMemType1: 0,
            },
            post,
        );
    }
}
