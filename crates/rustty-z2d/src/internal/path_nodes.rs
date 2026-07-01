// SPDX-License-Identifier: MPL-2.0
//   Copyright © 2024-2025 Chris Marchesi
//
// Ported to Rust for rustty from z2d 0.10.0 `src/internal/path_nodes.zig` (R3 vendored dep).

use crate::internal::point::Point;

/// Represents a moveto path node. This starts a new subpath and moves the
/// current point to (x, y).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PathMoveTo {
    pub point: Point,
}

/// Represents a lineto path node. This draws a line to (x, y) and sets it as
/// its current point.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PathLineTo {
    pub point: Point,
}

/// Represents a curveto path node. This draws a cubic bezier with the three
/// supplied control points from the current point. The new current point is
/// set to p3.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PathCurveTo {
    pub p1: Point,
    pub p2: Point,
    pub p3: Point,
}

/// Represents a closepath node.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PathClose {}

/// A tagged union of all path node types.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PathNode {
    MoveTo(PathMoveTo),
    LineTo(PathLineTo),
    CurveTo(PathCurveTo),
    ClosePath(PathClose),
}

impl PathNode {
    /// Returns true if all subpaths in the set of PathNodes are currently
    /// closed.
    pub fn is_closed_node_set(nodes: &[PathNode]) -> bool {
        if nodes.is_empty() {
            return false;
        }

        let mut closed = false;
        for (i, node) in nodes.iter().enumerate() {
            match node {
                PathNode::MoveTo(_) => {
                    if !closed && i != 0 {
                        break;
                    }
                }
                PathNode::ClosePath(_) => closed = true,
                _ => closed = false,
            }
        }

        closed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn is_closed_node_set() {
        {
            // Basic (closed)
            let nodes = [
                mv(1.0, 1.0),
                ln(2.0, 2.0),
                ln(3.0, 3.0),
                close(),
                mv(1.0, 1.0),
            ];
            assert!(PathNode::is_closed_node_set(&nodes));
        }
        {
            // Multiple subpaths, all closed
            let nodes = [
                mv(1.0, 1.0),
                ln(2.0, 2.0),
                ln(3.0, 3.0),
                close(),
                mv(1.0, 1.0),
                mv(4.0, 4.0),
                ln(5.0, 5.0),
                ln(6.0, 6.0),
                close(),
                mv(4.0, 4.0),
            ];
            assert!(PathNode::is_closed_node_set(&nodes));
        }
        {
            // Basic (not closed)
            let nodes = [
                mv(1.0, 1.0),
                ln(2.0, 2.0),
                mv(3.0, 3.0),
                ln(4.0, 4.0),
                ln(5.0, 5.0),
            ];
            assert!(!PathNode::is_closed_node_set(&nodes));
        }
        {
            // Closed in the middle
            let nodes = [
                mv(1.0, 1.0),
                ln(2.0, 2.0),
                close(),
                mv(1.0, 1.0),
                mv(3.0, 3.0),
                ln(4.0, 4.0),
                ln(5.0, 5.0),
            ];
            assert!(!PathNode::is_closed_node_set(&nodes));
        }
        {
            // Closed at the end (not in the middle)
            let nodes = [
                mv(1.0, 1.0),
                ln(2.0, 2.0),
                mv(3.0, 3.0),
                ln(4.0, 4.0),
                ln(5.0, 5.0),
                close(),
                mv(3.0, 3.0),
            ];
            assert!(!PathNode::is_closed_node_set(&nodes));
        }
        {
            // Empty node set
            assert!(!PathNode::is_closed_node_set(&[]));
        }
    }
}
