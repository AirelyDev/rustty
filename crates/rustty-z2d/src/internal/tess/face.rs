// SPDX-License-Identifier: MPL-2.0
//   Copyright © 2024-2025 Chris Marchesi
//   Copyright © 2002 University of Southern California
//
// Portions of the code in this file (where mentioned) have been derived and
// adapted from the Cairo project (https://www.cairographics.org/), notably
// cairo-path-stroke-polygon.c.
//
// Ported to Rust for rustty from z2d 0.10.0 `src/internal/tess/Face.zig`
// (R3 vendored dep). The Zig `PlotterVTable` (a type-erased `line_to` callback)
// is expressed here as a `&mut dyn FnMut(Point)` sink; the cap methods emit the
// same sequence of points (equivalent behavior, no raw pointers — keeps the
// Miri gate clean).

//! A Face represents a hypothetically-computed polygon edge for a stroked
//! line.
//!
//! The face is computed from p0 -> p1 (see init). Interactions, such as
//! intersections, are specifically dictated by the orientation of any two
//! faces in relation to each other, when the faces are treated as segments
//! along the path, traveling in the same direction (e.g., p0 -> p1, p1 -> p2).
//!
//! For each face, its stroked endpoints, denoted by cw (clockwise) and ccw
//! (counter-clockwise) are taken by rotating a point 90 degrees in that
//! direction along the line, starting from p0 (or p1), to half of the line
//! thickness, in the same direction of the line (e.g., p0 -> p1).

use crate::internal::point::Point;
use crate::internal::tess::pen::Pen;
use crate::internal::tess::slope::Slope;
use crate::options::CapMode;
use crate::transformation::{Transformation, IDENTITY};

#[derive(Debug, Clone, Copy)]
pub struct Face {
    pub p0: Point,
    pub p1: Point,
    pub width: f64,
    pub dev_slope: Slope,  // Device-space slope (normalized)
    pub user_slope: Slope, // User-space slope (normalized)
    pub half_width: f64,   // Half-width of thickness on init
    pub p0_cw: Point,
    pub p0_ccw: Point,
    pub p1_cw: Point,
    pub p1_ccw: Point,
    pub ctm: Transformation,
}

impl Face {
    /// Computes a Face from two points in the direction of p0 -> p1.
    pub fn init(p0: Point, p1: Point, thickness: f64, ctm: Transformation) -> Face {
        let dev_slope = {
            let mut s = Slope::init(p0, p1);
            s.normalize();
            s
        };
        Face::_init(p0, p1, dev_slope, thickness, ctm)
    }

    /// Computes a face off a single point and (normalized) device slope.
    pub fn init_single(
        point: Point,
        dev_slope: Slope,
        thickness: f64,
        ctm: Transformation,
    ) -> Face {
        Face::_init(point, point, dev_slope, thickness, ctm)
    }

    /// Computes a Face from two points in the direction of p0 -> p1.
    fn _init(p0: Point, p1: Point, dev_slope: Slope, thickness: f64, ctm: Transformation) -> Face {
        let half_width = thickness / 2.0;
        let offset_x: f64;
        let offset_y: f64;
        let mut user_slope = dev_slope;
        if !ctm.equal(IDENTITY) {
            // If we're transforming we need to transform our offsets for purposes
            // of correctly plotting joins and end cap points. Direction is mostly
            // already accounted for as our path is already assumed to be in device
            // space, but we need to warp our thickness and possibly reflect if the
            // ctm does that too.
            let mut dx = dev_slope.dx;
            let mut dy = dev_slope.dy;
            ctm.device_to_user_distance(&mut dx, &mut dy).unwrap(); // ctm is pre-validated
                                                                    // Save user slope for future calcs
            user_slope = {
                let mut s = Slope { dx, dy };
                s.normalize();
                s
            };
            let (mut ox, mut oy);
            if ctm.determinant() >= 0.0 {
                ox = -user_slope.dy * half_width;
                oy = user_slope.dx * half_width;
            } else {
                ox = user_slope.dy * half_width;
                oy = -user_slope.dx * half_width;
            }
            ctm.user_to_device_distance(&mut ox, &mut oy);
            offset_x = ox;
            offset_y = oy;
        } else {
            offset_x = -dev_slope.dy * half_width;
            offset_y = dev_slope.dx * half_width;
        }
        let offset_cw_x = offset_x;
        let offset_cw_y = offset_y;
        let offset_ccw_x = -offset_cw_x;
        let offset_ccw_y = -offset_cw_y;

        Face {
            p0,
            p1,
            width: thickness,
            dev_slope,
            user_slope,
            half_width,
            p0_cw: Point {
                x: p0.x + offset_cw_x,
                y: p0.y + offset_cw_y,
            },
            p0_ccw: Point {
                x: p0.x + offset_ccw_x,
                y: p0.y + offset_ccw_y,
            },
            p1_cw: Point {
                x: p1.x + offset_cw_x,
                y: p1.y + offset_cw_y,
            },
            p1_ccw: Point {
                x: p1.x + offset_ccw_x,
                y: p1.y + offset_ccw_y,
            },
            ctm,
        }
    }

    pub fn intersect(in_: Face, out: Face, clockwise: bool) -> Point {
        debug_assert!(in_.dev_slope.compare(out.dev_slope) != 0);

        // Intersection taken from Cairo's miter join in
        // cairo-path-stroke-polygon.c et al.
        let in_point = if clockwise { in_.p1_ccw } else { in_.p1_cw };
        let out_point = if clockwise { out.p0_ccw } else { out.p0_cw };
        // Normalize our slopes, if not done already.
        //
        // TODO: This can probably be taken out. We never *not* normalize slopes
        // anymore, so if this is particularly costly it probably be removed (in
        // favor of setting the expectation that we always expect normalized slopes
        // here).
        let in_slope = {
            let mut s = in_.dev_slope;
            s.normalize();
            s
        };
        let out_slope = {
            let mut s = out.dev_slope;
            s.normalize();
            s
        };

        let result_y = ((out_point.x - in_point.x) * in_slope.dy * out_slope.dy
            - out_point.y * out_slope.dx * in_slope.dy
            + in_point.y * in_slope.dx * out_slope.dy)
            / (in_slope.dx * out_slope.dy - out_slope.dx * in_slope.dy);

        let result_x = if in_slope.dy.abs() >= out_slope.dy.abs() {
            (result_y - in_point.y) * in_slope.dx / in_slope.dy + in_point.x
        } else {
            (result_y - out_point.y) * out_slope.dx / out_slope.dy + out_point.x
        };

        Point {
            x: result_x,
            y: result_y,
        }
    }

    pub fn cap_p0(
        &self,
        plot: &mut dyn FnMut(Point),
        cap_mode: CapMode,
        clockwise: bool,
        pen: Option<&Pen>,
    ) {
        let reversed = Face::init(self.p1, self.p0, self.width, self.ctm);
        reversed.cap(plot, cap_mode, clockwise, pen);
    }

    pub fn cap_p1(
        &self,
        plot: &mut dyn FnMut(Point),
        cap_mode: CapMode,
        clockwise: bool,
        pen: Option<&Pen>,
    ) {
        self.cap(plot, cap_mode, clockwise, pen);
    }

    fn cap(
        &self,
        plot: &mut dyn FnMut(Point),
        cap_mode: CapMode,
        clockwise: bool,
        pen: Option<&Pen>,
    ) {
        match cap_mode {
            CapMode::Butt => {
                self.cap_butt(plot, clockwise);
            }
            CapMode::Square => {
                self.cap_square(plot, clockwise);
            }
            CapMode::Round => {
                self.cap_round(plot, clockwise, pen);
            }
        }
    }

    fn cap_butt(&self, plot: &mut dyn FnMut(Point), clockwise: bool) {
        if clockwise {
            plot(self.p1_ccw);
            plot(self.p1_cw);
        } else {
            plot(self.p1_cw);
            plot(self.p1_ccw);
        }
    }

    fn cap_square(&self, plot: &mut dyn FnMut(Point), clockwise: bool) {
        let mut offset_x = self.user_slope.dx * self.half_width;
        let mut offset_y = self.user_slope.dy * self.half_width;
        self.ctm
            .user_to_device_distance(&mut offset_x, &mut offset_y);
        if clockwise {
            plot(self.p1_ccw);
            plot(Point {
                x: self.p1_ccw.x + offset_x,
                y: self.p1_ccw.y + offset_y,
            });
            plot(Point {
                x: self.p1_cw.x + offset_x,
                y: self.p1_cw.y + offset_y,
            });
            plot(self.p1_cw);
        } else {
            plot(self.p1_cw);
            plot(Point {
                x: self.p1_cw.x + offset_x,
                y: self.p1_cw.y + offset_y,
            });
            plot(Point {
                x: self.p1_ccw.x + offset_x,
                y: self.p1_ccw.y + offset_y,
            });
            plot(self.p1_ccw);
        }
    }

    fn cap_round(&self, plot: &mut dyn FnMut(Point), clockwise: bool, pen: Option<&Pen>) {
        // We need to calculate our fan along the end as if we were
        // dealing with a 180 degree joint. So, treat it as if there
        // were two lines going in exactly opposite directions, i.e., flip the
        // incoming slope for the outgoing one.
        debug_assert!(pen.is_some());
        let pen = pen.unwrap();
        let vit = pen.vertex_iterator_for(
            self.dev_slope,
            Slope {
                dx: -self.dev_slope.dx,
                dy: -self.dev_slope.dy,
            },
            clockwise,
        );
        if clockwise {
            plot(self.p1_ccw);
        } else {
            plot(self.p1_cw);
        }
        for v in vit {
            plot(Point {
                x: self.p1.x + v.point.x,
                y: self.p1.y + v.point.y,
            });
        }
        if clockwise {
            plot(self.p1_cw);
        } else {
            plot(self.p1_ccw);
        }
    }
}
