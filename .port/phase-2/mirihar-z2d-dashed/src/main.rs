//! Isolated, non-FFI Miri harness that faithfully models the borrow structure of
//! `dashed_plotter.rs`'s `joinAndCapInitial` — the primary SB/TB hazard of the
//! dashed tessellation port (MEMORY "long-lived raw alias into a field
//! interleaved with &mut to that field").
//!
//! The Zig `Plotter.InitialPolygon` embeds `result: *Polygon` aliasing the
//! plotter's own `result`, and `joinAndCapInitial` interleaves that alias with
//! `&mut self.result` mutations (the final `join`, the `concat`/`reassign`
//! surgery, and `plotOpenJoined(InitialPolygon, &initial, ...)`). Two candidates
//! (selected by argv[1], so Miri only interprets the executed path):
//!
//!   * `safe`     — the design the rustty port actually uses: `InitialPolygon`
//!                  stores NO result pointer; an `InitialView` stitches the
//!                  plotter's `result` (borrowed disjointly) to the initial
//!                  polygon's own fields. This must be clean under BOTH Stacked
//!                  Borrows and Tree Borrows.
//!
//!   * `rawalias` — the naive port that stores a `*mut Polygon` in the initial
//!                  polygon and uses it after an intervening `&mut self.result`.
//!                  This is the UB the safe design avoids; it must be REJECTED by
//!                  Miri (run it to confirm the hazard is real).
//!
//! `Contour` is modelled as an index arena (`first: Option<usize>`, node Vec),
//! exactly like the real `polygon::Contour`, so the `concat`/`plot` moves carry
//! no raw aliasing of their own — the hazard is solely the self-referential
//! `result`.
#![allow(dead_code)]

use std::env;
use std::process;

// ---- Point / Edge ----
#[derive(Clone, Copy)]
struct Point {
    x: f64,
    y: f64,
}

#[derive(Clone, Copy)]
struct Edge {
    x0: f64,
    x1: f64,
}

// ---- Contour: index-arena doubly-linked list (models polygon::Contour) ----
#[derive(Clone, Copy)]
struct Node {
    point: Point,
    prev: Option<usize>,
    next: Option<usize>,
}

struct Contour {
    nodes: Vec<Node>,
    first: Option<usize>,
    last: Option<usize>,
    len: usize,
}

impl Contour {
    fn new() -> Contour {
        Contour {
            nodes: Vec::new(),
            first: None,
            last: None,
            len: 0,
        }
    }

    fn corners_first(&self) -> Option<usize> {
        self.first
    }

    fn plot(&mut self, point: Point, before: Option<usize>) {
        let idx = self.nodes.len();
        self.nodes.push(Node {
            point,
            prev: None,
            next: None,
        });
        match before {
            Some(b) => {
                let p = self.nodes[b].prev;
                self.nodes[idx].prev = p;
                self.nodes[idx].next = Some(b);
                self.nodes[b].prev = Some(idx);
                match p {
                    Some(pi) => self.nodes[pi].next = Some(idx),
                    None => self.first = Some(idx),
                }
            }
            None => {
                self.nodes[idx].prev = self.last;
                match self.last {
                    Some(li) => self.nodes[li].next = Some(idx),
                    None => self.first = Some(idx),
                }
                self.last = Some(idx);
            }
        }
        self.len += 1;
    }

    fn plot_reverse(&mut self, point: Point) {
        let idx = self.nodes.len();
        self.nodes.push(Node {
            point,
            prev: None,
            next: self.first,
        });
        match self.first {
            Some(fi) => self.nodes[fi].prev = Some(idx),
            None => self.last = Some(idx),
        }
        self.first = Some(idx);
        self.len += 1;
    }

    fn concat(&mut self, other: &mut Contour) {
        if other.len == 0 {
            other.first = None;
            other.last = None;
            other.nodes.clear();
            return;
        }
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
        other.first = None;
        other.last = None;
        other.len = 0;
        other.nodes.clear();
    }
}

// ---- Polygon: the shared result ----
struct Polygon {
    edges: Vec<Edge>,
}

impl Polygon {
    fn new() -> Polygon {
        Polygon { edges: Vec::new() }
    }
    fn add_edges_from_contour(&mut self, c: &Contour) {
        // Walk the arena in list order, emitting one edge per adjacent pair.
        let mut cur = c.first;
        let mut prev: Option<Point> = None;
        while let Some(i) = cur {
            let p = c.nodes[i].point;
            if let Some(pp) = prev {
                self.edges.push(Edge { x0: pp.x, x1: p.x });
            }
            prev = Some(p);
            cur = c.nodes[i].next;
        }
    }
}

// ================= SAFE candidate (the rustty design) =================

/// The bundle of disjoint mutable references (models `StrokeState`).
struct State<'s> {
    result: &'s mut Polygon,
    outer: &'s mut Contour,
    inner: &'s mut Contour,
}

trait Plot {
    fn state(&mut self) -> State<'_>;
}

struct InitialPolygon {
    outer: Contour,
    inner: Contour,
}

/// Stitches the plotter's `result` (borrowed disjointly) to the initial
/// polygon's own fields — NO stored result pointer. Models `InitialView`.
struct InitialView<'v> {
    result: &'v mut Polygon,
    ip: &'v mut InitialPolygon,
}

impl Plot for InitialView<'_> {
    fn state(&mut self) -> State<'_> {
        State {
            result: self.result,
            outer: &mut self.ip.outer,
            inner: &mut self.ip.inner,
        }
    }
}

struct SafePlotter {
    result: Polygon,
    outer: Contour,
    inner: Contour,
    initial: Option<InitialPolygon>,
}

impl Plot for SafePlotter {
    fn state(&mut self) -> State<'_> {
        State {
            result: &mut self.result,
            outer: &mut self.outer,
            inner: &mut self.inner,
        }
    }
}

// Generic helpers — the `fn(T: type, self: *T, ...)` shape as free fns over Plot.
fn generic_join(p: &mut impl Plot, a: Point, b: Point, before: Option<usize>) {
    let st = p.state();
    st.outer.plot(a, before);
    st.inner.plot_reverse(b);
}

fn generic_plot_open_joined(p: &mut impl Plot, a: Point, b: Point) {
    let st = p.state();
    st.outer.plot(a, None);
    st.inner.plot_reverse(b);
    // concat inner into outer (both disjoint fields of st)
    let (outer, inner) = (&mut *st.outer, &mut *st.inner);
    outer.concat(inner);
    // add edges from the combined outer into the shared result
    st.result.add_edges_from_contour(&*st.outer);
    *st.outer = Contour::new();
    *st.inner = Contour::new();
}

impl SafePlotter {
    fn new() -> SafePlotter {
        let mut ip = InitialPolygon {
            outer: Contour::new(),
            inner: Contour::new(),
        };
        // seed the initial polygon with a couple corners
        ip.outer.plot(Point { x: 0.0, y: 0.0 }, None);
        ip.outer.plot(Point { x: 1.0, y: 0.0 }, None);
        ip.inner.plot(Point { x: 0.0, y: 1.0 }, None);

        let mut sp = SafePlotter {
            result: Polygon::new(),
            outer: Contour::new(),
            inner: Contour::new(),
            initial: Some(ip),
        };
        // seed the plotter's own contours
        sp.outer.plot(Point { x: 2.0, y: 2.0 }, None);
        sp.outer.plot(Point { x: 3.0, y: 2.0 }, None);
        sp.inner.plot(Point { x: 2.0, y: 3.0 }, None);
        sp
    }

    /// Faithful shape of `joinAndCapInitial` (the > 2 branch): a final join on
    /// the plotter, then concat/reassign surgery moving the plotter's contours
    /// into the initial polygon, then `plot_open_joined` against the initial
    /// polygon via the view — all interleaved with `self.result`.
    fn join_and_cap_initial(&mut self) {
        // Final join on the plotter itself (mutates self.result-adjacent state).
        generic_join(
            self,
            Point { x: 4.0, y: 4.0 },
            Point { x: 4.0, y: 5.0 },
            None,
        );

        // Surgery + cap via the view: borrow result + initial disjointly.
        let result = &mut self.result;
        let self_outer = &mut self.outer;
        let self_inner = &mut self.inner;
        let ip = self.initial.as_mut().unwrap();

        self_outer.concat(&mut ip.outer);
        ip.inner.concat(self_inner);
        ip.outer = std::mem::replace(self_outer, Contour::new());

        let mut view = InitialView { result, ip };
        generic_plot_open_joined(
            &mut view,
            Point { x: 6.0, y: 6.0 },
            Point { x: 6.0, y: 7.0 },
        );

        // Touch the result again after the view is done, to exercise the
        // plotter's own `result` post-surgery (must be a valid, unique borrow).
        self.result.add_edges_from_contour(&self.outer);
        self.initial = None;
        self.outer = Contour::new();
        self.inner = Contour::new();
    }
}

fn run_safe() {
    let mut sp = SafePlotter::new();
    sp.join_and_cap_initial();
    // Consume the result so the writes are observable.
    let mut acc = 0.0;
    for e in &sp.result.edges {
        acc += e.x0 + e.x1;
    }
    println!("safe: {} edges, acc={}", sp.result.edges.len(), acc);
}

// ============== RAWALIAS candidate (the UB the design avoids) ==============

struct BadInitial {
    outer: Contour,
    // The hazard: a raw pointer to the plotter's `result`, stored in the
    // sub-struct and used after an intervening `&mut self.result`.
    result: *mut Polygon,
}

struct BadPlotter {
    result: Polygon,
    outer: Contour,
    initial: Option<BadInitial>,
}

impl BadPlotter {
    fn new() -> BadPlotter {
        BadPlotter {
            result: Polygon::new(),
            outer: {
                let mut c = Contour::new();
                c.plot(Point { x: 0.0, y: 0.0 }, None);
                c
            },
            initial: None,
        }
    }

    fn join_and_cap_initial_bad(&mut self) {
        // Stash a raw alias to result inside the sub-struct.
        let raw: *mut Polygon = &mut self.result;
        self.initial = Some(BadInitial {
            outer: {
                let mut c = Contour::new();
                c.plot(Point { x: 1.0, y: 1.0 }, None);
                c
            },
            result: raw,
        });

        // Intervening unique &mut to self.result (the "final join" writing to
        // the plotter's own result). Under Stacked/Tree Borrows this reborrow
        // invalidates the earlier raw pointer.
        self.result.add_edges_from_contour(&self.outer);

        // Now use the stale raw alias — SB/TB UB.
        let ip = self.initial.as_ref().unwrap();
        unsafe {
            (*ip.result).add_edges_from_contour(&ip.outer);
        }
    }
}

fn run_rawalias() {
    let mut bp = BadPlotter::new();
    bp.join_and_cap_initial_bad();
    println!("rawalias: {} edges", bp.result.edges.len());
}

fn main() {
    let which = env::args().nth(1).unwrap_or_else(|| "safe".to_string());
    match which.as_str() {
        "safe" => run_safe(),
        "rawalias" => run_rawalias(),
        other => {
            eprintln!("unknown candidate: {other} (expected: safe | rawalias)");
            process::exit(2);
        }
    }
}
