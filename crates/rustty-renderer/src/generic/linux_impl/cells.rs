//! PORTS: ghostty src/renderer/generic.zig — Chunk 3b, the CPU cell-fill cluster
//! (`rebuildCells`, `rebuildRow`, `addUnderline`, `addOverline`, `addStrikethrough`,
//! `addGlyph`, `addCursor`, `addPreeditCell`, `rebuildOverlay`, plus `PreeditRange` and
//! the row-local `selected` enum).
//!
//! This file is part of the `Renderer(OpenGL)` arm (`generic::linux_impl`).

use super::*;

// PORT NOTE (macos_impl generic.rs:2762-2772): reference `const PreeditRange = struct`
// — the CPU cell-fill preedit skip-range.
#[derive(Clone, Copy)]
struct PreeditRange {
    y: CellCountInt,
    x: [CellCountInt; 2],
    cp_offset: usize,
}

/// The per-cell selection precedence used by [`Renderer::rebuild_row`].
// PORT NOTE (macos_impl generic.rs:2774-2788): reference anonymous enum `const
// selected: enum { false, selection, search, search_selected }` (inline in the loop,
// generic.zig:2752-2757), hoisted to a module-private named enum (`False` renames
// `.false`). `SearchSelected` faithfully mirrors `.search_selected`.
#[derive(Clone, Copy, PartialEq, Eq)]
#[allow(clippy::enum_variant_names)]
enum Selected {
    False,
    Selection,
    Search,
    SearchSelected,
}

/// PORT NOTE (macos_impl generic.rs:2790-2817): the un-caught error sources of
/// `rebuild_row` (the reference `!void`). The only `try`s NOT `catch`-logged inline
/// are `run_iter.next` (a `collection::Error`) and `font_shaper.shape`. FAITHFUL
/// DIVERGENCE: `macos_impl` names the shaper arm `shape::coretext::Error` (CoreText
/// is the macOS shaper); the Linux backend is `harfbuzz::Shaper`
/// (`rustty_font_shaper::shape::Shaper` cfg-selects `harfbuzz` off macOS), so the arm
/// is `shape::harfbuzz::Error` here. The payloads carry the faithful error value the
/// reference logs (`log.warn`). UPDATED (task-14c-5): the `log` facade is wired and
/// this arm's site is now LIVE too — `rebuild_cells` binds the payload and emits
/// `log::warn!(target: LOG_SCOPE, "error building row y={y} err={err:?}")`
/// (`generic.zig:2627`), matching the `macos_impl` twin. The `#[derive(Debug)]` is
/// what the `{err:?}` rendering reads; the `#[allow(dead_code)]` STAYS because
/// rustc's `dead_code` lint does not count a `#[derive(Debug)]` field read as a use,
/// so the variant payloads would still be reported "never read" without it (same
/// reasoning as `generic.rs:3309`).
#[derive(Debug)]
#[allow(dead_code)]
enum RebuildRowError {
    Collection(collection::Error),
    Shape(shape::harfbuzz::Error),
}

impl From<collection::Error> for RebuildRowError {
    fn from(e: collection::Error) -> Self {
        RebuildRowError::Collection(e)
    }
}
impl From<shape::harfbuzz::Error> for RebuildRowError {
    fn from(e: shape::harfbuzz::Error) -> Self {
        RebuildRowError::Shape(e)
    }
}

impl Renderer {
    pub fn rebuild_cells(
        &mut self,
        preedit: Option<state::Preedit>,
        cursor_style_: Option<cursor::Style>,
        links: &CellSet,
    ) {
        // const state: *terminal.RenderState = &self.terminal_state;
        //
        // SAFETY (the REAL invariant — NOT "disjoint fields", GPU_RD_CHUNK3B_DESIGN
        // §C.3(1)): no INLINE-self alias read through `state` is held live across a
        // nested `&mut *self` reborrow. Every inline scalar a post-`rebuild_row`
        // block needs (`colors.{foreground,background,cursor,palette}`,
        // `cursor.{viewport,cell}`, `rows/cols/dirty`) is Copy-hoisted to a local
        // BELOW, BEFORE the row loop's `&mut *self` sub-calls (remedy 1). The
        // per-row `dirty=false` write-back goes through a fresh `&mut
        // self.terminal_state` *before* `rebuild_row`, so it is never aliased with
        // a read. `page::Cell`/`Style`/`RGB`/`Colors`-scalars are `Copy`, so hoisted
        // values leave the `*state` region by copy. The pre-loop `self.cells.*` /
        // `self.uniforms.*` writes are FIELD-level `&mut` (they retag only that
        // field, not the whole `Renderer` allocation), so `state` survives them.
        // Registered in `.port/phase-2/MIRI_EXCEPTIONS.md` (adversary-only — the
        // Metal-gated 3b test cannot run Miri).
        let state: *const RenderState = &self.terminal_state;

        // const start = try std.time.Instant.now();
        // const start_micro = std.time.microTimestamp();
        // defer {
        //     const end = std.time.Instant.now() catch unreachable;
        //     // "[rebuildCells time] <START us>\t<TIME_TAKEN us>"
        //     std.log.warn("[rebuildCells time] {}\t{}", .{start_micro, end.since(start) / std.time.ns_per_us});
        // }

        let grid_size_diff = self.cells.size.rows != unsafe { (*state).rows }
            || self.cells.size.columns != unsafe { (*state).cols };

        if grid_size_diff {
            let mut new_size = self.cells.size;
            new_size.rows = unsafe { (*state).rows };
            new_size.columns = unsafe { (*state).cols };
            self.cells.resize(new_size);

            // Update our uniforms accordingly, otherwise
            // our background cells will be out of place.
            self.uniforms.grid_size = [new_size.columns, new_size.rows];
        }

        let rebuild = unsafe { (*state).dirty } == Dirty::Full || grid_size_diff;
        if rebuild {
            // If we are doing a full rebuild, then we clear the entire cell buffer.
            self.cells.reset();

            // We also reset our padding extension depending on the screen type
            match self.config.padding_color {
                WindowPaddingColor::Background => {}

                // For extension, assume we are extending in all directions.
                // For "extend" this may be disabled due to heuristics below.
                WindowPaddingColor::Extend | WindowPaddingColor::ExtendAlways => {
                    self.uniforms.padding_extend = UniformsPaddingExtend {
                        up: true,
                        down: true,
                        left: true,
                        right: true,
                    };
                }
            }
        }

        // From this point on we never fail. We produce some kind of
        // working terminal state, even if incorrect.
        // (reference `errdefer comptime unreachable;` — the Rust port is infallible
        // from here, hence `rebuild_cells` returns `()`.)

        // §C.3 REMEDY 1: Copy-hoist every inline `terminal_state` scalar the
        // cursor/preedit blocks read AFTER the row loop, BEFORE the loop.
        let hoisted_foreground = unsafe { (*state).colors.foreground };
        let hoisted_background = unsafe { (*state).colors.background };
        let hoisted_colors_cursor = unsafe { (*state).colors.cursor };
        let hoisted_palette = unsafe { (*state).colors.palette };
        let hoisted_cursor_cell = unsafe { (*state).cursor.cell };
        // `Viewport` is not `Copy`, so hoist its three scalar fields by value.
        let hoisted_cursor_vp: Option<(CellCountInt, CellCountInt, bool)> =
            unsafe { (*state).cursor.viewport.as_ref() }.map(|v| (v.x, v.y, v.wide_tail));
        let rows = unsafe { (*state).rows };
        let cols = unsafe { (*state).cols };

        // If our cell contents buffer is shorter than the screen viewport,
        // we render the rows that fit, starting from the bottom. If instead
        // the viewport is shorter than the cell contents buffer, we align
        // the top of the viewport with the top of the contents buffer.
        let row_len: usize = core::cmp::min(rows as usize, self.cells.size.rows as usize);

        // Determine our x/y range for preedit. We don't want to render anything
        // here because we will render the preedit separately.
        let preedit_range: Option<PreeditRange> = if let Some(preedit_v) = preedit.as_ref() {
            'preedit: {
                // We base the preedit on the position of the cursor in the
                // viewport. If the cursor isn't visible in the viewport we
                // don't show it.
                let (cvx, cvy, _) = match hoisted_cursor_vp {
                    Some(v) => v,
                    None => break 'preedit None,
                };

                // If our preedit row isn't dirty then we don't need the
                // preedit range. This also avoids an issue later where we
                // unconditionally add preedit cells when this is set.
                if !rebuild && !self.terminal_state.row_data[cvy as usize].dirty {
                    break 'preedit None;
                }

                let range = preedit_v.range(cvx, cols - 1);
                Some(PreeditRange {
                    y: cvy,
                    x: [range.start, range.end],
                    cp_offset: range.cp_offset,
                })
            }
        } else {
            None
        };

        // The SGR style AT the cursor cell (ref :2458-2465, distinct from the
        // VISUAL cursor `style` below), hoisted BEFORE the row loop (remedy 1:
        // `Style` is `Copy`, and `rebuild_row` never mutates `row_data`, so the
        // value is identical whether read here or in the cursor block). It feeds
        // the `cell-foreground`/`cell-background` cursor-color + cursor-text arms.
        let cursor_sgr_style: Style = match hoisted_cursor_vp {
            Some((cvx, cvy, _)) => {
                // PORT NOTE (reshape): read the SoA `raw`/`style` columns.
                let cc = &self.terminal_state.row_data[cvy as usize].cells;
                if cc.raw()[cvx as usize].has_styling() {
                    cc.style()[cvx as usize]
                } else {
                    Style::default()
                }
            }
            None => Style::default(),
        };

        for y_usize in 0..row_len {
            let y: CellCountInt = y_usize as CellCountInt;

            if !rebuild {
                // Only rebuild if we are doing a full rebuild or this row is dirty.
                if !self.terminal_state.row_data[y_usize].dirty {
                    continue;
                }

                // Clear the cells if the row is dirty
                self.cells.clear(y);
            }

            // Unmark the dirty state in our render state. (Fresh `&mut
            // self.terminal_state` write-back BEFORE `rebuild_row`, so it is not
            // aliased with any read — §C.3.)
            self.terminal_state.row_data[y_usize].dirty = false;

            // Get our row data from our state
            // (PORT NOTE: the reference projects the row's SoA columns once —
            // `state.row_data.slice()` + `.items(.raw/.cells/.dirty/.selection/
            // .highlights)`, ref :2364-2370; rustty's `row_data` is AoS, so we read
            // each column per-row here inside the loop.)
            let row: page::Row = self.terminal_state.row_data[y_usize].raw;
            let selection = self.terminal_state.row_data[y_usize].selection;
            // HEAP-backed raw slices (the `Vec` backing buffers of
            // `row_data[y].cells`/`highlights`) — SB-SAFE across `rebuild_row`'s
            // `&mut *self` reborrows (§C.3(2)): a reborrow retags the inline
            // `Renderer`, not these separate heap allocations.
            // PORT NOTE (reshape): a `*const CellList` (the reference passes
            // `cells: *std.MultiArrayList(RenderState.Cell)` into `rebuildRow`).
            let cells_ptr: *const render::CellList = &self.terminal_state.row_data[y_usize].cells;
            let highlights_ptr: *const [Highlight] =
                self.terminal_state.row_data[y_usize].highlights.as_slice();

            if let Err(err) = self.rebuild_row(
                y,
                row,
                cells_ptr,
                preedit_range,
                selection,
                highlights_ptr,
                links,
            ) {
                // This should never happen except under exceptional
                // scenarios. In this case, we don't want to corrupt
                // our render state so just clear this row and keep
                // trying to finish it out.
                // log.warn("error building row y={} err={}", .{ y, err });
                log::warn!(target: LOG_SCOPE, "error building row y={y} err={err:?}");
                self.cells.clear(y);
            }
        }

        // Setup our cursor rendering information.
        'cursor: {
            // Clear our cursor by default.
            self.cells.set_cursor(None, None);
            self.uniforms.cursor_pos = [u16::MAX, u16::MAX];

            // If the cursor isn't visible on the viewport, don't show
            // a cursor. Otherwise, get our cursor cell, because we may
            // need it for styling.
            let (cvx, cvy, _cv_wide_tail) = match hoisted_cursor_vp {
                Some(v) => v,
                None => break 'cursor,
            };
            let cursor_style: Style = cursor_sgr_style;

            // If we have preedit text, we don't setup a cursor
            if preedit.is_some() {
                break 'cursor;
            }

            // If there isn't a cursor visual style requested then
            // we don't render a cursor.
            let style = match cursor_style_ {
                Some(s) => s,
                None => break 'cursor,
            };

            // Determine the cursor color.
            let cursor_color: color::RGB = 'cursor_color: {
                // If an explicit cursor color was set by OSC 12, use that.
                if let Some(v) = hoisted_colors_cursor {
                    break 'cursor_color v;
                }

                // Use our configured color if specified
                if let Some(v) = self.config.cursor_color {
                    match v {
                        TerminalColor::Color(color) => break 'cursor_color color.to_terminal_rgb(),

                        TerminalColor::CellForeground | TerminalColor::CellBackground => {
                            let fg_style = cursor_style.fg(Fg {
                                default: hoisted_foreground,
                                palette: &hoisted_palette,
                                bold: self.config.bold_color,
                            });
                            let bg_style = cursor_style
                                .bg(&hoisted_cursor_cell, &hoisted_palette)
                                .unwrap_or(hoisted_background);

                            break 'cursor_color match v {
                                TerminalColor::Color(_) => unreachable!(),
                                TerminalColor::CellForeground => {
                                    if cursor_style.flags.inverse {
                                        bg_style
                                    } else {
                                        fg_style
                                    }
                                }
                                TerminalColor::CellBackground => {
                                    if cursor_style.flags.inverse {
                                        fg_style
                                    } else {
                                        bg_style
                                    }
                                }
                            };
                        }
                    }
                }

                hoisted_foreground
            };

            // self.addCursor(&state.cursor, style, cursor_color); (§D.6)
            // SAFETY (§C.3/§D.6): BOTH `self` and `cursor_ptr` are raw, exactly as
            // the reference `addCursor(self: *Self, cursor_state: *const ...)`.
            // Taking `add_cursor` raw means its receiver does NOT retag the whole
            // `Renderer` and pop `cursor_ptr` (the `*const` alias into inline
            // `self.terminal_state.cursor`) before the body derefs it — the SB UB
            // the Miri harness proved for a `&mut self` receiver.
            let self_raw: *mut Renderer = self;
            let cursor_ptr: *const render::Cursor = unsafe { &(*self_raw).terminal_state.cursor };
            unsafe { Renderer::add_cursor(self_raw, cursor_ptr, style, cursor_color) };

            // If the cursor is visible then we set our uniforms.
            if style == cursor::Style::Block {
                let wide = hoisted_cursor_cell.wide();

                self.uniforms.cursor_pos = [
                    // If we are a spacer tail of a wide cell, our cursor needs
                    // to move back one cell. The saturate is to ensure we don't
                    // overflow but this shouldn't happen with well-formed input.
                    match wide {
                        page::Wide::Narrow | page::Wide::SpacerHead | page::Wide::Wide => cvx,
                        page::Wide::SpacerTail => cvx.saturating_sub(1),
                    },
                    cvy,
                ];

                self.uniforms.bools.cursor_wide = match wide {
                    page::Wide::Narrow | page::Wide::SpacerHead => false,
                    page::Wide::Wide | page::Wide::SpacerTail => true,
                };

                let uniform_color: color::RGB = if let Some(txt) = self.config.cursor_text {
                    'blk: {
                        // If cursor-text is set, then compute the correct color.
                        // Otherwise, use the background color.
                        if let TerminalColor::Color(color) = txt {
                            // Use the color set by cursor-text, if any.
                            break 'blk color.to_terminal_rgb();
                        }

                        let fg_style = cursor_style.fg(Fg {
                            default: hoisted_foreground,
                            palette: &hoisted_palette,
                            bold: self.config.bold_color,
                        });
                        let bg_style = cursor_style
                            .bg(&hoisted_cursor_cell, &hoisted_palette)
                            .unwrap_or(hoisted_background);

                        break 'blk match txt {
                            // If the cell is reversed, use the opposite cell color instead.
                            TerminalColor::CellForeground => {
                                if cursor_style.flags.inverse {
                                    bg_style
                                } else {
                                    fg_style
                                }
                            }
                            TerminalColor::CellBackground => {
                                if cursor_style.flags.inverse {
                                    fg_style
                                } else {
                                    bg_style
                                }
                            }
                            _ => unreachable!(),
                        };
                    }
                } else {
                    hoisted_background
                };

                self.uniforms.cursor_color =
                    [uniform_color.r, uniform_color.g, uniform_color.b, 255];
            }
        }

        // Setup our preedit text.
        if let Some(preedit_v) = preedit.as_ref() {
            'preedit: {
                let range = match preedit_range {
                    Some(r) => r,
                    None => break 'preedit,
                };
                let mut x = range.x[0];
                for cp in preedit_v.codepoints[range.cp_offset..].iter() {
                    if let Err(err) = self.add_preedit_cell(
                        *cp,
                        point::Coordinate {
                            x,
                            y: range.y as u32,
                        },
                        hoisted_foreground,
                    ) {
                        // log.warn("error building preedit cell, will be invalid
                        //           x={} y={}, err={}", .{ x, range.y, err });
                        log::warn!(
                            target: LOG_SCOPE,
                            "error building preedit cell, will be invalid x={} y={}, err={:?}",
                            x,
                            range.y,
                            err
                        );
                    }

                    x += if cp.wide { 2 } else { 1 };
                }
            }
        }

        // Update that our cells rebuilt
        self.cells_rebuilt = true;

        // Log some things
        // log.debug("rebuildCells complete cached_runs={}", .{
        //     self.font_shaper_cache.count(),
        // });
    }

    // fn rebuildRow(self, y, row, cells: *MultiArrayList(Cell), preedit_range,
    //               selection, highlights: *const ArrayList(Highlight), links) !void
    // PORT NOTE (reshape): `cells: *MultiArrayList(Cell)` -> `*const CellList`.
    //
    // SAFETY (§C.3(2,3)): `cells`/`highlights` are HEAP-backed raw ptrs (the `Vec`
    // backing buffers of `row_data[y]`), SB-SAFE across this method's sub-calls
    // (a different allocation from the `Renderer`). The one INLINE-self leg is the
    // shaper hook: `run_iter`'s `RunIteratorHook { shaper: *mut Shaper }` aliases
    // inline `self.font_shaper` and must stay live across the whole run loop.
    //
    // A `&mut self`-receiver `self.font_shaper.shape(...)` (a SIBLING reborrow of
    // `font_shaper`) or `self.add_glyph(...)` (a WHOLE-`Renderer` `&mut *self`
    // receiver reborrow, which covers `font_shaper`) POPS the hook's tag —
    // Undefined Behavior under BOTH Stacked and Tree Borrows, permanently (the tag
    // is not restored when the transient borrow ends). This was proven on an
    // isolated, non-FFI Miri harness reproducing this exact borrow structure. So
    // the loop is kept SB/TB-clean (the `self_raw` established below) by:
    //   (a) `run_iterator` returns an iterator holding only raw ptrs (the hook + a
    //       synthetic-lifetime `&[render::Cell]` from `cells`), borrowing no `self`;
    //   (b) `shape` is invoked THROUGH the hook's own raw ptr
    //       `run_iter.hooks.shaper` — a CHILD reborrow of the hook, which does not
    //       pop it — never `self.font_shaper.shape`;
    //   (c) the `add_*` helpers are RAW-self associated fns called via `self_raw`,
    //       using only NARROW field access (never a whole `&mut *self_raw`); and
    //   (d) direct `self.field` access in the loop body (`self.cells.bg_cell`,
    //       `self.uniforms`, `self.font_shaper_cache`) is disjoint from
    //       `font_shaper`, so it retags a different location and leaves the hook.
    // (GPU_RD_CHUNK3B_DESIGN §C.3 remedy 2; the Metal-gated test cannot run Miri,
    // so this is registered in MIRI_EXCEPTIONS.md — but, unlike an FFI exception,
    // the borrow structure IS SB/TB-clean by construction, harness-proven.)
    // The `shaper_cells` raw ptr is a heap slice into the shaper `cell_buf` / cache
    // Box (§C.3(3)): captured immediately after `shape`/`get`, consumed within the
    // same run's glyph loop, `put` COPIES (does not free) it, and `run_iter.next`
    // nulls it before the next `shape` overwrites the buffer.
    // (`too_many_arguments`: faithful to the reference `rebuildRow` arg list.)
    #[allow(clippy::too_many_arguments)]
    fn rebuild_row(
        &mut self,
        y: CellCountInt,
        row: page::Row,
        cells: *const render::CellList,
        preedit_range: Option<PreeditRange>,
        selection: Option<[CellCountInt; 2]>,
        highlights: *const [Highlight],
        links: &CellSet,
    ) -> Result<(), RebuildRowError> {
        // const state = &self.terminal_state;
        let state: *const RenderState = &self.terminal_state;

        // §C.3 REMEDY 1: hoist the inline `terminal_state` scalars the per-cell
        // color resolution reads, BEFORE the shape loop's `&mut *self` sub-calls.
        let palette = unsafe { (*state).colors.palette };
        let foreground = unsafe { (*state).colors.foreground };
        let background = unsafe { (*state).colors.background };
        let cols = unsafe { (*state).cols };

        // If our viewport is wider than our cell contents buffer,
        // we still only process cells up to the width of the buffer.
        // (`cells_ref`/its columns are HEAP-backed — the `Vec` backing buffers
        // of `row_data[y].cells` — so they survive `&mut *self`; §C.3(2).)
        let cells_ref: &render::CellList = unsafe { &*cells };
        let cells_slice: render::CellSlice = cells_ref.slice();
        // PORT NOTE (reshape): the reference reads two SoA projections
        // `cells.items(.raw)` / `.items(.style)`; rustty's cells are the SoA
        // `CellList`, so `cells_raw` IS the `raw` column (`cells_slice.raw`, a
        // borrow into `row_data`'s heap — a separate allocation from `Renderer`,
        // not one of the §C.3 raw-pointer triad) and `cells_slice.style[x]` is
        // the `.items(.style)` projection.
        let cells_raw: &[page::Cell] = cells_slice.raw;
        let cells_len = core::cmp::min(cells_raw.len(), self.cells.size.columns as usize);

        // On primary screen, we still apply vertical padding
        // extension under certain conditions we feel are safe.
        //
        // This helps make some scenarios look better while
        // avoiding scenarios we know do NOT look good.
        match self.config.padding_color {
            // These already have the correct values set above.
            WindowPaddingColor::Background | WindowPaddingColor::ExtendAlways => {}

            // Apply heuristics for padding extension.
            WindowPaddingColor::Extend => {
                if y == 0 {
                    self.uniforms.padding_extend.up =
                        !never_extend_bg(row, cells_raw, cells_slice.style, &palette, background);
                } else if y == self.cells.size.rows - 1 {
                    self.uniforms.padding_extend.down =
                        !never_extend_bg(row, cells_raw, cells_slice.style, &palette, background);
                }
            }
        }

        // Iterator of runs for shaping.
        let mut run_iter_opts: shape::RunOptions = shape::RunOptions {
            grid: self.font_grid,
            cells: cells_slice,
            selection,

            // We want to do font shaping as long as the cursor is
            // visible on this viewport.
            cursor_x: 'cursor_x: {
                let vp = match unsafe { (*state).cursor.viewport.as_ref() } {
                    Some(vp) => vp,
                    None => break 'cursor_x None,
                };
                if vp.y != y {
                    break 'cursor_x None;
                }
                Some(vp.x as usize)
            },
        };
        run_iter_opts.apply_break_config(shape::FontShapingBreak {
            cursor: self.config.font_shaping_break.cursor,
        });
        // Raw `*mut Renderer` used for the interleaved cell-fill helpers below
        // (§C.3 shaper-hook remedy): `run_iter` holds a `RunIteratorHook`'s raw
        // `*mut Shaper` alias into `self.font_shaper` that MUST stay live across the
        // whole run loop, but each `self.add_glyph(...)`/`add_underline(...)` call
        // and each `self.font_shaper.shape(...)` would, as a `&mut self` reborrow,
        // pop that hook tag (Miri-proven UB, SB + TB). So: (a) the `add_*` helpers
        // are taken RAW via `self_raw` (narrow field access only — no whole
        // `&mut *self_raw`), and (b) `shape` is invoked through the hook's OWN raw
        // pointer `run_iter.hooks.shaper` (a child reborrow of the hook, which does
        // not pop it), NOT through `self.font_shaper` (a sibling reborrow, which
        // does). Narrow `self.field` access (`self.cells.bg_cell`, `self.uniforms`,
        // `self.font_shaper_cache`) is disjoint from `font_shaper` and stays safe.
        let self_raw: *mut Renderer = self;
        let mut run_iter = self.font_shaper.run_iterator(run_iter_opts);
        let mut shaper_run: Option<shape::TextRun> = run_iter.next(self.alloc)?;
        let mut shaper_cells: Option<*const [shape::Cell]> = None;
        let mut shaper_cells_i: usize = 0;

        'cells: for x in 0..cells_len {
            let cell: &page::Cell = &cells_raw[x];
            let managed_style: Style = cells_slice.style[x];

            // If this cell falls within our preedit range then we
            // skip this because preedits are setup separately.
            if let Some(range) = preedit_range {
                'preedit: {
                    // We're not on the preedit line, no actions necessary.
                    if range.y != y {
                        break 'preedit;
                    }
                    // We're before the preedit range, no actions necessary.
                    if (x as CellCountInt) < range.x[0] {
                        break 'preedit;
                    }
                    // We're in the preedit range, skip this cell.
                    if (x as CellCountInt) <= range.x[1] {
                        continue 'cells;
                    }
                    // After exiting the preedit range we need to catch
                    // the run position up because of the missed cells.
                    // In all other cases, no action is necessary.
                    if (x as CellCountInt) != range.x[1] + 1 {
                        break 'preedit;
                    }

                    // Step the run iterator until we find a run that ends
                    // after the current cell, which will be the soonest run
                    // that might contain glyphs for our cell.
                    while let Some(run) = shaper_run {
                        if run.offset as usize + run.cells as usize > x {
                            break;
                        }
                        shaper_run = run_iter.next(self.alloc)?;
                        shaper_cells = None;
                        shaper_cells_i = 0;
                    }

                    let run = match shaper_run {
                        Some(r) => r,
                        None => break 'preedit,
                    };

                    // If we haven't shaped this run, do so now.
                    if shaper_cells.is_none() {
                        shaper_cells = match self.font_shaper_cache.get(&run) {
                            // Try to read the cells from the shaping cache if we can.
                            Some(c) => Some(c as *const [shape::Cell]),
                            None => {
                                // Otherwise we have to shape them.
                                // SAFETY (§C.3 shaper-hook remedy): shape THROUGH the
                                // hook's own raw `*mut Shaper` (`run_iter.hooks.
                                // shaper`), a child reborrow that does NOT pop the
                                // hook — a `self.font_shaper.shape(...)` sibling
                                // reborrow WOULD pop it (Miri-proven UB). Same Shaper
                                // object, byte-identical to the reference `self.
                                // font_shaper.shape(run)`.
                                let new_cells = unsafe { (*run_iter.hooks.shaper).shape(run) }?;
                                let new_ptr = new_cells as *const [shape::Cell];

                                // Try to cache them. If caching fails for any reason we
                                // continue because it is just a performance optimization,
                                // not a correctness issue.
                                // self.font_shaper_cache.put(self.alloc, run, new_cells) catch |err| {
                                //     log.warn("error caching font shaping results err={}", .{err});
                                // };
                                //
                                // LOG PORT NOTE (task-14c-5, N9 — Linux-side residual
                                // of the WAIVED row S116-F2): this `log.warn` is NOT
                                // restorable, exactly as in the Metal twin
                                // (task-14c-1). `ShaperCache::put`
                                // (`rustty-font-shaper/src/shaper/cache.rs:136`) is
                                // INFALLIBLE in rustty — it allocates through the
                                // infallible `Global`/`Box` path — so there is no
                                // `catch` arm to hang the warn on and emitting it
                                // unconditionally would put a diagnostic on the
                                // SUCCESS path. See ADJUDICATION.md#s116-f2.
                                self.font_shaper_cache.put(&run, unsafe { &*new_ptr });

                                // The cells we get from direct shaping are always owned
                                // by the shaper and valid until the next shaping call so
                                // we can safely use them.
                                Some(new_ptr)
                            }
                        };
                    }

                    // Advance our index until we reach or pass
                    // our current x position in the shaper cells.
                    let shaper_cells_unwrapped = unsafe { &*shaper_cells.unwrap() };
                    while (run.offset as usize + shaper_cells_unwrapped[shaper_cells_i].x as usize)
                        < x
                    {
                        shaper_cells_i += 1;
                    }
                }
            }

            let wide = cell.wide();
            let style: Style = if cell.has_styling() {
                managed_style
            } else {
                Style::default()
            };

            // True if this cell is selected
            let selected: Selected = 'selected: {
                // Order below matters for precedence.

                // Selection should take the highest precedence.
                let x_compare: usize = if wide == page::Wide::SpacerTail {
                    x.saturating_sub(1)
                } else {
                    x
                };
                if let Some(sel) = selection {
                    if x_compare >= sel[0] as usize && x_compare <= sel[1] as usize {
                        break 'selected Selected::Selection;
                    }
                }

                // If we're highlighted, then we're selected. In the
                // future we want to use a different style for this
                // but this to get started.
                for hl in unsafe { &*highlights }.iter() {
                    if x_compare >= hl.range[0] as usize && x_compare <= hl.range[1] as usize {
                        let tag: HighlightTag = match hl.tag {
                            0 => HighlightTag::SearchMatch,
                            1 => HighlightTag::SearchMatchSelected,
                            other => unreachable!("invalid HighlightTag {other}"),
                        };
                        break 'selected match tag {
                            HighlightTag::SearchMatch => Selected::Search,
                            HighlightTag::SearchMatchSelected => Selected::SearchSelected,
                        };
                    }
                }

                Selected::False
            };

            // The `_style` suffixed values are the colors based on
            // the cell style (SGR), before applying any additional
            // configuration, inversions, selections, etc.
            let bg_style: Option<color::RGB> = style.bg(cell, &palette);
            let fg_style: color::RGB = style.fg(Fg {
                default: foreground,
                palette: &palette,
                bold: self.config.bold_color,
            });

            // The final background color for the cell.
            let bg: Option<color::RGB> = match selected {
                // If we have an explicit selection background color
                // specified in the config, use that.
                //
                // If no configuration, then our selection background
                // is our foreground color.
                Selected::Selection => match self.config.selection_background {
                    Some(v) => match v {
                        TerminalColor::Color(color) => Some(color.to_terminal_rgb()),
                        TerminalColor::CellForeground => {
                            if style.flags.inverse {
                                bg_style
                            } else {
                                Some(fg_style)
                            }
                        }
                        TerminalColor::CellBackground => {
                            if style.flags.inverse {
                                Some(fg_style)
                            } else {
                                bg_style
                            }
                        }
                    },
                    None => Some(foreground),
                },

                Selected::Search => match self.config.search_background {
                    TerminalColor::Color(color) => Some(color.to_terminal_rgb()),
                    TerminalColor::CellForeground => {
                        if style.flags.inverse {
                            bg_style
                        } else {
                            Some(fg_style)
                        }
                    }
                    TerminalColor::CellBackground => {
                        if style.flags.inverse {
                            Some(fg_style)
                        } else {
                            bg_style
                        }
                    }
                },

                Selected::SearchSelected => match self.config.search_selected_background {
                    TerminalColor::Color(color) => Some(color.to_terminal_rgb()),
                    TerminalColor::CellForeground => {
                        if style.flags.inverse {
                            bg_style
                        } else {
                            Some(fg_style)
                        }
                    }
                    TerminalColor::CellBackground => {
                        if style.flags.inverse {
                            Some(fg_style)
                        } else {
                            bg_style
                        }
                    }
                },

                // Not selected
                Selected::False => {
                    if style.flags.inverse != cell::is_covering(cell.codepoint()) {
                        // Two cases cause us to invert (use the fg color as the bg)
                        // - The "inverse" style flag.
                        // - A "covering" glyph; we use fg for bg in that
                        //   case to help make sure that padding extension
                        //   works correctly.
                        //
                        // If one of these is true (but not the other)
                        // then we use the fg style color for the bg.
                        Some(fg_style)
                    } else {
                        // Otherwise they cancel out.
                        bg_style
                    }
                }
            };

            let fg: color::RGB = {
                // Our happy-path non-selection background color
                // is our style or our configured defaults.
                let final_bg = bg_style.unwrap_or(background);

                // Whether we need to use the bg color as our fg color:
                // - Cell is selected, inverted, and set to cell-foreground
                // - Cell is selected, not inverted, and set to cell-background
                // - Cell is inverted and not selected
                match selected {
                    Selected::Selection => match self.config.selection_foreground {
                        Some(v) => match v {
                            TerminalColor::Color(color) => color.to_terminal_rgb(),
                            TerminalColor::CellForeground => {
                                if style.flags.inverse {
                                    final_bg
                                } else {
                                    fg_style
                                }
                            }
                            TerminalColor::CellBackground => {
                                if style.flags.inverse {
                                    fg_style
                                } else {
                                    final_bg
                                }
                            }
                        },
                        None => background,
                    },

                    Selected::Search => match self.config.search_foreground {
                        TerminalColor::Color(color) => color.to_terminal_rgb(),
                        TerminalColor::CellForeground => {
                            if style.flags.inverse {
                                final_bg
                            } else {
                                fg_style
                            }
                        }
                        TerminalColor::CellBackground => {
                            if style.flags.inverse {
                                fg_style
                            } else {
                                final_bg
                            }
                        }
                    },

                    Selected::SearchSelected => match self.config.search_selected_foreground {
                        TerminalColor::Color(color) => color.to_terminal_rgb(),
                        TerminalColor::CellForeground => {
                            if style.flags.inverse {
                                final_bg
                            } else {
                                fg_style
                            }
                        }
                        TerminalColor::CellBackground => {
                            if style.flags.inverse {
                                fg_style
                            } else {
                                final_bg
                            }
                        }
                    },

                    Selected::False => {
                        if style.flags.inverse {
                            final_bg
                        } else {
                            fg_style
                        }
                    }
                }
            };

            // Foreground alpha for this cell.
            let alpha: u8 = if style.flags.faint {
                self.config.faint_opacity
            } else {
                255
            };

            // Set the cell's background color.
            {
                let rgb = bg.unwrap_or(background);

                // Determine our background alpha. If we have transparency configured
                // then this is dynamic depending on some situations. This is all
                // in an attempt to make transparency look the best for various
                // situations. See inline comments.
                let bg_alpha: u8 = 'bg_alpha: {
                    let default: u8 = 255;

                    // Cells that are selected should be fully opaque.
                    if selected != Selected::False {
                        break 'bg_alpha default;
                    }

                    // Cells that are reversed should be fully opaque.
                    if style.flags.inverse {
                        break 'bg_alpha default;
                    }

                    // If the user requested to have opacity on all cells, apply it.
                    if self.config.background_opacity_cells && bg_style.is_some() {
                        let mut opacity: f64 = default as f64;
                        opacity *= self.config.background_opacity;
                        break 'bg_alpha opacity as u8;
                    }

                    // Cells that have an explicit bg color should be fully opaque.
                    if bg_style.is_some() {
                        break 'bg_alpha default;
                    }

                    // Otherwise, we won't draw the bg for this cell,
                    // we'll let the already-drawn background color
                    // show through.
                    0
                };

                *self.cells.bg_cell(y as usize, x) = [rgb.r, rgb.g, rgb.b, bg_alpha];
            }

            // If the invisible flag is set on this cell then we
            // don't need to render any foreground elements, so
            // we just skip all glyphs with this x coordinate.
            //
            // NOTE: This behavior matches xterm. Some other terminal
            // emulators, e.g. Alacritty, still render text decorations
            // and only make the text itself invisible. The decision
            // has been made here to match xterm's behavior for this.
            if style.flags.invisible {
                continue;
            }

            // Give links a single underline, unless they already have
            // an underline, in which case use a double underline to
            // distinguish them.
            let underline: sgr::Underline = 'underline: {
                if links.contains(&point::Coordinate {
                    x: x as CellCountInt,
                    y: y as u32,
                }) {
                    break 'underline if style.flags.underline == sgr::Underline::Single {
                        sgr::Underline::Double
                    } else {
                        sgr::Underline::Single
                    };
                }
                style.flags.underline
            };

            // We draw underlines first so that they layer underneath text.
            // This improves readability when a colored underline is used
            // which intersects parts of the text (descenders).
            if underline != sgr::Underline::None {
                // SAFETY (§C.3): raw-self helper — see `self_raw`/`add_underline`.
                if let Err(err) = unsafe {
                    Renderer::add_underline(
                        self_raw,
                        x as CellCountInt,
                        y,
                        underline,
                        style.underline_color(&palette).unwrap_or(fg),
                        alpha,
                    )
                } {
                    // log.warn("error adding underline to cell, will be invalid
                    //           x={} y={}, err={}", .{ x, y, err });
                    log::warn!(
                        target: LOG_SCOPE,
                        "error adding underline to cell, will be invalid x={x} y={y}, err={err:?}"
                    );
                }
            }

            if style.flags.overline {
                // SAFETY (§C.3): raw-self helper — see `self_raw`/`add_underline`.
                if let Err(err) =
                    unsafe { Renderer::add_overline(self_raw, x as CellCountInt, y, fg, alpha) }
                {
                    // log.warn("error adding overline to cell, will be invalid
                    //           x={} y={}, err={}", .{ x, y, err });
                    log::warn!(
                        target: LOG_SCOPE,
                        "error adding overline to cell, will be invalid x={x} y={y}, err={err:?}"
                    );
                }
            }

            // If we're at or past the end of our shaper run then
            // we need to get the next run from the run iterator.
            if shaper_cells.is_some() && shaper_cells_i >= unsafe { &*shaper_cells.unwrap() }.len()
            {
                shaper_run = run_iter.next(self.alloc)?;
                shaper_cells = None;
                shaper_cells_i = 0;
            }

            if let Some(run) = shaper_run {
                'glyphs: {
                    // If we haven't shaped this run yet, do so.
                    if shaper_cells.is_none() {
                        shaper_cells = match self.font_shaper_cache.get(&run) {
                            // Try to read the cells from the shaping cache if we can.
                            Some(c) => Some(c as *const [shape::Cell]),
                            None => {
                                // Otherwise we have to shape them.
                                // SAFETY (§C.3 shaper-hook remedy): shape THROUGH the
                                // hook's own raw `*mut Shaper` (`run_iter.hooks.
                                // shaper`), a child reborrow that does NOT pop the
                                // hook — a `self.font_shaper.shape(...)` sibling
                                // reborrow WOULD pop it (Miri-proven UB). Same Shaper
                                // object, byte-identical to the reference `self.
                                // font_shaper.shape(run)`.
                                let new_cells = unsafe { (*run_iter.hooks.shaper).shape(run) }?;
                                let new_ptr = new_cells as *const [shape::Cell];

                                // Try to cache them. If caching fails for any reason we
                                // continue because it is just a performance optimization,
                                // not a correctness issue.
                                // self.font_shaper_cache.put(self.alloc, run, new_cells) catch |err| {
                                //     log.warn("error caching font shaping results err={}", .{err});
                                // };
                                //
                                // LOG PORT NOTE (task-14c-5, N9 — Linux-side residual
                                // of the WAIVED row S116-F2): this `log.warn` is NOT
                                // restorable, exactly as in the Metal twin
                                // (task-14c-1). `ShaperCache::put`
                                // (`rustty-font-shaper/src/shaper/cache.rs:136`) is
                                // INFALLIBLE in rustty — it allocates through the
                                // infallible `Global`/`Box` path — so there is no
                                // `catch` arm to hang the warn on and emitting it
                                // unconditionally would put a diagnostic on the
                                // SUCCESS path. See ADJUDICATION.md#s116-f2.
                                self.font_shaper_cache.put(&run, unsafe { &*new_ptr });

                                // The cells we get from direct shaping are always owned
                                // by the shaper and valid until the next shaping call so
                                // we can safely use them.
                                Some(new_ptr)
                            }
                        };
                    }

                    let shaped_cells: &[shape::Cell] = match shaper_cells {
                        Some(p) => unsafe { &*p },
                        None => break 'glyphs,
                    };

                    // If there are no shaper cells for this run, ignore it.
                    // This can occur for runs of empty cells, and is fine.
                    if shaped_cells.is_empty() {
                        break 'glyphs;
                    }

                    // If we encounter a shaper cell to the left of the current
                    // cell then we have some problems. This logic relies on x
                    // position monotonically increasing.
                    assert!(run.offset as usize + shaped_cells[shaper_cells_i].x as usize >= x);

                    // NOTE: An assumption is made here that a single cell will never
                    // be present in more than one shaper run. If that assumption is
                    // violated, this logic breaks.

                    while shaper_cells_i < shaped_cells.len()
                        && run.offset as usize + shaped_cells[shaper_cells_i].x as usize == x
                    {
                        // SAFETY (§C.3): raw-self helper — see `self_raw`/`add_glyph`.
                        if let Err(err) = unsafe {
                            Renderer::add_glyph(
                                self_raw,
                                x as CellCountInt,
                                y,
                                cols as usize,
                                cells_raw,
                                shaped_cells[shaper_cells_i],
                                shaper_run.unwrap(),
                                fg,
                                alpha,
                            )
                        } {
                            // log.warn("error adding glyph to cell, will be invalid
                            //           x={} y={}, err={}", .{ x, y, err });
                            log::warn!(
                                target: LOG_SCOPE,
                                "error adding glyph to cell, will be invalid x={x} y={y}, err={err:?}"
                            );
                        }
                        shaper_cells_i += 1;
                    }
                }
            }

            // Finally, draw a strikethrough if necessary.
            if style.flags.strikethrough {
                // SAFETY (§C.3): raw-self helper — see `self_raw`/`add_underline`.
                if let Err(err) = unsafe {
                    Renderer::add_strikethrough(self_raw, x as CellCountInt, y, fg, alpha)
                } {
                    // log.warn("error adding strikethrough to cell, will be invalid
                    //           x={} y={}, err={}", .{ x, y, err });
                    log::warn!(
                        target: LOG_SCOPE,
                        "error adding strikethrough to cell, will be invalid x={x} y={y}, err={err:?}"
                    );
                }
            }
        }

        Ok(())
    }

    /// Add an underline decoration to the specified cell
    // PORT NOTE (§C.3 aliasing remedy): taken RAW (`self_raw: *mut Renderer`),
    // NOT `&mut self`, mirroring the reference `addUnderline(self: *Self, ...)`.
    // It is called from `rebuild_row`'s shaper run loop while the
    // `RunIteratorHook`'s raw `*mut Shaper` alias into `self.font_shaper` is still
    // live; a `&mut self` receiver reborrow would retag the whole `Renderer`
    // (font_shaper included) and pop that hook tag — Undefined Behavior under both
    // Stacked and Tree Borrows (Miri-proven). Taken raw with only NARROW field
    // access (`(*self_raw).field`, never a whole `&mut *self_raw`), it leaves the
    // hook tag intact. See the §C.3 SAFETY block on `rebuild_row`.
    unsafe fn add_underline(
        self_raw: *mut Renderer,
        x: CellCountInt,
        y: CellCountInt,
        style: sgr::Underline,
        color: color::RGB,
        alpha: u8,
    ) -> Result<(), RenderError> {
        let sprite: Sprite = match style {
            sgr::Underline::None => unreachable!(),
            sgr::Underline::Single => Sprite::Underline,
            sgr::Underline::Double => Sprite::UnderlineDouble,
            sgr::Underline::Dotted => Sprite::UnderlineDotted,
            sgr::Underline::Dashed => Sprite::UnderlineDashed,
            sgr::Underline::Curly => Sprite::UnderlineCurly,
        };

        // SAFETY: `self.font_grid` is the shared `*mut SharedGrid` handle (the
        // `Options.font_grid` decision); the grid owns an internal `RwLock`, so
        // the `&mut` through the raw ptr is the reference `self.font_grid.
        // renderGlyph` exactly. `(*self_raw).{font_grid,alloc,grid_metrics}` are
        // NARROW reads of disjoint fields (never a whole-`self` reborrow).
        let render = unsafe {
            (*(*self_raw).font_grid).render_glyph(
                (*self_raw).alloc,
                SPRITE_INDEX,
                sprite as u32,
                RenderOptions {
                    cell_width: Some(1),
                    grid_metrics: (*self_raw).grid_metrics,
                    ..RenderOptions::default()
                },
            )
        }?;

        // SAFETY: narrow `&mut (*self_raw).cells` (the `Contents` field), disjoint
        // from `self.font_shaper` — does not pop the caller's live shaper hook.
        unsafe {
            (*self_raw).cells.add(
                Key::Underline,
                CellText {
                    atlas: Atlas::Grayscale,
                    grid_pos: [x, y],
                    color: [color.r, color.g, color.b, alpha],
                    glyph_pos: [render.glyph.atlas_x, render.glyph.atlas_y],
                    glyph_size: [render.glyph.width, render.glyph.height],
                    bearings: [render.glyph.offset_x as i16, render.glyph.offset_y as i16],
                    ..Default::default()
                },
            );
        }
        Ok(())
    }

    /// Add a overline decoration to the specified cell
    // PORT NOTE: raw-self (`self_raw: *mut Renderer`) — see `add_underline` (§C.3):
    // called from the shaper run loop, so a `&mut self` receiver would pop the live
    // hook. Narrow field access only.
    unsafe fn add_overline(
        self_raw: *mut Renderer,
        x: CellCountInt,
        y: CellCountInt,
        color: color::RGB,
        alpha: u8,
    ) -> Result<(), RenderError> {
        // SAFETY: see `add_underline` — narrow `(*self_raw).{font_grid,alloc,
        // grid_metrics}` reads; the grid handle owns its own `RwLock`.
        let render = unsafe {
            (*(*self_raw).font_grid).render_glyph(
                (*self_raw).alloc,
                SPRITE_INDEX,
                Sprite::Overline as u32,
                RenderOptions {
                    cell_width: Some(1),
                    grid_metrics: (*self_raw).grid_metrics,
                    ..RenderOptions::default()
                },
            )
        }?;

        // SAFETY: narrow `&mut (*self_raw).cells`, disjoint from `self.font_shaper`.
        unsafe {
            (*self_raw).cells.add(
                Key::Overline,
                CellText {
                    atlas: Atlas::Grayscale,
                    grid_pos: [x, y],
                    color: [color.r, color.g, color.b, alpha],
                    glyph_pos: [render.glyph.atlas_x, render.glyph.atlas_y],
                    glyph_size: [render.glyph.width, render.glyph.height],
                    bearings: [render.glyph.offset_x as i16, render.glyph.offset_y as i16],
                    ..Default::default()
                },
            );
        }
        Ok(())
    }

    /// Add a strikethrough decoration to the specified cell
    // PORT NOTE: raw-self (`self_raw: *mut Renderer`) — see `add_underline` (§C.3):
    // called from the shaper run loop, so a `&mut self` receiver would pop the live
    // hook. Narrow field access only.
    unsafe fn add_strikethrough(
        self_raw: *mut Renderer,
        x: CellCountInt,
        y: CellCountInt,
        color: color::RGB,
        alpha: u8,
    ) -> Result<(), RenderError> {
        // SAFETY: see `add_underline` — narrow `(*self_raw).{font_grid,alloc,
        // grid_metrics}` reads; the grid handle owns its own `RwLock`.
        let render = unsafe {
            (*(*self_raw).font_grid).render_glyph(
                (*self_raw).alloc,
                SPRITE_INDEX,
                Sprite::Strikethrough as u32,
                RenderOptions {
                    cell_width: Some(1),
                    grid_metrics: (*self_raw).grid_metrics,
                    ..RenderOptions::default()
                },
            )
        }?;

        // SAFETY: narrow `&mut (*self_raw).cells`, disjoint from `self.font_shaper`.
        unsafe {
            (*self_raw).cells.add(
                Key::Strikethrough,
                CellText {
                    atlas: Atlas::Grayscale,
                    grid_pos: [x, y],
                    color: [color.r, color.g, color.b, alpha],
                    glyph_pos: [render.glyph.atlas_x, render.glyph.atlas_y],
                    glyph_size: [render.glyph.width, render.glyph.height],
                    bearings: [render.glyph.offset_x as i16, render.glyph.offset_y as i16],
                    ..Default::default()
                },
            );
        }
        Ok(())
    }

    // Add a glyph to the specified cell.
    // PORT NOTE: raw-self (`self_raw: *mut Renderer`), mirroring the reference
    // `addGlyph(self: *Self, ...)` — see `add_underline` (§C.3). It is called from
    // the shaper run loop, so a `&mut self` receiver reborrow would pop the live
    // `RunIteratorHook` alias into `self.font_shaper` (Miri-proven UB, SB + TB).
    // Narrow field access only. (`too_many_arguments`: faithful to the reference
    // `addGlyph` arg list.)
    #[allow(clippy::too_many_arguments)]
    unsafe fn add_glyph(
        self_raw: *mut Renderer,
        x: CellCountInt,
        y: CellCountInt,
        cols: usize,
        // PORT NOTE (reshape, §D.5): a plain safe `&[page::Cell]` — the SoA
        // `raw` column projection (`cells_slice.raw`) in `rebuild_row`. It is a
        // shared borrow into `row_data`'s heap (a separate allocation from
        // `Renderer` and never mutated here), so it is NOT one of the §C.3
        // raw-pointer triad and coexists with the narrow `&mut self` field
        // access below. This mirrors the reference passing `cells.items(.raw)`.
        cell_raws: &[page::Cell],
        shaper_cell: shape::Cell,
        shaper_run: shape::TextRun,
        color: color::RGB,
        alpha: u8,
    ) -> Result<(), RenderError> {
        let cell = cell_raws[x as usize];
        let cp = cell.codepoint();

        // Render
        // SAFETY: see `add_underline` — narrow `(*self_raw).{font_grid,alloc,
        // grid_metrics,config}` reads; the grid handle owns its own `RwLock`.
        let render = unsafe {
            (*(*self_raw).font_grid).render_glyph(
                (*self_raw).alloc,
                shaper_run.font_index,
                shaper_cell.glyph_index,
                RenderOptions {
                    grid_metrics: (*self_raw).grid_metrics,
                    thicken: (*self_raw).config.font_thicken,
                    thicken_strength: (*self_raw).config.font_thicken_strength,
                    cell_width: Some(cell.grid_width()),
                    // If there's no Nerd Font constraint for this codepoint
                    // then, if it's a symbol, we constrain it to fit inside
                    // its cell(s), we don't modify the alignment at all.
                    constraint: get_constraint(cp).unwrap_or_else(|| {
                        if cell::is_symbol(cp) {
                            Constraint {
                                size: ConstraintSize::Fit,
                                ..Constraint::none
                            }
                        } else {
                            Constraint::none
                        }
                    }),
                    constraint_width: cell::constraint_width(cell_raws, x as usize, cols),
                },
            )
        }?;

        // If the glyph is 0 width or height, it will be invisible
        // when drawn, so don't bother adding it to the buffer.
        if render.glyph.width == 0 || render.glyph.height == 0 {
            return Ok(());
        }

        // SAFETY: narrow `&mut (*self_raw).cells`, disjoint from `self.font_shaper`.
        unsafe {
            (*self_raw).cells.add(
                Key::Text,
                CellText {
                    atlas: match render.presentation {
                        Presentation::Emoji => Atlas::Color,
                        Presentation::Text => Atlas::Grayscale,
                    },
                    bools: CellTextBools {
                        no_min_contrast: cell::no_min_contrast(cp),
                        ..Default::default()
                    },
                    grid_pos: [x, y],
                    color: [color.r, color.g, color.b, alpha],
                    glyph_pos: [render.glyph.atlas_x, render.glyph.atlas_y],
                    glyph_size: [render.glyph.width, render.glyph.height],
                    bearings: [
                        (render.glyph.offset_x + shaper_cell.x_offset as i32) as i16,
                        (render.glyph.offset_y + shaper_cell.y_offset as i32) as i16,
                    ],
                },
            );
        }
        Ok(())
    }

    // fn addCursor(self, cursor_state: *const RenderState.Cursor,
    //              cursor_style: renderer.CursorStyle, cursor_color: RGB) void
    // PORT NOTE (§C.3/§D.6): BOTH `self` and `cursor_state` are RAW, exactly as in
    // the reference `addCursor(self: *Self, cursor_state: *const RenderState.Cursor,
    // ...)` (ref :3226). The caller's `&st.cursor` aliases INLINE
    // `self.terminal_state.cursor`; taking `self` as `&mut self` here would retag
    // the whole `Renderer` on the receiver reborrow and POP that `cursor_state`
    // alias BEFORE this body dereferences it — Undefined Behavior under Stacked
    // Borrows (Miri-proven). Taken raw, `self` and `cursor_state` coexist as two raw
    // pointers, exactly as the reference. All `self` access below is NARROW
    // (`(*self_raw).field`), never a whole `&mut *self_raw`.
    unsafe fn add_cursor(
        self_raw: *mut Renderer,
        cursor_state: *const render::Cursor,
        cursor_style: cursor::Style,
        cursor_color: color::RGB,
    ) {
        // const cursor_vp = cursor_state.viewport orelse return;
        // (`Viewport` is not `Copy`; copy out its three scalars by value.)
        let (vp_x, vp_y, vp_wide_tail) = match unsafe { (*cursor_state).viewport.as_ref() } {
            Some(v) => (v.x, v.y, v.wide_tail),
            None => return,
        };
        let cell_wide = unsafe { (*cursor_state).cell.wide() };

        // Add the cursor. We render the cursor over the wide character if
        // we're on the wide character tail.
        let (wide, x): (bool, CellCountInt) = if !vp_wide_tail {
            // The cursor goes over the screen cursor position.
            (cell_wide == page::Wide::Wide, vp_x)
        } else {
            // If we're part of a wide character, we move the cursor back
            // to the actual character.
            (true, vp_x - 1)
        };

        // SAFETY: narrow reads of `(*self_raw).{focused,config}` (see §C.3).
        let alpha: u8 = if !unsafe { (*self_raw).focused } {
            255
        } else {
            (255.0 * unsafe { (*self_raw).config.cursor_opacity }).ceil() as u8
        };

        let render = match cursor_style {
            cursor::Style::Block
            | cursor::Style::BlockHollow
            | cursor::Style::Bar
            | cursor::Style::Underline => {
                let sprite: Sprite = match cursor_style {
                    cursor::Style::Block => Sprite::CursorRect,
                    cursor::Style::BlockHollow => Sprite::CursorHollowRect,
                    cursor::Style::Bar => Sprite::CursorBar,
                    cursor::Style::Underline => Sprite::CursorUnderline,
                    cursor::Style::Lock => unreachable!(),
                };

                // SAFETY: see `add_underline` — narrow `(*self_raw).{font_grid,
                // alloc,grid_metrics}` reads; the grid handle owns its own `RwLock`.
                match unsafe {
                    (*(*self_raw).font_grid).render_glyph(
                        (*self_raw).alloc,
                        SPRITE_INDEX,
                        sprite as u32,
                        RenderOptions {
                            cell_width: Some(if wide { 2 } else { 1 }),
                            grid_metrics: (*self_raw).grid_metrics,
                            ..RenderOptions::default()
                        },
                    )
                } {
                    Ok(r) => r,
                    Err(err) => {
                        // log.warn("error rendering cursor glyph err={}", .{err});
                        log::warn!(target: LOG_SCOPE, "error rendering cursor glyph err={err:?}");
                        return;
                    }
                }
            }

            cursor::Style::Lock => {
                // SAFETY: see `add_underline` — narrow `(*self_raw).{font_grid,
                // alloc,grid_metrics}` reads; the grid handle owns its own `RwLock`.
                match unsafe {
                    (*(*self_raw).font_grid).render_codepoint(
                        (*self_raw).alloc,
                        0xF023, // lock symbol
                        rustty_font_shaper::Style::Regular,
                        Some(Presentation::Text),
                        RenderOptions {
                            cell_width: Some(if wide { 2 } else { 1 }),
                            grid_metrics: (*self_raw).grid_metrics,
                            ..RenderOptions::default()
                        },
                    )
                } {
                    Ok(Some(r)) => r,
                    Ok(None) => {
                        // This should never happen because we embed nerd
                        // fonts so we just log and return instead of fallback.
                        // log.warn("failed to find lock symbol for cursor
                        //           codepoint=0xF023", .{});
                        log::warn!(
                            target: LOG_SCOPE,
                            "failed to find lock symbol for cursor codepoint=0xF023"
                        );
                        return;
                    }
                    Err(err) => {
                        // log.warn("error rendering cursor glyph err={}", .{err});
                        log::warn!(target: LOG_SCOPE, "error rendering cursor glyph err={err:?}");
                        return;
                    }
                }
            }
        };

        // SAFETY: narrow `&mut (*self_raw).cells`, disjoint from `self.font_shaper`.
        unsafe {
            (*self_raw).cells.set_cursor(
                Some(CellText {
                    atlas: Atlas::Grayscale,
                    bools: CellTextBools {
                        is_cursor_glyph: true,
                        ..Default::default()
                    },
                    grid_pos: [x, vp_y],
                    color: [cursor_color.r, cursor_color.g, cursor_color.b, alpha],
                    glyph_pos: [render.glyph.atlas_x, render.glyph.atlas_y],
                    glyph_size: [render.glyph.width, render.glyph.height],
                    bearings: [render.glyph.offset_x as i16, render.glyph.offset_y as i16],
                }),
                Some(cursor_style),
            );
        }
    }

    // fn addPreeditCell(self, cp: State.Preedit.Codepoint, coord: terminal.Coordinate,
    //                   screen_fg: RGB) !void
    fn add_preedit_cell(
        &mut self,
        cp: state::Codepoint,
        coord: point::Coordinate,
        screen_fg: color::RGB,
    ) -> Result<(), RenderError> {
        // Render the glyph for our preedit text
        // SAFETY: see `add_underline` — `self.font_grid` raw-ptr deref.
        let render = match unsafe { &mut *self.font_grid }.render_codepoint(
            self.alloc,
            cp.codepoint,
            rustty_font_shaper::Style::Regular,
            Some(Presentation::Text),
            // .{
            //     .grid_metrics = self.grid_metrics,
            //     .thicken = self.config.font_thicken,
            //     .thicken_strength = self.config.font_thicken_strength,
            // },
            RenderOptions {
                grid_metrics: self.grid_metrics,
                thicken: self.config.font_thicken,
                thicken_strength: self.config.font_thicken_strength,
                ..RenderOptions::default()
            },
        ) {
            Ok(Some(r)) => r,
            Ok(None) => {
                // log.warn("failed to find font for preedit codepoint={X}",
                //          .{cp.codepoint});
                log::warn!(
                    target: LOG_SCOPE,
                    "failed to find font for preedit codepoint={:X}",
                    cp.codepoint
                );
                return Ok(());
            }
            Err(err) => {
                // log.warn("error rendering preedit glyph err={}", .{err});
                log::warn!(target: LOG_SCOPE, "error rendering preedit glyph err={err:?}");
                return Ok(());
            }
        };

        // Add our text
        self.cells.add(
            Key::Text,
            CellText {
                atlas: Atlas::Grayscale,
                grid_pos: [coord.x, coord.y as u16],
                color: [screen_fg.r, screen_fg.g, screen_fg.b, 255],
                glyph_pos: [render.glyph.atlas_x, render.glyph.atlas_y],
                glyph_size: [render.glyph.width, render.glyph.height],
                bearings: [render.glyph.offset_x as i16, render.glyph.offset_y as i16],
                ..Default::default()
            },
        );

        // Add underline
        // SAFETY (§C.3): `add_underline` is a raw-self helper; `add_preedit_cell`
        // holds no live inline-self alias (no shaper hook), so a fresh `self as
        // *mut Renderer` with narrow field access inside is sound.
        let self_raw: *mut Renderer = self;
        unsafe {
            Renderer::add_underline(
                self_raw,
                coord.x,
                coord.y as u16,
                sgr::Underline::Single,
                screen_fg,
                255,
            )
        }?;
        if cp.wide && coord.x < self.cells.size.columns - 1 {
            unsafe {
                Renderer::add_underline(
                    self_raw,
                    coord.x + 1,
                    coord.y as u16,
                    sgr::Underline::Single,
                    screen_fg,
                    255,
                )
            }?;
        }
        Ok(())
    }

    // fn rebuildOverlay(self, features: []const Overlay.Feature) Overlay.InitError!void
    //
    /// Build the overlay as configured. Returns null if there is no
    /// overlay currently configured.
    //
    // PORTS: generic.zig:2230-… `rebuildOverlay`; macos_impl generic.rs:4539-4622.
    // PURE CPU/z2d — no graphics-API surface at all (the `Overlay` is a z2d software
    // surface; the GPU only ever sees it later via `images.overlay_update`), so this
    // is a VERBATIM copy-adapt of the Metal twin.
    pub fn rebuild_overlay(
        &mut self,
        features: &[crate::overlay::Feature],
    ) -> Result<(), crate::overlay::InitError> {
        Self::rebuild_overlay_into(
            &mut self.overlay,
            &self.size,
            &self.terminal_state,
            features,
        )
    }

    /// `rebuildOverlay` over an explicit overlay slot, so `update_frame` can
    /// build the next overlay into a local without the draw mutex (UD363).
    pub fn rebuild_overlay_into(
        slot: &mut Option<Overlay>,
        size: &Size,
        terminal_state: &RenderState,
        features: &[crate::overlay::Feature],
    ) -> Result<(), crate::overlay::InitError> {
        // const alloc = self.alloc;
        // (rustty `Overlay` drops the reference `alloc` arg — its z2d surface
        // owns its `Vec`; `Overlay::init`/`apply_features` take no `alloc`.)

        // If we have no features enabled, don't build an overlay.
        // If we had a previous overlay, deallocate it.
        if features.is_empty() {
            if let Some(old) = slot.as_mut() {
                old.deinit();
                *slot = None;
            }

            return Ok(());
        }

        // If we had a previous overlay, clear it. Otherwise, init.
        'overlay: {
            if let Some(v) = slot.as_mut() {
                'existing: {
                    // Verify that our overlay size matches our screen
                    // size as we know it now. If not, deinit and reinit.
                    // Note: these intCasts are always safe because z2d
                    // stores as i32 but we always init with a u32.
                    let width: u32 = v.surface.get_width() as u32;
                    let height: u32 = v.surface.get_height() as u32;
                    let term_size = size.terminal();
                    if width != term_size.width || height != term_size.height {
                        break 'existing;
                    }

                    // We also depend on cell size.
                    if v.cell_size.width != size.cell.width
                        || v.cell_size.height != size.cell.height
                    {
                        break 'existing;
                    }

                    // Everything matches, so we can just reset the surface
                    // and redraw.
                    v.reset();
                    break 'overlay;
                }
            }

            // If we reached this point we want to reset our overlay.
            if let Some(v) = slot.as_mut() {
                v.deinit();
                *slot = None;
            }

            assert!(slot.is_none());
            let new: Overlay = Overlay::init(*size)?;
            *slot = Some(new);
        }

        // overlay.applyFeatures(alloc, &self.terminal_state, features);
        slot.as_mut()
            .unwrap()
            .apply_features(terminal_state, features);
        Ok(())
    }
}
