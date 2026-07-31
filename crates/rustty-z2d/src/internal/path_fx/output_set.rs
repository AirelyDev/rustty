// SPDX-License-Identifier: MPL-2.0
//   Copyright © 2024-2026 Chris Marchesi
//
// Ported to Rust for rustty from z2d 0.12.1
// `src/internal/path_fx/OutputSet.zig` (R3 vendored dep).

//! Output set for path effects. Much simpler than input sets, basically just
//! holds a set of points for the contour and the helpers to record them with a
//! path builder.

use crate::internal::path_nodes::PathNode;
use crate::internal::point::Point;
use crate::path::Path;

#[derive(Debug, Clone, Default)]
pub struct OutputSet {
    pub contours: Vec<Contour>,
}

impl OutputSet {
    /// PORT NOTE (rustty): upstream's `ToNodesError` is
    /// `Path.Error || Allocator.Error`. rustty's `Path::move_to`/`line_to`/
    /// `close` are infallible (allocation panics on OOM as everywhere else in
    /// the port, and none of the three can raise `NoCurrentPoint` here because
    /// `record_path` always emits a `move_to` first), so this returns the node
    /// vector directly.
    pub fn to_nodes(&self) -> Vec<PathNode> {
        let mut result = Path::empty();
        for contour in &self.contours {
            contour.record_path(&mut result);
        }

        result.nodes
    }
}

#[derive(Debug, Clone, Default)]
pub struct Contour {
    pub points: Vec<Point>,
    pub closed: bool,
}

impl Contour {
    /// Asserts that the contour is not closed.
    pub fn plot(&mut self, point: Point) {
        assert!(!self.closed);
        self.points.push(point);
    }

    /// Asserts that there is at least three segments in the contour, and that
    /// the contour is not closed.
    pub fn close(&mut self) {
        assert!(self.points.len() >= 3);
        assert!(!self.closed);
        self.closed = true;
    }

    fn record_path(&self, path: &mut Path) {
        if self.points.len() <= 1 {
            return;
        }

        path.move_to(self.points[0].x, self.points[0].y);
        for point in &self.points[1..] {
            path.line_to(point.x, point.y);
        }

        if self.closed {
            path.close();
        }
    }
}
