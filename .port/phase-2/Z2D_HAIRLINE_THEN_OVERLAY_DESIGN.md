# z2d Hairline Stroking → Renderer Overlay Unblock — Design

**Scope:** land the ported z2d **hairline stroking** fast-path in `rustty-z2d`
(the `painter::stroke` `opts.hairline` branch that today is
`unimplemented!("hairline stroking is not part of the z2d sprite subset")` at
`crates/rustty-z2d/src/painter.rs:225-228`), then reuse the already-designed
renderer overlay fill to unblock the four `crates/rustty-renderer/src/overlay.rs`
seams that panic on it. **Design only.** No `crates/` edits performed by this doc.

**Reference (read-only):** z2d 0.10.0, vendored in the Zig package cache at
`/Users/gael/.cache/zig/p/z2d-0.10.0-j5P_Hu-6FgBsZNgwphIqh17jDnj8_yPtD8yzjO6PpHRQ`
(the task's `ghostty/pkg/z2d` path does not exist on this checkout — z2d is a
fetched dependency, not vendored under `pkg/`; this cache dir is the R3 vendored
source the ported crate tracks, confirmed by the `z2d 0.10.0 … (R3 vendored dep)`
provenance headers in every `rustty-z2d/src/*.rs`).

**Rules:** `PORT_RULES.md` — R1 (100% fidelity: every line/comment/test byte-
faithful; the ported hairline/polyline algorithm must match the reference
exactly), R4 (ACYCLIC — z2d is a leaf; renderer already deps z2d, no new edge),
R5 (deferred = `unimplemented!()` seam + verbatim body). G3b comment-coverage
**covers** rustty-z2d — the new modules must carry the port-provenance comments
and stay green.

---

## 0. Headline: the STOP gate resolves **BOUNDED** — land it

`hairline_scope = **bounded**`.

The reference hairline fast-path is **two small, self-contained files** built on
standard, well-understood thin-line algorithms — *not* a large anti-aliased
polyline rasterizer disproportionate to the overlay payoff:

| Reference file | Lines | Reference tests | Algorithm |
|----------------|-------|-----------------|-----------|
| `src/internal/raster/hairline.zig` | 408 | **0** (`grep '^test'` → none) | Bresenham + Xiaolin-Wu 1px line plotter (integer decision params; the Wu variant is a fixed `u16` error accumulator) |
| `src/internal/tess/polyline_plotter.zig` | 221 | **0** | fill-plotter variant that emits a `Contour.List` (open polylines) instead of a `Polygon` |

Everything these two files depend on is **already ported** in `rustty-z2d`:

| Reference dependency | Ported location | Status |
|----------------------|-----------------|--------|
| `raster/shared.zig` `compositeOpaque`/`compositeOpacity` | `src/internal/raster/shared.rs:27,62` (`pub`) | ✅ |
| `Surface.getWidth/getHeight` (→ `i32`), `paintStride`, `compositeStride`, `clearStride`, `getStride` | `src/surface.rs:410,415,248,271,452,…` | ✅ |
| `options.AntiAliasMode {none,default,multisample_4x,supersample_4x}` | `src/options.rs:80` | ✅ |
| `compositor.Operator` / `Precision` / `requiresFloat` | `src/compositor.rs:22,129,95` | ✅ |
| `Contour` (`plot`/`plot_reverse`/`concat`/`corners_first`, index-arena DLL) | `src/internal/tess/polygon.rs:362` | ✅ |
| `Slope`, `Spline` (FnMut sink), `PointBuffer<1,2>`, `PathNode` | `slope.rs`, `spline.rs`, `point_buffer.rs`, `path_nodes.rs` | ✅ |
| `Dasher.validate` (subset guard) | precedent: `stroke_plotter.rs:33 dasher_validate` | ✅ (pattern) |
| `StrokeOptions{ hairline, dashes, dash_offset, operator, precision, tolerance, anti_aliasing_mode }` | `src/painter.rs:136` | ✅ |
| `Context.stroke()` wires `hairline: self.hairline` → `StrokeOptions` | `src/context.rs` (confirmed) | ✅ |

**Net new code** is ~4 items: one new rasterizer module (`hairline.rs`, faithful
408-line port), one new tessellation module (`polyline_plotter.rs`, ~150 lines
after the subset trims the Dasher machinery per the established precedent), a
trivial `Contour::List` (`Vec<Contour>`) wrapper + two corner accessors on the
existing `Contour`, and the 3-line painter branch swap. This is exactly the
"small fast-path + Bresenham/Wu-style thin-line plotter" the task calls
**bounded**. **Disposition: port it.**

### Subset boundary (the one deliberate trim, precedent-backed)

`polyline_plotter.plot` calls `Dasher.validate(dashes)`; on a *valid* dash array
it would build a `Dasher` and take the `dashedLineTo` branches. The z2d sprite
subset **never** ports the `Dasher`/`dashed_plotter` machinery, and
`stroke_plotter.rs` already established the faithful boundary: **port the
`Dasher.validate` guard; `unimplemented!()` if a valid dash array is ever
supplied** (`stroke_plotter.rs:7-17,33-44,59-61`). The overlay passes an **empty**
dash array (`StrokeOptions::default().dashes = Vec::new()`, `context.rs:96`), so
`validate` returns `false`, `dasher` is `None`, and every dashed branch is
statically skipped. This design applies the **identical** boundary to
`polyline_plotter`, keeping the port bounded while staying R1-faithful (the guard
is ported; only the unreachable dashed body is a marked `unimplemented!()` seam).

---

## 1. `Contour::List` + corner accessors (`src/internal/tess/polygon.rs`)

The reference `Contour.List` (`Polygon.zig:426-449`) is a thin `ArrayList(Contour)`
convenience wrapper with `empty` / `deinit` / `append`. In allocator-free Rust
it is a `Vec<Contour>`:

```rust
/// Convenience functionality for a set of polylines over an `ArrayList`.
/// Port of `Polygon.zig` `Contour.List` (the hairline/polyline plotter builds
/// one of these; the multisample fill path uses `Polygon` instead).
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
```

The hairline `run` reads each contour's corner points by walking the DLL
(`contour.corners.first.?` then `.next`, reading `.point`). The ported `Contour`
already exposes `corners_first() -> Option<usize>` but keeps its `nodes` arena
private; add the two read-only accessors that mirror
`Contour.Corner.fromNode(node).point` and `node.next`:

```rust
impl Contour {
    /// The (scaled) point stored at arena corner `idx`. Mirrors reading
    /// `Contour.Corner.fromNode(node).point` in the Zig original.
    pub fn corner_point(&self, idx: usize) -> crate::internal::point::Point {
        self.nodes[idx].point
    }

    /// The next corner after `idx`, or `None` at the tail. Mirrors `node.next`.
    pub fn corner_next(&self, idx: usize) -> Option<usize> {
        self.nodes[idx].next
    }
}
```

> **Naming note (R1-neutral).** The Zig type is `Contour.List`; Rust has no nested
> type namespacing under a struct, so `ContourList` is the faithful spelling
> (same precedent as `WorkingEdgeSet` etc. already in this file). A module-level
> `pub type List = ...` alias is *not* used because `List` unqualified is
> ambiguous at the crate root — call sites read `ContourList`, matching the
> file's flat naming.

---

## 2. `polyline_plotter` (new: `src/internal/tess/polyline_plotter.rs`)

Faithful port of `polyline_plotter.zig`, allocator-free, with the Dasher subset
boundary (§0). Signature mirrors the reference minus the `alloc` arg (the ported
tess layer is allocator-free, matching `stroke_plotter::plot`).

```rust
// SPDX-License-Identifier: MPL-2.0
//   Copyright © 2024-2025 Chris Marchesi
//
// Ported to Rust for rustty from z2d 0.10.0
// `src/internal/tess/polyline_plotter.zig` (R3 vendored dep).
//
// SUBSET NOTE (rustty): identical boundary to `stroke_plotter` — the hairline
// stroke path (the sole caller, via `painter::stroke`'s hairline branch) never
// uses dashed lines, so the `Dasher`/`dashedLineTo` branches are not ported; the
// leading `Dasher.validate` guard IS ported and reports a blocker
// (`unimplemented!`) if a valid dash array is ever supplied. With dashes
// excluded, the spline `line_to` callback (Zig `SplinePlotterCtx`) is expressed
// as a `&mut dyn FnMut(PathLineTo)` sink over the borrowed contour/point-buffer
// — equivalent behavior, no raw pointers (keeps the Miri gate clean), exactly as
// `stroke_plotter` does for its spline decomposition.

use crate::internal::error::InternalError;
use crate::internal::path_nodes::{PathLineTo, PathNode};
use crate::internal::point::Point;
use crate::internal::tess::point_buffer::PointBuffer;
use crate::internal::tess::polygon::{Contour, ContourList};
use crate::internal::tess::slope::Slope;      // (reserved for the dashed path; see note)
use crate::internal::tess::spline::Spline;

/// Converts a path to a `ContourList` (open polylines), not a `Polygon`.
///
/// Port of `polyline_plotter.plot`. See the module SUBSET NOTE for the dashed
/// boundary.
pub fn plot(
    nodes: &[PathNode],
    tolerance: f64,
    dashes: &[f64],
    _dash_offset: f64,
) -> Result<ContourList, InternalError> {
    // Dasher.validate guard — reuse the faithful `dasher_validate` port (see §2.1).
    if dasher_validate(dashes) {
        unimplemented!("dashed polyline tessellation is not part of the z2d sprite subset");
    }

    let mut result = ContourList::empty();
    let mut current_contour = Contour::new(1.0);
    let mut points: PointBuffer<1, 2> = PointBuffer::default();

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
                // Last node = the auto-added move_to after close_path; no-op it.
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
                // sink. The reverse order (design's earlier draft) is a borrow-
                // check error: an immutable `points.last()` read while the
                // mutable `pts` borrow is live and captured by the closure. This
                // mirrors the Zig `points.last().?` on line 96 being read before
                // the `SplinePlotterCtx` is built.
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
                // Skip if buffer only holds a move_to (single point).
                if points.len >= 2 {
                    // No-op if initial and current points are equal.
                    if points.last().unwrap().equal(points.first().unwrap()) {
                        continue;
                    }
                    // (dasher branch elided — subset)
                    current_contour.plot(points.first().unwrap(), None);
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
```

> **Borrow shape.** The `CurveTo` arm reborrows `current_contour` + `points` into
> the closure only for the duration of `spline.decompose()` (a scoped `&mut`),
> then releases them. `spline.a` is read out of `points.last()` into a detached
> `Point` value (`Point: Copy`) *before* `pts = &mut points` is taken, avoiding an
> aliasing conflict (the Zig `points.last().?` on line 96 is likewise read before
> the ctx is built).
>
> **Why the direct sink is viable here (contrast with `stroke_plotter.rs`).**
> `stroke_plotter.rs:159-179` collects the spline `line_to`s into a `Vec` FIRST,
> then replays them — but *only* because its sink fed `self._run_line_to(...)`,
> a method on the shared `self`, which the closure cannot borrow while
> `spline.decompose()` also borrows `self`. Here the sink targets
> `current_contour`/`points`, which are plain **locals** (not fields of a shared
> `self`), so the closure may hold both `&mut` borrows for the whole
> `spline.decompose()` scope with no self-borrow cycle — the direct-sink shape
> compiles and is closer to the reference `SplinePlotterCtx`. (Collect-first would
> also be R1-valid, but is unnecessary and is not used.)

### 2.1 The `Dasher.validate` port — de-dupe vs. duplicate (recommended: shared module)

`Dasher.validate` (`Dasher.zig`) is a real 8-line reference function; today its
port lives **privately** in `stroke_plotter.rs:33` as `dasher_validate`.
`polyline_plotter` needs the same guard.

- **Recommended:** add a tiny faithful `src/internal/tess/dasher.rs` that ports
  **only** `Dasher.validate` (the rest of `Dasher.zig` — `init`/`step`/`reset` —
  stays a documented subset blocker), expose `pub fn validate(dashes: &[f64]) ->
  bool`, and have **both** plotters call `dasher::validate`. This is the honest
  R1 provenance (one named port of one reference function) and de-dupes the
  guard. `stroke_plotter.rs:33-44` is repointed at `dasher::validate` in the same
  change (a 1-line body swap; its SUBSET NOTE already references `Dasher`).
- **Lighter alternative (no stroke_plotter churn):** duplicate the 8-line guard
  privately in `polyline_plotter.rs`. R1-valid but leaves two copies. The
  recommended shared module is preferred; either keeps the gate green.

```rust
// src/internal/tess/dasher.rs  (recommended)
// Ported from z2d 0.10.0 `src/internal/tess/Dasher.zig` (R3 vendored dep).
// SUBSET NOTE (rustty): only `validate` is ported — the dashed-line machinery
// (`init`/`step`/`reset`/`remain`/`on`) is not reached by the sprite subset
// (neither stroke nor hairline uses dashes; both guard on `validate` and
// `unimplemented!()` a valid dash array).

/// Port of `Dasher.validate`: true iff `dashes` is non-negative with at least
/// one positive entry.
pub fn validate(dashes: &[f64]) -> bool {
    let mut valid = false;
    for &d in dashes {
        if d < 0.0 {
            return false;
        }
        if d > 0.0 {
            valid = true;
        }
    }
    valid
}

#[cfg(test)]
mod tests {
    use super::*;

    // Port of `Dasher.zig` `test "validate"` (Dasher.zig:77-98). R1 obligation:
    // by promoting `validate` to a NAMED port in this module, its reference
    // `test{}` must be ported verbatim. Seven cases, mirroring the Zig
    // `testing.expect` / `testing.expect(!...)` assertions.
    #[test]
    fn validate_test() {
        // Good (single)
        assert!(validate(&[1.0]));

        // Good (even)
        assert!(validate(&[1.0, 2.0]));

        // Good (odd)
        assert!(validate(&[1.0, 2.0, 3.0]));

        // Good (zero stops)
        assert!(validate(&[0.0, 2.0, 0.0]));

        // Bad (single zero)
        assert!(!validate(&[0.0]));

        // Bad (all zeroes)
        assert!(!validate(&[0.0, 0.0, 0.0]));

        // Bad (negative value)
        assert!(!validate(&[1.0, -2.0, 3.0]));
    }
}
```

> **R1 fidelity — the ported reference test (adversary fix).** `Dasher.zig` itself
> carries `test "validate"` (Dasher.zig:77-98, 7 assertions). Promoting the
> `validate` guard to a named `dasher.rs` port takes on the R1 obligation to port
> that test — it is ported verbatim in the `mod tests` above. This closes the gap
> the design opened: §5's "the reference has **no** hairline or polyline tests"
> is true only for `hairline.zig`/`polyline_plotter.zig` (both `grep '^test'` →
> 0), but `Dasher.zig` — the source of `validate` — does have one, and it now
> lands with the guard.
>
> **Lighter-alternative caveat.** If the *duplicate-the-guard-privately* path
> (no shared `dasher.rs`) is taken instead, the `test "validate"` still must be
> ported alongside whichever module the `validate` copy lands in (e.g. a private
> `#[cfg(test)]` case in `polyline_plotter.rs`), for the same R1 reason. Note the
> pre-existing private `dasher_validate` in `stroke_plotter.rs:33` did **not**
> carry this test; the recommended shared-module extraction is the clean place to
> discharge the obligation once, and repointing `stroke_plotter` at
> `dasher::validate` retroactively covers it too.

---

## 3. `hairline` rasterizer (new: `src/internal/raster/hairline.rs`)

Faithful port of the whole 408-line `hairline.zig`. All functions map 1:1; the
only Rust-shape adaptations are (a) the `DrawOptions` struct carries `&mut
Surface` + `&Pattern` instead of `*Surface`/`*const Pattern`, (b) `deltaStep`
returns `(i32, i32)` (Zig's `i2` step is always ±1; a plain `i32` is faithful and
avoids a bespoke 2-bit type), and (c) `@intFromFloat(@round(x))` →
`x.round() as i32`. Nothing else changes: the Bresenham decision parameters, the
Wu `u16` error accumulator with `@addWithOverflow` → `overflowing_add`, the
`>> 8` opacity complement and `^ 0xFF`, and the clamp/`inBox` bounds logic are
all reproduced verbatim.

```rust
// SPDX-License-Identifier: MPL-2.0
//   Copyright © 2024-2025 Chris Marchesi
//
// Ported to Rust for rustty from z2d 0.10.0
// `src/internal/raster/hairline.zig` (R3 vendored dep).
//
//! Various hairline stroking functions. These functions draw at the minimum
//! technical stroke width (1 pixel). [...verbatim module doc-comment ported...]

use crate::compositor::{self, Operator, Precision};
use crate::internal::point::Point;
use crate::internal::raster::shared::{composite_opacity, composite_opaque};
use crate::internal::tess::polygon::{Contour, ContourList};
use crate::options::AntiAliasMode;
use crate::pattern::Pattern;
use crate::surface::Surface;

pub fn run(
    surface: &mut Surface,
    pattern: &Pattern,
    contours: &ContourList,
    operator: Operator,
    precision: Precision,
    aa_mode: AntiAliasMode,
) {
    let effective_precision = if operator.requires_float() { Precision::Float } else { precision };
    for contour in &contours.list {
        match contour.len {
            0 => {}
            1 => {
                let idx = contour.corners_first().unwrap();
                let point = contour.corner_point(idx);
                // NOTE: OOB co-ordinates are handled by the surface [...verbatim...]
                composite_opaque(
                    operator, surface, pattern,
                    point.x.round() as i32, point.y.round() as i32,
                    1, effective_precision,
                );
            }
            _ => {
                let mut points: [Point; 2] = [Point { x: 0.0, y: 0.0 }; 2];
                let first = contour.corners_first().unwrap();
                points[0] = contour.corner_point(first);
                let mut idx: usize = 1;             // Zig `u1`; toggled with ^= 1
                let mut node_ = contour.corner_next(first);
                while let Some(node) = node_ {
                    points[idx] = contour.corner_point(node);
                    draw_line(
                        DrawOptions {
                            surface, pattern, operator,
                            precision: effective_precision, aa_mode,
                        },
                        points[0].x.round() as i32, points[0].y.round() as i32,
                        points[1].x.round() as i32, points[1].y.round() as i32,
                    );
                    idx ^= 1;
                    node_ = contour.corner_next(node);
                }
            }
        }
    }
}
```

> **Borrow shape for `DrawOptions`.** Zig's `DrawOptions` holds raw `*Surface`,
> so `run` re-passes the same surface pointer into each `drawLine`. In Rust the
> struct holds `surface: &mut Surface` (plus `&Pattern`); `run` moves the `&mut`
> reborrow into `DrawOptions` **per `draw_line` call** (a fresh reborrow each loop
> iteration — the previous `DrawOptions` is fully consumed by the call before the
> next reborrow), so no simultaneous `&mut` aliasing arises. `draw_line` and its
> helpers take `DrawOptions` by value and pass `opts.surface` on to
> `composite_*`. This mirrors the by-value `DrawOptions` param the reference uses
> and keeps the raw-`&mut`-arg Miri hazard (see `rust-mut-args-strongly-protected`
> memory) out of scope — the surface is threaded, never re-read across an
> `assert_integrity`.

The remaining functions are direct ports (bodies elided here for brevity — all
byte-faithful to the reference cited lines):

```rust
struct DrawOptions<'a> {                              // hairline.zig:84  DrawOptions
    surface: &'a mut Surface,
    pattern: &'a Pattern,
    operator: Operator,
    precision: Precision,
    aa_mode: AntiAliasMode,
}

fn draw_line(opts: DrawOptions, x0: i32, y0: i32, x1: i32, y1: i32) { … }   // :92
//   full inBox box-check, then dispatch:
//     dx==0 -> draw_vertical | dy==0 -> draw_horizontal
//     dx<dy -> aa? draw_wu_vertical   : draw_bres_vertical
//     else  -> aa? draw_wu_horizontal : draw_bres_horizontal

fn draw_horizontal(opts: DrawOptions, x0: i32, x1: i32, y: i32) { … }        // :123
fn draw_vertical(opts: DrawOptions, x: i32, y0: i32, y1: i32) { … }          // :140
fn draw_bres_horizontal(opts: DrawOptions, x0,y0,x1,y1: i32) { … }           // :158
fn draw_bres_vertical(opts: DrawOptions, x0,y0,x1,y1: i32) { … }             // :192
fn draw_wu_horizontal(opts: DrawOptions, x0,y0,x1,y1: i32) { … }             // :224
fn draw_wu_vertical(opts: DrawOptions, x0,y0,x1,y1: i32) { … }               // :301
fn delta_step(a: i32, b: i32) -> (i32, i32) { … }                           // :369
fn err_inc(a: i32, b: i32) -> u16 { … }                                     // :386
fn in_box(surface: &Surface, x0,y0,x1,y1: i32) -> bool { … }                // :398
```

**Two fidelity points to reproduce exactly:**

1. **Wu error accumulator** (`:255`, `:327`): Zig `err, const overflow =
   @addWithOverflow(err, err_inc)` → Rust `let (new_err, overflow) =
   err.overflowing_add(err_inc); err = new_err;` then `if overflow { y += sy; }`.
   The opacity complement `@intCast(err >> 8)` → `(err >> 8) as u8`, and the two
   `compositeOpacity` calls pass `opacity_complement ^ 0xFF` and
   `opacity_complement`. The reflexive-recursion reorder guards
   (`if x0 > x1 { return draw_wu_horizontal(opts, x1, y1, x0, y0); }`) are ported
   verbatim — but note `opts` is **moved** into the recursive call (Rust), which
   is fine because the guard `return`s immediately.

2. **`draw_bres_*` recursion** (`:159-161`, `:193-195`): same move-then-return
   guard shape. `deltaStep` destructuring `const dy, const sy = deltaStep(...)`
   → `let (dy, sy) = delta_step(...);`.

`err_inc`'s `math.maxInt(u16)` → `u16::MAX`; `(@as(u32, @intCast(a)) << 16) / d`
→ `((a as u32) << 16) / (b as u32)` then `as u16` (the reference `@intCast`
asserts it fits; the port keeps the same truncation semantics — the value is a
fraction `< 1 << 16` by construction since this branch has `a < b`).

---

## 4. Painter hairline branch (`src/painter.rs:225-228`)

Replace the `unimplemented!()` with the faithful branch (reference
`painter.zig:213-235`). The ported painter is allocator-free, so `plot` drops the
`alloc` arg and the Zig `defer contours.deinit(alloc)` becomes Rust `Drop`:

```rust
// We can fast-path here if we're using hairline stroking.
if opts.hairline {
    let contours = polyline_plotter::plot(
        nodes,
        opts.tolerance,
        &opts.dashes,
        opts.dash_offset,
    )?;                               // InternalError -> StrokeError::Internal (From impl exists)
    hairline::run(
        surface,
        pattern,
        &contours,
        opts.operator,
        opts.precision,
        opts.anti_aliasing_mode,      // reference passes the RAW opts field, not the
                                      // alpha1-forced `aa_mode` local (painter.zig:223).
                                      // In this subset there is no alpha1 surface, so
                                      // `aa_mode == opts.anti_aliasing_mode` anyway; the
                                      // raw field is passed to match the reference byte-
                                      // for-byte.
    );
    return Ok(());
}
```

Import additions at the top of `painter.rs` (beside the existing
`use crate::internal::tess::{fill_plotter, stroke_plotter};`):

```rust
use crate::internal::raster::{direct, hairline, multisample};   // + hairline
use crate::internal::tess::{fill_plotter, polyline_plotter, stroke_plotter}; // + polyline_plotter
```

The `?` compiles because `StrokeError` already has `From<InternalError>`
(`painter.rs:196-200`) — the exact same path the non-hairline stroke uses for
`fill_plotter`/`stroke_plotter` errors. **No signature change to `stroke`.**

### Module registration + blocker-note corrections

- `src/internal/raster/mod.rs`: add `pub mod hairline;` and update the module doc
  — strike `hairline` from "The `supersample` and `hairline` rasterizers are
  blockers." to "The `supersample` rasterizer is a blocker." (`supersample` stays
  a blocker; only `hairline` lands).
- `src/internal/tess/mod.rs`: add `pub mod polyline_plotter;` (+ `pub mod dasher;`
  if the recommended shared guard is taken).
- `src/lib.rs:50-51`: the "NOT yet ported" bullet
  *"Hairline stroking (`polyline_plotter` + `hairline`) … — neither is reached by
  the sprite canvas."* is now **ported** — move it out of the blocker list (it is
  reached by the **renderer overlay**, not the sprite canvas; note that
  distinction), leaving only the genuinely-still-deferred dashed
  (`Dasher`/`dashed_plotter`) machinery in the blocker bullet.

These are G3b-covered comments in rustty-z2d — they must be edited (not left
stale) to keep the port-status comment gate honest.

---

## 5. z2d hairline tests (host-only, in `hairline.rs` `mod tests`)

The reference has **no** `hairline.zig` or `polyline_plotter.zig` tests to port
(both files `grep '^test'` → 0). **One reference `test{}` IS in scope, though:**
`Dasher.zig` — the source of the `validate` guard this design formalizes into
`dasher.rs` — carries `test "validate"` (Dasher.zig:77-98). That test is ported
verbatim into `dasher.rs` `mod tests` (see §2.1), discharging the R1 obligation
the named `validate` port creates. (This corrects an earlier over-broad reading of
"no tests to port" that silently ignored the `Dasher.zig` test.) Beyond that,
per the task, author host-only known-answer tests asserting a 1px line/rect stroke
lays down the expected coverage pixels. Use the existing
z2d test idiom (`Surface::init` alpha8/rgba, `get_pixel`, the
`alpha_at`/coverage-scan helpers already in `context.rs:323`,
`internal/raster/*` tests). The `AntiAliasMode::None` path is fully
deterministic (single-pixel `composite_opaque`), so exact assertions are safe.

```rust
#[cfg(test)]
mod tests {
    use super::*;
    // Build a small opaque source Pattern + a Surface, plot a Contour by hand
    // (Contour::plot), wrap in a ContourList, run(), then assert pixels.

    #[test]
    fn hairline_horizontal_line_none() {
        // 8x8 surface, contour (1,2)->(6,2), aa=None, SrcOver opaque.
        // Assert: pixels (1..=6, 2) opaque; (0,2) and (7,2) untouched; row 1 & 3
        // untouched. Exercises draw_horizontal (dy==0) + clamp.
    }

    #[test]
    fn hairline_vertical_line_none() {
        // contour (3,1)->(3,6); assert column drawn via draw_vertical (dx==0).
    }

    #[test]
    fn hairline_diagonal_bresenham_none() {
        // contour (0,0)->(5,3), aa=None -> draw_bres_horizontal (dx>dy).
        // Assert the exact Bresenham staircase set: recompute the decision-param
        // walk in the test and compare the touched-pixel set to run()'s output.
    }

    #[test]
    fn hairline_diagonal_wu_default() {
        // Same endpoints, aa=Default -> draw_wu_horizontal. Assert the ROBUST
        // invariants only (adversary fix): (1) both endpoints are full-intensity
        // (composite_opaque, alpha == source alpha), and (2) at least one interior
        // column carries a partial-coverage pixel (0 < alpha < source alpha),
        // proving the Wu error accumulator ran.
        //
        // DO NOT assert the bare surface-pixel complement sum `a + a' ==
        // source_alpha`: the two `compositeOpacity` *opacity arguments*
        // (`opacity_complement ^ 0xFF` and `opacity_complement`) sum to exactly
        // 255 by construction, but the resulting *composited surface alphas* can
        // be off-by-one after the premultiply rounding in `composite_stride`
        // (this design's Open-Risk R1). For byte-exact Wu parity, recompute the
        // `err`/`err_inc`/overflow walk in-test and pin each interior pixel to the
        // computed `opacity_complement ^ 0xFF` / `opacity_complement` source
        // opacity fed to `compositeOpacity` (assert on the source-opacity walk,
        // which IS exact, not on the composited byte). Both approaches prove the
        // accumulator; the complement invariant holds on the arguments, not the
        // surface bytes.
    }

    #[test]
    fn hairline_rect_via_painter_stroke() {
        // The overlay's real shape: 4 line_to's + close_path through
        // painter::stroke with StrokeOptions{ hairline:true, aa:None }. Assert the
        // 1px border pixels are set and the interior center is untouched (stroke
        // only — no fill). This is the end-to-end no-longer-panics assertion.
    }

    #[test]
    fn hairline_dashed_is_blocker() {
        // StrokeOptions{ hairline:true, dashes: vec![2.0, 2.0] } must hit the
        // polyline_plotter Dasher.validate unimplemented!() — assert via
        // std::panic::catch_unwind that it panics (mirrors the subset-guard
        // contract; parity with stroke_plotter's dashed blocker).
    }
}
```

**Existing z2d tests stay green:** the port only *adds* modules + accessors and
swaps the painter branch behind the `if opts.hairline` guard (non-hairline stroke
and all fills are untouched — they never set `opts.hairline`). The `ContourList`
+ two `Contour` accessors are additive. The `dasher::validate` extraction (if
taken) is behavior-preserving for `stroke_plotter`.

---

## 6. Overlay unblock (reuse `RENDERER_OVERLAY_RENDER_DESIGN.md` verbatim)

Once §1-§5 land and `ctx.hairline = true; ctx.stroke()` no longer panics, the
four `crates/rustty-renderer/src/overlay.rs` seams are filled **exactly** per the
already-proven, committed design `.port/phase-2/RENDERER_OVERLAY_RENDER_DESIGN.md`
— no re-derivation. That doc is a known-good, adversary-reviewed fill; this
section only records what changes about its **preconditions**:

- **The single stale assumption it embeds is now true.** RENDERER_OVERLAY §7
  (and its §0 note) already treats z2d hairline as available (`ctx.hairline`
  field exists); its only gap was the *runtime* panic, which this design closes.
  Its "Compile boundary: rustty-renderer only" (its §8) holds **for the overlay
  edit** — but the *combined* work in this design spans **rustty-z2d then
  rustty-renderer** (two crates, sequenced: z2d first, renderer second).
- The four seams, `OverlayError` expansion, dead-code removals, imports, and the
  host-only `overlay_draws_hyperlink_rect` / `overlay_draws_semantic_prompt_bar`
  pixel tests + the `generic.rs` `catch_unwind`-scaffold retirement are all
  specified there (RENDERER_OVERLAY §2-§6, §8). Reuse as written.

**Overlay test now passes end-to-end.** RENDERER_OVERLAY §5's
`overlay_draws_hyperlink_rect` builds a real `Terminal`→`RenderState`, pokes
`set_hyperlink(true)`, `Overlay::init` + `apply_features(&[HighlightHyperlinks])`,
and asserts the drawn fill pixel `a != 0` and an untouched cell `a == 0`. The
`highlight_grid_rect` it drives does `ctx.fill()` (already worked) **then**
`ctx.hairline = true; ctx.stroke()` (the border) — which previously panicked and
now draws the 1px border via this design's `hairline::run`. That is the concrete
overlay-side proof that the hairline landing unblocks the overlay.

---

## 7. Dead-code + no-regression checklist

- **rustty-z2d (this design):**
  - New: `raster/hairline.rs`, `tess/polyline_plotter.rs`, (recommended)
    `tess/dasher.rs`; `ContourList` + 2 `Contour` accessors in `polygon.rs`; the
    painter branch swap + 2 import lines; 3 mod-registration/blocker-note edits.
  - `hairline::run` / `polyline_plotter::plot` become **live** (called by
    `painter::stroke`'s hairline branch) → no `#[allow(dead_code)]` needed, no
    dead-code warning. `dasher::validate` is called by both plotters.
  - The `corner_point`/`corner_next` accessors are `pub` and called by
    `hairline::run` (same crate) → live.
  - Existing z2d tests (context 6, painter 1, surface 7, polygon 1,
    stroke_plotter 2, fill_plotter 2, sparse_coverage 10, …) stay green — the
    change is additive + guarded behind `opts.hairline`.
  - **R1 reference-test coverage (no dropped `test{}`):** `Dasher.zig`
    `test "validate"` (7 assertions) is ported verbatim into `dasher.rs`
    `mod tests` (§2.1) — the sole reference `test{}` touched by the files this
    design ports from. `hairline.zig`/`polyline_plotter.zig` have zero reference
    tests; the new host-only hairline KATs (§5) are additive.
  - **G3b (covers rustty-z2d):** every new module carries the SPDX + `Ported to
    Rust for rustty from z2d 0.10.0 … (R3 vendored dep)` provenance header and the
    per-fn `// reference.zig:NN` anchors; the corrected blocker notes in
    `lib.rs`/`raster/mod.rs` keep the port-status comments honest. Gate stays
    green.
- **rustty-renderer (reused RENDERER_OVERLAY_RENDER_DESIGN §6):** 4 seams filled,
  `OverlayError` +2 variants, 3× `#[allow(dead_code)]` dropped, 55 renderer tests
  + 3 overlay tests stay green, new pixel tests additive, `generic.rs`
  `catch_unwind` scaffold retired. G3b **skips** rustty-renderer.
- **R4 acyclic:** z2d gains **no** dependency (hairline/polyline only touch
  intra-crate modules); renderer already deps z2d + vt. No new crate edge.
- **Backstop:** `cargo build --workspace` + `cargo test -p rustty-z2d` (hairline
  KATs) + `cargo test -p rustty-renderer -p rustty-vt` (overlay). Miri over the
  z2d tess/raster tests (the `&mut dyn FnMut` sink + by-value `DrawOptions` keep
  raw pointers out, per `rust-mut-args-strongly-protected`).

---

## 8. Compile boundary

**Two crates, strictly sequenced:**

1. **rustty-z2d first** — land §1-§5 (hairline + polyline_plotter + ContourList +
   painter branch + KATs). This crate compiles and tests green **on its own**, no
   renderer involvement. This is the load-bearing unblock.
2. **rustty-renderer second** — apply `RENDERER_OVERLAY_RENDER_DESIGN.md` (§6
   here) once z2d ships. Compiles against the now-working z2d hairline.

No `rustty-render-types` / `rustty-vt` source change (rustty-vt: zero edits, per
RENDERER_OVERLAY §6). R4 acyclic preserved.

---

## 9. Open risks

- **R1 — Wu partial-coverage exact values.** The Wu KAT
  (`hairline_diagonal_wu_default`) asserts only full-intensity endpoints plus the
  *presence* of at least one interior partial-coverage pixel — **not** a bare
  surface-pixel complement sum. The two `compositeOpacity` opacity *arguments*
  (`opacity_complement ^ 0xFF` and `opacity_complement`) sum to exactly 255 by
  construction, but the composited *surface alpha bytes* can be off-by-one after
  `composite_stride`'s premultiply rounding, so `a + a' == source_alpha` on the
  surface is **not** a safe assertion (adversary fix — the earlier "robust to
  exact rounding" complement-sum claim was incorrect). A tighter test recomputes
  the `err`/`err_inc`/overflow walk in-test and pins each interior pixel to the
  computed source opacity fed to `compositeOpacity` (exact on the argument walk) —
  recommended as a follow-up if byte-exact Wu parity is wanted.
- **R2 — `deltaStep` `i2` → `i32`.** Zig returns an `i2` step (∈ {-1, +1}); the
  port uses `i32`. The value is only ever `±1` and only added to a coordinate, so
  widening is behavior-identical. Recorded inline at `delta_step`.
- **R3 — `err_inc` `@intCast` truncation.** The reference `@intCast(c / d)` to
  `u16` asserts the quotient fits (guaranteed `< 1<<16` when `a < b`, the only
  call context). The port uses `as u16`; if a future caller violated `a < b` the
  Zig would panic in safe builds and the Rust would silently truncate — but the
  two hairline call sites (`err_inc(dy,dx)` with `dx>dy`, `err_inc(dx,dy)` with
  `dy>dx`) both satisfy `a < b`, matching the reference contract. Noted inline.
- **R4 — `DrawOptions<'a>` reborrow per call.** The per-iteration `&mut Surface`
  reborrow into a by-value `DrawOptions` is the faithful analogue of Zig's raw
  `*Surface` re-pass. If the borrow checker rejects the exact spelling, the
  fallback is to have `run` pass `surface`/`pattern` + the three scalars as
  **separate args** to `draw_line` (dropping the `DrawOptions` struct), which is
  R1-equivalent (the struct is a Zig ergonomic, not semantic). Prefer the struct;
  fall back only if needed.
- **R5 — `dasher::validate` extraction touches `stroke_plotter`.** The
  recommended shared-module de-dupe edits one line in `stroke_plotter.rs`
  (behavior-preserving). If minimizing blast radius is preferred, duplicate the
  guard in `polyline_plotter.rs` instead (§2.1 alternative) — both keep every
  gate green.
- **R6 — overlay proof depends on RENDERER_OVERLAY fidelity.** This design does
  **not** re-derive the overlay seams; it relies on the committed
  `RENDERER_OVERLAY_RENDER_DESIGN.md` being applied verbatim. If that doc has
  drifted from the current `overlay.rs`, re-verify its line anchors before the
  renderer-side step (the z2d step is independent and unaffected).
```