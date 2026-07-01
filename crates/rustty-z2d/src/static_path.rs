// SPDX-License-Identifier: MPL-2.0
//   Copyright © 2024-2025 Chris Marchesi
//
// Ported to Rust for rustty from z2d 0.10.0 `src/static_path.zig` (R3 vendored dep).

//! Represents a wrapper over the fully unmanaged representation of a `Path`;
//! nodes are stored in a static buffer of the specified `len`, and
//! initialization of the wrapped path is done using `Path::init_buffer`.
//!
//! Make sure to call `init` after declaring a `StaticPath` to initialize.
//!
//! All top-level methods of `StaticPath` are infallible and are idiomatically
//! wrapped to their appropriate counterparts in `Path`. Note that methods that
//! do usually fail due to non-memory reasons (e.g., no current point, etc.) are
//! safety-checked undefined behavior (a panic) when they would normally return
//! an error.
//!
//! Not all `Path` methods are wrapped, nor are the fields exposed through
//! setters. If you require a feature of `Path` that is not wrapped or exposed
//! (such as manipulation of the transformation matrix), you can access the
//! instance directly through the `wrapped_path` field.

use crate::path::Path;

/// A static-buffer-backed wrapper over `Path`. `N` is the node buffer length.
#[derive(Debug, Clone, Default)]
pub struct StaticPath<const N: usize> {
    /// The wrapped path. Access this directly for features not wrapped by the
    /// `StaticPath` helpers (e.g., the transformation matrix).
    pub wrapped_path: Path,
}

impl<const N: usize> StaticPath<N> {
    /// Creates a new, uninitialized `StaticPath`. Call `init` before use.
    pub fn new() -> StaticPath<N> {
        StaticPath {
            wrapped_path: Path::empty(),
        }
    }

    /// Initialize the wrapped path with the node buffer.
    pub fn init(&mut self) {
        self.wrapped_path = Path::init_buffer(N);
    }

    /// Rests the path set, clearing all nodes and state.
    pub fn reset(&mut self) {
        self.wrapped_path.reset();
    }

    /// Starts a new path, and moves the current point to it.
    pub fn move_to(&mut self, x: f64, y: f64) {
        self.wrapped_path.move_to_assume_capacity(x, y);
    }

    /// Begins a new sub-path relative to the current point. Calling this
    /// without a current point triggers safety-checked undefined behavior.
    pub fn rel_move_to(&mut self, x: f64, y: f64) {
        self.wrapped_path.rel_move_to_assume_capacity(x, y).unwrap();
    }

    /// Draws a line from the current point to the specified point and sets it
    /// as the current point. Acts as a `move_to` instead if there is no current
    /// point.
    pub fn line_to(&mut self, x: f64, y: f64) {
        self.wrapped_path.line_to_assume_capacity(x, y);
    }

    /// Draws a line relative to the current point. Calling this without a
    /// current point triggers safety-checked undefined behavior.
    pub fn rel_line_to(&mut self, x: f64, y: f64) {
        self.wrapped_path.rel_line_to_assume_capacity(x, y).unwrap();
    }

    /// Draws a cubic bezier with the three supplied control points from the
    /// current point. The new current point is set to (`x3`, `y3`). Calling
    /// this without a current point triggers safety-checked undefined behavior.
    #[allow(clippy::too_many_arguments)]
    pub fn curve_to(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, x3: f64, y3: f64) {
        self.wrapped_path
            .curve_to_assume_capacity(x1, y1, x2, y2, x3, y3)
            .unwrap();
    }

    /// Draws a cubic bezier relative to the current point. Calling this without
    /// a current point triggers safety-checked undefined behavior.
    #[allow(clippy::too_many_arguments)]
    pub fn rel_curve_to(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, x3: f64, y3: f64) {
        self.wrapped_path
            .rel_curve_to_assume_capacity(x1, y1, x2, y2, x3, y3)
            .unwrap();
    }

    /// Closes the path by drawing a line from the current point by the starting
    /// point. No effect if there is no current point.
    ///
    /// Note that path closes require two points, one for the `close_path`
    /// entry, and one for the implicit `move_to` entry; ensure your
    /// `StaticPath` has enough space for both.
    pub fn close(&mut self) {
        self.wrapped_path.close_assume_capacity();
    }

    /// The node buffer, as populated so far.
    pub fn nodes(&self) -> &[crate::internal::path_nodes::PathNode] {
        &self.wrapped_path.nodes
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::internal::path_nodes::{PathClose, PathCurveTo, PathLineTo, PathMoveTo, PathNode};
    use crate::internal::point::Point;

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
    fn cv(p1: (f64, f64), p2: (f64, f64), p3: (f64, f64)) -> PathNode {
        PathNode::CurveTo(PathCurveTo {
            p1: Point { x: p1.0, y: p1.1 },
            p2: Point { x: p2.0, y: p2.1 },
            p3: Point { x: p3.0, y: p3.1 },
        })
    }
    fn close() -> PathNode {
        PathNode::ClosePath(PathClose {})
    }

    #[test]
    fn static_path() {
        {
            // Basic
            let mut path: StaticPath<5> = StaticPath::new();
            path.init();
            path.move_to(10.0, 10.0);
            path.line_to(30.0, 10.0);
            path.line_to(20.0, 30.0);
            path.close();
            assert_eq!(
                &[
                    mv(10.0, 10.0),
                    ln(30.0, 10.0),
                    ln(20.0, 30.0),
                    close(),
                    mv(10.0, 10.0)
                ],
                path.nodes()
            );
        }
        {
            // Relative line helpers
            let mut path: StaticPath<5> = StaticPath::new();
            path.init();
            path.move_to(10.0, 10.0);
            path.rel_line_to(20.0, 0.0);
            path.rel_line_to(-10.0, 20.0);
            path.close();
            assert_eq!(
                &[
                    mv(10.0, 10.0),
                    ln(30.0, 10.0),
                    ln(20.0, 30.0),
                    close(),
                    mv(10.0, 10.0)
                ],
                path.nodes()
            );
        }
        {
            // curveTo
            let mut path: StaticPath<2> = StaticPath::new();
            path.init();
            path.move_to(10.0, 10.0);
            path.curve_to(30.0, 10.0, 30.0, 30.0, 10.0, 30.0);
            assert_eq!(
                &[mv(10.0, 10.0), cv((30.0, 10.0), (30.0, 30.0), (10.0, 30.0))],
                path.nodes()
            );
        }
        {
            // relCurveTo
            let mut path: StaticPath<2> = StaticPath::new();
            path.init();
            path.move_to(10.0, 10.0);
            path.rel_curve_to(20.0, 0.0, 20.0, 20.0, 0.0, 20.0);
            assert_eq!(
                &[mv(10.0, 10.0), cv((30.0, 10.0), (30.0, 30.0), (10.0, 30.0))],
                path.nodes()
            );
        }
        {
            // relMoveTo
            let mut path: StaticPath<3> = StaticPath::new();
            path.init();
            path.move_to(10.0, 10.0);
            path.line_to(20.0, 20.0);
            path.rel_move_to(10.0, 10.0);
            assert_eq!(
                &[mv(10.0, 10.0), ln(20.0, 20.0), mv(30.0, 30.0)],
                path.nodes()
            );
        }
        {
            // reset
            let mut path: StaticPath<5> = StaticPath::new();
            path.init();
            path.move_to(10.0, 10.0);
            path.line_to(30.0, 10.0);
            path.line_to(20.0, 30.0);
            path.close();
            assert_eq!(
                &[
                    mv(10.0, 10.0),
                    ln(30.0, 10.0),
                    ln(20.0, 30.0),
                    close(),
                    mv(10.0, 10.0)
                ],
                path.nodes()
            );
            path.reset();
            path.move_to(100.0, 100.0);
            path.line_to(300.0, 100.0);
            path.line_to(200.0, 300.0);
            path.close();
            assert_eq!(
                &[
                    mv(100.0, 100.0),
                    ln(300.0, 100.0),
                    ln(200.0, 300.0),
                    close(),
                    mv(100.0, 100.0)
                ],
                path.nodes()
            );
        }
    }
}
