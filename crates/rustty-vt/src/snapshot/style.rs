//! Snapshot style entry encoding.
//!
//! Each entry describes one terminal style. A record can use these entries to
//! build a style table and assign indexes according to that record's format.
//! Indexing, ordering, etc. are properties of the containing record.
//!
//! Styles are encoded field by field from the terminal's native `Style` type.
//! The native packed representation is deliberately not part of the snapshot
//! format. This gives flexibility for changing one side or the other.
//!
//! All integers are unsigned and little-endian.
//!
//! | Offset | Size | Field                     |
//! | -----: | ---: | :------------------------ |
//! |      0 |    4 | Foreground color          |
//! |      4 |    4 | Background color          |
//! |      8 |    4 | Underline color           |
//! |     12 |    2 | Style flags (`u16`)       |
//! |     14 |    2 | Reserved, must be zero    |
//!
//! The trailing reserved field is explicit wire padding that rounds each
//! style entry from 14 bytes to 16 bytes. This makes entry offsets and byte
//! counts straightforward.
//!
//! Each color begins with a one-byte kind followed by three data bytes:
//!
//! | Kind | Meaning | Data bytes                         |
//! | ---: | :------ | :--------------------------------- |
//! |    0 | None    | All zero                           |
//! |    1 | Palette | Palette index, then two zero bytes |
//! |    2 | RGB     | Red, green, blue                   |
//!
//! Style flag bits 0 through 7 are bold, italic, faint, blink, inverse,
//! invisible, strikethrough, and overline. Bits 8 through 10 contain the
//! underline kind. Bits 11 through 15 must be zero.
//!
//! | Underline | Meaning |
//! | --------: | :------ |
//! |         0 | None    |
//! |         1 | Single  |
//! |         2 | Double  |
//! |         3 | Curly   |
//! |         4 | Dotted  |
//! |         5 | Dashed  |
//!
//! Underline values 6 and 7 are invalid in snapshot version 1.
//!
//! PORTS: ghostty src/terminal/snapshot/style.zig (END STATE: the single-pass
//! entry codec of 1359973ae — `parse` over a complete fixed-size buffer, with
//! `parse_or_null` for enclosing codecs decoding flat payloads).

// const std = @import("std");
// const test_fixture = @import("fixture.zig");
// const sgr = @import("../sgr.zig");
// const terminal_style = @import("../style.zig");
use super::io;
use crate::sgr;
use crate::style as terminal_style;

/// Number of bytes in one encoded style entry. This size is part of the
/// wire format: the codec reads and writes fixed offsets within an entry
/// of exactly this size, so if the layout changes, the snapshot version
/// and golden fixtures must also change.
pub const LEN: usize = 16;

/// The wire flag word. (Zig: file-local `packed struct(u16)`; the port keeps
/// explicit fields plus `bits`/`from_bits` honoring the exact bit layout.)
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Flags {
    bold: bool,
    italic: bool,
    faint: bool,
    blink: bool,
    inverse: bool,
    invisible: bool,
    strikethrough: bool,
    overline: bool,
    /// `underline: u3` — bits 8..10.
    underline: u8,
    /// `reserved: u5` — bits 11..15.
    reserved: u8,
}

impl Flags {
    /// `@bitCast(flags)` to the packed `u16`.
    fn bits(self) -> u16 {
        (self.bold as u16)
            | (self.italic as u16) << 1
            | (self.faint as u16) << 2
            | (self.blink as u16) << 3
            | (self.inverse as u16) << 4
            | (self.invisible as u16) << 5
            | (self.strikethrough as u16) << 6
            | (self.overline as u16) << 7
            | (self.underline as u16 & 0x7) << 8
            | (self.reserved as u16 & 0x1f) << 11
    }

    /// `@bitCast(raw)` from the packed `u16`.
    fn from_bits(raw: u16) -> Flags {
        Flags {
            bold: raw & (1 << 0) != 0,
            italic: raw & (1 << 1) != 0,
            faint: raw & (1 << 2) != 0,
            blink: raw & (1 << 3) != 0,
            inverse: raw & (1 << 4) != 0,
            invisible: raw & (1 << 5) != 0,
            strikethrough: raw & (1 << 6) != 0,
            overline: raw & (1 << 7) != 0,
            underline: ((raw >> 8) & 0x7) as u8,
            reserved: ((raw >> 11) & 0x1f) as u8,
        }
    }
}

/// The wire color kind byte. (Zig: file-local `enum(u8)`.)
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ColorKind {
    None = 0,
    Palette = 1,
    Rgb = 2,
}

impl ColorKind {
    /// `std.enums.fromInt(ColorKind, raw)`
    fn from_int(raw: u8) -> Option<ColorKind> {
        Some(match raw {
            0 => ColorKind::None,
            1 => ColorKind::Palette,
            2 => ColorKind::Rgb,
            _ => return None,
        })
    }
}

/// Semantic validation errors for one complete style entry buffer.
/// (Zig: file-local `ParseError`.)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ParseError {
    /// A color kind is not defined by snapshot version 1.
    InvalidColorKind,

    /// Bytes unused by the selected color kind are not zero.
    InvalidColor,

    /// The encoded underline kind is not defined by snapshot version 1.
    InvalidUnderline,

    /// One or more reserved style flag bits are set.
    InvalidFlags,

    /// The trailing reserved field is not zero.
    InvalidReserved,
}

/// Errors possible while decoding one style entry.
// pub const DecodeError = std.Io.Reader.Error || ParseError;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecodeError {
    // std.Io.Reader.Error
    ReadFailed,
    EndOfStream,
    // ParseError
    InvalidColorKind,
    InvalidColor,
    InvalidUnderline,
    InvalidFlags,
    InvalidReserved,
}

impl From<io::ReadError> for DecodeError {
    fn from(err: io::ReadError) -> DecodeError {
        match err {
            io::ReadError::ReadFailed => DecodeError::ReadFailed,
            io::ReadError::EndOfStream => DecodeError::EndOfStream,
        }
    }
}

impl From<ParseError> for DecodeError {
    fn from(err: ParseError) -> DecodeError {
        match err {
            ParseError::InvalidColorKind => DecodeError::InvalidColorKind,
            ParseError::InvalidColor => DecodeError::InvalidColor,
            ParseError::InvalidUnderline => DecodeError::InvalidUnderline,
            ParseError::InvalidFlags => DecodeError::InvalidFlags,
            ParseError::InvalidReserved => DecodeError::InvalidReserved,
        }
    }
}

/// Encode one terminal style as a fixed-size snapshot style entry.
///
/// The entry is assembled in a fixed buffer and written once, so hot
/// encoders perform a single writer call per style.
pub fn encode(
    value: terminal_style::Style,
    writer: &mut dyn io::Writer,
) -> Result<(), io::WriteFailed> {
    let mut encoded = [0u8; LEN];
    encode_color_buf(
        (&mut encoded[0..4]).try_into().unwrap(),
        value.fg_color,
    );
    encode_color_buf(
        (&mut encoded[4..8]).try_into().unwrap(),
        value.bg_color,
    );
    encode_color_buf(
        (&mut encoded[8..12]).try_into().unwrap(),
        value.underline_color,
    );

    let flags = Flags {
        bold: value.flags.bold,
        italic: value.flags.italic,
        faint: value.flags.faint,
        blink: value.flags.blink,
        inverse: value.flags.inverse,
        invisible: value.flags.invisible,
        strikethrough: value.flags.strikethrough,
        overline: value.flags.overline,
        underline: value.flags.underline as u8,
        reserved: 0,
    };
    encoded[12..14].copy_from_slice(&flags.bits().to_le_bytes());
    writer.write_all(&encoded)
}

/// Decode and validate one fixed-size snapshot style entry.
pub fn decode(reader: &mut dyn io::Reader) -> Result<terminal_style::Style, DecodeError> {
    let mut encoded = [0u8; LEN];
    reader.read_slice_all(&mut encoded)?;
    parse(&encoded).map_err(DecodeError::from)
}

/// Decode and validate one complete fixed-size style entry buffer.
fn parse(encoded: &[u8; LEN]) -> Result<terminal_style::Style, ParseError> {
    let fg_color = parse_color(encoded[0..4].try_into().unwrap())?;
    let bg_color = parse_color(encoded[4..8].try_into().unwrap())?;
    let underline_color = parse_color(encoded[8..12].try_into().unwrap())?;

    let flags = Flags::from_bits(u16::from_le_bytes(encoded[12..14].try_into().unwrap()));
    if flags.reserved != 0 {
        return Err(ParseError::InvalidFlags);
    }

    // std.enums.fromInt(sgr.Attribute.Underline, flags.underline)
    let underline = match flags.underline {
        0 => sgr::Underline::None,
        1 => sgr::Underline::Single,
        2 => sgr::Underline::Double,
        3 => sgr::Underline::Curly,
        4 => sgr::Underline::Dotted,
        5 => sgr::Underline::Dashed,
        _ => return Err(ParseError::InvalidUnderline),
    };

    let reserved = u16::from_le_bytes(encoded[14..16].try_into().unwrap());
    if reserved != 0 {
        return Err(ParseError::InvalidReserved);
    }

    Ok(terminal_style::Style {
        fg_color,
        bg_color,
        underline_color,
        flags: terminal_style::Flags {
            bold: flags.bold,
            italic: flags.italic,
            faint: flags.faint,
            blink: flags.blink,
            inverse: flags.inverse,
            invisible: flags.invisible,
            strikethrough: flags.strikethrough,
            overline: flags.overline,
            underline,
        },
    })
}

/// Decode strictly after consuming one complete fixed-size style entry.
///
/// Unlike `decode`, a semantic error leaves `reader` at the next entry. This
/// lets an enclosing codec catch the error and choose its own fallback without
/// losing the surrounding payload boundary.
pub fn decode_or_discard(
    reader: &mut dyn io::Reader,
) -> Result<terminal_style::Style, DecodeError> {
    let mut encoded = [0u8; LEN];
    reader.read_slice_all(&mut encoded)?;
    parse(&encoded).map_err(DecodeError::from)
}

/// Decode one complete entry, returning null for invalid semantic contents.
///
/// Reader errors remain structural and are always propagated. This is the
/// lenient entry point for enclosing codecs which can safely replace an
/// invalid fixed-size style without treating truncation as invalid styling.
pub fn decode_or_null(
    reader: &mut dyn io::Reader,
) -> Result<Option<terminal_style::Style>, io::ReadError> {
    match decode_or_discard(reader) {
        Ok(style) => Ok(Some(style)),
        Err(DecodeError::ReadFailed) => Err(io::ReadError::ReadFailed),
        Err(DecodeError::EndOfStream) => Err(io::ReadError::EndOfStream),
        Err(DecodeError::InvalidColorKind)
        | Err(DecodeError::InvalidColor)
        | Err(DecodeError::InvalidUnderline)
        | Err(DecodeError::InvalidFlags)
        | Err(DecodeError::InvalidReserved) => Ok(None),
    }
}

/// `decode_or_null` over one already-buffered entry, for enclosing codecs
/// that parse many entries from a flat payload without reader calls.
pub fn parse_or_null(encoded: &[u8; LEN]) -> Option<terminal_style::Style> {
    parse(encoded).ok()
}

fn encode_color_buf(encoded: &mut [u8; 4], value: terminal_style::Color) {
    match value.tag() {
        terminal_style::ColorTag::None => {
            encoded[0] = ColorKind::None as u8;
        }
        terminal_style::ColorTag::Palette => {
            encoded[0] = ColorKind::Palette as u8;
            encoded[1] = value.palette();
        }
        terminal_style::ColorTag::Rgb => {
            let rgb = value.rgb();
            encoded[0] = ColorKind::Rgb as u8;
            encoded[1] = rgb.r;
            encoded[2] = rgb.g;
            encoded[3] = rgb.b;
        }
    }
}

fn parse_color(encoded: &[u8; 4]) -> Result<terminal_style::Color, ParseError> {
    // Kind must be something we know about.
    let kind = ColorKind::from_int(encoded[0]).ok_or(ParseError::InvalidColorKind)?;

    match kind {
        ColorKind::None => {
            if encoded[1] == 0 && encoded[2] == 0 && encoded[3] == 0 {
                Ok(terminal_style::Color::None)
            } else {
                Err(ParseError::InvalidColor)
            }
        }
        ColorKind::Palette => {
            if encoded[2] == 0 && encoded[3] == 0 {
                Ok(terminal_style::Color::Palette(encoded[1]))
            } else {
                Err(ParseError::InvalidColor)
            }
        }
        ColorKind::Rgb => Ok(terminal_style::Color::Rgb(crate::color::RGB {
            r: encoded[1],
            g: encoded[2],
            b: encoded[3],
        })),
    }
}

#[cfg(test)]
mod tests {
    use super::super::fixture;
    use super::*;

    // const test_golden_fixture = test_fixture.parse(@embedFile("testdata/style-v1.hex"));
    fn test_golden_fixture() -> Vec<u8> {
        fixture::parse(include_str!("testdata/style-v1.hex"))
    }

    // test "golden encoding and decoding"
    #[test]
    fn golden_encoding_and_decoding() {
        let test_golden_fixture = test_golden_fixture();

        let value = terminal_style::Style {
            fg_color: terminal_style::Color::None,
            bg_color: terminal_style::Color::Palette(0x7f),
            underline_color: terminal_style::Color::Rgb(crate::color::RGB {
                r: 0x12,
                g: 0x34,
                b: 0x56,
            }),
            flags: terminal_style::Flags {
                bold: true,
                italic: true,
                faint: true,
                blink: true,
                inverse: true,
                invisible: true,
                strikethrough: true,
                overline: true,
                underline: sgr::Underline::Curly,
            },
        };
        let mut buf = [0u8; LEN];
        let mut writer = io::FixedWriter::new(&mut buf);
        encode(value, &mut writer).unwrap();
        fixture::expect_equal(
            fixture::Kind::Bytes,
            "src/snapshot/testdata/style-v1.hex",
            "snapshot_fixture-style-v1.hex",
            &test_golden_fixture,
            writer.buffered(),
        );

        let mut source = io::FixedReader::new(&test_golden_fixture);
        let mut read_buf = [0u8; 1];
        let mut limited = io::LimitedReader::new(&mut source, io::Limit::Unlimited, &mut read_buf);
        assert!(value.eql(&decode(&mut limited).unwrap()));
    }

    // test "flag bit layout"
    #[test]
    fn flag_bit_layout() {
        struct Case {
            value: Flags,
            expected: u16,
        }
        let cases = [
            Case {
                value: Flags {
                    bold: true,
                    ..Flags::default()
                },
                expected: 1 << 0,
            },
            Case {
                value: Flags {
                    italic: true,
                    ..Flags::default()
                },
                expected: 1 << 1,
            },
            Case {
                value: Flags {
                    faint: true,
                    ..Flags::default()
                },
                expected: 1 << 2,
            },
            Case {
                value: Flags {
                    blink: true,
                    ..Flags::default()
                },
                expected: 1 << 3,
            },
            Case {
                value: Flags {
                    inverse: true,
                    ..Flags::default()
                },
                expected: 1 << 4,
            },
            Case {
                value: Flags {
                    invisible: true,
                    ..Flags::default()
                },
                expected: 1 << 5,
            },
            Case {
                value: Flags {
                    strikethrough: true,
                    ..Flags::default()
                },
                expected: 1 << 6,
            },
            Case {
                value: Flags {
                    overline: true,
                    ..Flags::default()
                },
                expected: 1 << 7,
            },
            Case {
                value: Flags {
                    underline: sgr::Underline::Single as u8,
                    ..Flags::default()
                },
                expected: 1 << 8,
            },
            Case {
                value: Flags {
                    underline: sgr::Underline::Double as u8,
                    ..Flags::default()
                },
                expected: 2 << 8,
            },
            Case {
                value: Flags {
                    underline: sgr::Underline::Curly as u8,
                    ..Flags::default()
                },
                expected: 3 << 8,
            },
            Case {
                value: Flags {
                    underline: sgr::Underline::Dotted as u8,
                    ..Flags::default()
                },
                expected: 4 << 8,
            },
            Case {
                value: Flags {
                    underline: sgr::Underline::Dashed as u8,
                    ..Flags::default()
                },
                expected: 5 << 8,
            },
            Case {
                value: Flags {
                    reserved: 1,
                    ..Flags::default()
                },
                expected: 1 << 11,
            },
        ];

        for case in cases {
            assert_eq!(case.expected, case.value.bits());
            // The port's from_bits is the @bitCast inverse; keep it honest.
            assert_eq!(case.value, Flags::from_bits(case.expected));
        }
    }

    // PORT NOTE: `std.testing.expectError` compares only the error value; the
    // base `Style` deliberately has no `PartialEq` (its equality is the custom
    // `eql`), so these asserts discard the unused Ok payload with
    // `.map(|_| ())` before the exact-error comparison (the crate's
    // no-`Debug`/`PartialEq`-Ok pattern, see pagelist/tests.rs).

    // test "reject invalid colors"
    #[test]
    fn reject_invalid_colors() {
        for offset in [0usize, 4, 8] {
            let mut invalid_kind = [0u8; LEN];
            invalid_kind[offset] = 3;
            let mut reader = io::FixedReader::new(&invalid_kind);
            assert_eq!(
                Err(DecodeError::InvalidColorKind),
                decode(&mut reader).map(|_| ()),
            );
        }

        let mut invalid_none = [0u8; LEN];
        invalid_none[1] = 1;
        let mut none_reader = io::FixedReader::new(&invalid_none);
        assert_eq!(
            Err(DecodeError::InvalidColor),
            decode(&mut none_reader).map(|_| ()),
        );

        let mut invalid_palette = [0u8; LEN];
        invalid_palette[0] = ColorKind::Palette as u8;
        invalid_palette[2] = 1;
        let mut palette_reader = io::FixedReader::new(&invalid_palette);
        assert_eq!(
            Err(DecodeError::InvalidColor),
            decode(&mut palette_reader).map(|_| ()),
        );
    }

    // test "reject invalid flags and reserved field"
    #[test]
    fn reject_invalid_flags_and_reserved_field() {
        for underline in [6u16, 7] {
            let mut invalid_underline = [0u8; LEN];
            invalid_underline[12..14].copy_from_slice(&(underline << 8).to_le_bytes());
            let mut reader = io::FixedReader::new(&invalid_underline);
            assert_eq!(
                Err(DecodeError::InvalidUnderline),
                decode(&mut reader).map(|_| ()),
            );
        }

        let mut invalid_flags = [0u8; LEN];
        invalid_flags[12..14].copy_from_slice(&(1u16 << 11).to_le_bytes());
        let mut flags_reader = io::FixedReader::new(&invalid_flags);
        assert_eq!(
            Err(DecodeError::InvalidFlags),
            decode(&mut flags_reader).map(|_| ()),
        );

        let mut invalid_reserved = [0u8; LEN];
        invalid_reserved[14..16].copy_from_slice(&1u16.to_le_bytes());
        let mut reserved_reader = io::FixedReader::new(&invalid_reserved);
        assert_eq!(
            Err(DecodeError::InvalidReserved),
            decode(&mut reserved_reader).map(|_| ()),
        );
    }

    // test "decodeOrDiscard preserves the next entry boundary"
    #[test]
    fn decode_or_discard_preserves_the_next_entry_boundary() {
        use crate::snapshot::io::Reader as _;

        let mut fixture_bytes = [0u8; LEN + 1];
        fixture_bytes[0] = 3;
        fixture_bytes[LEN] = 0xFF;

        let mut reader = io::FixedReader::new(&fixture_bytes);
        assert_eq!(
            Err(DecodeError::InvalidColorKind),
            decode_or_discard(&mut reader).map(|_| ()),
        );
        assert_eq!(0xFFu8, reader.take_byte().unwrap());
    }

    // test "decodeOrNull distinguishes semantic errors from truncation"
    #[test]
    fn decode_or_null_distinguishes_semantic_errors_from_truncation() {
        let mut invalid = [0u8; LEN];
        invalid[0] = 3;
        let mut invalid_reader = io::FixedReader::new(&invalid);
        // try std.testing.expectEqual(null, try decodeOrNull(&invalid_reader));
        assert!(decode_or_null(&mut invalid_reader).unwrap().is_none());

        let mut truncated = io::FixedReader::new(&invalid[..LEN - 1]);
        assert_eq!(
            Err(io::ReadError::EndOfStream),
            decode_or_null(&mut truncated).map(|_| ()),
        );
    }

    // test "reject every truncation"
    #[test]
    fn reject_every_truncation() {
        let fixture_bytes = [0u8; LEN];
        for fixture_len in 0..LEN {
            let mut reader = io::FixedReader::new(&fixture_bytes[..fixture_len]);
            assert_eq!(
                Err(DecodeError::EndOfStream),
                decode(&mut reader).map(|_| ()),
            );
        }
    }
}
