//! rustty-render-types
//!
//! PORTS: ghostty src/renderer/size.zig (+ hoisted terminal/size.zig per ambiguity A1) —
//! size types extracted as a leaf depended on by font/renderer/config/input/vt-c (break #5).
//!
//! PORT STATUS: done
//!
//! This crate is a clean LEAF over rustty-util only (for `quirks::inlineAssert`).
//! It does NOT depend on rustty-vt: the terminal/size.zig file is hoisted in
//! here (the A1 hoist) precisely to break that cycle. The renderer-size content
//! (renderer/size.zig) lives in this crate root; the hoisted terminal/size.zig
//! content lives in `term_size`. Where the reference `renderer/size.zig` writes
//! `@import("../terminal/size.zig")`, this port references `crate::term_size`.

pub mod term_size;

// const std = @import("std");
// const Allocator = std.mem.Allocator;
// const terminal_size = @import("../terminal/size.zig");
use crate::term_size as terminal_size;

// const log = std.log.scoped(.renderer_size);

/// Controls how extra whitespace around the terminal grid is distributed.
//
// PORT NOTE: the reference enum tags are `false`, `true`, `equal` — `false`/
// `true` are reserved words in Rust, so the variants are spelled `False`/`True`/
// `Equal` (the `balancePadding` `switch` arms map `.false`/`.true`/`.equal`
// accordingly). No semantics change.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PaddingBalance {
    /// No balancing; padding is applied as specified explicitly.
    False,
    /// Balances padding but caps the top padding so the first row doesn't
    /// drift too far from the top of the window. Excess vertical space is
    /// shifted to the bottom.
    True,
    /// Distributes leftover space equally on all sides so the grid is
    /// centered within the screen.
    Equal,
}

// `PaddingBalance` is used as the `window-padding-balance` config field type.
// The reference's `Config.changed` resolves its `deepEqual` via the `.@"enum"`
// arm (`old == new`). The orphan rule requires this `DeepEqual` impl to live in
// `PaddingBalance`'s home crate (rustty-render-types) rather than rustty-config.
impl rustty_datastruct::comparison::DeepEqual for PaddingBalance {
    fn deep_equal(&self, other: &Self) -> bool {
        self == other
    }
}

// pub const AlphaBlending = enum { native, linear, "linear-corrected"; pub fn isLinear };
//
// PORT NOTE (A1 hoist): `AlphaBlending` is DEFINED in ghostty `config/Config.zig`
// (`pub const AlphaBlending`), but the renderer (`Metal.zig` initShaders/initTarget/
// textureOptions) branches on `blending.isLinear()` to pick the sRGB pixel format.
// Because `rustty-config` depends on `rustty-renderer` (the Message::ChangeConfig
// back-edge), the renderer cannot name a config type. So the enum + `is_linear` are
// hoisted to this leaf (depended on by BOTH config and renderer) — the same hoist as
// `PaddingBalance`/`Size`/`Health`. `rustty-config` re-exports it so its public path
// (`value_types::AlphaBlending`) and every use-site keep working. Its config-facing
// PARSE/FORMAT (`string_to_enum`/`to_name`) stays a CONFIG concern (free fns there).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlphaBlending {
    Native,
    Linear,
    LinearCorrected,
}

impl AlphaBlending {
    // pub fn isLinear(self: AlphaBlending) bool
    pub fn is_linear(self) -> bool {
        // return switch (self) {
        //     .native => false,
        //     .linear, .@"linear-corrected" => true,
        // };
        match self {
            AlphaBlending::Native => false,
            AlphaBlending::Linear | AlphaBlending::LinearCorrected => true,
        }
    }
}

// `AlphaBlending` is used as the `alpha-blending` config field type. The reference's
// `Config.changed` resolves its `deepEqual` via the `.@"enum"` arm (`old == new`). The
// orphan rule requires this `DeepEqual` impl to live in `AlphaBlending`'s home crate
// (rustty-render-types), mirroring `PaddingBalance` above.
impl rustty_datastruct::comparison::DeepEqual for AlphaBlending {
    fn deep_equal(&self, other: &Self) -> bool {
        self == other
    }
}

// ===========================================================================
// Chunk 3a Part A — `DerivedConfig` config-type hoists (break config->renderer).
//
// PORT NOTE (A1 hoist, GPU_RD_CHUNK3A_DESIGN.md Part A): these six value-types
// are DEFINED in ghostty `config/Config.zig` but named as `renderer/generic.zig`
// `DerivedConfig` field types. Because `rustty-config` depends on `rustty-renderer`
// (the Message::ChangeConfig back-edge), the renderer cannot name a config type; so
// each plain value-type + its `DeepEqual` (+ `FontShapingBreak`'s `Default`) is
// hoisted to this leaf (depended on by BOTH config and renderer) — the same hoist as
// `PaddingBalance`/`Size`/`Health`/`AlphaBlending`. `rustty-config` re-exports each so
// `value_types::X`, the field/TypeId/defaults/Key use-sites keep resolving. Every
// config-facing PARSE/FORMAT/C-API projection stays a CONFIG concern (config-local
// free fns), so NO new crate edge is introduced (check_acyclic stays GREEN):
// `TerminalColor`/`Path`/`RepeatablePath` are NOT here — they would drag
// `rustty-vt`/`rustty-args`/`rustty-cfg-fmt` and cycle (see the design's Part B).
// ===========================================================================

// pub const WindowPaddingColor = enum { background, extend, "extend-always" };
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowPaddingColor {
    Background,
    Extend,
    ExtendAlways,
}

impl rustty_datastruct::comparison::DeepEqual for WindowPaddingColor {
    fn deep_equal(&self, other: &Self) -> bool {
        self == other
    }
}

// pub const WindowColorspace = enum { srgb, "display-p3" };
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowColorspace {
    Srgb,
    DisplayP3,
}

impl rustty_datastruct::comparison::DeepEqual for WindowColorspace {
    fn deep_equal(&self, other: &Self) -> bool {
        self == other
    }
}

// pub const BackgroundImagePosition = enum { "top-left", "top-center", "top-right", "center-left", "center-center", "center-right", "bottom-left", "bottom-center", "bottom-right", center };
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackgroundImagePosition {
    TopLeft,
    TopCenter,
    TopRight,
    CenterLeft,
    CenterCenter,
    CenterRight,
    BottomLeft,
    BottomCenter,
    BottomRight,
    Center,
}

impl rustty_datastruct::comparison::DeepEqual for BackgroundImagePosition {
    fn deep_equal(&self, other: &Self) -> bool {
        self == other
    }
}

// pub const BackgroundImageFit = enum { contain, cover, stretch, none };
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackgroundImageFit {
    Contain,
    Cover,
    Stretch,
    None,
}

impl rustty_datastruct::comparison::DeepEqual for BackgroundImageFit {
    fn deep_equal(&self, other: &Self) -> bool {
        self == other
    }
}

/// See "font-shaping-break" for documentation
// pub const FontShapingBreak = packed struct { cursor: bool = true };
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FontShapingBreak {
    pub cursor: bool,
}

impl Default for FontShapingBreak {
    fn default() -> Self {
        Self { cursor: true }
    }
}

// `FontShapingBreak` is a packed-flag struct field; the reference's `Config.changed`
// resolves its `deepEqual` via the `.@"struct"` arm (recurse field-by-field). All
// fields are `bool`, so field-by-field `deep_equal` collapses to `==`.
impl rustty_datastruct::comparison::DeepEqual for FontShapingBreak {
    fn deep_equal(&self, other: &Self) -> bool {
        self == other
    }
}

// pub const BackgroundBlur = union(enum) { false, true, "macos-glass-regular", "macos-glass-clear", radius: u8 };
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackgroundBlur {
    False,
    True,
    MacosGlassRegular,
    MacosGlassClear,
    Radius(u8),
}

// background-blur: BackgroundBlur
// (union(enum) { false, true, macos-glass-regular, macos-glass-clear,
//   radius: u8 }). No `equal` decl -> the reference's `.@"union"` arm: compare the
// active tag, then `deepEqual` the active payload.
impl rustty_datastruct::comparison::DeepEqual for BackgroundBlur {
    fn deep_equal(&self, other: &Self) -> bool {
        match (self, other) {
            (BackgroundBlur::False, BackgroundBlur::False) => true,
            (BackgroundBlur::True, BackgroundBlur::True) => true,
            (BackgroundBlur::MacosGlassRegular, BackgroundBlur::MacosGlassRegular) => true,
            (BackgroundBlur::MacosGlassClear, BackgroundBlur::MacosGlassClear) => true,
            (BackgroundBlur::Radius(a), BackgroundBlur::Radius(b)) => {
                rustty_datastruct::comparison::DeepEqual::deep_equal(a, b)
            }
            _ => false,
        }
    }
}

/// All relevant sizes for a rendered terminal. These are all the sizes that
/// any functionality should need to know about the terminal in order to
/// convert between any coordinate systems.
///
/// See the individual field type documentation for more information on each
/// field. One important note is that any pixel values should already be scaled
/// to the current DPI of the screen. If the DPI changes, the sizes should be
/// recalculated and we expect this to be done by the caller.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Size {
    pub screen: ScreenSize,
    pub cell: CellSize,
    pub padding: Padding,
}

impl Size {
    /// Return the grid size for this size. The grid size is calculated by
    /// taking the screen size, removing padding, and dividing by the cell
    /// dimensions.
    pub fn grid(self) -> GridSize {
        GridSize::init(self.screen.sub_padding(self.padding), self.cell)
    }

    /// The size of the terminal. This is the same as the screen without
    /// padding.
    pub fn terminal(self) -> ScreenSize {
        self.screen.sub_padding(self.padding)
    }

    /// Set the padding to be balanced around the grid. The balanced
    /// padding is calculated AFTER the explicit padding is taken
    /// into account.
    pub fn balance_padding(&mut self, explicit: Padding, mode: PaddingBalance) {
        // This ensure grid() does the right thing
        self.padding = explicit;

        // Now we can calculate the balanced padding
        self.padding = Padding::balanced(self.screen, self.grid(), self.cell);

        match mode {
            PaddingBalance::False => unreachable!(),
            PaddingBalance::Equal => {}
            PaddingBalance::True => {
                // Cap the top padding to avoid excessive space above the
                // first row. The maximum is the balanced explicit horizontal
                // padding plus half a cell width. Any excess is shifted to
                // the bottom.
                //
                // PORT NOTE: reference defect (R13), UD71 — the sum saturates
                // (see `ScreenSize::sub_padding`).
                let max_top = explicit
                    .left
                    .saturating_add(explicit.right)
                    .saturating_add(self.cell.width)
                    / 2;
                let vshift = self.padding.top.saturating_sub(max_top);
                self.padding.top -= vshift;
                self.padding.bottom += vshift;
            }
        }
    }
}

/// A coordinate. This is defined as a tagged union to allow for different
/// coordinate systems to be represented.
///
/// A coordinate is only valid within the context of a stable Size value.
/// If any of the sizes in the Size struct change, the coordinate is no
/// longer valid and must be recalculated. A conversion function is provided
/// to migrate to a new Size (which may result in failure).
///
/// The coordinate systems are:
///
///   * surface: (0, 0) is the top-left of the surface (with padding). Negative
///     values are allowed and are off the surface. Likewise, values greater
///     than the surface size are off the surface. Units are pixels.
///
///   * terminal: (0, 0) is the top-left of the terminal grid. This is the
///     same as the surface but with the padding removed. Negative values and
///     values greater than the grid size are allowed and are off the terminal.
///     Units are pixels.
///
///   * grid: (0, 0) is the top-left of the grid. Units are in cells. Negative
///     values are not allowed but values greater than the grid size are
///     possible and are off the grid.
///
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Coordinate {
    Surface(CoordinateSurface),
    Terminal(CoordinateTerminal),
    Grid(CoordinateGrid),
}

/// The tag of `Coordinate` — `@typeInfo(Coordinate).@"union".tag_type.?` in the
/// reference. Used by `convert` to select the destination coordinate system.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CoordinateTag {
    Surface,
    Terminal,
    Grid,
}

// pub const Surface = struct { x: f64, y: f64 };
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct CoordinateSurface {
    pub x: f64,
    pub y: f64,
}

// pub const Terminal = struct { x: f64, y: f64 };
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct CoordinateTerminal {
    pub x: f64,
    pub y: f64,
}

// pub const Grid = struct { x: GridSize.Unit, y: GridSize.Unit };
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct CoordinateGrid {
    pub x: GridSizeUnit,
    pub y: GridSizeUnit,
}

impl Coordinate {
    /// The active tag of this coordinate (`@as(Tag, self)` in the reference).
    pub fn tag(self) -> CoordinateTag {
        match self {
            Coordinate::Surface(_) => CoordinateTag::Surface,
            Coordinate::Terminal(_) => CoordinateTag::Terminal,
            Coordinate::Grid(_) => CoordinateTag::Grid,
        }
    }

    /// Convert a coordinate to a different space within the same Size.
    pub fn convert(self, to: CoordinateTag, size: Size) -> Coordinate {
        // Unlikely fast-path but avoid work.
        if self.tag() == to {
            return self;
        }

        // To avoid the combinatorial explosion of conversion functions, we
        // convert to the surface system first and then reconvert from there.
        let surface = self.convert_to_surface(size);

        match to {
            CoordinateTag::Surface => Coordinate::Surface(surface),
            CoordinateTag::Terminal => Coordinate::Terminal(CoordinateTerminal {
                x: surface.x - (size.padding.left as f64),
                y: surface.y - (size.padding.top as f64),
            }),
            CoordinateTag::Grid => {
                // Get rid of the padding.
                let term = match Coordinate::Surface(surface).convert(CoordinateTag::Terminal, size)
                {
                    Coordinate::Terminal(t) => t,
                    _ => unreachable!(),
                };

                // We need our grid to clamp
                let grid = size.grid();

                // Calculate the grid position.
                let cell_width: f64 = size.cell.width as f64;
                let cell_height: f64 = size.cell.height as f64;
                let clamped_x: f64 = (0.0f64).max(term.x);
                let clamped_y: f64 = (0.0f64).max(term.y);
                // PORT NOTE: reference defect (R13), UPSTREAM-DEFECTS.md UD100.
                // The reference's `@intFromFloat` into the `u16` grid unit
                // (`size.zig:144-145`) traps in ReleaseSafe on a position more
                // than 65535 cells from the origin, which a C-ABI mouse position
                // can be (and on `inf` from a zero cell width). Rust's `as`
                // saturates, and the `@min` below clamps to the grid, which is
                // what the surrounding code does for every other overshoot.
                let col: GridSizeUnit = (clamped_x / cell_width) as GridSizeUnit;
                let row: GridSizeUnit = (clamped_y / cell_height) as GridSizeUnit;
                let clamped_col: GridSizeUnit = col.min(grid.columns - 1);
                let clamped_row: GridSizeUnit = row.min(grid.rows - 1);
                Coordinate::Grid(CoordinateGrid {
                    x: clamped_col,
                    y: clamped_row,
                })
            }
        }
    }

    /// Convert a coordinate to the surface coordinate system.
    fn convert_to_surface(self, size: Size) -> CoordinateSurface {
        match self {
            Coordinate::Surface(v) => v,
            Coordinate::Terminal(v) => CoordinateSurface {
                x: v.x + (size.padding.left as f64),
                y: v.y + (size.padding.top as f64),
            },
            Coordinate::Grid(v) => {
                let col: f64 = v.x as f64;
                let row: f64 = v.y as f64;
                let cell_width: f64 = size.cell.width as f64;
                let cell_height: f64 = size.cell.height as f64;
                let padding_left: f64 = size.padding.left as f64;
                let padding_top: f64 = size.padding.top as f64;
                CoordinateSurface {
                    x: col * cell_width + padding_left,
                    y: row * cell_height + padding_top,
                }
            }
        }
    }
}

/// The dimensions of a single "cell" in the terminal grid.
///
/// The dimensions are dependent on the current loaded set of font glyphs.
/// We calculate the width based on the widest character and the height based
/// on the height requirement for an underscore (the "lowest" -- visually --
/// character).
///
/// The units for the width and height are in world space. They have to
/// be normalized for any renderer implementation.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct CellSize {
    pub width: u32,
    pub height: u32,
}

/// The dimensions of the screen that the grid is rendered to. This is the
/// terminal screen, so it is likely a subset of the window size. The dimensions
/// should be in pixels.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ScreenSize {
    pub width: u32,
    pub height: u32,
}

impl ScreenSize {
    /// Subtract padding from the screen size.
    //
    // PORT NOTE: reference defect (R13), UD71. The reference saturates the
    // subtraction (`-|`) but ADDS the two paddings unchecked, so a
    // `window-padding-x`/`-y` of 2147483648 or more per side (at 72 dpi)
    // overflows `u32`: ReleaseSafe panics "integer overflow", ReleaseFast wraps
    // the padding to nothing (measured, Zig 0.16.0). The sum saturates here, so
    // a padding wider than the screen leaves no room for the grid, which the
    // reference already handles (the grid keeps one cell and `resize` warns
    // "Is your padding reasonable?"). The same for `blank_padding` and
    // `Size::balance_padding`.
    pub fn sub_padding(self, padding: Padding) -> ScreenSize {
        ScreenSize {
            width: self
                .width
                .saturating_sub(padding.left.saturating_add(padding.right)),
            height: self
                .height
                .saturating_sub(padding.top.saturating_add(padding.bottom)),
        }
    }

    /// Calculates the amount of blank space around the grid. This is possible
    /// when padding isn't balanced.
    ///
    /// The "self" screen size here should be the unpadded screen.
    pub fn blank_padding(self, padding: Padding, grid: GridSize, cell: CellSize) -> Padding {
        let grid_width = grid.columns as u32 * cell.width;
        let grid_height = grid.rows as u32 * cell.height;
        // (R13, UD71 — saturating; see `sub_padding`.)
        let padded_width = grid_width.saturating_add(padding.left.saturating_add(padding.right));
        let padded_height = grid_height.saturating_add(padding.top.saturating_add(padding.bottom));

        // Note these have to use a saturating subtraction to avoid underflow
        // because our padding can cause the padded sizes to be larger than
        // our real screen if the screen is shrunk to a minimal size such
        // as 1x1.
        let leftover_width = self.width.saturating_sub(padded_width);
        let leftover_height = self.height.saturating_sub(padded_height);

        Padding {
            top: 0,
            bottom: leftover_height,
            right: leftover_width,
            left: 0,
        }
    }

    /// Returns true if two sizes are equal.
    pub fn equals(self, other: ScreenSize) -> bool {
        self.width == other.width && self.height == other.height
    }
}

// pub const Unit = terminal_size.CellCountInt;
//
// PORT NOTE: `GridSize.Unit` is a member type alias in the reference. Rust has
// no in-`impl` type alias for a non-trait associated type usable in field
// position, so it is hoisted to a crate-level alias `GridSizeUnit` with the same
// definition; every reference use of `GridSize.Unit` maps to `GridSizeUnit`.
pub type GridSizeUnit = terminal_size::CellCountInt;

/// The dimensions of the grid itself, in rows/columns units.
// `columns: Unit = 0, rows: Unit = 0` — the reference defaults both fields to 0,
// which `#[derive(Default)]` reproduces exactly (the integer types default to 0).
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct GridSize {
    pub columns: GridSizeUnit,
    pub rows: GridSizeUnit,
}

impl GridSize {
    /// Initialize a grid size based on a screen and cell size.
    pub fn init(screen: ScreenSize, cell: CellSize) -> GridSize {
        let mut result: GridSize = GridSize::default();
        result.update(screen, cell);
        result
    }

    /// Update the columns/rows for the grid based on the given screen and
    /// cell size.
    pub fn update(&mut self, screen: ScreenSize, cell: CellSize) {
        let cell_width: f32 = cell.width as f32;
        let cell_height: f32 = cell.height as f32;
        let screen_width: f32 = screen.width as f32;
        let screen_height: f32 = screen.height as f32;
        // PORT NOTE: reference defect (R13), UPSTREAM-DEFECTS.md UD100. The
        // reference's `@intFromFloat` into the `u16` unit (`size.zig:258-259`)
        // traps in ReleaseSafe when a screen holds more than 65535 cells, or on
        // `inf` from a zero cell size (ReleaseFast wraps 100000 to 34464,
        // measured, Zig 0.16.0). Screen and cell sizes reach here from the C
        // ABI (`rustty_surface_set_size`) and the apprt. Rust's `as` saturates
        // to the largest grid the unit holds instead.
        let calc_cols: GridSizeUnit = (screen_width / cell_width) as GridSizeUnit;
        let calc_rows: GridSizeUnit = (screen_height / cell_height) as GridSizeUnit;
        self.columns = 1.max(calc_cols);
        self.rows = 1.max(calc_rows);
    }

    /// Returns true if two sizes are equal.
    pub fn equals(self, other: GridSize) -> bool {
        self.columns == other.columns && self.rows == other.rows
    }
}

/// The padding to add to a screen.
// `top/bottom/right/left: u32 = 0` — the reference defaults every field to 0,
// which `#[derive(Default)]` reproduces exactly.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Padding {
    pub top: u32,
    pub bottom: u32,
    pub right: u32,
    pub left: u32,
}

impl Padding {
    /// Returns padding that balances the whitespace around the screen
    /// for the given grid and cell sizes.
    pub fn balanced(screen: ScreenSize, grid: GridSize, cell: CellSize) -> Padding {
        // Turn our cell sizes into floats for the math
        let cell_width: f32 = cell.width as f32;
        let cell_height: f32 = cell.height as f32;

        // The size of our full grid
        let grid_width = (grid.columns as f32) * cell_width;
        let grid_height = (grid.rows as f32) * cell_height;

        // The empty space to the right of a line and bottom of the last row
        let space_right = (screen.width as f32) - grid_width;
        let space_bot = (screen.height as f32) - grid_height;

        // The padding is split equally along both axes.
        let padding_right = (space_right / 2.0).floor();
        let padding_left = padding_right;

        let padding_bot = (space_bot / 2.0).floor();
        let padding_top = padding_bot;

        let zero = 0.0f32;
        Padding {
            top: zero.max(padding_top) as u32,
            bottom: zero.max(padding_bot) as u32,
            right: zero.max(padding_right) as u32,
            left: zero.max(padding_left) as u32,
        }
    }

    /// Add another padding to this one
    // PARITY: named `add` after the reference `Padding.add`; not the
    // `std::ops::Add::add` trait method.
    #[allow(clippy::should_implement_trait)]
    pub fn add(self, other: Padding) -> Padding {
        Padding {
            top: self.top + other.top,
            bottom: self.bottom + other.bottom,
            right: self.right + other.right,
            left: self.left + other.left,
        }
    }

    /// Equality test between two paddings.
    pub fn eql(self, other: Padding) -> bool {
        self.top == other.top
            && self.bottom == other.bottom
            && self.right == other.right
            && self.left == other.left
    }
}

#[cfg(test)]
#[allow(non_snake_case)]
mod tests {
    use super::*;

    /// UD100 (R13, task R13-D): grid arithmetic whose result leaves the `u16`
    /// unit. The reference traps on `@intFromFloat` in `GridSize.update` and in
    /// the surface-to-grid conversion (measured, Zig 0.16.0: 100000 px of 1 px
    /// cells and a zero cell width both trap; ReleaseFast wraps to 34464 and
    /// gives 65535). The port saturates and clamps.
    #[test]
    fn grid_arithmetic_outside_u16_saturates() {
        // Positive control.
        let g = GridSize::init(
            ScreenSize {
                width: 800,
                height: 600,
            },
            CellSize {
                width: 10,
                height: 20,
            },
        );
        assert_eq!((80, 30), (g.columns, g.rows));

        let g = GridSize::init(
            ScreenSize {
                width: 100_000,
                height: 600,
            },
            CellSize {
                width: 1,
                height: 20,
            },
        );
        assert_eq!(
            (u16::MAX, 30),
            (g.columns, g.rows),
            "saturated, not wrapped to 34464"
        );
        let g = GridSize::init(
            ScreenSize {
                width: 800,
                height: 600,
            },
            CellSize {
                width: 0,
                height: 0,
            },
        );
        assert_eq!((u16::MAX, u16::MAX), (g.columns, g.rows), "inf saturates");

        // A mouse position far outside the surface clamps to the last cell.
        let size = Size {
            screen: ScreenSize {
                width: 800,
                height: 600,
            },
            cell: CellSize {
                width: 10,
                height: 20,
            },
            padding: Padding::default(),
        };
        for (x, y) in [
            (1e12, 1e12),
            (f64::INFINITY, f64::INFINITY),
            (f64::NAN, f64::NAN),
        ] {
            let grid =
                Coordinate::Surface(CoordinateSurface { x, y }).convert(CoordinateTag::Grid, size);
            let want = if x.is_nan() { (0, 0) } else { (79, 29) };
            match grid {
                Coordinate::Grid(g) => assert_eq!(want, (g.x, g.y), "({x}, {y})"),
                other => panic!("expected a grid coordinate, got {other:?}"),
            }
        }
    }

    #[test]
    fn Size_balancePadding_equal_distributes_whitespace_equally() {
        // screen=1050x850, cell=10x20, explicit=4 each side
        // grid: (1050-8)/10=104 cols, (850-8)/20=42 rows
        // leftover: 1050-1040=10 horizontal, 850-840=10 vertical
        // balanced: left=right=5, top=bottom=5
        let mut size: Size = Size {
            screen: ScreenSize {
                width: 1050,
                height: 850,
            },
            cell: CellSize {
                width: 10,
                height: 20,
            },
            padding: Padding::default(),
        };
        size.balance_padding(
            Padding {
                top: 4,
                bottom: 4,
                left: 4,
                right: 4,
            },
            PaddingBalance::Equal,
        );
        assert_eq!(size.padding.left, size.padding.right);
        assert_eq!(size.padding.top, size.padding.bottom);
        assert!(size.padding.top > 0);
    }

    #[test]
    fn Size_balancePadding_true_shifts_excess_top_to_bottom() {
        // screen=1090x1070, cell=20x40, explicit=0
        // grid: 1090/20=54 cols, 1070/40=26 rows
        // leftover: 1090-1080=10, 1070-1040=30
        // balanced: left=right=5, top=bottom=15
        // vshift cap: (0+0+20)/2=10, vshift=15-10=5
        // result: top=10, bottom=20
        let mut size: Size = Size {
            screen: ScreenSize {
                width: 1090,
                height: 1070,
            },
            cell: CellSize {
                width: 20,
                height: 40,
            },
            padding: Padding::default(),
        };
        size.balance_padding(Padding::default(), PaddingBalance::True);
        assert_eq!(size.padding.left, size.padding.right);
        assert!(size.padding.top < size.padding.bottom);
        assert_eq!(10u32, size.padding.top);
        assert_eq!(20u32, size.padding.bottom);
    }

    #[test]
    fn Padding_balanced_on_zero() {
        // On some systems, our screen can be zero-sized for a bit, and we
        // don't want to end up with negative padding.
        let grid: GridSize = GridSize {
            columns: 100,
            rows: 37,
        };
        let cell: CellSize = CellSize {
            width: 10,
            height: 20,
        };
        let screen: ScreenSize = ScreenSize {
            width: 0,
            height: 0,
        };
        let padding = Padding::balanced(screen, grid, cell);
        assert_eq!(Padding::default(), padding);
    }

    #[test]
    fn GridSize_update_exact() {
        let mut grid: GridSize = GridSize::default();
        grid.update(
            ScreenSize {
                width: 100,
                height: 40,
            },
            CellSize {
                width: 5,
                height: 10,
            },
        );

        assert_eq!(20 as GridSizeUnit, grid.columns);
        assert_eq!(4 as GridSizeUnit, grid.rows);
    }

    #[test]
    fn GridSize_update_rounding() {
        let mut grid: GridSize = GridSize::default();
        grid.update(
            ScreenSize {
                width: 20,
                height: 40,
            },
            CellSize {
                width: 6,
                height: 15,
            },
        );

        assert_eq!(3 as GridSizeUnit, grid.columns);
        assert_eq!(2 as GridSizeUnit, grid.rows);
    }

    #[test]
    fn coordinate_conversion() {
        // A size for testing purposes. Purposely easy to calculate numbers.
        let test_size: Size = Size {
            screen: ScreenSize {
                width: 100,
                height: 100,
            },

            cell: CellSize {
                width: 5,
                height: 10,
            },

            padding: Padding::default(),
        };

        // Each pair is a test case of [expected, actual]. We only test
        // one-way conversion because conversion can be lossy due to clamping
        // and so on.
        let table: &[[Coordinate; 2]] = &[
            [
                Coordinate::Grid(CoordinateGrid { x: 0, y: 0 }),
                Coordinate::Surface(CoordinateSurface { x: 0.0, y: 0.0 }),
            ],
            [
                Coordinate::Grid(CoordinateGrid { x: 1, y: 0 }),
                Coordinate::Surface(CoordinateSurface { x: 6.0, y: 0.0 }),
            ],
            [
                Coordinate::Grid(CoordinateGrid { x: 1, y: 1 }),
                Coordinate::Surface(CoordinateSurface { x: 6.0, y: 10.0 }),
            ],
            [
                Coordinate::Grid(CoordinateGrid { x: 0, y: 0 }),
                Coordinate::Surface(CoordinateSurface { x: -10.0, y: -10.0 }),
            ],
            [
                Coordinate::Grid(CoordinateGrid {
                    x: test_size.grid().columns - 1,
                    y: test_size.grid().rows - 1,
                }),
                Coordinate::Surface(CoordinateSurface {
                    x: 100_000.0,
                    y: 100_000.0,
                }),
            ],
        ];

        for pair in table {
            let expected = pair[0];
            let actual = pair[1].convert(expected.tag(), test_size);
            assert_eq!(expected, actual);
        }
    }
}
