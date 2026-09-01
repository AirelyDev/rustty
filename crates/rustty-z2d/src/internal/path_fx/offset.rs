// SPDX-License-Identifier: MPL-2.0
//   Copyright © 2024-2026 Chris Marchesi
//
// Ported to Rust for rustty from z2d 0.12.1 `src/internal/path_fx/offset.zig`
// (R3 vendored dep).

//! Path offsetting.
//!
//! This is a very simple path offset based off of adjusting a path inward
//! (inset) or outward (outset) based on the direction of the polygon or path.
//!
//! Closed paths are expected to represent simple polygons (no
//! self-intersections). Open paths are allowed but are expected to not change
//! direction.
//!
//! Paths that do not conform to these constraints may produce undesirable
//! results.

use super::input_set::{InputSet, Orientation, Segment, Slope};
use super::output_set::{Contour, OutputSet};
use super::shared;
use crate::internal::error::InternalError;
use crate::internal::path_nodes::PathNode;
use crate::internal::point::Point;

/// PORT NOTE (rustty): upstream's `Error` is
/// `InputSet.FromNodesError || OutputSet.ToNodesError || Allocator.Error`,
/// which after rustty's infallible allocation and infallible `Path` builders
/// reduces to `InternalError`.
pub fn run(in_: &[PathNode], tolerance: f64, offset: f64) -> Result<Vec<PathNode>, InternalError> {
    let mut input_set = InputSet::from_nodes(in_, tolerance)?;
    let mut output_set = OutputSet::default();
    if offset == 0.0 {
        // no-op all nodes if zero offset was supplied
        for contour in &input_set.contours {
            shared::noop_contour(&mut output_set, contour);
        }
    } else {
        for contour in &mut input_set.contours {
            if contour.closed {
                contour.align_segments();
                run_closed(&mut output_set, contour, offset);
            } else {
                run_open(&mut output_set, contour, offset);
            }
        }
    }

    Ok(output_set.to_nodes())
}

/// Asserts that the input contour is closed and has at least 3 nodes.
fn run_closed(out: &mut OutputSet, in_: &super::input_set::Contour, offset: f64) {
    assert!(in_.closed);
    assert!(in_.segments.len() >= 3);

    let mut result = Contour::default();
    let clockwise = in_.segments[0].orientation == Orientation::Cw;

    // Initial point
    {
        let in_seg = in_.segments[in_.segments.len() - 1];
        let out_seg = in_.segments[0];
        result.plot(intersect(in_seg, out_seg, offset, clockwise));
    }

    // Everything else
    for (in_idx, out_seg) in in_.segments[1..].iter().enumerate() {
        let in_seg = in_.segments[in_idx];
        result.plot(intersect(in_seg, *out_seg, offset, clockwise));
    }

    result.close();
    out.contours.push(result);
}

/// Asserts that the input contour has at least 2 nodes.
fn run_open(out: &mut OutputSet, in_: &super::input_set::Contour, offset: f64) {
    let mut result = Contour::default();
    let clockwise = match in_.segments[0].orientation {
        Orientation::Cw => true,
        Orientation::Ccw => false,
        Orientation::Invalid => {
            // Invalid means that there was only one element, always assume
            // clockwise and return.
            assert_eq!(in_.segments.len(), 1);
            result.plot(offset_single(
                in_.segments[0].p0,
                in_.segments[0].slope,
                offset,
                true,
            ));
            result.plot(offset_single(
                in_.segments[0].p1,
                in_.segments[0].slope,
                offset,
                true,
            ));
            out.contours.push(result);
            return;
        }
    };

    result.plot(offset_single(
        in_.segments[0].p0,
        in_.segments[0].slope,
        offset,
        clockwise,
    ));
    for (in_idx, out_seg) in in_.segments[1..].iter().enumerate() {
        let in_seg = in_.segments[in_idx];
        result.plot(intersect(in_seg, *out_seg, offset, clockwise));
    }
    let last = in_.segments[in_.segments.len() - 1];
    result.plot(offset_single(last.p1, last.slope, offset, clockwise));

    out.contours.push(result);
}

/// Perform a projected line-line intersection off of the inner points of the
/// two consecutive segments, based on the supplied offset and direction.
fn intersect(in_: Segment, out: Segment, offset: f64, clockwise: bool) -> Point {
    // We use a parametric derivation to calculate our intersections (note that
    // this is how it's done also in InputSet and Face, but I want to provide a
    // better explanation for both myself and anyone else looking at the code.
    //
    // In the form that we need it in here, our intersecting lines are
    // represented by: 1) a point, and 2) a vector (our normalized slope). This
    // is similar to the parametric intersections calculated in InputSet, but
    // here, we do not place restrictions on our parametric projections (i.e.,
    // we assume the lines extend infinitely), so we can simplify things below
    // to the following two equations:
    //
    //   i = p1 + t * v1
    //   i = p2 + u * v2
    //
    //   or:
    //
    //   p1 + t * v1 = p2 + u * v2
    //
    // Rearranged:
    //
    //   t * v1 - u * v2 = p2 - p1
    //
    // This can now be expressed as two equations for each of the x and
    // y-coordinates (sub in our normalized dx and dy for our vectors):
    //
    //   t * dx1 - u * dx2 = x2 - x1
    //   t * dy1 - u * dy2 = y2 - y1
    //
    // This can now be solved under Cramer's Rule, as it can be expressed as:
    //
    // [ dx1 dx2 ] [  t ] [ x2 - x1 ]
    // [ dy1 dy2 ] [ -u ] [ y2 - y1 ]
    //
    // Solving for t:
    //
    //  | (x2 - x1) dx2 |
    //  | (y2 - y1) dy2 |
    // -------------------
    //     | dx1 dx2 |
    //     | dy1 dy2 |
    //
    // Equals:
    //
    //  (x2 - x1) * dy2 - (y2 - y1) * dx2
    // -----------------------------------
    //        dx1 * dy2 - dy1 * dx2
    //
    // Now that we know t, and do not place any restrictions on it, we can
    // compute the intersection easily by plugging it into our inbound equation:
    //
    // x = x1 + t * dx1
    // y = y1 + t * dy1
    //
    // Since the points are the same, and since we're projecting infinitely, we
    // don't need to worry about solving for u (or -u, in this instance).

    // Shortened slopes so that our equations are more sane
    let dx1 = in_.slope.dx;
    let dy1 = in_.slope.dy;
    let dx2 = out.slope.dx;
    let dy2 = out.slope.dy;

    // Offset adjustment for points
    let xoff1 = -dy1 * offset;
    let yoff1 = dx1 * offset;
    let xoff2 = -dy2 * offset;
    let yoff2 = dx2 * offset;

    // Adjusted inner points
    let (x1, y1, x2, y2) = if clockwise {
        (
            in_.p1.x - xoff1,
            in_.p1.y - yoff1,
            out.p0.x - xoff2,
            out.p0.y - yoff2,
        )
    } else {
        (
            in_.p1.x + xoff1,
            in_.p1.y + yoff1,
            out.p0.x + xoff2,
            out.p0.y + yoff2,
        )
    };

    // Get our divisor and assert (note: we should be far past the point of
    // ensuring that this is non-zero)
    let div = dx1 * dy2 - dy1 * dx2;
    debug_assert!(div != 0.0);

    // Solve for t
    let t = ((x2 - x1) * dy2 - (y2 - y1) * dx2) / div;

    // Done
    Point {
        x: x1 + t * dx1,
        y: y1 + t * dy1,
    }
}

// used for start and end points on open contour.
fn offset_single(point: Point, slope: Slope, offset: f64, clockwise: bool) -> Point {
    let dx1 = slope.dx;
    let dy1 = slope.dy;
    let xoff1 = -dy1 * offset;
    let yoff1 = dx1 * offset;
    if clockwise {
        Point {
            x: point.x - xoff1,
            y: point.y - yoff1,
        }
    } else {
        Point {
            x: point.x + xoff1,
            y: point.y + yoff1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::internal::path_nodes::{PathClose, PathLineTo, PathMoveTo};

    fn m(x: f64, y: f64) -> PathNode {
        PathNode::MoveTo(PathMoveTo {
            point: Point { x, y },
        })
    }

    fn l(x: f64, y: f64) -> PathNode {
        PathNode::LineTo(PathLineTo {
            point: Point { x, y },
        })
    }

    fn z() -> PathNode {
        PathNode::ClosePath(PathClose {})
    }

    /// The reference's `e2e` input: a clockwise star, a counter-clockwise
    /// triangle (which exercises the leftmost-point re-alignment), two open
    /// "fake curves" in each direction, and a single line.
    fn e2e_input() -> Vec<PathNode> {
        vec![
            // Star
            m(15.0, 13.0),
            l(22.2, 13.0),
            l(25.0, 5.0),
            l(27.8, 13.0),
            l(35.0, 13.0),
            l(29.22641509433962, 17.07547169811321),
            l(32.0, 25.0),
            l(25.0, 20.058823529411764),
            l(18.0, 25.0),
            l(20.77358490566038, 17.075471698113205),
            z(),
            m(15.0, 13.0),
            // Triangle (counter-clockwise, testing point re-alignment to leftmost)
            m(110.0, 110.0),
            l(60.0, 10.0),
            l(10.0, 110.0),
            z(),
            m(110.0, 110.0),
            // Fake curve #1 (open, clockwise)
            m(130.0, 20.0),
            l(149.0, 30.0),
            l(160.0, 50.0),
            l(160.0, 70.0),
            l(150.0, 90.0),
            l(130.0, 100.0),
            // Fake curve #2 (open, counter-clockwise)
            m(100.0, 160.0),
            l(90.0, 150.0),
            l(70.0, 140.0),
            l(50.0, 140.0),
            l(30.0, 150.0),
            l(21.0, 160.0),
            // Single line
            m(10.0, 300.0),
            l(110.0, 400.0),
        ]
    }

    /// Ported verbatim from the reference's `test "offset, e2e"`. The expected
    /// node sets are z2d 0.12.1's own recorded values, so this pins rustty's
    /// geometry to the oracle's to the last bit rather than to hand-derived
    /// numbers.
    #[test]
    // Miri's libm is not bit-faithful for the IMPRECISE math this path uses.
    // `input_set.rs:320`'s `dx.hypot(dy)` differs from the host libm by 1-4 ULP
    // under Miri, and the error propagates into every derived node: measured
    // with a standalone probe, `hypot(3,4)` is `0x4014000000000001` under Miri
    // where the host (and IEEE) give exactly 5.0 = `0x4014000000000000`, while
    // `sqrt` — which IEEE requires to be correctly rounded, and which Miri
    // implements exactly — agrees bit-for-bit on the same inputs. So the
    // divergence is Miri's math library, not this port's arithmetic. It is also
    // stable, not random: identical across repeated runs and across
    // `-Zmiri-seed` values.
    //
    // These goldens are z2d 0.12.1's own recorded node sets (see the doc comment
    // above), so loosening them to a tolerance would give up exactly the
    // last-bit oracle agreement they exist to pin — the wrong trade. Neither
    // test covers any unsafe: `grep -c unsafe` over all five `path_fx/*.rs` is
    // 0, so removing them from the Miri lane forfeits no UB coverage. Both run
    // in ci.sh's HARD `cargo test --workspace` step.
    // See .port/phase-2/MIRI_EXCEPTIONS.md, "rustty-z2d path_fx offset e2e".
    #[cfg_attr(
        miri,
        ignore = "Miri libm: hypot differs from the host by ULPs; goldens are bit-exact"
    )]
    fn offset_e2e_inset() {
        let expected = vec![
            // Star
            m(21.301442007780807, 15.0),
            l(23.61896201004171, 15.0),
            l(25.0, 11.054177171547744),
            l(26.38103798995829, 15.0),
            l(28.698557992219197, 15.0),
            l(26.84016931116092, 16.311803774864668),
            l(28.047780421610277, 19.762121233291396),
            l(25.0, 17.610746818037082),
            l(21.952219578389723, 19.762121233291396),
            l(23.15983068883908, 16.31180377486466),
            z(),
            m(21.301442007780807, 15.0),
            // Triangle
            m(13.236067977499792, 108.0),
            l(106.76393202250021, 108.0),
            l(60.0, 14.472135954999583),
            z(),
            m(13.236067977499792, 108.0),
            // Fake curve #1
            m(129.06850713433477, 21.769836444763964),
            l(147.53701445992533, 31.490103458232674),
            l(158.0, 50.51371353109574),
            l(158.0, 69.5278640450004),
            l(148.50928801500012, 88.50928801500015),
            l(129.10557280900008, 98.21114561800017),
            // Fake curve #2
            m(98.58578643762691, 161.4142135623731),
            l(88.8152817055072, 151.64370883025342),
            l(69.5278640450004, 142.0),
            l(50.472135954999594, 142.0),
            l(31.233206599390776, 151.61946467780442),
            l(22.486588292494332, 161.3379294632449),
            // Single line
            m(8.585786437626904, 301.4142135623731),
            l(108.58578643762691, 401.4142135623731),
        ];

        let out = run(&e2e_input(), 0.1, -2.0).unwrap();
        assert_eq!(expected, out);
    }

    /// Ported verbatim from the reference's `test "offset, e2e"` (outset half).
    #[test]
    // Miri's libm is not bit-faithful for the IMPRECISE math this path uses.
    // `input_set.rs:320`'s `dx.hypot(dy)` differs from the host libm by 1-4 ULP
    // under Miri, and the error propagates into every derived node: measured
    // with a standalone probe, `hypot(3,4)` is `0x4014000000000001` under Miri
    // where the host (and IEEE) give exactly 5.0 = `0x4014000000000000`, while
    // `sqrt` — which IEEE requires to be correctly rounded, and which Miri
    // implements exactly — agrees bit-for-bit on the same inputs. So the
    // divergence is Miri's math library, not this port's arithmetic. It is also
    // stable, not random: identical across repeated runs and across
    // `-Zmiri-seed` values.
    //
    // These goldens are z2d 0.12.1's own recorded node sets (see the doc comment
    // above), so loosening them to a tolerance would give up exactly the
    // last-bit oracle agreement they exist to pin — the wrong trade. Neither
    // test covers any unsafe: `grep -c unsafe` over all five `path_fx/*.rs` is
    // 0, so removing them from the Miri lane forfeits no UB coverage. Both run
    // in ci.sh's HARD `cargo test --workspace` step.
    // See .port/phase-2/MIRI_EXCEPTIONS.md, "rustty-z2d path_fx offset e2e".
    #[cfg_attr(
        miri,
        ignore = "Miri libm: hypot differs from the host by ULPs; goldens are bit-exact"
    )]
    fn offset_e2e_outset() {
        let expected = vec![
            // Star
            m(8.698557992219195, 11.0),
            l(20.78103798995829, 11.0),
            l(25.0, -1.0541771715477442),
            l(29.21896201004171, 11.0),
            l(41.3014420077808, 11.0),
            l(31.612660877518323, 17.83913962136175),
            l(35.95221957838972, 30.237878766708604),
            l(25.0, 22.506900240786447),
            l(14.047780421610279, 30.237878766708604),
            l(18.387339122481677, 17.839139621361745),
            z(),
            m(8.698557992219195, 11.0),
            // Triangle
            m(6.763932022500208, 112.0),
            l(113.23606797749979, 112.0),
            l(60.0, 5.527864045000417),
            z(),
            m(6.763932022500208, 112.0),
            // Fake curve #1
            m(130.93149286566523, 18.230163555236036),
            l(150.46298554007467, 28.509896541767326),
            l(162.0, 49.48628646890426),
            l(162.0, 70.4721359549996),
            l(151.49071198499988, 91.49071198499985),
            l(130.89442719099992, 101.78885438199983),
            // Fake curve #2
            m(101.41421356237309, 158.5857864376269),
            l(91.1847182944928, 148.35629116974658),
            l(70.4721359549996, 138.0),
            l(49.527864045000406, 138.0),
            l(28.766793400609224, 148.38053532219558),
            l(19.513411707505668, 158.6620705367551),
            // Single line
            m(11.414213562373096, 298.5857864376269),
            l(111.41421356237309, 398.5857864376269),
        ];

        let out = run(&e2e_input(), 0.1, 2.0).unwrap();
        assert_eq!(expected, out);
    }

    /// Ported verbatim from the reference's `test "offset, zero value"`.
    #[test]
    fn offset_zero_value_is_a_noop() {
        let in_ = e2e_input();
        let out = run(&in_, 0.1, 0.0).unwrap();
        assert_eq!(in_, out);
    }

    /// The plan's own case list (P3-W8 T3 Step 1): a closed rect inset by 2 must
    /// shrink by 2 on every side. Written independently of the reference's e2e
    /// numbers so the algorithm is pinned by a value anyone can derive by hand.
    #[test]
    fn offset_closed_rect_inset() {
        let in_ = vec![
            m(0.0, 0.0),
            l(10.0, 0.0),
            l(10.0, 10.0),
            l(0.0, 10.0),
            z(),
            m(0.0, 0.0),
        ];
        let out = run(&in_, 0.1, -2.0).unwrap();
        assert_eq!(
            vec![
                m(2.0, 2.0),
                l(8.0, 2.0),
                l(8.0, 8.0),
                l(2.0, 8.0),
                z(),
                m(2.0, 2.0),
            ],
            out
        );
    }

    /// A closed rect *outset* by 2 grows by 2 on every side.
    #[test]
    fn offset_closed_rect_outset() {
        let in_ = vec![
            m(0.0, 0.0),
            l(10.0, 0.0),
            l(10.0, 10.0),
            l(0.0, 10.0),
            z(),
            m(0.0, 0.0),
        ];
        let out = run(&in_, 0.1, 2.0).unwrap();
        assert_eq!(
            vec![
                m(-2.0, -2.0),
                l(12.0, -2.0),
                l(12.0, 12.0),
                l(-2.0, 12.0),
                z(),
                m(-2.0, -2.0),
            ],
            out
        );
    }

    /// An open two-segment path: the caps stay perpendicular (`offset_single`)
    /// and the interior vertex is a projected line-line intersection.
    #[test]
    fn offset_open_path() {
        let in_ = vec![m(0.0, 0.0), l(10.0, 0.0), l(10.0, 10.0)];
        let out = run(&in_, 0.1, -2.0).unwrap();
        assert_eq!(vec![m(0.0, 2.0), l(8.0, 2.0), l(8.0, 10.0)], out);
    }

    /// Degenerate input: consecutive duplicate points contribute no segment
    /// (`InputSet::from_nodes` skips them), and a path that degenerates to a
    /// single point produces nothing at all.
    #[test]
    fn offset_degenerate_segments() {
        // Duplicate points inside an otherwise valid rect are dropped, so the
        // result matches the non-degenerate rect exactly.
        let in_ = vec![
            m(0.0, 0.0),
            l(0.0, 0.0),
            l(10.0, 0.0),
            l(10.0, 0.0),
            l(10.0, 10.0),
            l(0.0, 10.0),
            l(0.0, 10.0),
            z(),
            m(0.0, 0.0),
        ];
        let out = run(&in_, 0.1, -2.0).unwrap();
        assert_eq!(
            vec![
                m(2.0, 2.0),
                l(8.0, 2.0),
                l(8.0, 8.0),
                l(2.0, 8.0),
                z(),
                m(2.0, 2.0),
            ],
            out
        );

        // A zero-length "path" (move_to to the same point, then close) yields
        // no contours and therefore no nodes.
        let empty = run(&[m(5.0, 5.0), l(5.0, 5.0), z(), m(5.0, 5.0)], 0.1, -2.0).unwrap();
        assert!(empty.is_empty(), "expected no nodes, got {empty:?}");

        // A bare move_to is likewise ignored.
        let bare = run(&[m(5.0, 5.0)], 0.1, -2.0).unwrap();
        assert!(bare.is_empty(), "expected no nodes, got {bare:?}");
    }

    /// A curve_to is flattened through the spline decomposer before offsetting,
    /// so `tolerance` is honoured and no `curve_to` survives into the output.
    #[test]
    fn offset_flattens_curves() {
        let in_ = vec![
            m(0.0, 0.0),
            PathNode::CurveTo(crate::internal::path_nodes::PathCurveTo {
                p1: Point { x: 10.0, y: 0.0 },
                p2: Point { x: 20.0, y: 10.0 },
                p3: Point { x: 20.0, y: 20.0 },
            }),
        ];
        let out = run(&in_, 0.1, -1.0).unwrap();
        assert!(out.len() > 2, "curve should flatten to many nodes");
        assert!(
            out.iter().all(|n| !matches!(n, PathNode::CurveTo(_))),
            "no curve_to may survive flattening"
        );
    }
}
