// SPDX-License-Identifier: MPL-2.0
//   Copyright © 2024-2025 Chris Marchesi
//
// Ported to Rust for rustty from z2d 0.10.0 `src/internal/tess/Polygon.zig`
// (R3 vendored dep).
//
// SUBSET NOTE (rustty): `Contour` / `addEdgesFromContour` (used by the stroke
// tessellator) ARE ported here (they were previously out of the fill-only
// subset). The Zig `Contour` is a `std.DoublyLinkedList` of heap `Corner` nodes
// spliced via `@fieldParentPtr`; the Rust port models it as an index-arena
// doubly-linked list (`nodes: Vec<Node>` with `prev`/`next` indices). Nodes are
// never removed individually — only the whole contour is dropped or moved — so
// the arena indices are stable and the model is faithful (and raw-pointer-free,
// which keeps the Miri gate clean).
//
// The Zig `WorkingEdgeSet` re-orders the *shared* polygon edge list in place
// through a const pointer; here it owns a copy of the edge list instead. This is
// behavior-equivalent for the rasterizer because the active edge set is
// re-selected on every breakpoint and re-sorted by x before winding — so the
// persistent reordering is unobservable in the output (and the copy is
// raw-pointer-free, which keeps the Miri gate clean).

use crate::options::FillRule;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Edge {
    pub y0: f64,
    pub y1: f64,
    pub x_start: f64,
    pub x_inc: f64,
}

impl Edge {
    fn dir(&self) -> i32 {
        if self.y0 < self.y1 {
            return -1;
        }
        1
    }

    fn top(&self) -> f64 {
        if self.y0 < self.y1 {
            return self.y0;
        }
        self.y1
    }

    fn bottom(&self) -> f64 {
        if self.y0 < self.y1 {
            return self.y1;
        }
        self.y0
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Polygon {
    pub edges: Vec<Edge>,
    /// NOTE: Only needs to be set if adding edges directly (not from contours).
    pub scale: f64,
    pub extent_top: f64,
    pub extent_bottom: f64,
    pub extent_left: f64,
    pub extent_right: f64,
}

impl Polygon {
    pub fn new(scale: f64) -> Polygon {
        Polygon {
            edges: Vec::new(),
            scale,
            extent_top: 0.0,
            extent_bottom: 0.0,
            extent_left: 0.0,
            extent_right: 0.0,
        }
    }

    pub fn add_edge(
        &mut self,
        p0: crate::internal::point::Point,
        p1: crate::internal::point::Point,
    ) {
        // assert for NaNs
        debug_assert!(p0.x.is_finite() && p0.y.is_finite());
        debug_assert!(p1.x.is_finite() && p1.y.is_finite());
        let p0_scaled = (p0.x * self.scale, p0.y * self.scale);
        let p1_scaled = (p1.x * self.scale, p1.y * self.scale);

        let edge: Edge = if p0_scaled.1 < p1_scaled.1 {
            // Down edge
            Edge {
                y0: p0_scaled.1,
                y1: p1_scaled.1,
                x_start: p0_scaled.0,
                x_inc: (p1_scaled.0 - p0_scaled.0) / (p1_scaled.1 - p0_scaled.1),
            }
        } else if p0_scaled.1 > p1_scaled.1 {
            // Up edge
            Edge {
                y0: p0_scaled.1,
                y1: p1_scaled.1,
                x_start: p1_scaled.0,
                x_inc: (p0_scaled.0 - p1_scaled.0) / (p0_scaled.1 - p1_scaled.1),
            }
        } else {
            return; // Filter out horizontal edges
        };

        // Check extents
        let extent_top = edge.top();
        let extent_bottom = edge.bottom();
        let (extent_left, extent_right) = if p0_scaled.0 < p1_scaled.0 {
            (p0_scaled.0, p1_scaled.0)
        } else {
            (p1_scaled.0, p0_scaled.0)
        };
        if self.edges.is_empty() {
            self.extent_top = extent_top;
            self.extent_bottom = extent_bottom;
            self.extent_left = extent_left;
            self.extent_right = extent_right;
        } else {
            if extent_top < self.extent_top {
                self.extent_top = extent_top;
            }
            if extent_bottom > self.extent_bottom {
                self.extent_bottom = extent_bottom;
            }
            if extent_left < self.extent_left {
                self.extent_left = extent_left;
            }
            if extent_right > self.extent_right {
                self.extent_right = extent_right;
            }
        }

        self.edges.push(edge);
    }

    /// Iterate over the supplied contour and add each individual line as an edge.
    ///
    /// Note this does not de-initialize the contour, so deinit needs to be called
    /// after it if that's the intent.
    pub fn add_edges_from_contour(&mut self, contour: &Contour) {
        let mut node_ = contour.first;
        let mut initial_point_: Option<crate::internal::point::Point> = None;
        let mut last_point_: Option<crate::internal::point::Point> = None;
        while let Some(node) = node_ {
            let current_point = contour.nodes[node].point;
            if initial_point_.is_none() {
                initial_point_ = Some(current_point);
            }
            if let Some(last_point) = last_point_ {
                self.add_edge(last_point, current_point);
            }
            last_point_ = Some(current_point);
            node_ = contour.nodes[node].next;
        }
        if let Some(initial_point) = initial_point_ {
            if let Some(last_point) = last_point_ {
                self.add_edge(last_point, initial_point);
            }
        }
    }

    /// Returns true if our polygon list is somewhere within the box starting at
    /// (0,0) with the width `box_width` and the height `box_height`. Used to
    /// check if we should actually proceed with drawing.
    pub fn in_box(&self, scale: f64, box_width: i32, box_height: i32) -> bool {
        if !self.extent_left.is_finite()
            || !self.extent_top.is_finite()
            || !self.extent_right.is_finite()
            || !self.extent_bottom.is_finite()
        {
            panic!("invalid polygon dimensions. this is a bug, please report it");
        }

        if !scale.is_finite() || scale < 1.0 {
            panic!("invalid value for scale. this is a bug, please report it");
        }

        if box_width < 1 || box_height < 1 {
            panic!("invalid box width or height. this is a bug, please report it");
        }

        // Round our polygon to the appropriate dimensions. For scanline fill, we
        // make sure to push our box out so the whole of the polygon fits in the
        // box.
        let poly_start_x: i32 = (self.extent_left / scale).floor() as i32;
        let poly_start_y: i32 = (self.extent_top / scale).floor() as i32;
        let poly_end_x: i32 = (self.extent_right / scale).ceil() as i32;
        let poly_end_y: i32 = (self.extent_bottom / scale).ceil() as i32;

        // Our polygon width and height
        let poly_width: i32 = poly_end_x - poly_start_x;
        let poly_height: i32 = poly_end_y - poly_start_y;

        // If this happens, there's an error in our polygon plotting.
        if poly_width < 0 || poly_height < 0 {
            panic!("negative polygon width or height. this is a bug, please report it");
        }

        // If one of our width or height are zero, nothing drawable was
        // ultimately plotted, possibly a degenerate case.
        if poly_width == 0 || poly_height == 0 {
            return false;
        }

        // Check the upper-left of our drawing area to make sure, that in the
        // case of negative start offsets, we actually reach into the surface. If
        // we don't (i.e., if it's not wide or high enough), we're not in the
        // box.
        if poly_start_x + poly_width < 0 || poly_start_y + poly_height < 0 {
            return false;
        }

        // Finally, if our start co-ordinates are outside the right or upper
        // bounds of the surface, we're not in the box.
        if poly_start_x >= box_width || poly_start_y >= box_height {
            return false;
        }

        // We're in the box.
        true
    }
}

/// The active-edge table used during scanline rasterization. See the module
/// note for how this differs (unobservably) from the Zig original.
pub struct WorkingEdgeSet {
    all: Vec<Edge>,
    edges: Vec<Edge>,
    x_values: Vec<i32>,
}

impl WorkingEdgeSet {
    pub fn init(polygon: &Polygon) -> WorkingEdgeSet {
        WorkingEdgeSet {
            all: polygon.edges.clone(),
            edges: Vec::new(),
            x_values: Vec::new(),
        }
    }

    pub fn breakpoints(&self) -> Vec<i32> {
        fn insert(list: &mut Vec<i32>, value: i32) {
            if list.is_empty() {
                list.push(value);
                return;
            }
            let mut low: usize = 0;
            let mut high: usize = list.len();
            while low < high {
                let mid = low + (high - low) / 2;
                if list[mid] < value {
                    low = mid + 1;
                } else {
                    high = mid;
                }
            }
            let insertion_idx = low;
            if insertion_idx == list.len() {
                list.push(value);
            } else if list[insertion_idx] != value {
                list.insert(insertion_idx, value);
            }
        }

        let mut result: Vec<i32> = Vec::with_capacity(self.all.len() * 2);
        for e in &self.all {
            insert(&mut result, e.top().round() as i32);
            insert(&mut result, e.bottom().round() as i32);
        }
        result
    }

    pub fn rescan(&mut self, line_y: i32) {
        if self.all.is_empty() {
            return;
        }

        // We take our line measurements at the middle of the line; this helps
        // "break the tie" with lines that fall exactly on point boundaries.
        let line_y_middle = line_y as f64 + 0.5;

        self.edges.clear();
        for e in &self.all {
            if e.top() < line_y_middle && e.bottom() >= line_y_middle {
                self.edges.push(*e);
            }
        }
        self.x_values = vec![0; self.edges.len()];

        debug_assert!(self.edges.len() == self.x_values.len());
    }

    pub fn inc(&mut self, y: i32) {
        let y_mid = y as f64 + 0.5;
        for (idx, edge) in self.edges.iter().enumerate() {
            self.x_values[idx] =
                (edge.x_start + (edge.x_inc * (y_mid - edge.top()))).round() as i32;
        }
    }

    pub fn sort(&mut self) {
        // Sort edges and x_values together by x value. z2d uses `pdqContext`
        // (an unstable in-place sort); we mirror the "swap both arrays"
        // ordering with an index permutation.
        let n = self.x_values.len();
        let mut order: Vec<usize> = (0..n).collect();
        order.sort_unstable_by(|&a, &b| self.x_values[a].cmp(&self.x_values[b]));
        let new_edges: Vec<Edge> = order.iter().map(|&i| self.edges[i]).collect();
        let new_x: Vec<i32> = order.iter().map(|&i| self.x_values[i]).collect();
        self.edges = new_edges;
        self.x_values = new_x;
    }

    pub fn filter(&mut self, fill_rule: FillRule) -> &[i32] {
        // In-place filter depending on the fill rule (even-odd is just
        // pass-thru).
        match fill_rule {
            FillRule::EvenOdd => &self.x_values,
            FillRule::NonZero => {
                // Filter in-place based on winding rule; consecutive edges in
                // the same direction are filtered out after the first one, so
                // that only one set of each direction happens one after the
                // other.
                let mut winding_number: i32 = 0;
                let mut to: usize = 0;
                for from in 0..self.x_values.len() {
                    self.x_values[to] = self.x_values[from];
                    if winding_number == 0 {
                        winding_number += self.edges[from].dir();
                        to += 1;
                    } else {
                        winding_number += self.edges[from].dir();
                        if winding_number == 0 {
                            to += 1;
                        }
                    }
                }
                &self.x_values[0..to]
            }
        }
    }
}

/// A node in a `Contour`'s index-arena doubly-linked list. See the module note
/// for how this models the Zig `std.DoublyLinkedList` of `Corner` nodes.
#[derive(Debug, Clone, Copy)]
struct Node {
    point: crate::internal::point::Point,
    prev: Option<usize>,
    next: Option<usize>,
}

/// Represents a polyline (contour) that will be later assembled into a
/// polygon, or converted to a set of edges to be added to a larger
/// polygon/edge collection.
#[derive(Debug, Clone)]
pub struct Contour {
    nodes: Vec<Node>,
    first: Option<usize>,
    last: Option<usize>,
    pub len: usize,
    pub scale: f64,
}

impl Contour {
    pub fn new(scale: f64) -> Contour {
        Contour {
            nodes: Vec::new(),
            first: None,
            last: None,
            len: 0,
            scale,
        }
    }

    /// The arena index of the first corner, for use as a `before` handle in
    /// `plot`. Mirrors reading `corners.first` in the Zig original.
    pub fn corners_first(&self) -> Option<usize> {
        self.first
    }

    /// The (scaled) point stored at arena corner `idx`. Mirrors reading
    /// `Contour.Corner.fromNode(node).point` in the Zig original.
    pub fn corner_point(&self, idx: usize) -> crate::internal::point::Point {
        self.nodes[idx].point
    }

    /// The next corner after `idx`, or `None` at the tail. Mirrors `node.next`.
    pub fn corner_next(&self, idx: usize) -> Option<usize> {
        self.nodes[idx].next
    }

    /// Plots a point on the contour. If `before` is specified, the point is
    /// plotted before it.
    pub fn plot(&mut self, point: crate::internal::point::Point, before_: Option<usize>) {
        debug_assert!(point.x.is_finite() && point.y.is_finite());
        let scaled = crate::internal::point::Point {
            x: point.x * self.scale,
            y: point.y * self.scale,
        };

        let idx = self.nodes.len();
        self.nodes.push(Node {
            point: scaled,
            prev: None,
            next: None,
        });
        match before_ {
            Some(before) => self.insert_before(before, idx),
            None => self.append(idx),
        }
        self.len += 1;
    }

    /// Like `plot`, but adds points in the reverse direction (i.e., at the start
    /// of the polygon instead of the end).
    pub fn plot_reverse(&mut self, point: crate::internal::point::Point) {
        debug_assert!(point.x.is_finite() && point.y.is_finite());
        let scaled = crate::internal::point::Point {
            x: point.x * self.scale,
            y: point.y * self.scale,
        };

        let idx = self.nodes.len();
        self.nodes.push(Node {
            point: scaled,
            prev: None,
            next: None,
        });
        self.prepend(idx);
        self.len += 1;
    }

    fn append(&mut self, idx: usize) {
        self.nodes[idx].prev = self.last;
        self.nodes[idx].next = None;
        match self.last {
            Some(li) => self.nodes[li].next = Some(idx),
            None => self.first = Some(idx),
        }
        self.last = Some(idx);
    }

    fn prepend(&mut self, idx: usize) {
        self.nodes[idx].next = self.first;
        self.nodes[idx].prev = None;
        match self.first {
            Some(fi) => self.nodes[fi].prev = Some(idx),
            None => self.last = Some(idx),
        }
        self.first = Some(idx);
    }

    fn insert_before(&mut self, before: usize, idx: usize) {
        let p = self.nodes[before].prev;
        self.nodes[idx].prev = p;
        self.nodes[idx].next = Some(before);
        self.nodes[before].prev = Some(idx);
        match p {
            Some(pi) => self.nodes[pi].next = Some(idx),
            None => self.first = Some(idx),
        }
    }

    /// Concatenates a contour into this one and detaches its contours from its
    /// linked list.
    pub fn concat(&mut self, other: &mut Contour) {
        if other.len == 0 {
            // Nothing to move; still reset the (already empty) other to mirror
            // `concatByMoving` resetting the source list.
            other.first = None;
            other.last = None;
            other.nodes.clear();
            return;
        }
        // Copy `other`'s nodes into our arena, rebasing their indices by our
        // current length (arena indices are stable, so this is a faithful splice
        // of the two doubly-linked lists).
        let offset = self.nodes.len();
        for n in &other.nodes {
            self.nodes.push(Node {
                point: n.point,
                prev: n.prev.map(|i| i + offset),
                next: n.next.map(|i| i + offset),
            });
        }
        let other_first = other.first.unwrap() + offset;
        let other_last = other.last.unwrap() + offset;
        match self.last {
            Some(li) => {
                self.nodes[li].next = Some(other_first);
                self.nodes[other_first].prev = Some(li);
            }
            None => self.first = Some(other_first),
        }
        self.last = Some(other_last);
        self.len += other.len;

        // Reset the other list to empty.
        other.first = None;
        other.last = None;
        other.len = 0;
        other.nodes.clear();
    }
}

/// Convenience functionality for a set of polylines over an `ArrayList`.
///
/// Port of `Polygon.zig` `Contour.List` (the hairline/polyline plotter builds
/// one of these; the multisample fill path uses `Polygon` instead). In
/// allocator-free Rust the `std.ArrayListUnmanaged(Contour)` is a `Vec<Contour>`;
/// the Zig `deinit` (which frees each contour's corners, then the list) is Rust
/// `Drop`.
#[derive(Debug, Clone, Default)]
pub struct ContourList {
    pub list: Vec<Contour>,
}

impl ContourList {
    /// Port of `Contour.List.empty`.
    pub fn empty() -> ContourList {
        ContourList { list: Vec::new() }
    }

    /// Port of `Contour.List.append`. (`deinit` is Rust `Drop`.)
    pub fn append(&mut self, contour: Contour) {
        self.list.push(contour);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn poly(left: f64, top: f64, right: f64, bottom: f64) -> Polygon {
        Polygon {
            edges: Vec::new(),
            scale: 1.0,
            extent_top: top,
            extent_bottom: bottom,
            extent_left: left,
            extent_right: right,
        }
    }

    #[test]
    fn polygon_in_box() {
        // (name, polygon, scale, box_width, box_height, expected). NOTE: the
        // upstream test invokes `inBox(scale, box_height, box_width)` — with
        // width/height swapped — but every case uses equal dimensions, so the
        // swap is immaterial; we reproduce the call as written.
        struct Case {
            name: &'static str,
            polygon: Polygon,
            scale: f64,
            box_width: i32,
            box_height: i32,
            expected: bool,
        }
        let cases = [
            Case {
                name: "basic",
                polygon: poly(10.0, 10.0, 20.0, 20.0),
                scale: 1.0,
                box_height: 30,
                box_width: 30,
                expected: true,
            },
            Case {
                name: "overlap, upper",
                polygon: poly(2.5, -5.0, 7.5, 5.0),
                scale: 1.0,
                box_height: 10,
                box_width: 10,
                expected: true,
            },
            Case {
                name: "overlap, lower",
                polygon: poly(2.5, 5.0, 7.5, 15.0),
                scale: 1.0,
                box_height: 10,
                box_width: 10,
                expected: true,
            },
            Case {
                name: "overlap, left",
                polygon: poly(-5.0, 2.5, 5.0, 7.5),
                scale: 1.0,
                box_height: 10,
                box_width: 10,
                expected: true,
            },
            Case {
                name: "overlap, right",
                polygon: poly(5.0, 2.5, 15.0, 7.5),
                scale: 1.0,
                box_height: 10,
                box_width: 10,
                expected: true,
            },
            Case {
                name: "overlap, upper left",
                polygon: poly(-5.0, -5.0, 5.0, 5.0),
                scale: 1.0,
                box_height: 10,
                box_width: 10,
                expected: true,
            },
            Case {
                name: "overlap, upper right",
                polygon: poly(5.0, -5.0, 15.0, 5.0),
                scale: 1.0,
                box_height: 10,
                box_width: 10,
                expected: true,
            },
            Case {
                name: "overlap, lower left",
                polygon: poly(-5.0, 5.0, 5.0, 15.0),
                scale: 1.0,
                box_height: 10,
                box_width: 10,
                expected: true,
            },
            Case {
                name: "overlap, lower right",
                polygon: poly(5.0, 5.0, 15.0, 15.0),
                scale: 1.0,
                box_height: 10,
                box_width: 10,
                expected: true,
            },
            Case {
                name: "OOB, upper",
                polygon: poly(2.5, -15.0, 7.5, -5.0),
                scale: 1.0,
                box_height: 10,
                box_width: 10,
                expected: false,
            },
            Case {
                name: "OOB, lower",
                polygon: poly(2.5, 15.0, 7.5, 25.0),
                scale: 1.0,
                box_height: 10,
                box_width: 10,
                expected: false,
            },
            Case {
                name: "OOB, left",
                polygon: poly(-15.0, 2.5, -5.0, 7.5),
                scale: 1.0,
                box_height: 10,
                box_width: 10,
                expected: false,
            },
            Case {
                name: "OOB, right",
                polygon: poly(15.0, 2.5, 25.0, 7.5),
                scale: 1.0,
                box_height: 10,
                box_width: 10,
                expected: false,
            },
            Case {
                name: "OOB, upper left",
                polygon: poly(-20.0, -20.0, -10.0, -10.0),
                scale: 1.0,
                box_height: 10,
                box_width: 10,
                expected: false,
            },
            Case {
                name: "OOB, upper right",
                polygon: poly(20.0, -20.0, 30.0, -10.0),
                scale: 1.0,
                box_height: 10,
                box_width: 10,
                expected: false,
            },
            Case {
                name: "OOB, lower left",
                polygon: poly(-20.0, 20.0, -10.0, 30.0),
                scale: 1.0,
                box_height: 10,
                box_width: 10,
                expected: false,
            },
            Case {
                name: "OOB, lower right",
                polygon: poly(20.0, 20.0, 30.0, 30.0),
                scale: 1.0,
                box_height: 10,
                box_width: 10,
                expected: false,
            },
            Case {
                name: "scale",
                polygon: poly(20.0, 20.0, 60.0, 60.0),
                scale: 4.0,
                box_height: 10,
                box_width: 10,
                expected: true,
            },
            Case {
                name: "degenerate x axis",
                polygon: poly(5.0, 2.5, 5.0, 7.5),
                scale: 1.0,
                box_height: 10,
                box_width: 10,
                expected: false,
            },
            Case {
                name: "degenerate y axis",
                polygon: poly(2.5, 5.0, 7.5, 5.0),
                scale: 1.0,
                box_height: 10,
                box_width: 10,
                expected: false,
            },
            Case {
                name: "rounding into upper left, x-axis",
                polygon: poly(-2.0, -2.0, -0.75, 0.0),
                scale: 1.0,
                box_height: 10,
                box_width: 10,
                expected: true,
            },
            Case {
                name: "rounding into upper left, y-axis",
                polygon: poly(-2.0, -2.0, 0.0, -0.75),
                scale: 1.0,
                box_height: 10,
                box_width: 10,
                expected: true,
            },
            Case {
                name: "rounding into lower right, x-axis",
                polygon: poly(9.5, 9.0, 11.0, 11.0),
                scale: 1.0,
                box_height: 10,
                box_width: 10,
                expected: true,
            },
            Case {
                name: "rounding into lower right, y-axis",
                polygon: poly(9.0, 9.5, 11.0, 11.0),
                scale: 1.0,
                box_height: 10,
                box_width: 10,
                expected: true,
            },
        ];
        for tc in &cases {
            assert_eq!(
                tc.expected,
                tc.polygon.in_box(tc.scale, tc.box_height, tc.box_width),
                "case: {}",
                tc.name
            );
        }
    }
}
