# Renderer z2d Debug-Overlay Render — Fill Design

**Scope:** fill the four deferred R5 `unimplemented!()` seams in
`crates/rustty-renderer/src/overlay.rs` — the CPU/z2d debug overlay
(`highlight_grid_rect`, `highlight_pixel_rect`, `highlight_hyperlinks`,
`highlight_semantic_prompts`). Reference: `ghostty/src/renderer/Overlay.zig`
(Zig 0.15.2). This is the **last** non-windowing renderer render seam.

**Design only.** No `crates/` edits performed by this doc. Rules: `PORT_RULES.md`
R1 (100% fidelity — every line/comment byte-faithful), R4 (acyclic), R5 (anything
still deferred stays an `unimplemented!()` seam + verbatim body). G3b
comment-coverage **skips** rustty-renderer but **covers** rustty-vt.

---

## 0. Headline: the STOP gate resolves GREEN — land all four seams

**`vt_metadata_feasible` = ALREADY EXPOSED (zero vt change needed).**

The task's STOP gate asks whether exposing the hyperlink / semantic-prompt
metadata that the two higher-level seams walk is a bounded+additive rustty-vt
change or a large/blocked one. **Neither** — it is a *no-op*: the metadata is
**already public** on the ported types, so no rustty-vt edit of any kind is
required (no new accessor, no new field, no behavior change, no test change, and
therefore no G3b exposure at all).

The reference overlay reads exactly four pieces of per-row / per-cell metadata,
all off the `raw` copies inside `RenderState`:

| Reference (`Overlay.zig`)         | Zig type read            | rustty-vt accessor (ALREADY `pub`)                          |
|-----------------------------------|--------------------------|-------------------------------------------------------------|
| `row.hyperlink`                   | `page.Row.hyperlink`     | `page::Row::hyperlink(self) -> bool`  (`page.rs:913`)       |
| `row_raw[y].semantic_prompt`      | `page.Row.semantic_prompt` | `page::Row::semantic_prompt(self) -> SemanticPrompt` (`page.rs:931`) |
| `raw_cells[x].hyperlink`          | `page.Cell.hyperlink`    | `page::Cell::hyperlink(self) -> bool` (`page.rs:637`)       |
| `raw_cells[x].semantic_content`   | `page.Cell.semantic_content` | `page::Cell::semantic_content(self) -> SemanticContent` (`page.rs:649`) |

Reachability from the overlay (all already `pub`, cross-crate visible):

- `render::RenderState.row_data: Vec<Row>`   (`render.rs:91`, `pub`)
- `render::Row.raw: page::Row`               (`render.rs:182`, `pub`)
- `render::Row.cells: Vec<Cell>`             (`render.rs:185`, `pub`)
- `render::Cell.raw: page::Cell`             (`render.rs:213`, `pub`)

The enums the walk compares against derive the needed traits already:

- `page::SemanticPrompt { None=0, Prompt=1, PromptContinuation=2 }` —
  `#[derive(Clone, Copy, Debug, PartialEq, Eq)]` (`page.rs:764`). Covers the
  reference `== .none` / `== .prompt_continuation` compares.
- `page::SemanticContent { Output=0, Input=1, Prompt=2 }` —
  `#[derive(Clone, Copy, Debug, PartialEq, Eq)]` (`page.rs:401`). Covers the
  reference `== .output` / `!= content` compares and the `switch (content)`.

**Disposition: land ALL FOUR seams.** No `grid_rect`/`pixel_rect`-only fallback
and no abort. The seam comments that say "the `RenderState` row/cell metadata is
not yet exposed" (overlay.rs `:20`, `:226`, `:271`, `:364`) were written before
the render→page path was confirmed public; they are corrected by this fill.

> **Where the earlier "deferred" note came from.** The Chunk-3b port note
> (overlay.rs `:16-21`) deferred these leaves for two reasons: (a) the ported
> z2d `Context` "lacked `set_anti_aliasing_mode`/`set_hairline`", and (b) "the
> `RenderState` row/cell metadata is not yet exposed." Both are now false: (a)
> `anti_aliasing_mode` and `hairline` are **public fields** on the ported
> `Context` (`context.rs:50,58`), and (b) the accessors above are public. The
> only remaining reason the seams stayed `unimplemented!()` is that no golden
> exercised them — this design supplies a host-only pixel test instead.

---

## 1. z2d `Context` mapping — confirmed field/method names

The reference rect helpers drive the managed z2d drawing API. Confirmed against
`crates/rustty-z2d/src/context.rs`:

| Reference call                    | Ported Rust                                          | Fallible? |
|-----------------------------------|------------------------------------------------------|-----------|
| `z2d.Context.init(alloc, &sfc)`   | `Context::init(&mut self.surface)` (alloc dropped)   | no        |
| `defer ctx.deinit()`              | `ctx.deinit()` (resets managed path)                 | no        |
| `ctx.setAntiAliasingMode(.none)`  | `ctx.anti_aliasing_mode = AntiAliasMode::None;` (**field**, `context.rs:50`) | no |
| `ctx.setHairline(true)`           | `ctx.hairline = true;` (**field**, `context.rs:58`)  | no        |
| `try ctx.moveTo(x, y)`            | `ctx.move_to(x, y);`  (**infallible in subset**)     | no        |
| `try ctx.lineTo(x, y)`            | `ctx.line_to(x, y);`  (**infallible in subset**)     | no        |
| `try ctx.closePath()`             | `ctx.close_path();`   (**infallible in subset**)     | no        |
| `ctx.setSourceToPixel(px)`        | `ctx.set_source_to_pixel(px);`                       | no        |
| `try ctx.fill()`                  | `ctx.fill().map_err(OverlayError::Fill)?;`           | **yes** (`painter::FillError`) |
| `ctx.setLineWidth(1)`             | `ctx.set_line_width(1.0);` (f64)                     | no        |
| `try ctx.stroke()`                | `ctx.stroke().map_err(OverlayError::Stroke)?;`       | **yes** (`painter::StrokeError`) |

Note (fidelity, not a deviation): the ported `move_to`/`line_to`/`close_path`
return `()`, not `Result` — the z2d subset made the managed path-builder
infallible (`context.rs:189,196,262`). The reference `try` on those three is
therefore structurally absent; only `fill`/`stroke` remain fallible. This is
recorded inline where the `try` disappears.

Import additions for `overlay.rs` (top of file, beside the existing z2d uses):

```rust
use rustty_z2d::context::Context;
use rustty_z2d::options::AntiAliasMode;
use rustty_z2d::painter;   // for OverlayError::{Fill, Stroke}(painter::…Error)
```

---

## 2. `OverlayError` — the faithful inferred error set

The reference `highlightGridRect`/`highlightPixelRect` return `!void`. The Zig
inferred error union is `error{Overflow}` (from `std.math.mul` /
`std.math.cast … orelse return error.Overflow`) merged with the z2d
`fill`/`stroke` error sets. In the ported subset only `fill`/`stroke` are
fallible, so the faithful Rust set is `Overflow + FillError + StrokeError`.

Replace the current placeholder (overlay.rs `:517-521`, which carries only
`Overflow` and is `#[allow(dead_code)]` / never-constructed) with:

```rust
/// The inferred error set of the overlay rect-drawing seams: `error.Overflow`
/// (from `std.math.mul`/`cast`) plus the z2d fill/stroke errors. In the ported
/// z2d subset `move_to`/`line_to`/`close_path` are infallible, so only
/// `fill`/`stroke` contribute — hence the two wrapping variants.
#[derive(Debug)]
enum OverlayError {
    Overflow,
    Fill(painter::FillError),
    Stroke(painter::StrokeError),
}
```

- Remove the `#[allow(dead_code)]` / "never constructed (seam)" attribute — the
  variants are now constructed by the rect helpers.
- No `From` impls needed; the two fallible calls use `.map_err(OverlayError::…)`.

---

## 3. `highlight_grid_rect` / `highlight_pixel_rect` — the rect draw helpers

Signatures are **unchanged** from the current seams (the `alloc` arg was already
dropped in Chunk-3b; keep the `#[allow(clippy::too_many_arguments)]`). Remove the
`#[allow(dead_code)]` — both become live once the higher-level seams call them.

### 3.1 `highlight_grid_rect` (x/y/width/height all in grid cells)

```rust
#[allow(clippy::too_many_arguments)]
fn highlight_grid_rect(
    &mut self,
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    border_color: Pixel,
    fill_color: Pixel,
) -> Result<(), OverlayError> {
    // All math below uses checked arithmetic to avoid overflows. The
    // inputs aren't trusted and the path this is in isn't hot enough
    // to wrarrant unsafe optimizations.
    let cw = self.cell_size.width as usize;
    let ch = self.cell_size.height as usize;

    // Calculate our width/height in pixels.
    // const px_width = std.math.cast(i32, try std.math.mul(usize, width, cell.w))
    let px_width: i32 = width
        .checked_mul(cw)
        .and_then(|v| i32::try_from(v).ok())
        .ok_or(OverlayError::Overflow)?;
    let px_height: i32 = height
        .checked_mul(ch)
        .and_then(|v| i32::try_from(v).ok())
        .ok_or(OverlayError::Overflow)?;

    // Calculate pixel coordinates
    let start_x: f64 = x
        .checked_mul(cw)
        .and_then(|v| i32::try_from(v).ok())
        .ok_or(OverlayError::Overflow)? as f64;
    let start_y: f64 = y
        .checked_mul(ch)
        .and_then(|v| i32::try_from(v).ok())
        .ok_or(OverlayError::Overflow)? as f64;
    let end_x: f64 = start_x + px_width as f64;
    let end_y: f64 = start_y + px_height as f64;

    // Grab our context to draw
    let mut ctx: Context = Context::init(&mut self.surface);

    // Don't need AA because we use sharp edges
    ctx.anti_aliasing_mode = AntiAliasMode::None;
    // Can use hairline since we have 1px borders
    ctx.hairline = true;

    // Draw rectangle path
    ctx.move_to(start_x, start_y);
    ctx.line_to(end_x, start_y);
    ctx.line_to(end_x, end_y);
    ctx.line_to(start_x, end_y);
    ctx.close_path();

    // Fill
    ctx.set_source_to_pixel(fill_color);
    ctx.fill().map_err(OverlayError::Fill)?;

    // Border
    ctx.set_line_width(1.0);
    ctx.set_source_to_pixel(border_color);
    ctx.stroke().map_err(OverlayError::Stroke)?;

    ctx.deinit(); // defer ctx.deinit()
    Ok(())
}
```

**Fidelity note on `std.math.cast(i32, std.math.mul(usize, a, b))`:** the Zig
form first multiplies with overflow-checking (`error.Overflow`) then narrows to
`i32` (`orelse return error.Overflow`). `checked_mul(...).and_then(i32::try_from
… .ok()).ok_or(Overflow)?` collapses both failure modes onto the same
`error.Overflow` arm, matching the Zig semantics exactly (either the `usize`
product overflows, or it exceeds `i32::MAX`).

**`ctx.deinit()` placement.** Zig's `defer ctx.deinit()` runs at scope exit —
*after* fill/stroke, and also on the early `?` returns. In Rust the early
`return Err` on `?` drops `ctx` (which is not yet created at the overflow-check
points — those precede `Context::init`, matching Zig, where all `try
std.math.mul` precede `.init`). After `Context::init`, no fallible-narrowing
remains; the only `?` sites are fill/stroke, and on their error the `ctx` value
is dropped (its `Drop`-free managed `Path` simply frees) — equivalent to the
deferred `deinit`, which only `path.reset()`s. The explicit trailing
`ctx.deinit()` preserves the reference call on the success path. (No observable
difference either way; keeping the explicit call is the byte-faithful choice.)

### 3.2 `highlight_pixel_rect` (x/y grid cells, width in **pixels**, height cells)

Identical shape; the only differences mirror the reference: `px_width` comes
straight from `width_px` (a plain `i32` cast, no multiply), and `px_height`
multiplies `height * cell.height`.

```rust
#[allow(clippy::too_many_arguments)]
fn highlight_pixel_rect(
    &mut self,
    x: usize,
    y: usize,
    width_px: usize,
    height: usize,
    border_color: Pixel,
    fill_color: Pixel,
) -> Result<(), OverlayError> {
    let cw = self.cell_size.width as usize;
    let ch = self.cell_size.height as usize;

    // const px_width = std.math.cast(i32, width_px) orelse return error.Overflow;
    let px_width: i32 = i32::try_from(width_px).map_err(|_| OverlayError::Overflow)?;
    let px_height: i32 = height
        .checked_mul(ch)
        .and_then(|v| i32::try_from(v).ok())
        .ok_or(OverlayError::Overflow)?;

    let start_x: f64 = x
        .checked_mul(cw)
        .and_then(|v| i32::try_from(v).ok())
        .ok_or(OverlayError::Overflow)? as f64;
    let start_y: f64 = y
        .checked_mul(ch)
        .and_then(|v| i32::try_from(v).ok())
        .ok_or(OverlayError::Overflow)? as f64;
    let end_x: f64 = start_x + px_width as f64;
    let end_y: f64 = start_y + px_height as f64;

    let mut ctx: Context = Context::init(&mut self.surface);
    ctx.anti_aliasing_mode = AntiAliasMode::None;
    ctx.hairline = true;

    ctx.move_to(start_x, start_y);
    ctx.line_to(end_x, start_y);
    ctx.line_to(end_x, end_y);
    ctx.line_to(start_x, end_y);
    ctx.close_path();

    ctx.set_source_to_pixel(fill_color);
    ctx.fill().map_err(OverlayError::Fill)?;

    ctx.set_line_width(1.0);
    ctx.set_source_to_pixel(border_color);
    ctx.stroke().map_err(OverlayError::Stroke)?;

    ctx.deinit();
    Ok(())
}
```

The verbatim-Zig comment blocks currently preserved above each seam
(overlay.rs `:375-432` and `:454-496`) stay in place as the R1 provenance record.

---

## 4. `highlight_hyperlinks` / `highlight_semantic_prompts` — the RenderState walk

Signatures unchanged (`&mut self, state: &RenderState`). The Zig
`row_slice.items(.raw)` / `.items(.cells)` MultiArrayList-parallel-array idiom
maps to iterating `state.row_data` and reading `.raw` / `.cells` per element.

### 4.1 `highlight_hyperlinks`

```rust
fn highlight_hyperlinks(&mut self, state: &RenderState) {
    let border_color = Color::Hyperlink.rect_border();
    let fill_color = Color::Hyperlink.rect_fill();

    // for (row_raw, row_cells, 0..) |row, cells, y|
    for (y, row) in state.row_data.iter().enumerate() {
        if !row.raw.hyperlink() {
            continue;
        }

        let raw_cells = &row.cells;

        let mut x: usize = 0;
        while x < raw_cells.len() {
            // Skip cells without hyperlinks
            if !raw_cells[x].raw.hyperlink() {
                x += 1;
                continue;
            }

            // Found start of a hyperlink run
            let start_x = x;

            // Find end of contiguous hyperlink cells
            while x < raw_cells.len() && raw_cells[x].raw.hyperlink() {
                x += 1;
            }
            let end_x = x;

            if let Err(_err) = self.highlight_grid_rect(
                start_x,
                y,
                end_x - start_x,
                1,
                border_color,
                fill_color,
            ) {
                // std.log.warn("Error drawing hyperlink border: {}", .{err});
                // PORT NOTE: no logging facade wired (see overlay.rs :39-41);
                // the reference `catch |err| log.warn(...)` collapses to
                // swallowing the error, preserved as this comment.
            }
        }
    }
}
```

> **Borrow note.** The `for … in state.row_data` borrow of `state` (a
> `&RenderState`, immutable) coexists with the `&mut self` call
> `self.highlight_grid_rect(...)` because `state` and `self` are disjoint
> allocations — the immutable read never aliases the `&mut self.surface` the
> rect helper takes. No raw-ptr detour is needed (unlike the `rebuild_overlay`
> caller, where `terminal_state` was a *field* of `self`; here `state` is a
> separate borrow).

### 4.2 `highlight_semantic_prompts`

Two blocks, exactly as the reference. Block A walks rows for prompt bars
(index-based `while` with continuation coalescing); Block B walks cells for
contiguous semantic runs.

```rust
fn highlight_semantic_prompts(&mut self, state: &RenderState) {
    // Highlight the row-level semantic prompt bars. The prompts are easy
    // because they're part of the row metadata.
    {
        let prompt_border = Color::SemanticPrompt.rect_border();
        let prompt_fill = Color::SemanticPrompt.rect_fill();

        let mut y: usize = 0;
        while y < state.row_data.len() {
            // If its not a semantic prompt row, skip it.
            if state.row_data[y].raw.semantic_prompt() == SemanticPrompt::None {
                y += 1;
                continue;
            }

            // Find the full length of the semantic prompt row by connecting
            // all continuations.
            let start_y = y;
            y += 1;
            while y < state.row_data.len()
                && state.row_data[y].raw.semantic_prompt()
                    == SemanticPrompt::PromptContinuation
            {
                y += 1;
            }
            let end_y = y; // Exclusive

            // const bar_width = @min(@as(usize, 5), self.cell_size.width);
            let bar_width = std::cmp::min(5usize, self.cell_size.width as usize);
            if let Err(_err) = self.highlight_pixel_rect(
                0,
                start_y,
                bar_width,
                end_y - start_y,
                prompt_border,
                prompt_fill,
            ) {
                // log.warn("Error drawing semantic prompt bar: {}", .{err});
            }
        }
    }

    // Highlight contiguous semantic cells within rows.
    // for (row_cells, 0..) |cells, y|
    for (y, row) in state.row_data.iter().enumerate() {
        let raw_cells = &row.cells;

        let mut x: usize = 0;
        while x < raw_cells.len() {
            let content = raw_cells[x].raw.semantic_content();
            let start_x = x;

            // We skip output because its just the rest of the non-prompt
            // parts and it makes the overlay too noisy.
            if content == SemanticContent::Output {
                x += 1;
                continue;
            }

            // Find the end of this content.
            x += 1;
            while x < raw_cells.len() {
                let next = raw_cells[x].raw.semantic_content();
                if next != content {
                    break;
                }
                x += 1;
            }

            // switch (content) { .prompt=>…, .input=>…, .output=>unreachable }
            let color: Color = match content {
                SemanticContent::Prompt => Color::SemanticPrompt,
                SemanticContent::Input => Color::SemanticInput,
                SemanticContent::Output => unreachable!(),
            };

            if let Err(_err) = self.highlight_grid_rect(
                start_x,
                y,
                x - start_x,
                1,
                color.rect_border(),
                color.rect_fill(),
            ) {
                // log.warn("Error drawing semantic content highlight: {}", .{err});
            }
        }
    }
}
```

**Fidelity subtlety — `content` capture.** The reference binds
`const cell = raw_cells[x]; const content = cell.semantic_content;` *before* the
`.output` early-out, then reuses `content` in the tail `switch`. Because the
`continue` path never reaches the `switch`, `content` in the `match` is
guaranteed non-`Output`, so `SemanticContent::Output => unreachable!()` is
faithful to Zig's `.output => unreachable`. (The Rust port reads
`.semantic_content()` once into `content` and re-reads per-`next` cell in the
inner loop — identical to the Zig `next.semantic_content` re-read.)

Both verbatim-Zig comment blocks (overlay.rs `:229-268` and `:277-361`) remain
as the R1 provenance record.

---

## 5. Host-only offscreen overlay pixel test

The overlay renders on the CPU to a z2d `Surface` — **no Metal, no GPU**. So the
new test is host-only (no `test_renderer`, which returns `None` without a Metal
device). It lives in `overlay.rs`'s existing `mod tests` (which already builds
`Overlay` directly and is dead-code/G3b-exempt as rustty-renderer).

**Recipe (controlled, exercises the real walk + z2d raster):**

1. Build a real `rustty_vt::terminal::Terminal` at 10×3 and feed `b"AB"` so
   `row_data` is populated with real rows/cols via `state.update(...)` — this is
   the only faithful way to get a `RenderState` with valid rows (each
   `render::Row` needs a real `Pin`; building it by hand cross-crate is possible
   but requires a synthetic `Pin { node: null_mut(), … }` — see OPEN RISK R2).
2. **Poke the metadata bits** through the already-`pub` accessors — `raw` is a
   `pub` field and the setters are `pub &mut self` methods:
   ```rust
   state.row_data[0].raw.set_hyperlink(true);
   state.row_data[0].cells[0].raw.set_hyperlink(true);
   state.row_data[0].cells[1].raw.set_hyperlink(true);
   ```
   This isolates the overlay-walk-under-test from the OSC-8 / OSC-133 parsing
   pipeline (which has its own vt tests); the overlay only reads these bits.
3. Build `Overlay::init(size)` with `size` matching the terminal: cell 8×16,
   screen 80×48, zero padding → terminal surface 80×48.
4. `overlay.apply_features(&state, &[Feature::HighlightHyperlinks]);`
5. Assert pixels via `overlay.surface.get_pixel(x, y) -> Option<Pixel::Rgba>`:
   - **Drawn:** center of highlighted cell (0,0) = pixel `(4, 8)` → `a != 0`
     (the hyperlink fill is `rect_fill()` = alpha-96 premultiplied RGBA; SrcOver
     over the transparent surface leaves `a == 96` at the fill interior — z2d's
     `AntiAliasMode::None` fill gives full interior coverage, per the existing
     `context.rs` fill test).
   - **Untouched:** center of a non-highlighted cell, e.g. row 2 → pixel
     `(4, 40)` → `a == 0`.

The test uses the **same** vt test-harness idiom as the existing renderer and vt
tests — `allocator_api2::alloc::Global` (NOT a nonexistent `rustty_vt::alloc`;
confirmed verbatim in `crates/rustty-renderer/src/cell.rs:421` and
`crates/rustty-vt/src/render.rs:1065`) plus the local `opts(cols, rows)` helper
copied byte-for-byte from `cell.rs:428-438` / `render.rs:1071-1082`:

```rust
#[test]
fn overlay_draws_hyperlink_rect() {
    // Imports copied verbatim from the existing vt/renderer test harness
    // (cell.rs:421-426, render.rs:1063-1065): `Global` is allocator-api2's,
    // which is exactly what `Terminal::init` / `RenderState::update` /
    // `Terminal::deinit` take (render.rs:36 `type Allocator = Global`).
    use allocator_api2::alloc::Global;
    use rustty_vt::terminal::{Colors, Options, Terminal};
    use rustty_vt::render::RenderState;
    use rustty_vt::{kitty, modes::{self as modespkg}, size as vt_size};

    // Local opts helper — verbatim from cell.rs:428-438 / render.rs:1071-1082
    // (`Options` has no field defaults in Rust, so the reference `.{ .cols, .rows }`
    // shorthand is spelled out).
    fn opts(cols: vt_size::CellCountInt, rows: vt_size::CellCountInt) -> Options {
        Options {
            cols,
            rows,
            max_scrollback: 10_000,
            colors: Colors::default(),
            default_modes: modespkg::ModePacked::default(),
            kitty_image_storage_limit: 320 * 1000 * 1000,
            kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
        }
    }

    let alloc = Global;
    let mut t = Terminal::init(alloc, opts(10, 3)).unwrap();
    // Scoped stream, exactly the render.rs:1117-1121 idiom (vt_stream/next_slice/
    // deinit are all pub — terminal.rs:625, stream.rs:632/623).
    {
        let mut s = t.vt_stream();
        s.next_slice(b"AB");
        s.deinit();
    }

    let mut state = RenderState::empty();
    state.update(alloc, &mut t).unwrap();

    // Mark row 0 cols 0..1 as a hyperlink run.
    state.row_data[0].raw.set_hyperlink(true);
    state.row_data[0].cells[0].raw.set_hyperlink(true);
    state.row_data[0].cells[1].raw.set_hyperlink(true);

    let sz = size(80, 48);                       // cell 8x16 (existing helper)
    let mut o = Overlay::init(sz).unwrap();
    o.apply_features(&state, &[Feature::HighlightHyperlinks]);

    // Drawn inside cell (0,0):
    match o.surface.get_pixel(4, 8) {
        Some(Pixel::Rgba(px)) => assert!(px.a != 0, "hyperlink fill not drawn"),
        other => panic!("expected rgba, got {other:?}"),
    }
    // Untouched cell (0,2):
    match o.surface.get_pixel(4, 40) {
        Some(Pixel::Rgba(px)) => assert_eq!(px.a, 0, "non-link cell drawn"),
        other => panic!("expected rgba, got {other:?}"),
    }

    o.deinit();
    // Teardown mirrors the render/cell tests (render.rs:1141-1142): tear down the
    // RenderState before the Terminal. `RenderState::deinit` only does
    // `row_data = Vec::new()` (render.rs:292-294) and `viewport_pin` is explicitly
    // NOT a tracked pin (render.rs:103-105), so this is harmless — Rust drop frees
    // everything — but it matches the established teardown order.
    state.deinit(alloc);
    t.deinit(alloc);
}
```

> **Import/harness note (adversary fold).** An earlier draft wrote
> `use rustty_vt::alloc::Global;` with a `// whatever the vt test alloc is`
> placeholder and left `opts(10, 3)` as `/* … */ ...`. There is **no**
> `rustty_vt::alloc` module (the vt crate has only `bitmap_allocator.rs`); the
> correct, precedent-confirmed import is `allocator_api2::alloc::Global`
> (allocator-api2 is already a normal dep of rustty-renderer). Both are corrected
> above with the `opts` helper spelled out and `state.deinit(alloc)` restored to
> the teardown.

The `size(80, 48)` note: this helper is the renderer's own test `size` (the
overlay `Size`), distinct from the vt `size` module — hence the vt import is
aliased `size as vt_size` above so the two do not collide.

(An analogous `overlay_draws_semantic_prompt_bar` may set
`state.row_data[0].raw.set_semantic_prompt(SemanticPrompt::Prompt)` and assert
the left-edge bar pixel `(2, 8)` is drawn — width `min(5, 8) = 5`, so `x ∈ [0,5)`.)

**Retiring the 3b `catch_unwind` scaffold.** The existing
`rebuild_overlay_builds_and_clears` (generic.rs `:6307`) wraps the seam panic in
`catch_unwind` and swallows it. Once the seams are filled, that
`SemanticPrompts` drive no longer panics. Minimal follow-up: drop the
`take_hook`/`catch_unwind`/`set_hook` scaffolding so the test drives
`rebuild_overlay(&[Feature::SemanticPrompts])` directly and still asserts
`r.overlay.is_some()` (now a real, drawn overlay). Update the stale comment at
`:6317-6325` accordingly. (This is the only generic.rs edit; it is a test-only
simplification, not a behavior change.)

---

## 6. Dead-code + no-regression checklist

- **Remove** `#[allow(dead_code)]` from `highlight_grid_rect` (`:434`),
  `highlight_pixel_rect` (`:498`), and `OverlayError` (`:518`) — all three
  become live once the higher-level seams call the rect helpers and the helpers
  construct `OverlayError`. Keep `#[allow(clippy::too_many_arguments)]` on both
  rect helpers.
- **Keep** all verbatim-Zig comment blocks (`:229-268`, `:277-361`, `:375-432`,
  `:454-496`) — R1 provenance. **Style decision (adversary fold, R1-neutral):**
  the filled bodies below carry their *own* inline `//` comments *and* sit under
  the preserved verbatim Zig block, which is redundant against the file's own
  filled-function convention (`init`/`pending_image`/`reset` carry inline
  comments only, no preserved block). Both spellings satisfy R1. This design
  **deliberately keeps both** for these four seams: they are the renderer's
  adversary/verify R1 backstop (G3b skips rustty-renderer), so a line-for-line
  verbatim block that the adversary can diff directly against `Overlay.zig`
  earns its keep here even though it diverges from the file's inline-only norm.
  This is a conscious, consistent choice across all four seams — not an
  oversight. (Dropping the now-redundant verbatim blocks would *also* be R1-valid
  if a future port lead prefers matching the file's inline-only convention;
  either way, apply it uniformly to all four.)
- **Correct** the four stale "metadata not yet exposed / z2d lacks
  AA/hairline" notes (`:16-21`, `:226`, `:271`, `:364`) to reflect that the fill
  landed. (These are rustty-renderer comments → **G3b-exempt**; free to edit.)
- **rustty-vt: NO edits.** Zero source, zero comment, zero test change → G3b for
  rustty-vt stays green trivially, R4 acyclic is untouched (rustty-renderer
  already depends on rustty-vt + rustty-z2d; no new edges).
- **The 55 renderer tests + 3 overlay tests + all vt tests stay green.** The
  three existing overlay unit tests (`overlay_surface_roundtrip`,
  `overlay_invalid_dimensions`, `color_alpha_pixels`) are untouched. New tests
  are additive.
- **`cargo build --workspace` + `cargo test -p rustty-renderer -p rustty-vt`**
  are the backstop (per the intrusive-ptr/placeholder-fill memory).

---

## 7. Open risks

- **R1 — z2d SrcOver-onto-transparent exact interior value.** The test asserts
  `a != 0` at the fill interior (robust) rather than pinning the exact
  premultiplied `(r,g,b,a)`. If a tighter assertion is wanted, `a == 96` is the
  expected fill alpha (alpha_pixel(96), premultiplied, composited over `a==0`
  destination leaves the source through). Low risk — the existing `context.rs`
  fill test already proves `AntiAliasMode::None` fill fully covers the interior.
- **R2 — test relies on `Terminal` + `state.update` to populate `row_data`.**
  Hand-building `render::Row` cross-crate is *possible* (all fields incl.
  `Pin { node: *mut Node, … }` are `pub`; pass `std::ptr::null_mut()` since the
  overlay never reads `pin`), but building via a live `Terminal` is more
  faithful and avoids naming `pagelist::Node`. The exact `Terminal::init` /
  `opts` / `vt_stream` / `Global` paths must be copied from the existing vt
  `render.rs` tests (they use `Terminal::init(alloc, opts(10,3))` +
  `t.vt_stream()` + `s.next_slice(...)`). No new vt API needed.
- **R3 — `bar_width` clamp vs. cell width.** The reference `@min(5,
  cell.width)`: with the test cell width 8, `bar_width = 5`, so the prompt-bar
  test must sample `x < 5`. With a narrow cell (< 5px) the bar shrinks — mirror
  the reference exactly (`std::cmp::min`), no clamping surprises.
- **R4 — `ctx.deinit()` vs. Rust drop.** Both free only the managed `Path`; the
  explicit call is kept for byte-fidelity with `defer ctx.deinit()`. No
  functional difference. If a future clippy lint flags the explicit call as
  redundant, it may be dropped with a PORT NOTE — but default is to keep it.
- **R5 — the adversary/verify pass is the R1 backstop** for the renderer (G3b
  skips it). This design keeps every reference line/comment addressable so the
  adversary can diff against `Overlay.zig` line-for-line.

---

## 8. Summary of edits (all in `crates/rustty-renderer/`)

| File | Edit | G3b |
|------|------|-----|
| `src/overlay.rs` | fill 4 seam bodies; expand `OverlayError` (+2 variants); drop 3× `#[allow(dead_code)]`; add 3 z2d imports; correct 4 stale notes; add 1–2 host-only pixel tests | exempt |
| `src/generic.rs` | retire the `catch_unwind` scaffold in `rebuild_overlay_builds_and_clears`; drive `SemanticPrompts` directly | exempt |
| `crates/rustty-vt/**` | **none** | trivially green |

**Compile boundary:** rustty-renderer only. No rustty-vt / rustty-z2d /
rustty-render-types source change. R4 acyclic preserved (no new crate edges).
