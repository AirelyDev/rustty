//! rustty-font opentype sfnt
//!
//! PORTS: ghostty src/font/opentype/sfnt.zig
//!
//! PORT STATUS: done

// PORT NOTE: the reference imports are
//   const std = @import("std");
//   const assert = @import("../../quirks.zig").inlineAssert;
//   const Allocator = std.mem.Allocator;
// `std.mem.Allocator` is not threaded through this port: the reference `SFNT`
// allocates a `[]TableRecord` via the passed allocator and frees it in `deinit`.
// In Rust the records are owned by a `Vec<TableRecord>`, so ownership/lifetime is
// handled by the type system and no `Allocator`/`deinit` is needed; `init` takes
// only `data` and returns the parsed `SFNT`. The big-endian fields the reference
// byte-swaps via `std.mem.byteSwapAllFields` are read field-by-field from the
// big-endian on-disk bytes here (SFNT files are always big endian).

// PORT NOTE: `const assert = @import("../../quirks.zig").inlineAssert;` — the
// shared quirks assert, ported in `rustty-util` (kept under its reference name).
use rustty_util::quirks::inlineAssert as inline_assert;

/// 8-bit unsigned integer.
pub type Uint8 = u8;

/// 8-bit signed integer.
pub type Int8 = i8;

/// 16-bit unsigned integer.
pub type Uint16 = u16;

/// 16-bit signed integer.
pub type Int16 = i16;

// PORT NOTE: Zig's `u24` is a native 24-bit integer; Rust has no `u24`, so the
// `uint24`/`Offset24` aliases below are represented with `u32` (the smallest
// std width that holds a 24-bit value). They are unused by any ported test but
// kept for parity with the reference type table.
/// 24-bit unsigned integer.
pub type Uint24 = u32;

/// 32-bit unsigned integer.
pub type Uint32 = u32;

/// 32-bit signed integer.
pub type Int32 = i32;

/// 32-bit signed fixed-point number (16.16)
pub type Fixed = FixedPoint<i32, 16, 16>;

/// int16 that describes a quantity in font design units.
pub type Fword = i16;

/// uint16 that describes a quantity in font design units.
pub type Ufword = u16;

/// 16-bit signed fixed number with the low 14 bits of fraction (2.14).
pub type F2Dot14 = FixedPoint<i16, 2, 14>;

/// Date and time represented in number of seconds since 12:00 midnight, January 1, 1904, UTC. The value is represented as a signed 64-bit integer.
pub type Longdatetime = i64;

/// Array of four uint8s (length = 32 bits) used to identify a table,
/// design-variation axis, script, language system, feature, or baseline.
pub type Tag = [u8; 4];

/// 8-bit offset to a table, same as uint8, NULL offset = 0x00
pub type Offset8 = u8;

/// Short offset to a table, same as uint16, NULL offset = 0x0000
pub type Offset16 = u16;

/// 24-bit offset to a table, same as uint24, NULL offset = 0x000000
pub type Offset24 = u32;

/// Long offset to a table, same as uint32, NULL offset = 0x00000000
pub type Offset32 = u32;

/// Packed 32-bit value with major and minor version numbers
// PORT NOTE: the reference is `packed struct(u32) { minor: u16, major: u16 }`,
// where the field order is the in-memory bit order (minor in the low 16 bits).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Version16Dot16 {
    pub minor: u16,
    pub major: u16,
}

/// 32-bit signed 26.6 fixed point numbers.
pub type F26Dot6 = FixedPoint<i32, 26, 6>;

/// A fixed-point number with `INT_BITS` integer bits and `FRAC_BITS`
/// fractional bits, backed by the signed integer type `T`.
// PORT NOTE: the reference `FixedPoint` is `fn FixedPoint(comptime T, int_bits,
// frac_bits) type` returning an anonymous `packed struct(T)` whose fields are
// `frac: Frac` (an unsigned `frac_bits`-wide int) and `int: Int` (a signed
// `int_bits`-wide int, matching T's signedness), laid out low-to-high. Rust has
// no integer-width generics, so the struct is backed directly by `T` and the
// `int`/`frac` views are computed via shifts/masks, reproducing the same bit
// layout (`frac` in the low `FRAC_BITS` bits, `int` above it). The comptime
// `assert(int_bits + frac_bits == type_info.bits)` is preserved via the
// `FixedPointRepr` impls, which only exist for the exact widths used.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FixedPoint<T: FixedPointRepr, const INT_BITS: u64, const FRAC_BITS: u64> {
    /// The raw backing value, holding both the integer and fractional parts.
    raw: T,
}

/// Backing-integer behavior for [`FixedPoint`], capturing the per-width
/// arithmetic the reference performs against the comptime-derived `T`, `Frac`,
/// and `Int` types.
// PORT NOTE: this trait stands in for the comptime computations in the reference
// (`frac_factor`, `half`, `Frac`, `Int`, the `@bitCast`/`@floatFromInt`
// conversions). Implementations exist only for the `(T, INT_BITS, FRAC_BITS)`
// triples actually used (`i32`/16/16, `i16`/2/14, `i32`/26/6), which is what the
// reference `assert(int_bits + frac_bits == type_info.bits)` guards.
pub trait FixedPointRepr: Copy {
    /// Number of bits in `T` (`@typeInfo(T).int.bits`).
    const BITS: u64;
    /// The fractional scale factor `2**frac_bits` as an `f64`.
    fn frac_factor(frac_bits: u64) -> f64;
    /// `@as(T, 1) << (frac_bits - 1)`, used by `round`.
    fn half(frac_bits: u64) -> Self;
    /// `self & half != 0` test for `round`.
    fn frac_has_half(self, half: Self) -> bool;
    /// `self.int`: the signed integer part (high bits), as `T`.
    fn int_part(self, frac_bits: u64) -> Self;
    /// `self.int + 1`, for the round-up branch.
    fn int_plus_one(self, frac_bits: u64) -> Self;
    /// `@floatFromInt(@as(T, @bitCast(self)))`.
    fn to_f64(self) -> f64;
    /// `@bitCast(@as(T, @intFromFloat(@round(float * frac_factor))))`.
    fn from_f64(value: f64) -> Self;
}

impl<T: FixedPointRepr, const INT_BITS: u64, const FRAC_BITS: u64>
    FixedPoint<T, INT_BITS, FRAC_BITS>
{
    /// Construct directly from `int` and `frac` field views, matching the
    /// reference struct-literal form `.{ .int = ..., .frac = ... }`.
    // PORT NOTE: the reference constructs `FixedPoint` values with struct
    // literals naming the `int`/`frac` fields. Since this port stores the packed
    // `raw` value, this helper assembles `raw` from those two views so tests can
    // express the same literals (`F26Dot6{ .int = 26, .frac = 38 }`).
    pub fn from_parts(int: i64, frac: u64) -> Self
    where
        T: FixedPointReprParts,
    {
        Self {
            raw: T::from_parts(int, frac, FRAC_BITS),
        }
    }

    /// Construct directly from the packed backing value.
    // PORT NOTE: when a `FixedPoint` is read off disk as part of an `extern
    // struct` (e.g. `Head.fontRevision` via `readStructEndian(..., .big)`), the
    // bytes are bit-cast straight into the packed `T`. This mirrors that: the
    // caller passes the host-native value of the big-endian on-disk word.
    pub fn from_raw(raw: T) -> Self {
        Self { raw }
    }

    /// Return the packed backing value.
    // PORT NOTE: the reference recovers the raw fixed-point word via
    // `@as(i32, @bitCast(fp))` (e.g. freetype face's `renderGlyph` stores a
    // transformed outline point back into `p.x`/`p.y`). This accessor exposes
    // that same backing word for such round-trips.
    pub fn to_raw(self) -> T {
        self.raw
    }

    /// Convert to a floating-point value.
    pub fn to_f64(self) -> f64 {
        // PORT NOTE: comptime
        //   const type_info: std.builtin.Type.Int = @typeInfo(T).int;
        //   comptime assert(int_bits + frac_bits == type_info.bits);
        // is preserved as a runtime assert (always true for the supported
        // widths) so the relationship is still enforced.
        inline_assert(INT_BITS + FRAC_BITS == T::BITS);
        self.raw.to_f64() / T::frac_factor(FRAC_BITS)
    }

    /// Build a fixed-point value from a float.
    // PORT NOTE: the reference `from` takes `anytype`; the only callers pass
    // float literals, so this takes `f64`.
    pub fn from(float: f64) -> Self {
        inline_assert(INT_BITS + FRAC_BITS == T::BITS);
        Self {
            raw: T::from_f64((float * T::frac_factor(FRAC_BITS)).round()),
        }
    }

    /// Round to the nearest integer, .5 rounds away from 0.
    pub fn round(self) -> T {
        let half = T::half(FRAC_BITS);
        if self.raw.frac_has_half(half) {
            self.raw.int_plus_one(FRAC_BITS)
        } else {
            self.raw.int_part(FRAC_BITS)
        }
    }
}

/// Lets [`FixedPoint`] assemble its packed `raw` value from `int`/`frac` views,
/// for the struct-literal style the reference tests use.
pub trait FixedPointReprParts: Sized {
    /// Assemble the packed backing value from the integer and fractional views.
    fn from_parts(int: i64, frac: u64, frac_bits: u64) -> Self;
}

impl FixedPointRepr for i32 {
    const BITS: u64 = 32;
    fn frac_factor(frac_bits: u64) -> f64 {
        // PORT NOTE: `@floatFromInt(std.math.pow(u64, 2, frac_bits))`.
        2u64.pow(frac_bits as u32) as f64
    }
    fn half(frac_bits: u64) -> Self {
        1i32 << (frac_bits - 1)
    }
    fn frac_has_half(self, half: Self) -> bool {
        // The reference tests `self.frac & half`, where `frac` is the low
        // `frac_bits`. `half` already only has that one bit set, so masking the
        // full `raw` value against it is equivalent.
        (self & half) != 0
    }
    fn int_part(self, frac_bits: u64) -> Self {
        self >> frac_bits
    }
    fn int_plus_one(self, frac_bits: u64) -> Self {
        (self >> frac_bits) + 1
    }
    fn to_f64(self) -> f64 {
        f64::from(self)
    }
    fn from_f64(value: f64) -> Self {
        value as i32
    }
}

impl FixedPointReprParts for i32 {
    fn from_parts(int: i64, frac: u64, frac_bits: u64) -> Self {
        ((int as i32) << frac_bits) | (frac as i32)
    }
}

impl FixedPointRepr for i16 {
    const BITS: u64 = 16;
    fn frac_factor(frac_bits: u64) -> f64 {
        2u64.pow(frac_bits as u32) as f64
    }
    fn half(frac_bits: u64) -> Self {
        1i16 << (frac_bits - 1)
    }
    fn frac_has_half(self, half: Self) -> bool {
        (self & half) != 0
    }
    fn int_part(self, frac_bits: u64) -> Self {
        self >> frac_bits
    }
    fn int_plus_one(self, frac_bits: u64) -> Self {
        (self >> frac_bits) + 1
    }
    fn to_f64(self) -> f64 {
        f64::from(self)
    }
    fn from_f64(value: f64) -> Self {
        value as i16
    }
}

impl FixedPointReprParts for i16 {
    fn from_parts(int: i64, frac: u64, frac_bits: u64) -> Self {
        ((int as i16) << frac_bits) | (frac as i16)
    }
}

#[cfg(test)]
mod fixed_point_tests {
    use super::*;

    // test FixedPoint
    #[test]
    fn fixed_point() {
        let p26d6 = F26Dot6::from(26.6);
        assert_eq!(F26Dot6::from_parts(26, 38), p26d6);
        assert_eq!(26.59375, p26d6.to_f64());
        assert_eq!(27, p26d6.round());

        let n26d6 = F26Dot6::from(-26.6);
        assert_eq!(F26Dot6::from_parts(-27, 26), n26d6);
        assert_eq!(-26.59375, n26d6.to_f64());
        assert_eq!(-27, n26d6.round());
    }
}

/// Wrapper for parsing a SFNT font and accessing its tables.
///
/// References:
/// - https://learn.microsoft.com/en-us/typography/opentype/spec/otff
/// - https://developer.apple.com/fonts/TrueType-Reference-Manual/RM06/Chap6.html
pub struct Sfnt<'a> {
    pub directory: Directory,

    pub data: &'a [u8],
}

/// The table directory of a parsed SFNT font.
// PORT NOTE: the reference `Directory` is a `const Directory = struct { ... }`
// nested in `SFNT`; Rust has no nested type declarations, so it is hoisted to a
// sibling type. Its `records` is a `[]TableRecord` allocated/freed via the
// reference allocator; here it is an owned `Vec<TableRecord>`.
pub struct Directory {
    pub offset: OffsetSubtable,
    pub records: Vec<TableRecord>,
}

/// The static (fixed-sized) portion of the table directory
///
/// This struct matches the memory layout of the TrueType/OpenType
/// TableDirectory, but does not include the TableRecord array, since
/// that is dynamically sized, so we parse it separately.
///
/// In the TrueType reference manual this
/// is referred to as the "offset subtable".
///
/// https://learn.microsoft.com/en-us/typography/opentype/spec/otff#table-directory
// PORT NOTE: the reference `OffsetSubtable` is an `extern struct` with `align(1)`
// fields read big-endian off disk (then byte-swapped on little-endian hosts).
// The Rust port parses those big-endian fields explicitly in `Sfnt::init`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OffsetSubtable {
    /// Indicates the type of font file we're reading.
    ///   - 0x00_01_00_00   ----   TrueType
    ///   - 0x74_72_75_65  'true'  TrueType
    ///   - 0x4F_54_54_4F  'OTTO'  OpenType
    ///   - 0x74_79_70_31  'typ1'  PostScript
    pub sfnt_version: Uint32,
    /// Number of tables.
    pub num_tables: Uint16,
    /// Maximum power of 2 less than or equal to numTables, times 16 ((2**floor(log2(numTables))) * 16, where “**” is an exponentiation operator).
    pub search_range: Uint16,
    /// Log2 of the maximum power of 2 less than or equal to numTables (log2(searchRange/16), which is equal to floor(log2(numTables))).
    pub entry_selector: Uint16,
    /// numTables times 16, minus searchRange ((numTables * 16) - searchRange).
    pub range_shift: Uint16,
}

/// A single record in the SFNT table directory.
// PORT NOTE: the reference `TableRecord` is an `extern struct` with `align(1)`
// big-endian fields; hoisted here from inside `Directory` and parsed explicitly.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TableRecord {
    /// Table identifier.
    pub tag: Tag,
    /// Checksum for this table.
    pub checksum: Uint32,
    /// Offset from beginning of font file.
    pub offset: Offset32,
    /// Length of this table.
    pub length: Uint32,
}

/// Errors that can be returned from [`Sfnt::init`].
// PORT NOTE: the reference `init` returns `!SFNT` whose error set is
// `error{EndOfStream}` (from `readNoEof`) unioned with `Allocator.Error`. The
// Rust port owns its records via `Vec`, so only the stream-underflow case remains.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InitError {
    /// Ran out of bytes before a full structure could be read.
    EndOfStream,
}

impl<'a> Sfnt<'a> {
    /// Parse a font from raw data. The struct will keep a
    /// reference to `data` and use it for future operations.
    // PORT NOTE: the reference signature is
    //   pub fn init(data: []const u8, alloc: Allocator) !SFNT
    // The allocator is dropped (records are owned by a `Vec`). The reference uses
    // `std.io.fixedBufferStream` + `readNoEof` + (on little-endian)
    // `byteSwapAllFields`; the SFNT format is big endian, so we read the fields
    // big-endian directly off `data`, which is equivalent on every host.
    pub fn init(data: &'a [u8]) -> Result<Sfnt<'a>, InitError> {
        let mut pos: usize = 0;

        // SFNT files use big endian, if our native endian is
        // not big we'll need to byte swap the values we read.
        //
        // PORT NOTE: the reference computes `byte_swap = native_endian != .big`
        // and conditionally swaps after a raw read. Reading each big-endian
        // field directly (below) yields the host-native value on every endian,
        // so no separate swap step is needed.

        let offset = OffsetSubtable {
            sfnt_version: read_be_u32(data, &mut pos)?,
            num_tables: read_be_u16(data, &mut pos)?,
            search_range: read_be_u16(data, &mut pos)?,
            entry_selector: read_be_u16(data, &mut pos)?,
            range_shift: read_be_u16(data, &mut pos)?,
        };

        let mut records: Vec<TableRecord> = Vec::with_capacity(offset.num_tables as usize);

        for _ in 0..offset.num_tables {
            let record = TableRecord {
                tag: read_tag(data, &mut pos)?,
                checksum: read_be_u32(data, &mut pos)?,
                offset: read_be_u32(data, &mut pos)?,
                length: read_be_u32(data, &mut pos)?,
            };
            records.push(record);
        }

        Ok(Sfnt {
            directory: Directory { offset, records },
            data,
        })
    }

    // PORT NOTE: the reference `pub fn deinit(self: SFNT, alloc: Allocator) void`
    // frees `self.directory.records`. In the Rust port the records are owned by
    // a `Vec`, freed automatically on drop, so there is no `deinit`.

    /// Returns the bytes of the table with the provided tag if present.
    pub fn get_table(&self, tag: &[u8; 4]) -> Option<&'a [u8]> {
        for record in &self.directory.records {
            if tag == &record.tag {
                let start = record.offset as usize;
                return Some(&self.data[start..start + record.length as usize]);
            }
        }

        None
    }
}

/// Read a big-endian `u16` at `*pos`, advancing it.
// PORT NOTE: stands in for `reader.readInt(uint16, .big)`/`readNoEof` of the
// big-endian on-disk fields; returns `EndOfStream` when the buffer underflows.
fn read_be_u16(data: &[u8], pos: &mut usize) -> Result<u16, InitError> {
    let bytes: [u8; 2] = data
        .get(*pos..*pos + 2)
        .ok_or(InitError::EndOfStream)?
        .try_into()
        .unwrap();
    *pos += 2;
    Ok(u16::from_be_bytes(bytes))
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

/// Read a 4-byte [`Tag`] at `*pos`, advancing it.
fn read_tag(data: &[u8], pos: &mut usize) -> Result<Tag, InitError> {
    let bytes: [u8; 4] = data
        .get(*pos..*pos + 4)
        .ok_or(InitError::EndOfStream)?
        .try_into()
        .unwrap();
    *pos += 4;
    Ok(bytes)
}

// PORT NOTE: the reference defines
//   const native_endian = @import("builtin").target.cpu.arch.endian();
// used only to decide whether to byte-swap after a raw read. The Rust port reads
// big-endian fields directly, so this declaration has no port-side equivalent.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::embedded;
    use crate::options;

    // test "parse font"
    #[test]
    fn parse_font() {
        // lib-vt source archives intentionally exclude full Ghostty font fixtures.
        if options::ARTIFACT == options::Artifact::Lib {
            return;
        }

        let test_font = embedded::JULIA_MONO;

        let sfnt = Sfnt::init(test_font).unwrap();

        assert_eq!(19, sfnt.directory.offset.num_tables);
        assert_eq!(b"prep", &sfnt.directory.records[18].tag);
    }

    // test "get table"
    #[test]
    fn get_table() {
        // lib-vt source archives intentionally exclude full Ghostty font fixtures.
        if options::ARTIFACT == options::Artifact::Lib {
            return;
        }

        let test_font = embedded::JULIA_MONO;

        let sfnt = Sfnt::init(test_font).unwrap();

        let svg = sfnt.get_table(b"SVG ").unwrap();

        assert_eq!(430, svg.len());
    }
}
