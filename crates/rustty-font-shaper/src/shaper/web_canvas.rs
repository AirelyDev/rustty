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
// The shaper's error set, named by `shape::ShaperError` (S-14).
#[cfg(target_arch = "wasm32")]
pub use wasm::Error;
// Re-export the C-ABI exports by path so the browser cdylib deliverable can
// force-retain them (`#[used]`), and so they participate in the public API.
#[cfg(target_arch = "wasm32")]
pub use wasm::{rustty_wc_shaper_free, rustty_wc_shaper_new, rustty_wc_shaper_shape};
// The Ghostty-ABI wasm exports (`pub const Wasm`, `shaper/web_canvas.zig:231-305`)
// — the UNPREFIXED names retained by `font/main.zig`'s
// `comptime { _ = shape.web_canvas.Wasm; }`. Feature-gated (`ghostty-wasm-abi`):
// only the `rustty_wasm.wasm` (`main_wasm.zig`) artifact carries them.
#[cfg(all(target_arch = "wasm32", feature = "ghostty-wasm-abi"))]
pub use wasm::{shaper_free, shaper_new, shaper_test};

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
    //
    // The reference's `std.log.scoped(.font_shaper)` tag, carried through the
    // `log` facade's `target:`
    // (`.port/parity-closure/DECISION-logging.md` §6).
    //
    // `#[allow(dead_code)]`: this file's two live sites are both inside the
    // `ghostty-wasm-abi`-gated submodule (the unprefixed `shaper_test` export),
    // so without that feature the const has no reader. Keeping it unconditional
    // means the two feature trees read identically.
    #[allow(dead_code)]
    const LOG_SCOPE: &str = "font_shaper";

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
    //     const RunBuf = std.MultiArrayList(struct {
    //         /// The codepoint for this cell. This must be used in conjunction
    //         /// with cluster to find the total set of codepoints for a given
    //         /// cell. See cluster for more information.
    //         codepoint: u32,
    //
    //         /// Cluster is set to the X value of the cell that this codepoint
    //         /// is part of. Note that a cell can have multiple codepoints
    //         /// with zero-width joiners (ZWJ) and such. Note that terminals
    //         /// do NOT handle full extended grapheme clustering well so it
    //         /// is possible a single grapheme extends multiple clusters.
    //         /// For example, skin tone emoji thumbs up may show up as two
    //         /// clusters: one with thumbs up and the ZWJ, and a second
    //         /// cluster with the tone block. It is up to the shaper to handle
    //         /// shaping these together into a single glyph, if it wishes.
    //         cluster: u32,
    //     });
    //
    //     /// The allocator used for run_buf.
    //     alloc: Allocator,
    //
    //     cell_buf: []font.shape.Cell,
    //     run_buf: RunBuf,
    // };
    // PORT NOTE: the `alloc` field has no Rust counterpart (`run_buf` is a
    // `Vec` that owns its allocator); the RunBuf field docs above describe the
    // `(codepoint, cluster)` tuple entries of `run_buf` below.
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

        /// Returns an iterator that returns one text run at a time for the
        /// given terminal row. Note that text runs are only valid one at a time
        /// for a Shaper struct since they share state.
        // pub fn runIterator(self: *Shaper, opts: font.shape.RunOptions) font.shape.RunIterator {
        pub fn run_iterator<'a>(&mut self, opts: shape::RunOptions<'a>) -> RunIterator<'a, Hook> {
            RunIterator {
                hooks: Hook {
                    shaper: self as *mut Shaper,
                },
                opts,
                i: 0,
            }
        }

        /// Shape the given text run. The text run must be the immediately
        /// previous text run that was iterated since the text run does share
        /// state with the Shaper struct.
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

            // We know we have at least two codepoints, so we now go through
            // each and perform grapheme clustering.
            //
            // Note that due to limitations of canvas, we can NOT support
            // font ligatures. However, we do support grapheme clustering.
            // This means we can render things like skin tone emoji but
            // we can't render things like single glyph "=>".
            // var break_state: uucode.grapheme.BreakState = .default;
            // var cp1: u21 = @intCast(codepoints[0]);
            let mut break_state: BreakState = BreakState::Default;
            let mut cp1: u32 = self.run_buf[0].0;

            let mut start: usize = 0;
            let mut i: usize = 1;
            // We loop to codepoints.len so that we can handle the end
            // case. In the end case, we always assume it is a grapheme
            // break. This isn't strictly true but its how terminals
            // work today.
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
                    // UTF-8 encode the codepoints in this cluster.
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
                    let face = unsafe { (*run.grid).resolver.collection.get_face(run.font_index)? };
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
        crate::install_wasm_log_backend();
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
        crate::install_wasm_log_backend();
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
        crate::install_wasm_log_backend();
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

    // -----------------------------------------------------------------------
    // The Ghostty-ABI wasm-compatible C-ABI (`pub const Wasm = struct { … }`,
    // `shaper/web_canvas.zig:231-305`) — the UNPREFIXED export names. Feature-
    // gated (`ghostty-wasm-abi`); see the re-export at the file head.
    // -----------------------------------------------------------------------

    #[cfg(feature = "ghostty-wasm-abi")]
    pub use ghostty_wasm_abi::{shaper_free, shaper_new, shaper_test};

    /// The wasm-compatible API.
    // pub const Wasm = struct {
    //     const wasm = @import("../../os/wasm.zig");
    //     const alloc = wasm.alloc;
    #[cfg(feature = "ghostty-wasm-abi")]
    mod ghostty_wasm_abi {
        use super::{collection, shape, Shaper};
        use crate::shared_grid::SharedGrid;
        use rustty_vt::alloc::Global;
        use rustty_vt::page::ContentTag;
        use rustty_vt::pagelist::Node;
        use rustty_vt::point::{Coordinate, Point};
        use rustty_vt::size::CellCountInt;
        use rustty_vt::terminal::{Colors, Options as TerminalOptions, Terminal};

        /// The error `shaper_test_` hands to `shaper_test`'s `catch |err|`.
        ///
        /// PORT NOTE (task-14c-4): the reference `shaper_test_` returns an
        /// inferred error union and the caught `err` is rendered with `{}`, i.e.
        /// the ERROR NAME. The port had collapsed every failure to `()`, which
        /// was invisible while the log was a comment but would have made the
        /// restored message read `err=()`. These variants carry the reference's
        /// own error name at each site so the diagnostic says what the reference
        /// would have said.
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub(super) enum ShaperTestError {
            /// `try terminal.Terminal.init(...)` / `try term.print(c)`.
            OutOfMemory,
            /// `try std.unicode.Utf8View.init(str)`.
            InvalidUtf8,
            /// `try self.shape(run)`.
            ShapeFailed,
            /// PORT-ONLY: the modern growable `cell_buf` can have zero reserved
            /// capacity, where the reference's `cell_buf` was a fixed `cap`
            /// allocation and `Terminal.init` would have been given 0 columns.
            /// The reference has no analogue; named so the message is not silent
            /// about which arm fired.
            ZeroCellBuf,
        }

        impl core::fmt::Display for ShaperTestError {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                // Zig renders a caught error as `error.Name` under `{}`.
                let name = match self {
                    Self::OutOfMemory => "OutOfMemory",
                    Self::InvalidUtf8 => "InvalidUtf8",
                    Self::ShapeFailed => "ShapeFailed",
                    Self::ZeroCellBuf => "ZeroCellBuf",
                };
                write!(f, "error.{name}")
            }
        }

        /// `export fn shaper_new(cap: usize) ?*Shaper`
        #[no_mangle]
        pub extern "C" fn shaper_new(cap: usize) -> *mut Shaper {
            // return shaper_new_(cap) catch null;
            //
            // fn shaper_new_(cap: usize) !*Shaper {
            //     const cell_buf = try alloc.alloc(font.shape.Cell, cap);
            //     var shaper = try Shaper.init(alloc, .{ .cell_buf = cell_buf });
            //     const result = try alloc.create(Shaper); result.* = shaper; return result;
            // }
            //
            // PORT NOTE: identical body to `rustty_wc_shaper_new` — the modern
            // `cell_buf` is growable, so `cap` is an advisory reservation.
            match Shaper::init(shape::Options::default()) {
                Ok(mut shaper) => {
                    shaper.cell_buf.reserve(cap);
                    shaper.run_buf.reserve(cap);
                    Box::into_raw(Box::new(shaper))
                }
                Err(_) => core::ptr::null_mut(),
            }
        }

        /// `export fn shaper_free(ptr: ?*Shaper) void`
        #[no_mangle]
        pub extern "C" fn shaper_free(ptr: *mut Shaper) {
            // if (ptr) |v| { alloc.free(v.cell_buf); v.deinit(); alloc.destroy(v); }
            if !ptr.is_null() {
                let mut shaper = unsafe { Box::from_raw(ptr) };
                shaper.deinit();
            }
        }

        /// Runs a test to verify shaping works properly.
        // export fn shaper_test(self: *Shaper, group: *font.GroupCache,
        //     str: [*]const u8, len: usize) void
        //
        // PORT NOTE (FAITHFUL MODERNIZATION, see the file header): the second
        // parameter is the font state — `*font.GroupCache` upstream, a type the
        // 2024 `Group` → `SharedGrid` refactor DELETED; the modern analog is
        // `*SharedGrid`, exactly as `rustty_wc_shaper_shape` takes. Likewise the
        // reference row loop drives the deleted 4-arg
        // `runIterator(group, row, null, null)`; the port reaches the SAME
        // `shape()` through the run-buffer primitive (`prepare`/`addCodepoint`
        // inlined), feeding each viewport cell's codepoints with its column as
        // the cluster — what the old iterator's hooks produced. `grid` may be
        // null for strings without multi-codepoint graphemes (only the
        // grapheme-cluster arm of `shape()` dereferences it).
        #[no_mangle]
        pub extern "C" fn shaper_test(
            self_: *mut Shaper,
            grid: *mut SharedGrid,
            str: *const u8,
            len: usize,
        ) {
            if self_.is_null() || (len > 0 && str.is_null()) {
                return;
            }
            let shaper = unsafe { &mut *self_ };
            let bytes = unsafe { core::slice::from_raw_parts(str, len) };
            // shaper_test_(self, group, str[0..len]) catch |err| {
            //     log.warn("error during shaper test err={}", .{err});
            // };
            if let Err(err) = shaper_test_(shaper, grid, bytes) {
                log::warn!(
                    target: super::LOG_SCOPE,
                    "error during shaper test err={err}"
                );
            }
        }

        // fn shaper_test_(self: *Shaper, group: *font.GroupCache, str: []const u8) !void {
        fn shaper_test_(
            shaper: &mut Shaper,
            grid: *mut SharedGrid,
            s: &[u8],
        ) -> Result<(), ShaperTestError> {
            // Create a terminal and print all our characters into it.
            // var term = try terminal.Terminal.init(alloc, self.cell_buf.len, 80);
            // defer term.deinit(alloc);
            //
            // PORT NOTE: `self.cell_buf.len` is the fixed capacity `shaper_new`
            // allocated; the modern growable `cell_buf`'s analog is its reserved
            // capacity. The reference's 2-arg `Terminal.init(alloc, cols, rows)`
            // was replaced upstream by `Options`; the remaining fields replicate
            // the reference `Terminal.Options` defaults, exactly as
            // `rustty-vt-c/src/terminal.rs` does.
            let cols = shaper
                .cell_buf
                .capacity()
                .min(usize::from(CellCountInt::MAX));
            if cols == 0 {
                return Err(ShaperTestError::ZeroCellBuf);
            }
            const ROWS: u16 = 80;
            let mut term = Terminal::init(
                Global,
                TerminalOptions {
                    cols: cols as CellCountInt,
                    rows: ROWS as CellCountInt,
                    max_scrollback_bytes: Some(10_000),
                    max_scrollback_lines: None,
                    colors: Colors::default(),
                    default_modes: rustty_vt::modes::ModePacked::default(),
                    default_cursor_style: rustty_vt::screen::CursorStyle::Block,
                    default_cursor_blink: Some(false),
                    kitty_image_storage_limit: 320 * 1000 * 1000,
                    kitty_image_loading_limits: rustty_vt::kitty::graphics_image::Limits::DIRECT,
                },
            )
            .map_err(|_| ShaperTestError::OutOfMemory)?;

            let result = shaper_test_rows(shaper, grid, &mut term, s);
            // defer term.deinit(alloc);
            term.deinit(Global);
            result
        }

        /// The body of `shaper_test_` after terminal creation, split out so the
        /// terminal's `deinit` runs on every path (the reference `defer`).
        fn shaper_test_rows(
            shaper: &mut Shaper,
            grid: *mut SharedGrid,
            term: &mut Terminal,
            s: &[u8],
        ) -> Result<(), ShaperTestError> {
            // Iterate over unicode codepoints and add to terminal
            // const view = try std.unicode.Utf8View.init(str);
            // var iter = view.iterator();
            // while (iter.nextCodepoint()) |c| { try term.print(c); }
            let text = core::str::from_utf8(s).map_err(|_| ShaperTestError::InvalidUtf8)?;
            for c in text.chars() {
                term.print(c as u32)
                    .map_err(|_| ShaperTestError::OutOfMemory)?;
            }

            // Iterate over the rows and print out all the runs we get.
            // var rowIter = term.screen.rowIterator(.viewport);
            // var y: usize = 0;
            // while (rowIter.next()) |row| { … }
            let pages = unsafe { &(*term.screens.active).pages };
            let (cols, rows) = (term.cols, term.rows);
            for y in 0..rows {
                // var iter = self.runIterator(group, row, null, null);
                // while (try iter.next(alloc)) |run| { … }
                //
                // PORT NOTE (modernization, see `shaper_test`): the deleted
                // 4-arg run iterator's hooks fed each cell's codepoints with the
                // cell column as cluster; reproduced via the run-buffer
                // primitive, one run per row.
                shaper.reset_run();
                for x in 0..cols {
                    let Some(cell) =
                        pages.get_cell(Point::Viewport(Coordinate { x, y: u32::from(y) }))
                    else {
                        continue;
                    };
                    let pcell = unsafe { *cell.cell };
                    let cp = pcell.codepoint();
                    if cp == 0 {
                        continue;
                    }
                    shaper.push_codepoint(cp, u32::from(x));
                    if pcell.content_tag() == ContentTag::CodepointGrapheme {
                        let page = unsafe { &*Node::page(cell.node) };
                        if let Some(extra) = page.lookup_grapheme(cell.cell) {
                            for &cp2 in extra {
                                shaper.push_codepoint(cp2, u32::from(x));
                            }
                        }
                    }
                }
                if shaper.run_buf.is_empty() {
                    continue;
                }

                // const cells = try self.shape(run);
                let n = shaper.run_buf.len();
                let run = shape::TextRun {
                    hash: 0,
                    offset: 0,
                    cells: n as u16,
                    grid,
                    font_index: collection::Index::new(crate::Style::Regular, 0),
                };
                let font_index = run.font_index;
                let run_cells = run.cells;
                let cells = shaper
                    .shape(run)
                    .map_err(|_| ShaperTestError::ShapeFailed)?;
                // log.info("y={} run={d} shape={any} idx={}", .{ y, run.cells,
                //     cells, run.font_index });
                //
                // PORT NOTE: Zig's `{any}` on a slice of structs is its default
                // aggregate rendering; `{:?}` is the Rust analogue. `run.cells`
                // and `run.font_index` are read BEFORE `shape` because `run` is
                // moved into it (the reference's `run` is a value the call does
                // not consume).
                log::info!(
                    target: super::LOG_SCOPE,
                    "y={} run={} shape={:?} idx={:?}",
                    y,
                    run_cells,
                    cells,
                    font_index
                );
            }
            Ok(())
        }
    }
}
