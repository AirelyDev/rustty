// SPDX-License-Identifier: MPL-2.0
//   Copyright © 2024-2025 Chris Marchesi
//   Copyright © 2002 University of Southern California
//
// Portions of the code in this file have been derived and adapted from the
// Cairo project (https://www.cairographics.org/), notably cairo-pen.c.
//
// Ported to Rust for rustty from z2d 0.10.0 `src/internal/tess/Pen.zig`
// (R3 vendored dep).

//! A Pen represents a circular area designed for specific stroking operations,
//! such as round joins and caps.

use std::f64::consts::PI;

use crate::internal::arc;
use crate::internal::point::Point;
use crate::internal::tess::slope::Slope;
use crate::transformation::Transformation;

#[derive(Debug, Clone, Copy)]
pub struct PenVertex {
    pub point: Point,
    pub slope_cw: Slope,
    pub slope_ccw: Slope,
}

/// The vertices, centered around (0,0) and distributed on even angles
/// around the pen.
#[derive(Debug, Clone)]
pub struct Pen {
    pub vertices: Vec<PenVertex>,
}

impl Pen {
    /// Initializes a pen at radius thickness / 2, with point distribution
    /// based on the maximum error along the radius, being equal to or less
    /// than tolerance.
    pub fn init(thickness: f64, tolerance: f64, ctm: Transformation) -> Pen {
        // You can find the proof for our calculation here in cairo-pen.c in the
        // Cairo project. It shows that ultimately, the maximum error of an ellipse
        // is along its major axis, and to get our needed number of vertices, we
        // can calculate the following:
        //
        // ceil(2 * Π / acos(1 - tolerance / M))
        //
        // Where M is the major axis.
        let radius = thickness / 2.0;
        let num_vertices: i32 = {
            let major_axis: f64 = arc::transformed_circle_major_axis(ctm, radius);
            // Note that our minimum number of vertices is always 4. There are
            // also situations where our tolerance may be so high that we'd
            // have a degenerate pen, so we just return 1 in that case.
            if tolerance >= major_axis * 4.0 {
                // Degenerate pen when our tolerance is higher than what would
                // be represented by the circle itself.
                1
            } else if tolerance >= major_axis {
                // Not degenerate, but can fast-path here as the tolerance is
                // so high we are going to need to represent it with the
                // minimum points anyway.
                4
            } else {
                // Calculate our delta first just in case we fall on zero for some
                // reason, and break on the minimum if it is.
                let delta = (1.0 - tolerance / major_axis).acos();
                if delta == 0.0 {
                    4
                } else {
                    // Regular calculation can be done now
                    let n: i32 = (2.0 * PI / delta).ceil() as i32;
                    if n < 4 {
                        // Below minimum
                        4
                    } else if n % 2 != 0 {
                        // Add a point for uneven vertex counts
                        n + 1
                    } else {
                        n
                    }
                }
            }
        };

        // We can now initialize and plot our vertices
        let count = num_vertices.max(0) as usize;
        let mut vertices: Vec<PenVertex> = Vec::with_capacity(count);

        // Add the points in a first pass. Note our baseline for determining points
        // is user space (as we're just plotting a circle, so we need to transform
        // off our ctm to get the correct ellipse for the pen.
        let reflect = ctm.determinant() < 0.0;
        for i in 0..count {
            let theta: f64 = {
                let mut t = 2.0 * PI * (i as f64) / (num_vertices as f64);
                if reflect {
                    t = -t;
                }
                t
            };
            let mut dx = radius * theta.cos();
            let mut dy = radius * theta.sin();
            ctm.user_to_device_distance(&mut dx, &mut dy);
            vertices.push(PenVertex {
                point: Point { x: dx, y: dy },
                slope_cw: Slope { dx: 0.0, dy: 0.0 },
                slope_ccw: Slope { dx: 0.0, dy: 0.0 },
            });
        }

        // Add the slopes in a separate pass so that we can add them relative
        // to the vertices surrounding it.
        for i in 0..count {
            let next: usize = if i >= (num_vertices - 1) as usize {
                0
            } else {
                i + 1
            };
            let prev: usize = (if i == 0 {
                num_vertices - 1
            } else {
                i as i32 - 1
            })
            .max(0) as usize;
            vertices[i].slope_cw = Slope::init(vertices[prev].point, vertices[i].point);
            vertices[i].slope_ccw = Slope::init(vertices[i].point, vertices[next].point);
        }

        Pen { vertices }
    }

    /// Returns an iterator for the vertex range from one face to the other,
    /// depending on the line direction.
    pub fn vertex_iterator_for(
        &self,
        from_slope: Slope,
        to_slope: Slope,
        clockwise: bool,
    ) -> VertexIterator<'_> {
        // The algorithm is basically a binary search back from the middle of
        // the vertex set. We search backwards for the vertex right after the
        // outer point of the end of the inbound face (i.e., the unjoined
        // stroke). This process is then repeated for the other direction to
        // locate the vertex right before the outer point of the start of the
        // outbound face.

        // Check the direction of the join so that we can return the
        // appropriate vertices in the correct order.
        let start: i32;
        let end: i32;
        let vertices_len: i32 = self.vertices.len() as i32;
        if clockwise {
            // Clockwise join
            let mut low: i32 = 0;
            let mut high: i32 = vertices_len;
            let mut i: i32 = (low + high) >> 1;
            while high - low > 1 {
                if Slope::compare(self.vertices[i as usize].slope_cw, from_slope) < 0 {
                    low = i;
                } else {
                    high = i;
                }
                i = (low + high) >> 1;
            }

            if Slope::compare(self.vertices[i as usize].slope_cw, from_slope) < 0 {
                i += 1;
                if i == vertices_len {
                    i = 0;
                }
            }
            start = i;

            if Slope::compare(to_slope, self.vertices[i as usize].slope_ccw) >= 0 {
                low = i;
                high = i + vertices_len;
                i = (low + high) >> 1;
                while high - low > 1 {
                    let j: i32 = if i >= vertices_len {
                        i - vertices_len
                    } else {
                        i
                    };
                    if Slope::compare(self.vertices[j as usize].slope_cw, to_slope) > 0 {
                        high = i;
                    } else {
                        low = i;
                    }
                    i = (low + high) >> 1;
                }

                if i >= vertices_len {
                    i -= vertices_len;
                }
            }

            end = i;
        } else {
            // Counter-clockwise join
            let mut low: i32 = 0;
            let mut high: i32 = vertices_len;
            let mut i: i32 = (low + high) >> 1;
            while high - low > 1 {
                if Slope::compare(from_slope, self.vertices[i as usize].slope_ccw) < 0 {
                    low = i;
                } else {
                    high = i;
                }
                i = (low + high) >> 1;
            }

            if Slope::compare(from_slope, self.vertices[i as usize].slope_ccw) < 0 {
                i += 1;
                if i == vertices_len {
                    i = 0;
                }
            }
            start = i;

            if Slope::compare(self.vertices[i as usize].slope_cw, to_slope) <= 0 {
                low = i;
                high = i + vertices_len;
                i = (low + high) >> 1;
                while high - low > 1 {
                    let j: i32 = if i >= vertices_len {
                        i - vertices_len
                    } else {
                        i
                    };
                    if Slope::compare(to_slope, self.vertices[j as usize].slope_ccw) > 0 {
                        high = i;
                    } else {
                        low = i;
                    }
                    i = (low + high) >> 1;
                }

                if i >= vertices_len {
                    i -= vertices_len;
                }
            }

            end = i;
        }

        VertexIterator {
            pen: self,
            end: end.max(0) as usize,
            idx: start.max(0) as usize,
            clockwise,
        }
    }
}

pub struct VertexIterator<'a> {
    pen: &'a Pen,
    end: usize,
    idx: usize,
    clockwise: bool,
}

impl Iterator for VertexIterator<'_> {
    type Item = PenVertex;

    fn next(&mut self) -> Option<PenVertex> {
        if self.idx == self.end {
            return None;
        }
        if self.clockwise {
            let result = self.pen.vertices[self.idx];
            self.idx += 1;
            if self.idx == self.pen.vertices.len() {
                self.idx = 0;
            }
            Some(result)
        } else {
            let result = self.pen.vertices[self.idx];
            if self.idx == 0 {
                self.idx = self.pen.vertices.len();
            }
            self.idx -= 1;
            Some(result)
        }
    }
}
