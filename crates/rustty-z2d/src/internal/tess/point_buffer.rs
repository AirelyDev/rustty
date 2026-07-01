// SPDX-License-Identifier: MPL-2.0
//   Copyright © 2024-2025 Chris Marchesi
//
// Ported to Rust for rustty from z2d 0.10.0 `src/internal/tess/point_buffer.zig`
// (R3 vendored dep).

use crate::internal::point::Point;

/// A buffer of points. When empty, the buffer fills up to the maximum, and then
/// rotates the elements according to the value of `SPLIT`; the number of
/// elements equal to the split are kept, and the rest are swapped in a FIFO
/// basis.
///
/// This is applied in fill and stroke: in fill, it stores the initial point
/// along with the last two points, in stroke, it stores enough data to
/// reconstruct the first join, if need be.
///
/// All helpers that return a point will return `None` if the respective index
/// is invalid.
#[derive(Debug, Clone, Copy)]
pub struct PointBuffer<const SPLIT: usize, const BUFFER_LEN: usize> {
    items: [Point; BUFFER_LEN],
    pub len: usize,
}

impl<const SPLIT: usize, const BUFFER_LEN: usize> Default for PointBuffer<SPLIT, BUFFER_LEN> {
    fn default() -> Self {
        PointBuffer {
            items: [Point { x: 0.0, y: 0.0 }; BUFFER_LEN],
            len: 0,
        }
    }
}

impl<const SPLIT: usize, const BUFFER_LEN: usize> PointBuffer<SPLIT, BUFFER_LEN> {
    pub fn add(&mut self, item: Point) {
        if self.len < BUFFER_LEN {
            self.items[self.len] = item;
            self.len += 1;
        } else {
            for idx in SPLIT..BUFFER_LEN - 1 {
                self.items[idx] = self.items[idx + 1];
            }
            self.items[BUFFER_LEN - 1] = item;
        }
    }

    pub fn reset(&mut self) {
        self.len = 0;
    }

    /// Returns the start of the buffer with the offset in `n` applied, as
    /// `self.items[n]`.
    pub fn head(&self, n: usize) -> Option<Point> {
        if n >= self.len {
            return None;
        }
        Some(self.items[n])
    }

    /// Returns the end of the buffer with the offset in `n` applied, as
    /// `self.items[self.len - n]`.
    pub fn tail(&self, n: usize) -> Option<Point> {
        if n == 0 {
            panic!("invalid tail index");
        }
        if self.len < n {
            return None;
        }
        Some(self.items[self.len - n])
    }

    pub fn last(&self) -> Option<Point> {
        if self.len == 0 {
            return None;
        }
        Some(self.items[self.len - 1])
    }

    pub fn first(&self) -> Option<Point> {
        if self.len == 0 {
            return None;
        }
        Some(self.items[0])
    }
}
