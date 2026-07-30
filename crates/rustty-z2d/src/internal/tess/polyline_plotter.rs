// SPDX-License-Identifier: MPL-2.0
//   Copyright © 2024-2025 Chris Marchesi
//
// Ported to Rust for rustty from z2d 0.10.0
// `src/internal/tess/polyline_plotter.zig` (R3 vendored dep).
//
// PORT NOTE (rustty): the dashed branch is ported — a validated dash array
// builds a `Dasher` and each `line_to`/`close_path`/decomposed-curve segment is
// walked in dash steps by `dashed_line_to` (appending a fresh contour to the
// result at each "on"→"off" transition). The spline `line_to` callback (Zig
// `SplinePlotterCtx`) is expressed as a `&mut dyn FnMut(PathLineTo)` sink over
// the borrowed contour / point-buffer / result / dasher — equivalent behavior,
// no raw pointers (keeps the Miri gate clean), exactly as `stroke_plotter` does
// for its spline decomposition.

//! A polyline plotter, converts a path to a Contour (not a Polygon).
//!
//! This is a variation of our fill plotter, as the logic is pretty much the
//! same, with the general exception of we need to keep less state (as we are
//! plotting a polyline, not edges), and the path can be open, not just
//! closed.

use crate::internal::error::InternalError;
use crate::internal::path_nodes::{PathLineTo, PathNode};
use crate::internal::point::Point;
use crate::internal::tess::dasher::{self, Dasher};
use crate::internal::tess::point_buffer::PointBuffer;
use crate::internal::tess::polygon::{Contour, ContourList};
use crate::internal::tess::slope::Slope;
use crate::internal::tess::spline::Spline;

/// Converts a path to a `ContourList` (open polylines), not a `Polygon`.
///
/// Port of `polyline_plotter.plot`. The `alloc` arg is dropped (the ported tess
/// layer is allocator-free, matching `stroke_plotter::plot`), and the Zig
/// `errdefer result.deinit(alloc)` becomes Rust `Drop`.
pub fn plot(
    nodes: &[PathNode],
    tolerance: f64,
    dashes: &[f64],
    dash_offset: f64,
) -> Result<ContourList, InternalError> {
    let mut result = ContourList::empty();
    let mut current_contour = Contour::new(1.0);

    let mut points: PointBuffer<1, 2> = PointBuffer::default();

    let mut dasher: Option<Dasher> = if dasher::validate(dashes) {
        Some(Dasher::init(dashes, dash_offset))
    } else {
        None
    };

    let n_nodes = nodes.len();
    for (i, node) in nodes.iter().enumerate() {
        match node {
            PathNode::MoveTo(n) => {
                // Add last contour if there were nodes in it
                if current_contour.len != 0 {
                    result.append(current_contour);
                    current_contour = Contour::new(1.0);
                }
                points.reset();
                if let Some(d) = &mut dasher {
                    d.reset();
                }
                // Check if this is the last node, and no-op if it is, as this
                // is the auto-added move_to node that is given after
                // close_path.
                if i == n_nodes - 1 {
                    break;
                }
                current_contour.plot(n.point, None);
                points.add(n.point);
            }
            PathNode::LineTo(n) => match points.last() {
                Some(last_point) => {
                    if !last_point.equal(n.point) {
                        if let Some(d) = &mut dasher {
                            dashed_line_to(
                                last_point,
                                n.point,
                                d,
                                &mut points,
                                &mut current_contour,
                                &mut result,
                            );
                        } else {
                            current_contour.plot(n.point, None);
                            points.add(n.point);
                        }
                    }
                }
                None => return Err(InternalError::InvalidState),
            },
            PathNode::CurveTo(n) => {
                if points.len == 0 {
                    return Err(InternalError::InvalidState);
                }
                // SplinePlotterCtx: the line_to sink walks each decomposed
                // segment (dashed, if a dasher is active), plotting to the
                // current contour + point buffer + result list.
                //
                // BORROW ORDER (load-bearing): read `spline.a` out of `points`
                // FIRST — `points.last()` yields a `Point` value (`Point` is
                // `Copy`) that lives independently of the buffer. Only AFTER `a`
                // is a detached value do we take the mutable captures for the
                // sink. This mirrors the Zig `points.last().?` (line 96) being
                // read before the `SplinePlotterCtx` is built.
                let a = points.last().unwrap();
                let contour = &mut current_contour;
                let pts = &mut points;
                let res = &mut result;
                let dsh = &mut dasher;
                let mut sink = |line: PathLineTo| -> Result<(), InternalError> {
                    match pts.last() {
                        Some(last_point) => {
                            if !last_point.equal(line.point) {
                                if let Some(d) = dsh.as_mut() {
                                    dashed_line_to(last_point, line.point, d, pts, contour, res);
                                } else {
                                    contour.plot(line.point, None);
                                    pts.add(line.point);
                                }
                            }
                            Ok(())
                        }
                        None => Err(InternalError::InvalidState),
                    }
                };
                let mut spline = Spline {
                    a,
                    b: n.p1,
                    c: n.p2,
                    d: n.p3,
                    tolerance,
                    plotter_impl: &mut sink,
                };
                spline.decompose()?;
            }
            PathNode::ClosePath(_) => {
                // Note that unlike our fill plotter, there's not much in the
                // way of special handling here, however we skip if all we have
                // in the buffer is a move_to, mainly because that would only
                // be a single point anyway.
                if points.len >= 2 {
                    // No-op if our initial and current points are equal
                    if points.last().unwrap().equal(points.first().unwrap()) {
                        continue;
                    }

                    if let Some(d) = &mut dasher {
                        dashed_line_to(
                            points.last().unwrap(),
                            points.first().unwrap(),
                            d,
                            &mut points,
                            &mut current_contour,
                            &mut result,
                        );
                    } else {
                        // Plot from the current point to the initial point
                        current_contour.plot(points.first().unwrap(), None);

                        // Set the current point to the initial point.
                        points.add(points.first().unwrap());
                    }
                }
            }
        }
    }

    // Add last contour if there were nodes in it
    if current_contour.len != 0 {
        result.append(current_contour);
    }

    Ok(result)
}

/// Port of `polyline_plotter.dashedLineTo`. Walks `p0`→`p1` in dash-sized steps,
/// plotting into `contour` while "on" and, at each dash transition, appending
/// the accumulated contour to `result` and starting a fresh one.
fn dashed_line_to(
    p0: Point,
    p1: Point,
    dasher: &mut Dasher,
    points: &mut PointBuffer<1, 2>,
    contour: &mut Contour,
    result: &mut ContourList,
) {
    let mut slope = Slope::init(p0, p1);
    let total_len = slope.normalize();
    let mut remaining_len = total_len;
    let mut step_len = dasher.remain.min(remaining_len);
    while remaining_len > 0.0 {
        remaining_len -= step_len;
        let x_offset = slope.dx * (total_len - remaining_len);
        let y_offset = slope.dy * (total_len - remaining_len);
        let current_dash_point = Point {
            x: p0.x + x_offset,
            y: p0.y + y_offset,
        };
        if !current_dash_point.equal(points.last().unwrap()) {
            points.add(current_dash_point);
        }
        if dasher.on {
            contour.plot(current_dash_point, None);
        }
        if dasher.step(step_len) {
            // Segment transition
            result.append(std::mem::replace(contour, Contour::new(1.0)));
            if dasher.on {
                contour.plot(current_dash_point, None);
            }
        }
        step_len = dasher.remain.min(remaining_len);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::internal::path_nodes::{PathClose, PathMoveTo};

    fn mv(x: f64, y: f64) -> PathNode {
        PathNode::MoveTo(PathMoveTo {
            point: Point { x, y },
        })
    }
    fn ln(x: f64, y: f64) -> PathNode {
        PathNode::LineTo(PathLineTo {
            point: Point { x, y },
        })
    }

    // Behavioral test replacing the dashed-panic blocker contract: a validated
    // dash array over a straight line yields one contour per "on" dash.
    #[test]
    fn dashed_polyline_produces_multiple_contours() {
        // `[2,2]` over a length-10 line yields three "on" dashes ([0,2],[4,6],
        // [8,10]). Faithful to the reference, `dashedLineTo` also appends the
        // (empty) "off" contour at each off→on transition, so the invariant we
        // pin is the count of NON-empty contours == 3 (the visible dashes).
        let nodes = vec![mv(0.0, 0.0), ln(10.0, 0.0)];
        let result = plot(&nodes, 0.01, &[2.0, 2.0], 0.0).unwrap();
        let non_empty = result.list.iter().filter(|c| c.len != 0).count();
        assert_eq!(3, non_empty);
    }

    #[test]
    fn undashed_polyline_single_contour() {
        // Without a dasher a straight open line is a single contour.
        let nodes = vec![mv(0.0, 0.0), ln(10.0, 0.0)];
        let result = plot(&nodes, 0.01, &[], 0.0).unwrap();
        assert_eq!(1, result.list.len());
    }

    #[test]
    fn dashed_closed_polyline_dashes_the_closing_segment() {
        // A closed square, dashed: the closing edge is also dashed, so more than
        // one contour is produced.
        let nodes = vec![
            mv(0.0, 0.0),
            ln(8.0, 0.0),
            ln(8.0, 8.0),
            ln(0.0, 8.0),
            PathNode::ClosePath(PathClose {}),
        ];
        let result = plot(&nodes, 0.01, &[2.0, 2.0], 0.0).unwrap();
        assert!(result.list.len() > 1);
    }
}
