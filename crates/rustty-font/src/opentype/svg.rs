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
        //
        // PORT NOTE: reference defect (R13), UPSTREAM-DEFECTS.md UD92. The
        // reference's `records` has no length, so a font's `SVG ` table can make
        // `svg.zig:43-44` read past the table: `records[0]` (and `records[len -
        // 1]`) when fewer than 12 bytes per record remain, `len - 1` wrapping to
        // 65535 when `numEntries` is 0 (ReleaseSafe: "integer overflow"), and
        // `hasGlyph`'s binary search over a count the table does not contain.
        // Both guards here return the reference's own `EndOfStream`, the error
        // `init` already returns for a short table (LEDGER S92-F2 / S92-F3).
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

    // test "SVG document list offset past end"
    //
    // VACUOUS-CONTROL WARNING (P3-W8 T4, recorded after actually running the
    // control): deleting the `if offset > data.len()` guard that `b988efcfe`
    // added does **NOT** make this test fail in rustty. It was run and observed
    // to still pass. The reason is structural: the reference sets
    // `reader.seek = offset` and only faults later, whereas rustty's
    // `read_be_u16`/`read_be_u32` use `slice::get(pos..pos + n)`, which already
    // returns `None` — hence `InitError::EndOfStream` — for **any** `pos` past
    // the end. The ported guard is therefore R1-faithful but observably
    // redundant in rustty, and this test pins the *behaviour* (which is what
    // upstream's test pins too), NOT that specific line. Do not read a green
    // run here as evidence that the guard line exists.
    //
    // The sibling `svg_document_list_record_count_past_end` below is NOT
    // vacuous: removing its guard was run and observed to fail with
    // `left: Some(EndOfStream)` / `right: None`.
    #[test]
    fn svg_document_list_offset_past_end() {
        let table = [
            0, 0, // Version
            0, 0, 0, 7, // Document list offset, one byte past end
        ];

        assert_eq!(Some(InitError::EndOfStream), SVG::init(&table).err());
    }

    // PORT NOTE (P3-W8 T4): rustty's guard is strictly STRONGER than the
    // reference's. `b988efcfe` only added the document-list *offset* bounds
    // check ported above; rustty additionally rejects a record *count* that
    // overruns the table (`len > (data.len() - pos) / 12`, documented as R6
    // UB-avoidance at the call site), because rustty materialises the records
    // as a real `&[[u8; 12]]` slice where the reference keeps a many-item
    // pointer and would only fault on a later out-of-bounds read. Upstream has
    // no test for this arm; this one pins it so the stronger behaviour cannot
    // be silently relaxed back to the reference's.
    #[test]
    fn svg_document_list_record_count_past_end() {
        let table = [
            0, 0, // Version
            0, 0, 0, 6, // Document list offset: the very next byte
            0, 2, // numEntries = 2, but only 0 bytes of records follow
        ];

        assert_eq!(Some(InitError::EndOfStream), SVG::init(&table).err());
    }

    // UD92 (R13, task R13-D): `numEntries == 0`. The reference reads
    // `records[0]` past the end of this table, then computes `len - 1` on a
    // `u16` 0 (ReleaseSafe traps "integer overflow"; ReleaseFast reads
    // `records[65535]`, about 786 KiB past it). The port returns `EndOfStream`.
    // The positive control is the same table with one real record.
    #[test]
    fn svg_document_list_with_no_records_is_end_of_stream() {
        let empty = [
            0, 0, // Version
            0, 0, 0, 6, // Document list offset: the very next byte
            0, 0, // numEntries = 0
        ];
        assert_eq!(Some(InitError::EndOfStream), SVG::init(&empty).err());

        let one = [
            0, 0, // Version
            0, 0, 0, 6, // Document list offset
            0, 1, // numEntries = 1
            0, 5, 0, 9, // startGlyphID 5, endGlyphID 9
            0, 0, 0, 0, 0, 0, 0, 0, // svgDocOffset, svgDocLength
        ];
        let svg = SVG::init(&one).expect("one record");
        assert_eq!((5, 9), (svg.start_glyph_id, svg.end_glyph_id));
        assert!(svg.has_glyph(7));
        assert!(!svg.has_glyph(10));
    }
}
