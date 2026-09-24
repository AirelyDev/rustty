//! rustty-font-shaper::shape
//!
//! PORTS: ghostty `src/font/shape.zig` (102 loc, 1 test) — the `Shaper`
//! interface + backend selector, plus the shared `Cell` / `Options` /
//! `RunOptions` types and the re-exports of the `shaper/` sub-files.

// const std = @import("std");
// const options = @import("main.zig").options;
// const run = @import("shaper/run.zig");
// const feature = @import("shaper/feature.zig");
// const configpkg = @import("../config.zig");
// const terminal = @import("../terminal/main.zig");
// const SharedGrid = @import("main.zig").SharedGrid;
use crate::shared_grid::SharedGrid;

// The `shaper/` sub-files. In rustty they live under `src/shaper/` (matching the
// reference directory) and are attached here as submodules of `shape`.
#[path = "shaper/cache.rs"]
pub mod cache;
#[path = "shaper/feature.rs"]
pub mod feature;
#[path = "shaper/run.rs"]
pub mod run;
// pub const noop = @import("shaper/noop.zig");
#[path = "shaper/noop.rs"]
pub mod noop;
// pub const harfbuzz = @import("shaper/harfbuzz.zig");
//
// PORT NOTE (Zig lazy analysis vs Rust eager compilation — THE impedance
// mismatch in this file). The reference `@import`s `shaper/harfbuzz.zig`
// UNCONDITIONALLY, right here, on every target. It can, because Zig only
// semantically analyses a declaration that is actually referenced: in a
// `.coretext` build the `Shaper` switch below picks `coretext.Shaper`, nothing
// ever names `harfbuzz.Shaper`, and `shaper/harfbuzz.zig` is never type-checked.
// Its `const Face = font.Face` would not even compile there — `font.Face` is
// `coretext.Face`, whose `hb_font` is `void` (`harfbuzz_shaper` is false), and
// `harfbuzz.shape(face.hb_font, ...)` wants a `harfbuzz.Font`. The reference is
// not "supporting" both; it is simply not looking.
//
// Rust compiles every `mod` in the tree eagerly, so the unconditional import has
// no faithful spelling. The two candidate readings are:
//
//   (a) keep the module compiled everywhere and make it type-check — which means
//       giving the CoreText face an `hb_font` it does not have in the ported
//       configuration, or making this file generic over a face it is not generic
//       over in the reference. Both invent API, and (a) is exactly the departure
//       that put a hard-coded `harfbuzz_shaper = true` in `face/coretext.rs`.
//   (b) compile the module on the targets where the reference would actually
//       analyse it — the targets whose default backend has
//       `hasHarfbuzz() == true`.
//
// (b) is what "faithful" means here: `cfg` is rustty's stand-in for Zig's
// laziness, and it reproduces the reference's OBSERVABLE content — in a
// `.coretext` build the HarfBuzz shaper is unreachable code in ghostty too (the
// `Shaper` selector below resolves to `coretext.Shaper`, and
// `build/SharedDeps.zig:267` does not even link HarfBuzz for a `.coretext`
// build). Nothing is lost: the file, every comment in it, and all 25 of its
// ported `test {}` blocks stay in the tree and compile and run in every build
// whose backend selects them — `hasHarfbuzz()`, the `rustty_font_has_harfbuzz`
// cfg: the Linux/FreeBSD/Windows defaults, and on macOS `-Dfont-backend=
// coretext_freetype` / `coretext_harfbuzz` — which is the same set the
// reference type-checks them on (`shape.zig`'s `test { _ = Shaper; }`).
// CORRECTION (S-14, N266): this gate used to be `not(target_os = "macos")`,
// which was only right while the backend option was folded to its default.
//
// WASM DIVERGENCE: the HarfBuzz shaper is not compiled on the `wasm32` web_canvas
// backend (no HarfBuzz on freestanding wasm); the `web_canvas` shaper is active.
// EMBEDDER DIVERGENCE (feature `harfbuzz`, DEFAULT-ON): the feature is retained
// in the gate because the module's FFI floor is `crate::hb`, which the feature
// owns; on the non-macOS targets the crate's `lib.rs` `compile_error!` already
// requires the feature to be on, so the gate never silently drops the shaper
// where it is the active one.
#[cfg(all(rustty_font_has_harfbuzz, feature = "harfbuzz"))]
#[path = "shaper/harfbuzz.rs"]
pub mod harfbuzz;
// pub const coretext = @import("shaper/coretext.zig");
//
// PORT NOTE: the same laziness rule, for the `.coretext` arm alone: "Note that
// coretext_freetype cannot use the coretext shaper because the coretext shaper
// requests CoreText font faces" (the selector below) — so the reference only
// analyses this file, and runs its tests, in a `.coretext` build.
#[cfg(rustty_font_backend = "coretext")]
#[path = "shaper/coretext.rs"]
pub mod coretext;
// pub const web_canvas = @import("shaper/web_canvas.zig");
//
// PORT NOTE: the CoreText shaper is CoreText-native (`.coretext` backend) and so
// is gated to macOS. The wasm `web_canvas` shaper is NOT part of this tier; it is
// BLOCKED (B1) and recorded verbatim as a documented-deferral module
// (`shaper/web_canvas.rs`) — a faithful live port cannot type-check against the
// current font tree (deleted `faceFromIndex`/`TextRun.group` upstream API +
// un-vendored `zig-js` face). See that file's header for the full B1 analysis.
#[path = "shaper/web_canvas.rs"]
pub mod web_canvas;
// The `Shaper` selector (below) models the `fontconfig_freetype` /
// `freetype_windows` configurations and points at `harfbuzz::Shaper` there; on
// macOS it points at `coretext::Shaper`, the Darwin default backend's shaper.

// pub const Cache = @import("shaper/Cache.zig");
pub use cache::Cache;
// pub const TextRun = run.TextRun;
pub use run::TextRun;
// pub const RunIterator = run.RunIterator;
pub use run::RunIterator;
// pub const Feature = feature.Feature;
pub use feature::Feature;
// pub const FeatureList = feature.FeatureList;
pub use feature::FeatureList;
// pub const default_features = feature.default_features;
pub use feature::DEFAULT_FEATURES;

/// Shaper implementation for our compile options.
// pub const Shaper = switch (options.backend) {
//     .freetype,
//     .freetype_windows,
//     .fontconfig_freetype,
//     .coretext_freetype,
//     .coretext_harfbuzz,
//     => harfbuzz.Shaper,
//
//     // Note that coretext_freetype cannot use the coretext
//     // shaper because the coretext shaper requests CoreText
//     // font faces.
//     .coretext => coretext.Shaper,
//
//     .coretext_noshape => noop.Shaper,
//
//     .web_canvas => web_canvas.Shaper,
// };
//
// PORT NOTE (selector): the reference selects the active shaper from
// `options.backend` — here the `rustty_font_backend` cfg (`-Dfont-backend`,
// default `Backend.default`: `.coretext` on Darwin, `.fontconfig_freetype`
// elsewhere). The HarfBuzz arms are exactly `hasHarfbuzz()`. The
// `coretext_noshape` (`noop`) arm is always built and tested via its own
// submodule (`shape.zig`: "Always test noop"). (N266 CLOSED by S-14: this was a
// `target_os` split that could only express the two defaults.)
//
// .freetype, .freetype_windows, .fontconfig_freetype, .coretext_freetype,
// .coretext_harfbuzz, => harfbuzz.Shaper,
#[cfg(rustty_font_has_harfbuzz)]
pub type Shaper = harfbuzz::Shaper;
// .coretext => coretext.Shaper,
#[cfg(rustty_font_backend = "coretext")]
pub type Shaper = coretext::Shaper;
// .coretext_noshape => noop.Shaper,
#[cfg(rustty_font_backend = "coretext_noshape")]
pub type Shaper = noop::Shaper;
// `.web_canvas => web_canvas.Shaper` (wasm browser backend).
#[cfg(rustty_font_backend = "web_canvas")]
pub type Shaper = web_canvas::Shaper;

/// The error type of the active [`Shaper`]'s fallible calls (`init`/`shape`).
/// RUSTTY-ONLY: the reference threads the shaper's inferred error set through
/// `!` unions at each caller (`renderer/generic.zig`); Rust callers name it
/// once, here, so they need not know which backend was built.
#[cfg(rustty_font_has_harfbuzz)]
pub type ShaperError = harfbuzz::Error;
#[cfg(rustty_font_backend = "coretext")]
pub type ShaperError = coretext::Error;
#[cfg(rustty_font_backend = "coretext_noshape")]
pub type ShaperError = noop::Error;
#[cfg(rustty_font_backend = "web_canvas")]
pub type ShaperError = web_canvas::Error;

/// A cell is a single glyph within a terminal that should be rendered
/// for a shaping call. Not all terminal cells may be present; only
/// cells that have a glyph that needs to be rendered.
// pub const Cell = struct { ... };
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cell {
    /// The X position of this shaper cell relative to the offset of the
    /// run. Because runs are always within a single row, it is expected
    /// that the caller can reconstruct the full position of the cell by
    /// using the known Y position of the cell and adding the X position
    /// to the run offset.
    pub x: u16,

    /// An additional offset to apply to the rendering.
    // x_offset: i16 = 0,
    // y_offset: i16 = 0,
    pub x_offset: i16,
    pub y_offset: i16,

    /// The glyph index for this cell. The font index to use alongside
    /// this cell is available in the text run. This glyph index is only
    /// valid for a given GroupCache and FontIndex that was used to create
    /// the runs.
    pub glyph_index: u32,
}

/// Options for shapers.
// pub const Options = struct { features: []const []const u8 = &.{} };
//
// PORT NOTE: `features` is a slice of feature strings (byte-slices) → `Vec<Vec<u8>>`
// (memory `zig-byteslice-to-rust-rawbytes`); the reference default `&.{}` is the
// empty `Vec` (`Default`).
#[derive(Clone, Debug, Default)]
pub struct Options {
    /// Font features to use when shaping.
    ///
    /// Note: eventually, this will move to font.Face probably as we may
    /// want to support per-face feature configuration. For now, we only
    /// support applying features globally.
    pub features: Vec<Vec<u8>>,
}

/// Faithful stand-in for `config.FontShapingBreak` (break #9: `rustty-config`
/// is injected via DI and is NOT a dep of this tier). The reference type is a
/// `packed struct { cursor: bool = true }`, read by `applyBreakConfig`.
// pub const FontShapingBreak = packed struct { cursor: bool = true };
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FontShapingBreak {
    pub cursor: bool,
}

impl Default for FontShapingBreak {
    fn default() -> Self {
        FontShapingBreak { cursor: true }
    }
}

/// Options for runIterator.
// pub const RunOptions = struct { ... };
pub struct RunOptions<'a> {
    /// The font state for the terminal screen. This is mutable because
    /// cached values may be updated during shaping.
    // grid: *SharedGrid,
    pub grid: *mut SharedGrid,

    /// The cells for the row to shape.
    // cells: std.MultiArrayList(terminal.RenderState.Cell).Slice = .empty,
    //
    // PORT NOTE (reshape): rustty's `RenderState` cells are the SoA `CellList`;
    // this is its borrowed `CellSlice` view (`.raw`/`.style` columns +
    // `.grapheme(x)`), the direct analogue of `MultiArrayList(Cell).Slice`. The
    // reference default `.empty` is an empty `CellSlice`.
    pub cells: rustty_vt::render::CellSlice<'a>,

    /// The x boundaries of the selection in this row.
    // selection: ?[2]u16 = null,
    pub selection: Option<[u16; 2]>,

    /// The cursor position within this row. This is used to break shaping
    /// on cursor boundaries. This can be disabled by setting this to
    /// null.
    // cursor_x: ?usize = null,
    pub cursor_x: Option<usize>,
}

impl RunOptions<'_> {
    /// Apply the font break configuration to the run.
    // pub fn applyBreakConfig(self, config: configpkg.FontShapingBreak) void {
    //     if (!config.cursor) self.cursor_x = null;
    // }
    pub fn apply_break_config(&mut self, config: FontShapingBreak) {
        if !config.cursor {
            self.cursor_x = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // test {
    //     _ = Cache;
    //     _ = Shaper;
    //
    //     // Always test noop
    //     _ = noop;
    // }
    //
    // PORT NOTE: the reference module test forces analysis of `Cache`, the
    // active `Shaper`, and `noop`. We mirror it by touching each so a
    // removed/renamed decl surfaces as a test failure (their own tests run via
    // the submodules).
    #[test]
    fn module() {
        let _ = core::mem::size_of::<Cache>();
        let _ = core::mem::size_of::<Shaper>();
        // _ = noop
        let _ = core::mem::size_of::<noop::Shaper>();
    }
}
