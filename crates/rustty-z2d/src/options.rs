// SPDX-License-Identifier: MPL-2.0
//   Copyright © 2024-2025 Chris Marchesi
//
// Ported to Rust for rustty from z2d 0.10.0 `src/options.zig` (R3 vendored dep).

//! Documents option enumerations used in various parts of the library, mostly
//! in `Context`.

/// The default maximum error tolerance used in spline and arc calculations.
pub const DEFAULT_TOLERANCE: f64 = 0.1;

/// Represents the kinds of fill rules for paths. This will determine how more
/// complex paths are filled, such as in the situation where multiple sub-paths
/// overlap, or a single sub-path traces its path in a way where its lines will
/// cross each other (consider the path undertaken when tracing a star, for
/// example).
///
/// The core concept that makes a fill rule is how it determines its
/// "insideness", or the areas to be filled.
///
/// Note that the exact internal implementation details for a particular fill
/// rule may not match the descriptions here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FillRule {
    /// Determines the insideness by drawing a ray from any point to infinity
    /// in any direction, with a starting count of zero. For any line that
    /// crosses from left to right, add 1, and for any line that crosses from
    /// right to left, subtract one. After all lines are crossed and accounted
    /// for, if the result is zero, the point is outside of the path and not
    /// drawn, otherwise it's inside and drawn.
    #[default]
    NonZero,

    /// Determines the insideness by drawing a ray from any point to infinity
    /// in any direction, counting the number of crossings on the way out. If
    /// after all crossings are accounted for, if the count is even, the point
    /// is outside of the path and not drawn, otherwise it's inside and drawn.
    EvenOdd,
}

/// Represents how lines are joined when stroking paths.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum JoinMode {
    /// Lines are joined with a miter (pointed end).
    ///
    /// See `Context::set_miter_limit` for details on how to control the miter
    /// limit.
    #[default]
    Miter,

    /// Lines are joined with a circle centered around the middle point of the
    /// joined line.
    Round,

    /// Lines are joined with a bevel (cut-off). The cut-off points are taken
    /// at the outer adjoining line ends as they would be seen if the lines
    /// were not closed.
    Bevel,
}

/// Represents how lines are capped when stroking paths.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CapMode {
    /// Lines are cut off at the ends, right at their center point
    /// perpendicular to their thickness. In other words, a path will not
    /// extend beyond the two endpoints.
    #[default]
    Butt,

    /// Lines are rounded at the ends.
    Round,

    /// Lines are squared on the ends, to the thickness / 2.
    Square,
}

/// Represents the anti-aliasing mode used when drawing paths, which smoothes
/// out jagged lines (aka "jaggies"). `None` represents no anti-aliasing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AntiAliasMode {
    /// Do not use anti-aliasing.
    None,

    /// Currently the same as `.multisample_4x`.
    #[default]
    Default,

    /// Multi-samples rasterization at 4x size to determine coverage, backed by
    /// a sparse buffer sized for the real mask scanline size. Recommended for
    /// most cases.
    Multisample4x,

    /// Super-samples the entire draw mask at 4x, which is then downsampled to
    /// create the anti-aliased mask. This method is rudimentary and should be
    /// highly accurate, but is slow and memory-intensive. It's recommended to
    /// use `.multisample_4x` unless otherwise specifically needed for the
    /// particular case.
    Supersample4x,
}
