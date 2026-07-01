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
// pub const coretext = @import("shaper/coretext.zig");
// pub const web_canvas = @import("shaper/web_canvas.zig");
//
// PORT NOTE: the HarfBuzz and CoreText *shapers* (and the wasm `web_canvas`
// shaper) are NOT part of the shaper-framework file set — they land in a later
// subtask. Their imports are preserved above as comments; the `Shaper` selector
// (below) currently scaffolds to `noop::Shaper` until they are ported.

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
// PORT NOTE (scaffold): the reference selects the active shaper from
// `options.backend`. The HarfBuzz/CoreText shapers land in a later subtask, so
// like `face.rs`'s initial backend scaffold, the selector currently resolves to
// the always-available `noop::Shaper`. It is re-pointed at the real
// `switch(options.backend)` result (macOS default `coretext` → `coretext.Shaper`;
// Linux default `fontconfig_freetype` → `harfbuzz.Shaper`) when those backends
// are ported. `noop` is always tested.
pub type Shaper = noop::Shaper;

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
    // PORT NOTE: rustty's `RenderState` cells are an AoS `render::Cell` slice
    // (carrying `.raw`/`.grapheme`/`.style`), not a `MultiArrayList`; the
    // reference default `.empty` is the empty slice (`&[]`).
    pub cells: &'a [rustty_vt::render::Cell],

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
