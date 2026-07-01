// SPDX-License-Identifier: MPL-2.0
//   Copyright © 2024-2025 Chris Marchesi
//
// Ported to Rust for rustty from z2d 0.10.0 `src/internal/tess/fill_plotter.zig`
// (R3 vendored dep). The Zig `SplinePlotterCtx` + `PlotterVTable` indirection is
// expressed here as a Rust closure captured by `Spline` (equivalent behavior,
// no raw pointers — keeps the Miri gate clean).

//! A polygon plotter for fill operations.

use crate::internal::error::InternalError;
use crate::internal::path_nodes::{PathLineTo, PathNode};
use crate::internal::tess::point_buffer::PointBuffer;
use crate::internal::tess::polygon::Polygon;
use crate::internal::tess::spline::Spline;

pub fn plot(nodes: &[PathNode], scale: f64, tolerance: f64) -> Result<Polygon, InternalError> {
    let mut result = Polygon::new(scale);

    let mut points: PointBuffer<1, 3> = PointBuffer::default();

    for (i, node) in nodes.iter().enumerate() {
        match node {
            PathNode::MoveTo(n) => {
                // Check if this is the last node, and no-op if it is, as this is
                // the auto-added move_to node that is given after close_path.
                if i == nodes.len() - 1 {
                    break;
                }
                points.reset();
                points.add(n.point);
            }
            PathNode::LineTo(n) => {
                if let Some(last_point) = points.last() {
                    if !last_point.equal(n.point) {
                        result.add_edge(last_point, n.point);
                        points.add(n.point);
                    }
                } else {
                    return Err(InternalError::InvalidState);
                }
            }
            PathNode::CurveTo(n) => {
                if points.len == 0 {
                    return Err(InternalError::InvalidState);
                }
                let a = points.last().unwrap();
                let mut err: Option<InternalError> = None;
                {
                    let result_ref = &mut result;
                    let points_ref = &mut points;
                    let mut plotter = |node: PathLineTo| -> Result<(), InternalError> {
                        if let Some(last_point) = points_ref.last() {
                            if !last_point.equal(node.point) {
                                result_ref.add_edge(last_point, node.point);
                                points_ref.add(node.point);
                            }
                            Ok(())
                        } else {
                            Err(InternalError::InvalidState)
                        }
                    };
                    let mut spline = Spline {
                        a,
                        b: n.p1,
                        c: n.p2,
                        d: n.p3,
                        tolerance,
                        plotter_impl: &mut plotter,
                    };
                    if let Err(e) = spline.decompose() {
                        err = Some(e);
                    }
                }
                if let Some(e) = err {
                    return Err(e);
                }
            }
            PathNode::ClosePath(_) => {
                // Only proceed if we have 3 points in the buffer; anything else
                // is a degenerate line (e.g., move_to -> line_to (2) or move_to
                // -> close_path (1)). These scenarios will be cleared on the
                // next move_to (which happens right after this).
                //
                // Note on the move_to -> line_to scenario, an edge will be
                // generated. While this will produce a broken image, it should
                // be fine as far as rasterization goes, as the rasterizer works
                // on edge pairs, with trailing odd edges discarded.
                if points.len >= 3 {
                    // No-op if our initial and current points are equal
                    if points.last().unwrap().equal(points.first().unwrap()) {
                        continue;
                    }

                    // Plot from the current point to the initial point
                    let last = points.last().unwrap();
                    let first = points.first().unwrap();
                    result.add_edge(last, first);

                    // Set the current point to the initial point.
                    points.add(first);
                }
            }
        }
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::internal::path_nodes::{PathClose, PathLineTo, PathMoveTo};
    use crate::internal::point::Point;
    use crate::internal::tess::polygon::Edge;

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
    fn close() -> PathNode {
        PathNode::ClosePath(PathClose {})
    }

    #[test]
    fn degenerate_line_to() {
        let nodes = vec![
            mv(5.0, 0.0),
            ln(10.0, 10.0),
            ln(10.0, 10.0),
            ln(0.0, 10.0),
            close(),
            mv(5.0, 0.0),
        ];
        let result = plot(&nodes, 1.0, 1.0).unwrap();
        // NOTE: only 2 edges should be here as there is a horizontal edge
        // ((10,10) -> (0, 10)), this is now filtered out.
        assert_eq!(2, result.edges.len());
        assert_eq!(
            vec![
                Edge {
                    y0: 0.0,
                    y1: 10.0,
                    x_start: 5.0,
                    x_inc: 0.5
                },
                Edge {
                    y0: 10.0,
                    y1: 0.0,
                    x_start: 5.0,
                    x_inc: -0.5
                },
            ],
            result.edges
        );
    }

    #[test]
    fn degenerate_close() {
        {
            // move_to -> line_to
            let nodes = vec![mv(5.0, 0.0), ln(10.0, 10.0), close(), mv(5.0, 0.0)];
            let result = plot(&nodes, 1.0, 1.0).unwrap();
            assert_eq!(1, result.edges.len());
            assert_eq!(
                vec![Edge {
                    y0: 0.0,
                    y1: 10.0,
                    x_start: 5.0,
                    x_inc: 0.5
                }],
                result.edges
            );
        }
        {
            // double close
            let nodes = vec![
                mv(5.0, 0.0),
                ln(10.0, 10.0),
                ln(0.0, 10.0),
                close(),
                mv(5.0, 0.0),
                close(),
                mv(5.0, 0.0),
            ];
            let result = plot(&nodes, 1.0, 1.0).unwrap();
            // NOTE: only 2 edges should be here as there is a horizontal edge
            // ((10,10) -> (0, 10)), this is now filtered out.
            assert_eq!(2, result.edges.len());
            assert_eq!(
                vec![
                    Edge {
                        y0: 0.0,
                        y1: 10.0,
                        x_start: 5.0,
                        x_inc: 0.5
                    },
                    Edge {
                        y0: 10.0,
                        y1: 0.0,
                        x_start: 5.0,
                        x_inc: -0.5
                    },
                ],
                result.edges
            );
        }
    }
}
