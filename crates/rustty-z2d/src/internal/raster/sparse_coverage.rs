// SPDX-License-Identifier: MPL-2.0
//   Copyright © 2024-2025 Chris Marchesi
//   Copyright 2006 The Android Open Source Project
//   Copyright 2020 Yevhenii Reizner
//
// Ported to Rust for rustty from z2d 0.10.0
// `src/internal/raster/sparse_coverage.zig` (R3 vendored dep).
//
// Portions of the code in this file have been derived and adapted from the
// tiny-skia project (https://github.com/linebender/tiny-skia), notably
// alpha_runs.rs.

//! A structure for representing run-length-encoded coverage for a single
//! scanline. This facilities anti-aliasing by allowing the recording of
//! coverage spans over single or multiple calls. In memory, this is stored
//! as two sparse buffers; one for the coverage as a []u8, and the other for
//! lengths whose underlying type is dependent on the total capacity; most
//! notably, this means that the coverage and length storage combined will only
//! use a maximum of 510 bytes of memory for scanlines of less than 256 pixels.
//!
//! Calculating actual coverage to add or set is an exercise local to the
//! specific rasterizer, i.e., no calculations for scale, etc, are not done
//! here.
//!
//! An example of how coverage is recorded is discussed here, in the MSAA
//! context (see rasterizer/multiplesample.zig for more details):
//!
//! To use the buffer when performing MSAA composition, spans are recorded in
//! groups of `scale` in super-sampled space (appropriately offset to fully
//! take advantage of the above space optimization for span lengths). After
//! recording the full set of sub-scanlines, values are written out by
//! x-coordinates in device space starting at x=0, and incrementing on the
//! length returned by `get`. As the `value` is the total coverage in pixels,
//! make sure to take the set count and calculate the actual opacity as a
//! multiple of `256 / (scale * scale)`, subtracting 1 and clamping afterwards
//! to give the 0-255 range.
//!
//! Consider also fast-pathing when the coverage value is either 0 or the
//! maximum (i.e., `scale * scale`).
//!
//! Note that in an effort to keep this hot-path code fast, it's expected that
//! any safety guarantees, e.g., bounding x-coordinates, clamping opacity
//! values, ensuring pixels are not counted more than once, are provided by the
//! caller.

/// Run-length storage for span lengths, sized by the buffer capacity. Faithful
/// to Zig's `LengthStorage` union whose active member is chosen at `init` by the
/// capacity (`u8` < 256, `u16` < 65536, else `u32`).
#[derive(Debug)]
pub enum LengthStorage {
    U8(Vec<u8>),
    U16(Vec<u16>),
    U32(Vec<u32>),
}

impl LengthStorage {
    fn set(&mut self, x: usize, len: u32) {
        match self {
            LengthStorage::U8(v) => v[x] = len as u8,
            LengthStorage::U16(v) => v[x] = len as u16,
            LengthStorage::U32(v) => v[x] = len,
        }
    }

    fn get(&self, x: usize) -> u32 {
        match self {
            LengthStorage::U8(v) => v[x] as u32,
            LengthStorage::U16(v) => v[x] as u32,
            LengthStorage::U32(v) => v[x],
        }
    }

    /// The active member's tag name, matching Zig's `@tagName(lengths)`.
    #[cfg(test)]
    fn tag_name(&self) -> &'static str {
        match self {
            LengthStorage::U8(_) => "u8",
            LengthStorage::U16(_) => "u16",
            LengthStorage::U32(_) => "u32",
        }
    }
}

pub struct SparseCoverageBuffer {
    pub values: Vec<u8>,
    pub lengths: LengthStorage,
    pub len: u32,
    pub capacity: u32,
}

impl SparseCoverageBuffer {
    // PORT NOTE: the Zig `init` allocates `capacity`-sized backing arrays that
    // are left `undefined` except index 0 (set to 0). The Rust port zero-fills
    // both arrays (safe; the run-length walk only ever reads positions written
    // by `extend`/`put`), giving the same observable behavior.
    pub fn init(capacity: u32) -> SparseCoverageBuffer {
        let cap = capacity as usize;
        let values = vec![0u8; cap];
        let lengths: LengthStorage = if capacity < 256 {
            LengthStorage::U8(vec![0u8; cap])
        } else if capacity < 65536 {
            LengthStorage::U16(vec![0u16; cap])
        } else {
            LengthStorage::U32(vec![0u32; cap])
        };

        SparseCoverageBuffer {
            values,
            lengths,
            len: 0,
            capacity,
        }
    }

    pub fn reset(&mut self) {
        self.len = 0;
        self.lengths.set(0, 0);
    }

    pub fn add_span(&mut self, x: u32, value: u8, len: u32) {
        self.extend(x, len);
        let mut x_cur: u32 = x;
        let x_end: u32 = x + len;
        while x_cur < x_end {
            let (coverage_value, coverage_len) = self.get(x_cur);
            let coverage_value = coverage_value + value;
            self.put_value(x_cur, coverage_value);
            x_cur += coverage_len;
        }
    }

    pub fn add_single(&mut self, x: u32, value: u8) {
        self.extend(x, 1);
        let (coverage_value, _) = self.get(x);
        let coverage_value = coverage_value + value;
        self.put_value(x, coverage_value);
    }

    /// Extends the buffer depending on the specified (x, len).
    ///
    /// When neither x nor len are out of range, extends the buffer by adding a
    /// split span at the head or tail of the union of all the spans in (x, x +
    /// len).
    ///
    /// When len is out of range, but x is not, extends the buffer by adding a
    /// unique span at the end of the buffer, and then splitting the buffer as
    /// per the previous case (i.e., the new (x, x + len) range will encompass
    /// the union of all spans back from the end, appropriately split, and the
    /// new span at the end).
    ///
    /// When x is out of range, extends the buffer by adding two spans: one
    /// from the end to x, and then from x to x + len.
    fn extend(&mut self, x: u32, len: u32) {
        if len == 0 {
            return;
        }

        // Check for situations where our x is fully out of range.
        if x == self.len {
            // Right on the range boundary, just need to add one span at the end.
            self.put(x, 0, len);
            self.len = x + len;
            return;
        }

        if x > self.len {
            // Past the range boundary, so we need to add a span for the
            // difference before adding the actual span (similar to a wholly
            // new entry).
            self.put(self.len, 0, x - self.len);
            self.put(x, 0, len);
            self.len = x + len;
            return;
        }

        // Split from the front first, checking any runs necessary to
        // insert the key with an appropriate new sparse length.
        self.split_inner(0, x);

        // Now we can split forward from our length to any remainders as
        // needed.
        //
        // First, check our span-to-add against the existing length.
        let span_len: u32 = x + len;
        if span_len > self.len {
            // We need to extend past the existing length, so we add the empty
            // span first before we split any necessary remainder.
            //
            // NOTE: extends for out-of-range indexes are handled higher up
            // (similar to empty sets).
            self.put(self.len, 0, span_len - self.len);
            self.len = span_len;
        }

        self.split_inner(x, len);
    }

    fn split_inner(&mut self, x: u32, len: u32) {
        let mut idx = x;
        let mut rem = len;
        loop {
            let (current_value, current_len) = self.get(idx);
            if rem < current_len {
                self.put(idx, current_value, rem);
                self.put(idx + rem, current_value, current_len - rem);
                break;
            } else if rem == current_len {
                break;
            }

            rem -= current_len;
            idx += current_len;
        }
    }

    pub fn get(&self, x: u32) -> (u8, u32) {
        (self.values[x as usize], self.lengths.get(x as usize))
    }

    fn put(&mut self, x: u32, value: u8, len: u32) {
        debug_assert!(x + len <= self.capacity);
        self.values[x as usize] = value;
        self.lengths.set(x as usize, len);
    }

    fn put_value(&mut self, x: u32, value: u8) {
        self.values[x as usize] = value;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extend_basic() {
        let mut coverage = SparseCoverageBuffer::init(10);
        coverage.put(0, 0, 4);
        coverage.put(4, 0, 4);
        coverage.len = 8;
        coverage.extend(2, 5);
        assert_eq!(8, coverage.len);
        assert_eq!((0, 2), coverage.get(0));
        assert_eq!((0, 2), coverage.get(2));
        assert_eq!((0, 3), coverage.get(4));
        assert_eq!((0, 1), coverage.get(7));
    }

    #[test]
    fn extend_new_buffer_zero_indexed() {
        let mut coverage = SparseCoverageBuffer::init(10);
        coverage.extend(0, 5);
        assert_eq!(5, coverage.len);
        assert_eq!((0, 5), coverage.get(0));
    }

    #[test]
    fn extend_new_buffer_non_zero() {
        let mut coverage = SparseCoverageBuffer::init(10);
        coverage.extend(2, 5);
        assert_eq!(7, coverage.len);
        assert_eq!((0, 2), coverage.get(0));
        assert_eq!((0, 5), coverage.get(2));
    }

    #[test]
    fn extend_split_closer_to_end_no_extend() {
        let mut coverage = SparseCoverageBuffer::init(10);
        coverage.put(0, 0, 4);
        coverage.put(4, 0, 4);
        coverage.len = 8;
        coverage.extend(7, 1);
        assert_eq!(8, coverage.len);
        assert_eq!((0, 4), coverage.get(0));
        assert_eq!((0, 3), coverage.get(4));
        assert_eq!((0, 1), coverage.get(7));
    }

    #[test]
    fn extend_split_closer_to_end_with_extend() {
        let mut coverage = SparseCoverageBuffer::init(10);
        coverage.put(0, 0, 4);
        coverage.put(4, 0, 4);
        coverage.len = 8;
        coverage.extend(7, 3);
        assert_eq!(10, coverage.len);
        assert_eq!((0, 4), coverage.get(0));
        assert_eq!((0, 3), coverage.get(4));
        assert_eq!((0, 1), coverage.get(7));
        assert_eq!((0, 2), coverage.get(8));
    }

    #[test]
    fn extend_append_right_after_end() {
        let mut coverage = SparseCoverageBuffer::init(10);
        coverage.put(0, 0, 4);
        coverage.put(4, 0, 4);
        coverage.len = 8;
        coverage.extend(8, 2);
        assert_eq!(10, coverage.len);
        assert_eq!((0, 4), coverage.get(0));
        assert_eq!((0, 4), coverage.get(4));
        assert_eq!((0, 2), coverage.get(8));
    }

    #[test]
    fn extend_put_past_end() {
        let mut coverage = SparseCoverageBuffer::init(11);
        coverage.put(0, 0, 4);
        coverage.put(4, 0, 4);
        coverage.len = 8;
        coverage.extend(9, 2);
        assert_eq!(11, coverage.len);
        assert_eq!((0, 4), coverage.get(0));
        assert_eq!((0, 4), coverage.get(4));
        assert_eq!((0, 1), coverage.get(8));
        assert_eq!((0, 2), coverage.get(9));
    }

    #[test]
    fn extend_zero_len() {
        let mut coverage = SparseCoverageBuffer::init(10);
        coverage.extend(0, 0);
        let (_, got_len) = coverage.get(0);
        assert_eq!(0, got_len);
    }

    #[test]
    fn extend_split_up_to_exactly_capacity() {
        let mut coverage = SparseCoverageBuffer::init(255);
        coverage.extend(192, 63);

        let (got_val, got_len) = coverage.get(0);
        assert_eq!(0, got_val);
        assert_eq!(192, got_len);
        let (got_val, got_len) = coverage.get(192);
        assert_eq!(0, got_val);
        assert_eq!(63, got_len);

        let mut idx: u32 = 0;
        let mut tracked_span_len: usize = 0;
        while idx < coverage.len {
            let (_, x_inc) = coverage.get(idx);
            idx += x_inc;
            tracked_span_len += 1;
        }
        assert_eq!(2, tracked_span_len);
    }

    #[test]
    fn length_storage_tag_by_capacity() {
        assert_eq!("u8", SparseCoverageBuffer::init(255).lengths.tag_name());
        assert_eq!("u16", SparseCoverageBuffer::init(256).lengths.tag_name());
        assert_eq!("u16", SparseCoverageBuffer::init(65535).lengths.tag_name());
        assert_eq!("u32", SparseCoverageBuffer::init(65536).lengths.tag_name());
    }
}
