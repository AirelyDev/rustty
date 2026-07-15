# Completeness spec — rustty-z2d rasterizer seams (the 9 `unimplemented!()`)

PORTS oracle (READ-ONLY): z2d 0.10.0 at
`~/.cache/zig/p/z2d-0.10.0-j5P_Hu-6FgBsZNgwphIqh17jDnj8_yPtD8yzjO6PpHRQ/src/`

Goal: fill the deferred z2d subset seams so rustty-z2d is a full-fidelity port
(supersample AA fill/stroke + dashed tessellation + out-of-subset packed-alpha
surface formats), preserving R1 (line+comment+test fidelity), R4 (acyclic — z2d
is a pure leaf, no risk), panic=abort semantics, and Miri-clean hazardous paths.

## The seams (grep-verified)

| # | rustty seam | oracle body |
|---|-------------|-------------|
| 1 | `painter.rs:128` supersample **fill** arm | `painter.zig` `.supersample_4x =>` → `supersample.zig::run` |
| 2 | `painter.rs:322` supersample **stroke** arm | same `supersample.zig::run` |
| 3 | `surface.rs:355` `Surface::init` alpha4/2/1 arm | `surface.zig` `PackedImageSurface(T).init` |
| 4 | `surface.rs:392` `Surface::init_pixel` alpha4/2/1 arm | `surface.zig` `PackedImageSurface(T).init` (pixel-typed) |
| 5 | `polyline_plotter.rs:47` dashed polyline | `polyline_plotter.zig` `dashedLineTo` + dasher wiring |
| 6 | `stroke_plotter.rs:46` dashed stroke | `dashed_plotter.zig` (whole file, 640 LOC) |
| 7 | `dasher.rs` machinery (only `validate` ported) | `Dasher.zig` `init`/`applyOffset`/`step`/`reset` |
| 8 | `hairline.rs:610` subset-guard (`hairline_dashed_is_blocker` test) | n/a — guard lives in polyline_plotter; replace the blocker test |
| 9 | NEW `internal/tess/dashed_plotter.rs` | `dashed_plotter.zig` (the file backing seam 6) |

Seams 5/6/7/8/9 are one interlocked workstream (dashed). Seams 1/2 are one
(supersample). Seams 3/4 are one (packed surfaces). Do them in that dependency
order — packed surfaces first (supersample's mask can then faithfully target
packed alpha; and it's the least entangled), then supersample, then dashed.

---

## Workstream A — packed-alpha surface formats (seams 3, 4)

**Oracle**: `surface.zig` `PackedImageSurface(comptime T)` (lines ~591-730+):
`init`/`initBuffer`/`deinit`/`downsample`/`downsampleBuffer`/`resizeBuffer`/
`asSurfaceInterface`/`_get`/`_set`/`_paintPixel`/`getPixel`/`putPixel`/
`paintStride`/`getStride`/`compositeStride`. Little-endian bit-packed `[]u8`.
Plus `pixel.zig` `Alpha4`/`Alpha2`/`Alpha1`: `average`, `Opaque`, `format`,
pack/unpack. In rustty these three structs already exist as *thin* types
(`pixel.rs:107-133`, `equal`/`as_pixel` only) — they lack `average`/`Opaque`/
packed get/set.

**Port targets**
- `pixel.rs`: extend `Alpha4`/`Alpha2`/`Alpha1` with `average(&[Self]) -> Self`,
  `Opaque` const, `format`, and the pack helpers (mirror `Alpha8::average` at
  `pixel.rs:485`). Byte-slice fidelity: packed masks are `u8` bit fields, not
  `String` — trivially satisfied.
- `surface.rs`: add a `PackedImageSurface<T>` generic (new inline type or
  `surface/packed.rs` submodule), add `Surface::ImageSurfaceAlpha4/2/1(...)`
  enum variants, and fill the two `unimplemented!()` arms at `:355`/`:392`.

**RIPPLE (must budget for this)** — adding 3 `Surface` enum variants forces
every exhaustive match over `Surface` to grow arms:
- `dispatch!` macro (`surface.rs:320`) — 3 arms. But packed surfaces are NOT
  `ImageSurface<T>`; they have a different backing, so `dispatch!` cannot fold
  them uniformly. Decide: either (a) give `dispatch!` packed arms that call the
  packed methods (requires packed to expose the same method surface —
  width/height/paint_stride/get_pixel/put_pixel/downsample/composite_stride), or
  (b) a second macro for packed-only dispatch. Oracle uses Zig `inline else`
  which is (a); prefer (a) for fidelity.
- `get_format` (`surface.rs:420`) — add Alpha4/2/1 → Format arms.
- any other `match self { Surface::... }` in surface.rs + across the crate
  (supersample's mask type switch will read these). `cargo build --workspace`
  is the backstop for exhaustiveness.

**Reference tests to port (assertion-for-assertion, R1)** — from `surface.zig`:
- `test "PackedImageSurface, alpha4"` (line 1064)
- `test "PackedImageSurface, alpha2"` (line 1199)
- `test "PackedImageSurface, alpha1"` (line 1329)
- `test "PackedImageSurface, dimension validation"` (line 1509)
- `test "PackedImageSurface.downsample, edge cases"` (line 1616)

These exercise init/paint/get/put/getStride/downsample on the packed path.

---

## Workstream B — supersample AA fill + stroke (seams 1, 2)

**Oracle**: `internal/raster/supersample.zig::run` (185 LOC). Algorithm:
1. `polygons.inBox(scale, w, h)` early-out (already ported as `in_box`).
2. Assert integer scale; compute bounded/unbounded box `x0..x1,y0..y1`.
3. Build a **scaled-up mask surface** (`mask_width×mask_height` clamped to the
   scaled target). Mask `surface_type`: the target's own type IF it's an
   alpha4/2/1 packed surface, **else `Alpha8`**. `opaque_px` is that type's
   `Opaque`.
4. Rasterize into the mask with the SAME `WorkingEdgeSet` scanline machine the
   multisample rasterizer already uses (`init`/`breakpoints`/`rescan`/`inc`/
   `sort`/`filter`), writing spans via `mask_sfc_scaled.paintStride(...)`.
5. `mask_sfc_scaled.downsample()` (already ported: `Surface::downsample`,
   `surface.rs:405`).
6. Final composite: **2-step** `SurfaceCompositor.run(surface, comp_x, comp_y, 2,
   {dst_in pattern↦mask, then operator}, precision)`.

**Port target**: new `internal/raster/supersample.rs` + un-defer the two
`painter.rs` arms (`:128` fill, `:322` stroke) to call `supersample::run(...)`.
Follow `multisample.rs` conventions (allocator-free; owned local mask Surface;
`Vec` for `filtered_edge_set.to_vec()`).

**Two faithful-but-narrowed divergences (call out in PORT NOTE, R5-safe)**:
- **Mask type**: BEFORE workstream A the alpha4/2/1 mask arms are unreachable-
  by-construction (those surfaces can't exist) → mask is always Alpha8. AFTER
  workstream A they become reachable → port the packed-mask arms faithfully.
  Sequence A before B so B is written against the real, reachable set.
- **Final composite**: the multi-step `SurfaceCompositor.run` is NOT ported
  anywhere in rustty (only `run_pixel_t` + per-stride `composite_opaque/opacity`
  in `internal/raster/shared.rs`). Reproduce the 2-step compositor per-pixel over
  the box: walk `comp_x..`, read the downsampled mask's per-pixel alpha, and for
  each run call `shared::composite_opaque` (alpha==full) / `composite_opacity`
  (0<alpha<full) — exactly how `multisample.rs` already reproduces the identical
  dst_in→operator pipeline per-stride. rustty `Pattern` has only
  `OpaquePattern` (`pattern.rs:25`); gradient/dither pattern sources are out of
  the ghostty subset, so the pattern.* switch collapses to the opaque arm.
  Document as faithful geometry + scalar-integer composite, no SIMD, no
  gradient/dither.

**Reference tests**: `supersample.zig` has NO inline `test{}` (spec/ golden dir
is stripped from the vendored package). → author behavioral tests (see
Acceptance): supersample-vs-multisample coverage parity on a known filled shape,
plus a golden alpha readback. REPLACE the painter supersample blocker contract
(no test asserts the supersample panic today, so nothing to delete — but the
crate-level `lib.rs` "NOT yet ported" note must be updated).

---

## Workstream C — dashed tessellation (seams 5, 6, 7, 8, 9)

**Oracle**: `Dasher.zig` (full), `dashed_plotter.zig` (640 LOC), the
`dashedLineTo` branch in `polyline_plotter.zig`, and the **generic** helpers in
`stroke_plotter.zig` (`plotSingle`/`plotOpenJoined`/`plotClosedJoined`/`join`,
each `fn(T: type, self: *T, ...)`).

**The structural blocker (drives the whole design)**: in Zig those four helpers
are generic over the plotter type `T` precisely so the dashed plotter can reuse
them against TWO types — its `Plotter` and its nested `InitialPolygon` struct.
In rustty they are currently **inherent methods on `Plotter`**
(`stroke_plotter.rs:248/272/319/378`, plus `CapPlotterCtx`/`Joiner`). To port
dashed faithfully they must be **refactored to be generic over a
`StrokePlotter` trait** implemented by both `dashed_plotter::Plotter` and
`dashed_plotter::InitialPolygon` (and by the plain `stroke_plotter::Plotter`,
unchanged behavior). Trait surface (the fields the helpers touch): `opts()`,
`pen()`, `outer(&mut)`, `inner(&mut)`, `clockwise_` get/set, and access to
`result`. This is the largest single edit.

**Port targets**
- `dasher.rs`: add fields `dashes/offset/idx/on/remain` + `init`/`applyOffset`/
  `step`/`reset` (currently only `validate`). `dashes` is `&[f64]`/`Vec<f64>`
  (byte/scalar slice fidelity — no String).
- `polyline_plotter.rs`: un-defer `:47`; add the `dashedLineTo` free fn +
  `Option<Dasher>` wiring in the move_to/line_to/curve_to/close_path arms
  (mirror `polyline_plotter.zig` lines 66-140 + `dashedLineTo`).
- NEW `internal/tess/dashed_plotter.rs`: port the whole `dashed_plotter.zig`
  `Plotter` with `initial_polygon: enum { None, Off(Point), On(InitialPolygon) }`,
  `runMoveTo/runLineTo/_runLineTo/runCurveTo/runClosePath/nextSegment/finish/
  plotDotted/saveInitial/finishInitialDotted/finishInitial/joinAndCapInitial`,
  `CurveToCtx`. Uses `Face::init_single`/`Pen.vertices`/`Slope`/`PointBuffer<2,5>`/
  `Contour::concat/plot/plot_reverse/corners_first` — all already ported.
- `stroke_plotter.rs`: refactor helpers to generic `StrokePlotter`; un-defer
  `:46`; when `dasher::validate(&opts.dashes)` route to `dashed_plotter::plot`.
- `painter.rs`: no code change needed — it already forwards `opts.dashes` to both
  plotters; removing the guards lets the dashed path flow.

**Reference tests**: `Dasher.zig` `test "validate"` already ported
(`dasher.rs:39`). `Dasher` `init`/`step`/`applyOffset` have NO inline tests;
dashed_plotter/polyline_plotter have none. → author behavioral dashed tests
(dashed line produces >1 contour/segment; dot/square zero-length dash; dash
offset transitions). **MUST-REPLACE blocker tests (count-regression = failure,
so replace in place, do not delete)**:
- `hairline.rs:610` `hairline_dashed_is_blocker` (asserts `catch_unwind` panic)
  → replace with a real dashed-hairline behavioral test (seam 8).
- any `stroke_plotter.rs` / `polyline_plotter.rs` dashed-panic test (audit
  `stroke_plotter.rs` `mod tests`, 2 tests) → replace panic-expectations with
  behavioral dashed assertions.

---

## SB / Miri hazards

- **HAZARD (dashed, primary)** — `dashed_plotter.zig` `InitialPolygon` embeds
  `result: *Polygon` aliasing the plotter's own `result`, plus *moved* copies of
  `outer`/`inner`/`points`/`pen`. `joinAndCapInitial` does live polygon surgery:
  `self.outer.concat(&initial.outer)`, `initial.outer = self.outer` (move),
  then `plotOpenJoined(InitialPolygon, &initial, ...)` — interleaved with the
  plotter's `result`. A naive Rust port storing a raw `*mut Polygon` /
  `&mut Polygon` to `result` inside `InitialPolygon` that stays live across
  `&mut self.result` mutations is exactly the MEMORY "long-lived raw alias into
  a field interleaved with &mut to that field" = **SB/TB UB**.
  **Safe design**: do NOT store the result pointer in `InitialPolygon`; thread
  `result: &mut Polygon` as an explicit argument to the generic helpers and rely
  on Rust field-splitting so `self.result` and the `initial_polygon` sub-struct
  borrow disjointly. rustty `Contour` is index-arena (`first: Option<usize>`,
  `plot`/`concat`/`plot_reverse` are index-based, `polygon.rs:364+`) so contour
  moves/concats carry NO raw aliasing — the hazard is solely the self-referential
  `result`. PROVE on an isolated harness `.port/phase-2/mirihar-z2d-dashed`
  reproducing the `joinAndCapInitial` shape (sub-struct holding contours + a
  shared result, exercising concat/reassign/plot). Pure Rust → **Miri** (not
  ASan; no FFI on this path).
- **HAZARD (supersample, low)** — `run` allocates a *local owned* mask `Surface`,
  downsamples it, composites into the target. No cross-field long-lived alias.
  `WorkingEdgeSet` borrows `&polygons` — identical to the already-Miri-clean
  multisample path, so no new hazard. Still: run the new `supersample.rs` path
  under the crate Miri sweep for the mask alloc/downsample/composite arithmetic.
- **HAZARD (packed, low)** — bit-packed `[]u8` `_get`/`_set` do shift/mask
  arithmetic + byte indexing; risk is out-of-bounds / shift-overflow, not
  aliasing. Run the 5 ported packed tests under Miri (catches OOB + UB in the
  bit math). No FFI.

No ASan needed anywhere in this workstream — all three paths are pure Rust and
Miri-executable.

## Acceptance

- `cargo build -p rustty-z2d` and `cargo build --workspace` green (the Surface
  enum-variant ripple compiles across every exhaustive match in the crate).
- `cargo test -p rustty-z2d`: **no port-count regression**. Baseline inline-test
  tally by file is recorded; new work must ADD (5 packed tests + Dasher
  init/step behavioral tests + supersample parity/golden tests + dashed
  behavioral tests) and REPLACE the blocker tests in place (hairline
  `*_is_blocker`, stroke_plotter dashed-panic) — never drop them.
- `cargo +nightly miri test -p rustty-z2d` green on the dashed, supersample, and
  packed paths.
- Isolated `.port/phase-2/mirihar-z2d-dashed` harness (the `joinAndCapInitial`
  result-aliasing shape) green under Miri.
- All `unimplemented!()` at painter.rs:128/322, surface.rs:355/392,
  stroke_plotter.rs:46, polyline_plotter.rs:47 removed; `dasher.rs` full
  machinery present; `dashed_plotter.rs` created; crate-level `lib.rs`
  "NOT yet ported / blockers" doc updated to reflect full parity.

## Risks / decisions needing sign-off

- **Multi-step `SurfaceCompositor.run` reproduction** (supersample step 6): the
  recommended path reuses `shared::composite_opaque/opacity` per-pixel rather
  than porting the N-step SIMD `SurfaceCompositor`. Faithful in output for the
  opaque-pattern subset; if a future gradient/dither Pattern lands, the full
  compositor would be needed. Flag as a scoped divergence, not a silent one.
- **`StrokePlotter` trait refactor** of the four generic helpers is a wide edit
  to `stroke_plotter.rs` with behavior-preservation risk for the ALREADY-passing
  non-dashed stroke tests — guard with the existing stroke_plotter tests + the
  hairline/sprite behavioral tests before/after.
- **Pre-existing suspicious line** `multisample.rs:93` (`(end_scanline+1)..
  sfc_width` — looks like it should be `sfc_height`) is OUT OF SCOPE (not one of
  the 9 seams); noted for a separate audit, not touched here.
