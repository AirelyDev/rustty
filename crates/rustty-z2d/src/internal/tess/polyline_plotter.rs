// SPDX-License-Identifier: MPL-2.0
//   Copyright © 2024-2025 Chris Marchesi
//
// Ported to Rust for rustty from z2d 0.10.0
// `src/internal/tess/polyline_plotter.zig` (R3 vendored dep).
//
// SUBSET NOTE (rustty): identical boundary to `stroke_plotter` — the hairline
// stroke path (the sole caller, via `painter::stroke`'s hairline branch) never
// uses dashed lines, so the `Dasher`/`dashedLineTo` branches are not ported; the
// leading `Dasher.validate` guard IS ported (shared `dasher::validate`) and
// reports a blocker (`unimplemented!`) if a valid dash array is ever supplied.
// With dashes excluded, the spline `line_to` callback (Zig `SplinePlotterCtx`)
// is expressed as a `&mut dyn FnMut(PathLineTo)` sink over the borrowed
// contour/point-buffer — equivalent behavior, no raw pointers (keeps the Miri
// gate clean), exactly as `stroke_plotter` does for its spline decomposition.

//! A polyline plotter, converts a path to a Contour (not a Polygon).
//!
//! This is a variation of our fill plotter, as the logic is pretty much the
//! same, with the general exception of we need to keep less state (as we are
//! plotting a polyline, not edges), and the path can be open, not just
//! closed.

use crate::internal::error::InternalError;
use crate::internal::path_nodes::{PathLineTo, PathNode};
use crate::internal::tess::dasher;
use crate::internal::tess::point_buffer::PointBuffer;
use crate::internal::tess::polygon::{Contour, ContourList};
use crate::internal::tess::spline::Spline;

/// Converts a path to a `ContourList` (open polylines), not a `Polygon`.
///
/// Port of `polyline_plotter.plot`. See the module SUBSET NOTE for the dashed
/// boundary. The `alloc` arg is dropped (the ported tess layer is allocator-
/// free, matching `stroke_plotter::plot`), and the Zig `errdefer
/// result.deinit(alloc)` becomes Rust `Drop`.
pub fn plot(
    nodes: &[PathNode],
    tolerance: f64,
    dashes: &[f64],
    _dash_offset: f64,
) -> Result<ContourList, InternalError> {
    // Dasher.validate guard — reuse the shared `dasher::validate` port. On a
    // valid dash array the Zig would build a `Dasher` and take the
    // `dashedLineTo` branches; the sprite subset never ports that machinery.
    if dasher::validate(dashes) {
        unimplemented!("dashed polyline tessellation is not part of the z2d sprite subset");
    }

    let mut result = ContourList::empty();
    let mut current_contour = Contour::new(1.0);

    let mut points: PointBuffer<1, 2> = PointBuffer::default();

    // (dasher elided — no dasher in subset)

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
                // (dasher.reset() elided — no dasher in subset)
                // Check if this is the last node, and no-op if it is, as this
                // is the auto-added move_to node that is given after
                // close_path.
                if i == n_nodes - 1 {
                    break;
                }
                current_contour.plot(n.point, None);
                points.add(n.point);
            }
            PathNode::LineTo(n) => {
                match points.last() {
                    Some(last_point) => {
                        if !last_point.equal(n.point) {
                            // (dasher branch elided — subset)
                            current_contour.plot(n.point, None);
                            points.add(n.point);
                        }
                    }
                    None => return Err(InternalError::InvalidState),
                }
            }
            PathNode::CurveTo(n) => {
                if points.len == 0 {
                    return Err(InternalError::InvalidState);
                }
                // SplinePlotterCtx, dasher-free: the line_to sink plots to the
                // current contour + point buffer (Zig `SplinePlotterCtx.line_to`).
                //
                // BORROW ORDER (load-bearing): read `spline.a` out of `points`
                // FIRST — `points.last()` yields a `Point` value (`Point` is
                // `Copy`) that lives independently of the buffer. Only AFTER `a`
                // is a detached value do we take `pts = &mut points` for the
                // sink. This mirrors the Zig `points.last().?` on line 96 being
                // read before the `SplinePlotterCtx` is built.
                let a = points.last().unwrap();
                let contour = &mut current_contour;
                let pts = &mut points;
                let mut sink = |line: PathLineTo| -> Result<(), InternalError> {
                    match pts.last() {
                        Some(last_point) => {
                            if !last_point.equal(line.point) {
                                contour.plot(line.point, None);
                                pts.add(line.point);
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

                    // (dasher branch elided — subset)
                    // Plot from the current point to the initial point
                    current_contour.plot(points.first().unwrap(), None);

                    // Set the current point to the initial point.
                    points.add(points.first().unwrap());
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
