//! `vaxis/src/Screen.zig` (libvaxis 0.6.0: 78 LOC, 1 named test) — the flat cell buffer (VAXIS_C2_DESIGN §2).
//!
//! PORT NOTE (ownership): Zig `buf: []Cell` (allocator-owned slice) → `Vec<Cell<'a>>`
//! (the crate OWNS the flat cell buffer). `Cell<'a>` carries `'a` because
//! `Character.grapheme: &'a [u8]`; cells written into the buffer borrow their grapheme
//! bytes from caller memory (segment text / `'static` border glyphs) — Zig's exact
//! "caller keeps the text alive" contract, made explicit by threading `'a` through
//! `Screen<'a>` and `Window<'a>`.
//!
//! PORT NOTE (allocator/error dropped): Zig `init(alloc, winsize) Allocator.Error!Screen`
//! and `deinit(alloc)` drop the allocator param and the error union — `Vec` alloc is
//! infallible (aborts on OOM), consistent with the crate's pure-leaf model; `deinit`
//! becomes `Drop` of the `Vec`.

use std::sync::Arc;

use crate::cell::{Cell, CursorShape};
use crate::gwidth::Method;
use crate::mouse::Shape;
use crate::Winsize;

/// `pub const Cursor = struct { row: u16 = 0, col: u16 = 0 };` (libvaxis 0.6.0; 0.5.1 had
/// the two `cursor_row` / `cursor_col` fields instead).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Cursor {
    pub row: u16,
    pub col: u16,
}

/// A Zig `[]Cursor` slice as libvaxis 0.6.0 uses it for `cursor_secondary`: an allocation
/// that `Vaxis` shares between `screen.cursor_secondary` and `state.cursor_secondary`
/// and compares by IDENTITY (`std.meta.eql` on a slice is `a.ptr == b.ptr and a.len ==
/// b.len`, not a content comparison).
///
/// PORT NOTE (slice identity + ownership): Zig frees and duplicates these slices by hand,
/// comparing pointers to decide which copy it owns. Here each allocation is an
/// `Arc<[Cursor]>` (freed when its last holder drops it) and identity is
/// [`Arc::ptr_eq`]. The empty slice `&.{}` is `None`: every `&.{}` of this type has the same
/// pointer in Zig (MEASURED with Zig 0.16.0: a default `Screen`'s and a default
/// `Vaxis.state`'s empty slices compare equal), so two empty slices are identical here too.
/// `Arc`, not `Rc`, because a `Vaxis` is shared with the `Loop`'s read thread.
#[derive(Clone, Debug, Default)]
pub struct CursorSlice(Option<Arc<[Cursor]>>);

impl CursorSlice {
    /// The empty slice (`&.{}`).
    pub const EMPTY: CursorSlice = CursorSlice(None);

    /// A fresh allocation holding `cursors` (Zig `toOwnedSlice` / `dupe`).
    pub fn from_vec(cursors: Vec<Cursor>) -> CursorSlice {
        if cursors.is_empty() {
            return CursorSlice::EMPTY;
        }
        CursorSlice(Some(Arc::from(cursors)))
    }

    pub fn as_slice(&self) -> &[Cursor] {
        match &self.0 {
            Some(a) => a,
            None => &[],
        }
    }

    /// `std.meta.eql(a, b)` for two slices: same pointer and same length.
    pub fn ptr_eql(&self, other: &CursorSlice) -> bool {
        match (&self.0, &other.0) {
            (None, None) => true,
            (Some(a), Some(b)) => Arc::ptr_eq(a, b) && a.len() == b.len(),
            _ => false,
        }
    }
}

pub struct Screen<'a> {
    pub width: u16,
    pub height: u16,

    pub width_pix: u16,
    pub height_pix: u16,

    pub buf: Vec<Cell<'a>>,

    pub cursor: Cursor,
    pub cursor_vis: bool,
    pub cursor_secondary: CursorSlice,

    pub width_method: Method,

    pub mouse_shape: Shape,
    pub cursor_shape: CursorShape,
}

impl Default for Screen<'_> {
    fn default() -> Self {
        // PORT NOTE: the non-zero Zig field-level defaults live at the field-declaration
        // site (`width_method: Method = .wcwidth`, `mouse_shape: Shape = .default`,
        // `cursor_shape: Cell.CursorShape = .default`), NOT as enum-level defaults;
        // reproduce them explicitly here (a manual `Default`, not `derive`).
        Screen {
            width: 0,
            height: 0,
            width_pix: 0,
            height_pix: 0,
            buf: Vec::new(),
            cursor: Cursor::default(),
            cursor_vis: false,
            cursor_secondary: CursorSlice::EMPTY,
            width_method: Method::Wcwidth,
            mouse_shape: Shape::Default,
            cursor_shape: CursorShape::Default,
        }
    }
}

impl<'a> Screen<'a> {
    pub fn init(winsize: Winsize) -> Screen<'a> {
        let w = winsize.cols;
        let h = winsize.rows;
        // Zig: buf = try alloc.alloc(Cell, @as(usize, @intCast(w)) * h); then
        // @memset(self.buf, base_cell) where `const base_cell: Cell = .{}` — the base cell
        // IS `Cell::default()`; `vec![…; n]` does the alloc + memset in one.
        let base_cell: Cell = Cell::default();
        Screen {
            buf: vec![base_cell; (w as usize) * (h as usize)],
            width: w,
            height: h,
            width_pix: winsize.x_pixel,
            height_pix: winsize.y_pixel,
            ..Screen::default()
        }
    }

    pub fn deinit(&mut self) {
        // PORT NOTE: Zig `alloc.free(self.buf)` → `Vec`'s `Drop` frees the buffer. This
        // shim is kept for parity; it just clears the buffer.
        self.buf.clear();
    }

    /// writes a cell to a location. 0 indexed
    pub fn write_cell(&mut self, col: u16, row: u16, cell: Cell<'a>) {
        if col >= self.width || row >= self.height {
            return;
        }
        let i = (row as usize) * (self.width as usize) + col as usize;
        debug_assert!(i < self.buf.len());
        self.buf[i] = cell;
    }

    pub fn read_cell(&self, col: u16, row: u16) -> Option<Cell<'a>> {
        if col >= self.width || row >= self.height {
            return None;
        }
        let i = (row as usize) * (self.width as usize) + col as usize;
        debug_assert!(i < self.buf.len());
        Some(self.buf[i])
    }

    pub fn clear(&mut self) {
        self.buf.fill(Cell::default());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ref_all_decls() {
        // Zig `test "refAllDecls" { std.testing.refAllDecls(@This()); }` — a smoke test
        // that reifies the module's public decls. Rust compiles all items regardless;
        // this constructs a `Screen` and touches `init`/`write_cell`/`read_cell`/`clear`
        // to keep the COUNT contract.
        let _ = Screen::default();
        let mut screen = Screen::init(Winsize {
            rows: 2,
            cols: 3,
            x_pixel: 0,
            y_pixel: 0,
        });
        screen.write_cell(0, 0, Cell::default());
        let _ = screen.read_cell(0, 0);
        screen.clear();
        screen.deinit();
    }
}
