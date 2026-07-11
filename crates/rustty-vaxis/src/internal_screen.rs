//! `vaxis/src/InternalScreen.zig` (133 LOC, 0 test) — the allocating back buffer for
//! diffing (VAXIS_C2_DESIGN §3).
//!
//! PORT NOTE (arena → per-cell `Vec<u8>`): Zig backs `char`/`uri`/`uri_id`
//! (`ArrayListUnmanaged(u8)`) with a per-screen `*ArenaAllocator` (bump-alloc, freed
//! wholesale in `deinit`). Rust replaces the arena with plain owned `Vec<u8>` per field —
//! each `InternalCell` owns its bytes; `deinit` becomes `Drop`. This is the faithful
//! ownership model (the arena was purely an allocation strategy; semantically each cell
//! owns copies of the grapheme/uri bytes that `writeCell` appends). NO lifetime
//! parameter on `InternalScreen`/`InternalCell` — they hold no borrows. The
//! `arena: *ArenaAllocator` field is dropped.
//!
//! PORT NOTE (allocator/error dropped): Zig `init(alloc, w, h) !InternalScreen` creates
//! the arena and returns an error union; Rust drops both (infallible `Vec`).
//!
//! PORT NOTE (`log`/`catch` dropped): Zig wraps each `appendSlice` in `catch { log.warn }`.
//! Rust `Vec` alloc is infallible → no error path; the `log = std.log.scoped(.vaxis)`
//! import and the three `catch`/`log.warn` blocks are dropped (the OOM branch is
//! unreachable in Rust).

use crate::cell::{Cell, Character, CursorShape, Hyperlink, Style};
use crate::mouse::Shape as MouseShape;

pub struct InternalCell {
    /// owned grapheme bytes
    pub char: Vec<u8>,
    pub style: Style,
    /// owned uri bytes
    pub uri: Vec<u8>,
    /// owned uri_id bytes
    pub uri_id: Vec<u8>,
    // if we got skipped because of a wide character
    pub skipped: bool,
    pub default: bool,

    // If we should skip rendering *this* round due to being printed over previously (from a scaled
    // cell, for example)
    pub skip: bool,

    pub scale: crate::cell::Scale,
}

impl Default for InternalCell {
    fn default() -> Self {
        // PORT NOTE (`default = true` quirk): the back-buffer cell defaults `default=true`
        // (a fresh diff cell IS "the default cell"), UNLIKE `Cell.default = false`. `eql`'s
        // fast-path depends on it. Preserve.
        InternalCell {
            char: Vec::new(),
            style: Style::default(),
            uri: Vec::new(),
            uri_id: Vec::new(),
            skipped: false,
            default: true,
            skip: false,
            scale: crate::cell::Scale::default(),
        }
    }
}

impl InternalCell {
    pub fn eql(&self, cell: &Cell) -> bool {
        // fastpath when both cells are default
        if self.default && cell.default {
            return true;
        }

        self.char == cell.char.grapheme
            && Style::eql(self.style, cell.style)
            && self.uri == cell.link.uri
            && self.uri_id == cell.link.params
    }
}

pub struct InternalScreen {
    pub width: u16,
    pub height: u16,

    pub buf: Vec<InternalCell>,

    pub cursor_row: u16,
    pub cursor_col: u16,
    pub cursor_vis: bool,
    pub cursor_shape: CursorShape,

    pub mouse_shape: MouseShape,
}

impl InternalScreen {
    /// sets each cell to the default cell
    pub fn init(w: u16, h: u16) -> InternalScreen {
        // PORT NOTE: the per-cell Zig `initCapacity(alloc, 1)` + `appendAssumeCapacity(' ')`
        // → `char: vec![b' ']`; `uri`/`uri_id` empty; other fields default (`default=true`).
        let mut buf: Vec<InternalCell> = Vec::with_capacity((w as usize) * (h as usize));
        for _ in 0..(w as usize) * (h as usize) {
            buf.push(InternalCell {
                char: vec![b' '],
                uri: Vec::new(),
                uri_id: Vec::new(),
                ..InternalCell::default()
            });
        }
        InternalScreen {
            width: w,
            height: h,
            buf,
            cursor_row: 0,
            cursor_col: 0,
            cursor_vis: false,
            cursor_shape: CursorShape::Default,
            mouse_shape: MouseShape::Default,
        }
    }

    pub fn deinit(&mut self) {
        // PORT NOTE: Zig `arena.deinit(); alloc.destroy(arena); self.* = undefined` →
        // `Drop` of the `Vec<InternalCell>`. The `self.* = undefined` poisoning has no Rust
        // analogue (the value is moved-from / dropped). This shim clears the buffer.
        self.buf.clear();
    }

    /// writes a cell to a location. 0 indexed
    pub fn write_cell(&mut self, col: u16, row: u16, cell: &Cell) {
        // PORT NOTE (`<` bounds quirk): Zig uses `self.width < col` / `self.height < row`
        // (NOT `>=`) — an off-by-one asymmetry vs `Screen`'s `>=`. Preserved VERBATIM.
        if self.width < col {
            // column out of bounds
            return;
        }
        if self.height < row {
            // height out of bounds
            return;
        }
        let i = (row as usize) * (self.width as usize) + col as usize;
        debug_assert!(i < self.buf.len());
        // Zig: clearRetainingCapacity() + appendSlice() → Vec::clear (retains capacity) +
        // extend_from_slice.
        self.buf[i].char.clear();
        self.buf[i].char.extend_from_slice(cell.char.grapheme);
        self.buf[i].uri.clear();
        self.buf[i].uri.extend_from_slice(cell.link.uri);
        self.buf[i].uri_id.clear();
        self.buf[i].uri_id.extend_from_slice(cell.link.params);
        self.buf[i].style = cell.style;
        self.buf[i].default = cell.default;
    }

    pub fn read_cell(&self, col: u16, row: u16) -> Option<Cell<'_>> {
        if self.width < col {
            // column out of bounds
            return None;
        }
        if self.height < row {
            // height out of bounds
            return None;
        }
        // PORT NOTE (index asymmetry): Zig `readCell` uses `(row * self.width) + col` in
        // u16 arithmetic (no `@intCast(usize)` unlike `writeCell`), then uses the result as
        // usize. Reproduced as usize arithmetic — value-identical for in-range inputs.
        let i = (row as usize) * (self.width as usize) + col as usize;
        debug_assert!(i < self.buf.len());
        let cell = &self.buf[i];
        // The returned `Cell<'_>` borrows from `&self` — the read cell is tied to the back
        // buffer (faithful to Zig's `.grapheme = cell.char.items` aliasing the arena).
        Some(Cell {
            char: Character {
                grapheme: &cell.char,
                ..Default::default()
            },
            style: cell.style,
            link: Hyperlink {
                uri: &cell.uri,
                params: &cell.uri_id,
            },
            default: cell.default,
            ..Default::default()
        })
    }
}
