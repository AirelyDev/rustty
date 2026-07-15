//! rustty-font-shaper::shape::web_canvas
//!
//! PORTS: ghostty `src/font/shaper/web_canvas.zig` (305 loc) — the browser/wasm
//! Canvas-backed font shaper (`.web_canvas` backend). LIVE port, cfg-gated to
//! `wasm32` (the active `Shaper` on the web_canvas backend).
//!
//! FAITHFUL MODERNIZATION (`faceFromIndex` → `Collection::get_face`): the
//! reference `shape()` looks the face up with
//! `run.group.group.faceFromIndex(run.font_index)` (line 188). `faceFromIndex`
//! and the `TextRun.group` field were DELETED from ghostty in 2024 (the
//! `Group` → `SharedGrid` refactor). The modern equivalent — already ported —
//! is `Collection::get_face(index)` reached through the run's `grid`
//! (`TextRun.grid: *SharedGrid`), exactly as the modern HarfBuzz/CoreText
//! shapers do (`shaper/harfbuzz.rs`: `(*run.grid).resolver.collection.get_face
//! (run.font_index)`). `Collection::get_face` performs the same deferred-load /
//! entry lookup the deleted `Group.faceFromIndex` did. We wire the web_canvas
//! shaper to that modern path; everything else (grapheme clustering, the private
//! grapheme-glyph-index reservation on the face) is a line-for-line port.
//!
//! The reference `Wasm.shaper_test` built a `terminal.Terminal`, printed the
//! string, and drove the OLD `runIterator(group, row, null, null)` 4-arg API
//! (also deleted; the modern `runIterator(self, opts)` takes a single
//! `RunOptions` over render-state cells). Rather than reconstruct the full render
//! pipeline in the wasm harness, the C-ABI below exposes `rustty_wc_shaper_shape`
//! — the modern analog that shapes an explicit `(codepoint, cluster)` run through
//! the SAME `shape()` the terminal path would reach. See the C-ABI notes.

#[cfg(target_arch = "wasm32")]
pub use wasm::Shaper;
// Re-export the C-ABI exports by path so the browser cdylib deliverable can
// force-retain them (`#[used]`), and so they participate in the public API.
#[cfg(target_arch = "wasm32")]
pub use wasm::{rustty_wc_shaper_free, rustty_wc_shaper_new, rustty_wc_shaper_shape};

#[cfg(target_arch = "wasm32")]
mod wasm {
    // const std = @import("std");
    // const assert = @import("../../quirks.zig").inlineAssert;
    // const Allocator = std.mem.Allocator;
    // const font = @import("../main.zig");
    // const terminal = @import("../../terminal/main.zig");
    // const unicode = @import("../../unicode/main.zig");
    // const uucode = @import("uucode");
    // const log = std.log.scoped(.font_shaper);
    use rustty_unicode::grapheme::{grapheme_break, BreakState};
    use rustty_util::quirks::inlineAssert as assert;

    use crate::collection;
    use crate::shape;
    use crate::shape::run::{RunIterator, RunIteratorHook};

    /// The error set of the web_canvas shaper.
    // PORT NOTE: the reference `shape()` returns an inferred error union of
    // `Allocator.Error`, the utf8-encode errors, and the face's
    // `graphemeGlyphIndex`/`getFace` errors. Modeled as one enum with `From`
    // conversions so `?` composes them.
    #[derive(Debug)]
    pub enum Error {
        OutOfMemory,
        CodepointTooLarge,
        Collection(collection::Error),
        Face(crate::face::FaceError),
    }
    impl From<collection::Error> for Error {
        fn from(e: collection::Error) -> Self {
            Error::Collection(e)
        }
    }
    impl From<crate::face::FaceError> for Error {
        fn from(e: crate::face::FaceError) -> Self {
            Error::Face(e)
        }
    }

    // pub const Shaper = struct {
    //     const RunBuf = std.MultiArrayList(struct { codepoint: u32, cluster: u32 });
    //     alloc: Allocator,
    //     cell_buf: []font.shape.Cell,
    //     run_buf: RunBuf,
    // };
    pub struct Shaper {
        /// The shared memory used for shaping results.
        // PORT NOTE: the reference `cell_buf` is a fixed `[]Cell` slice passed in
        // via `Options.cell_buf`. The modern shaper framework (`harfbuzz.rs`) uses
        // a growable `Vec<Cell>` instead; we match that so `shape()` can never
        // overflow a caller buffer.
        cell_buf: Vec<shape::Cell>,

        /// The shared memory used for storing information about a run. Each entry
        /// is `(codepoint, cluster)` — the reference `RunBuf` MultiArrayList.
        run_buf: Vec<(u32, u32)>,
    }

    impl Shaper {
        /// The cell_buf argument is the buffer to use for storing shaped results.
        /// This should be at least the number of columns in the terminal.
        // pub fn init(alloc: Allocator, opts: font.shape.Options) !Shaper {
        pub fn init(_opts: shape::Options) -> Result<Shaper, Error> {
            // Note: we do not support opts.font_features
            Ok(Shaper {
                cell_buf: Vec::new(),
                run_buf: Vec::new(),
            })
        }

        // pub fn deinit(self: *Shaper) void {
        pub fn deinit(&mut self) {
            self.run_buf.clear();
            self.cell_buf.clear();
        }

        // pub fn endFrame(self: *const Shaper) void { _ = self; }
        pub fn end_frame(&self) {}

        /// Returns an iterator that returns one text run at a time for the given
        /// terminal row. Note that text runs are only valid one at a time for a
        /// Shaper struct since they share state.
        // pub fn runIterator(self: *Shaper, opts: font.shape.RunOptions) font.shape.RunIterator {
        pub fn run_iterator<'a>(
            &mut self,
            opts: shape::RunOptions<'a>,
        ) -> RunIterator<'a, Hook> {
            RunIterator {
                hooks: Hook {
                    shaper: self as *mut Shaper,
                },
                opts,
                i: 0,
            }
        }

        /// Shape the given text run. The text run must be the immediately previous
        /// text run that was iterated since the text run does share state with the
        /// Shaper struct.
        ///
        /// The return value is only valid until the next shape call is called.
        // pub fn shape(self: *Shaper, run: font.shape.TextRun) ![]font.shape.Cell {
        pub fn shape(&mut self, run: shape::TextRun) -> Result<&[shape::Cell], Error> {
            // TODO: memory check that cell_buf can fit results

            // const codepoints = self.run_buf.items(.codepoint);
            // const clusters = self.run_buf.items(.cluster);
            // assert(codepoints.len == clusters.len);
            self.cell_buf.clear();
            let n = self.run_buf.len();

            match n {
                // Special cases: if we have no codepoints (is this possible?) then
                // our result is also an empty cell run.
                // 0 => return self.cell_buf[0..0],
                0 => return Ok(&self.cell_buf[0..0]),

                // If we have only 1 codepoint, then we assume that it is a single
                // grapheme and just let it through. At this point, we can't have
                // any more information to do anything else.
                1 => {
                    let (cp, cluster) = self.run_buf[0];
                    self.cell_buf.push(shape::Cell {
                        x: cluster as u16,
                        x_offset: 0,
                        y_offset: 0,
                        glyph_index: cp,
                    });
                    return Ok(&self.cell_buf);
                }

                _ => {}
            }

            // We know we have at least two codepoints, so we now go through each
            // and perform grapheme clustering.
            //
            // Note that due to limitations of canvas, we can NOT support font
            // ligatures. However, we do support grapheme clustering. This means we
            // can render things like skin tone emoji but we can't render things
            // like single glyph "=>".
            // var break_state: uucode.grapheme.BreakState = .default;
            // var cp1: u21 = @intCast(codepoints[0]);
            let mut break_state: BreakState = BreakState::Default;
            let mut cp1: u32 = self.run_buf[0].0;

            let mut start: usize = 0;
            let mut i: usize = 1;
            // We loop to codepoints.len so that we can handle the end case. In the
            // end case, we always assume it is a grapheme break. This isn't
            // strictly true but its how terminals work today.
            while i <= n {
                let grapheme_break_here = if i == n {
                    true
                } else {
                    // const cp2: u21 = @intCast(codepoints[i]); defer cp1 = cp2;
                    let cp2: u32 = self.run_buf[i].0;
                    let b = grapheme_break(cp1, cp2, &mut break_state);
                    cp1 = cp2;
                    b
                };

                // If this is NOT a grapheme break, cp2 is part of a single
                // grapheme cluster and we expect there could be more. We move on
                // to the next codepoint to try again.
                if !grapheme_break_here {
                    i += 1;
                    continue;
                }

                // This IS a grapheme break, meaning that cp2 is NOT part of cp1.
                // So we need to render the prior grapheme.
                // const len = i - start; assert(len > 0);
                let len = i - start;
                assert(len > 0);
                if len == 1 {
                    // If we have only a single codepoint then just render it as-is.
                    let (cp, cluster) = self.run_buf[start];
                    self.cell_buf.push(shape::Cell {
                        x: cluster as u16,
                        x_offset: 0,
                        y_offset: 0,
                        glyph_index: cp,
                    });
                } else {
                    // We must have multiple codepoints (see assert above). In this
                    // case we UTF-8 encode the codepoints and send them to the face
                    // to reserve a private glyph index.
                    //
                    // const cluster = cluster: { … utf8 encode codepoints[start..i] … };
                    let mut cluster: Vec<u8> = Vec::new();
                    for k in start..i {
                        let cp = self.run_buf[k].0;
                        let ch = char::from_u32(cp).ok_or(Error::CodepointTooLarge)?;
                        let mut buf = [0u8; 4];
                        cluster.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
                    }

                    // FAITHFUL MODERNIZATION (see file header): the reference is
                    //   var face = try run.group.group.faceFromIndex(run.font_index);
                    //   const index = try face.graphemeGlyphIndex(cluster);
                    // `faceFromIndex`/`run.group` were deleted upstream; the modern
                    // path reaches the same face via the run's shared grid.
                    let face = unsafe {
                        (*run.grid).resolver.collection.get_face(run.font_index)?
                    };
                    let index = unsafe { (*face).grapheme_glyph_index(&cluster)? };

                    let cluster_x = self.run_buf[start].1;
                    self.cell_buf.push(shape::Cell {
                        x: cluster_x as u16,
                        x_offset: 0,
                        y_offset: 0,
                        glyph_index: index,
                    });
                }

                start = i;
                i += 1;
            }

            // return self.cell_buf[0..cur];
            Ok(&self.cell_buf)
        }

        // ---- helpers for the C-ABI (`prepare`/`addCodepoint` inlined) ----

        /// Reset the run buffer (the reference `RunIteratorHook.prepare`).
        pub(crate) fn reset_run(&mut self) {
            self.run_buf.clear();
        }

        /// Append a `(codepoint, cluster)` to the run (the reference
        /// `RunIteratorHook.addCodepoint`).
        pub(crate) fn push_codepoint(&mut self, cp: u32, cluster: u32) {
            self.run_buf.push((cp, cluster));
        }
    }

    /// The hooks for RunIterator.
    // pub const RunIteratorHook = struct { shaper: *Shaper, ... };
    pub struct Hook {
        shaper: *mut Shaper,
    }

    impl RunIteratorHook for Hook {
        // pub fn prepare(self: RunIteratorHook) !void {
        fn prepare(&mut self) {
            // Reset the buffer for our current run
            unsafe { (*self.shaper).run_buf.clear() };
        }

        // pub fn addCodepoint(self: RunIteratorHook, cp: u32, cluster: u32) !void {
        fn add_codepoint(&mut self, cp: u32, cluster: u32) {
            unsafe { (*self.shaper).run_buf.push((cp, cluster)) };
        }

        // pub fn finalize(self: RunIteratorHook) !void { _ = self; }
        fn finalize(&mut self) {}
    }

    // -----------------------------------------------------------------------
    // The wasm-compatible C-ABI (`pub const Wasm = struct { … }`). Exported as
    // `rustty_wc_shaper_*` `#[no_mangle] extern "C"` so a JS host can create a
    // shaper and shape a run.
    // -----------------------------------------------------------------------

    /// `export fn shaper_new(cap: usize) ?*Shaper`
    #[no_mangle]
    pub extern "C" fn rustty_wc_shaper_new(cap: usize) -> *mut Shaper {
        match Shaper::init(shape::Options::default()) {
            Ok(mut shaper) => {
                // The reference allocates a `cell_buf` of `cap` cells; our cell_buf
                // is growable, so `cap` is an advisory reservation.
                shaper.cell_buf.reserve(cap);
                shaper.run_buf.reserve(cap);
                Box::into_raw(Box::new(shaper))
            }
            Err(_) => core::ptr::null_mut(),
        }
    }

    /// `export fn shaper_free(ptr: ?*Shaper) void`
    #[no_mangle]
    pub extern "C" fn rustty_wc_shaper_free(ptr: *mut Shaper) {
        if !ptr.is_null() {
            let mut shaper = unsafe { Box::from_raw(ptr) };
            shaper.deinit();
        }
    }

    /// Modern analog of the reference `shaper_test`: shape an explicit run of
    /// `(codepoint, cluster)` pairs and write the resulting cells into
    /// `out_cells`. Returns the number of cells produced, or -1 on error.
    ///
    /// PORT NOTE: the reference `shaper_test(self, group, str, len)` built a
    /// `terminal.Terminal`, printed the UTF-8 string into it, then drove the
    /// (deleted) `runIterator(group, row, null, null)` and logged the shaped runs.
    /// The modern render pipeline needed to reconstruct render-state cells is out
    /// of scope for the wasm harness; this primitive instead feeds the codepoints
    /// straight into the shaper's run buffer and calls the SAME `shape()` the
    /// terminal path reaches (single-codepoint runs need no font grid; multi-
    /// codepoint grapheme clusters use `grid.collection.get_face`). Pass a valid
    /// `*SharedGrid` when the run contains grapheme clusters; `grid` may be null
    /// for simple (single-codepoint-per-cell) strings.
    ///
    /// # Safety
    /// `cps`/`clusters` point to `n` `u32`s; `out_cells` has room for `out_cap`
    /// `Cell`s; `grid` is null or a valid `*SharedGrid`.
    #[no_mangle]
    pub extern "C" fn rustty_wc_shaper_shape(
        shaper: *mut Shaper,
        grid: *mut crate::shared_grid::SharedGrid,
        cps: *const u32,
        clusters: *const u32,
        n: usize,
        out_cells: *mut shape::Cell,
        out_cap: usize,
    ) -> isize {
        if shaper.is_null() || (n > 0 && (cps.is_null() || clusters.is_null())) {
            return -1;
        }
        let shaper = unsafe { &mut *shaper };

        // prepare + addCodepoint for each pair.
        shaper.reset_run();
        for k in 0..n {
            let cp = unsafe { *cps.add(k) };
            let cluster = unsafe { *clusters.add(k) };
            shaper.push_codepoint(cp, cluster);
        }

        // Build a TextRun and shape. font_index = regular[0]; unused for the
        // single-codepoint path (only grapheme-cluster cells dereference the grid).
        let run = shape::TextRun {
            hash: 0,
            offset: 0,
            cells: n as u16,
            grid,
            font_index: collection::Index::new(crate::Style::Regular, 0),
        };

        let cells = match shaper.shape(run) {
            Ok(c) => c,
            Err(_) => return -1,
        };

        let count = cells.len().min(out_cap);
        for (k, cell) in cells.iter().take(count).enumerate() {
            unsafe { *out_cells.add(k) = *cell };
        }
        count as isize
    }
}
