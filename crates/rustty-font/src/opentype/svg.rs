//! rustty-font opentype svg
//!
//! PORTS: ghostty src/font/opentype/svg.zig
//!
//! PORT STATUS: done

// PORT NOTE: the reference imports are
//   const std = @import("std");
//   const font = @import("../main.zig");
// `font` is only used by the reference test (to load a `Face` and `copyTable`);
// the rustty test parses the raw `SVG ` table bytes directly off `sfnt` (the pure
// leaf has no `Face`), so no `font` import is needed here.
use std::cmp::Ordering;

/// SVG glyphs description table.
///
/// This struct is focused purely on the operations we need for Ghostty,
/// namely to be able to look up whether an glyph ID is present in the SVG
/// table or not. This struct isn't meant to be a general purpose SVG table
/// reader.
///
/// References:
/// - https://www.w3.org/2013/10/SVG_in_OpenType/#thesvg
/// - https://learn.microsoft.com/en-us/typography/opentype/spec/svg
// PORT NOTE: the reference `records: []const [12]u8` borrows into the table bytes;
// modeled here as `records: &'a [[u8; 12]]` with a lifetime tying the view to the
// source data slice.
#[derive(Clone, Copy, Debug)]
pub struct SVG<'a> {
    /// The start and end glyph IDs (inclusive) that are present in the
    /// table. This is used to very quickly include/exclude a glyph from
    /// the table.
    pub start_glyph_id: u16,
    pub end_glyph_id: u16,

    /// All records in the table.
    pub records: &'a [[u8; 12]],
}

/// Error returned from [`SVG::init`].
// PORT NOTE: the reference `init` error set is
// `error{ EndOfStream, SVGVersionNotSupported }`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InitError {
    EndOfStream,
    SVGVersionNotSupported,
}

impl<'a> SVG<'a> {
    pub fn init(data: &'a [u8]) -> Result<SVG<'a>, InitError> {
        let mut pos: usize = 0;

        // Version
        if read_be_u16(data, &mut pos)? != 0 {
            return Err(InitError::SVGVersionNotSupported);
        }

        // Offset
        let offset = read_be_u32(data, &mut pos)? as usize;

        // Seek to the offset to get our document list
        if offset > data.len() {
            return Err(InitError::EndOfStream);
        }
        pos = offset;

        // Get our document records along with the start/end glyph range.
        let len = read_be_u16(data, &mut pos)? as usize;
        // PORT NOTE: the reference does `@ptrCast(data[getPos()..])` into a
        // many-item `[*]const [12]u8` then `records[0..len]`. Reproduced with a
        // bounds-checked raw slice: `[u8; 12]` has alignment 1 so the cast is
        // sound, and we reject `len` records that don't fit the remaining bytes
        // (an EndOfStream the reference would only hit on later OOB access) to
        // keep the port free of undefined behavior (R6).
        let avail = (data.len() - pos) / 12;
        if len > avail {
            return Err(InitError::EndOfStream);
        }
        let records: &'a [[u8; 12]] =
            unsafe { std::slice::from_raw_parts(data.as_ptr().add(pos).cast::<[u8; 12]>(), len) };
        if len == 0 {
            return Err(InitError::EndOfStream);
        }
        let start_range = glyph_range(&records[0])?;
        let end_range = if len == 1 {
            start_range
        } else {
            glyph_range(&records[len - 1])?
        };

        Ok(SVG {
            start_glyph_id: start_range.0,
            end_glyph_id: end_range.1,
            records,
        })
    }

    pub fn has_glyph(self, glyph_id: u16) -> bool {
        // Fast path: outside the table range
        if glyph_id < self.start_glyph_id || glyph_id > self.end_glyph_id {
            return false;
        }

        // Fast path, matches the start/end glyph IDs
        if glyph_id == self.start_glyph_id || glyph_id == self.end_glyph_id {
            return true;
        }

        // Slow path: binary search our records
        // PORT NOTE: `std.sort.binarySearch`'s comparator returns the order of the
        // *key* relative to each item; Rust's `binary_search_by` closure returns
        // the order of the *item* relative to the key, so the arms are mirrored
        // (see `compare_glyph_id`).
        self.records
            .binary_search_by(|record| compare_glyph_id(glyph_id, record))
            .is_ok()
    }
}

// PORT NOTE: the reference `compareGlyphId(glyph_id, record) Order` returns the
// order of `glyph_id` (the key) relative to the record; this returns the order of
// the record relative to the key (the shape `binary_search_by` wants), so each arm
// is inverted: reference `.lt` (key < item / error) → `Greater`, `.gt` → `Less`.
fn compare_glyph_id(glyph_id: u16, record: &[u8; 12]) -> Ordering {
    let (start, end) = match glyph_range(record) {
        Ok(v) => v,
        Err(_) => return Ordering::Greater,
    };
    if glyph_id < start {
        Ordering::Greater
    } else if glyph_id > end {
        Ordering::Less
    } else {
        Ordering::Equal
    }
}

fn glyph_range(record: &[u8]) -> Result<(u16, u16), InitError> {
    let mut pos: usize = 0;
    Ok((
        read_be_u16(record, &mut pos)?,
        read_be_u16(record, &mut pos)?,
    ))
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
    use crate::opentype::sfnt;

    // test "SVG"
    // PORT NOTE: the reference test loads a `Face` and `copyTable("SVG ")`; the
    // pure leaf reads the `SVG ` table straight off `sfnt` instead (same bytes),
    // avoiding a dependency on the shaper-tier `Face`.
    #[test]
    fn svg() {
        let test_font = crate::embedded::JULIA_MONO;

        let font = sfnt::Sfnt::init(test_font).unwrap();
        let table = font.get_table(b"SVG ").unwrap();

        let svg = SVG::init(table).unwrap();
        assert_eq!(11482, svg.start_glyph_id);
        assert_eq!(11482, svg.end_glyph_id);
        assert!(svg.has_glyph(11482));
    }
}
