//! `vaxis/src/Window.zig` (889 LOC, 8 test) — the window view (VAXIS_C2_DESIGN §4-6).
//!
//! PORT NOTE (the `Window → Screen` back-reference, §5): Zig `Window` holds `screen:
//! *Screen` and is a `Copy` value type (`self: Window` by value, `child()` returns a new
//! Window sharing the SAME `*Screen`). We port it as `screen: *mut Screen<'a>` and keep
//! `Window<'a>` `Copy`. `&mut Screen` is non-`Copy` and forbids the aliasing that
//! Copy-Window + child windows require, so a raw pointer is the byte-faithful match to Zig
//! `*Screen`. Discipline: every deref is a FRESH, short-lived, non-overlapping reborrow —
//! mutating methods take a transient `unsafe { &mut *self.screen }`, read methods
//! `unsafe { &*self.screen }`; no long-lived `&mut` is held across another access to the
//! same screen (see `fill`/`scroll`).
//!
//! PORT NOTE (`i17 → i32`, §5): the four offset fields are Zig `i17`; Rust has no `i17`, so
//! they widen to `i32` — it covers the range and the clip math stays exact.
//!
//! PORT NOTE (`undefined` screen → `null_mut()`, §5): the five "Window size" tests build a
//! Window over a Zig `undefined` screen; their `initChild`/`readCell`/`writeCell` all
//! bounds-guard-return BEFORE any deref (and `initChild` only copies the pointer). We map
//! `undefined` → `std::ptr::null_mut()`; it is provably never read.

use crate::cell::{Cell, Character, CursorShape, Segment, Style};
use crate::gwidth as gw;
use crate::mouse::Mouse;
use crate::screen::Screen;
use crate::unicode;

#[derive(Clone, Copy)]
pub struct Window<'a> {
    /// absolute horizontal offset from the screen
    pub x_off: i32,
    /// absolute vertical offset from the screen
    pub y_off: i32,
    /// relative horizontal offset, from parent window. This only accumulates if it is negative so that
    /// we can clip the window correctly
    pub parent_x_off: i32,
    /// relative vertical offset, from parent window. This only accumulates if it is negative so that
    /// we can clip the window correctly
    pub parent_y_off: i32,
    /// width of the window. This can't be larger than the terminal screen
    pub width: u16,
    /// height of the window. This can't be larger than the terminal screen
    pub height: u16,

    pub screen: *mut Screen<'a>,
}

impl<'a> Window<'a> {
    /// Creates a new window with offset relative to parent and size clamped to the
    /// parent's size. Windows do not retain a reference to their parent and are
    /// unaware of resizes.
    fn init_child(
        self,
        x_off: i32,
        y_off: i32,
        maybe_width: Option<u16>,
        maybe_height: Option<u16>,
    ) -> Window<'a> {
        let max_height = (self.height as i32 - y_off).max(0);
        let max_width = (self.width as i32 - x_off).max(0);
        let width: u16 = maybe_width.unwrap_or(max_width as u16);
        let height: u16 = maybe_height.unwrap_or(max_height as u16);

        Window {
            x_off: x_off + self.x_off,
            y_off: y_off + self.y_off,
            parent_x_off: (self.parent_x_off + x_off).min(0),
            parent_y_off: (self.parent_y_off + y_off).min(0),
            width: width.min(max_width as u16),
            height: height.min(max_height as u16),
            screen: self.screen,
        }
    }

    /// create a child window
    pub fn child(self, opts: ChildOptions<'a>) -> Window<'a> {
        let result = self.init_child(opts.x_off, opts.y_off, opts.width, opts.height);

        let glyphs = match opts.border.glyphs {
            Glyphs::SingleRounded => SINGLE_ROUNDED,
            Glyphs::SingleSquare => SINGLE_SQUARE,
            Glyphs::Custom(custom) => custom,
        };

        let top_left = Character {
            grapheme: glyphs[0],
            width: 1,
        };
        let horizontal = Character {
            grapheme: glyphs[1],
            width: 1,
        };
        let top_right = Character {
            grapheme: glyphs[2],
            width: 1,
        };
        let vertical = Character {
            grapheme: glyphs[3],
            width: 1,
        };
        let bottom_right = Character {
            grapheme: glyphs[4],
            width: 1,
        };
        let bottom_left = Character {
            grapheme: glyphs[5],
            width: 1,
        };
        let style = opts.border.style;

        let h = result.height;
        let w = result.width;

        let loc: Locations = match opts.border.r#where {
            Where::None => return result,
            Where::All => Locations {
                top: true,
                bottom: true,
                right: true,
                left: true,
            },
            Where::Bottom => Locations {
                bottom: true,
                ..Default::default()
            },
            Where::Right => Locations {
                right: true,
                ..Default::default()
            },
            Where::Left => Locations {
                left: true,
                ..Default::default()
            },
            Where::Top => Locations {
                top: true,
                ..Default::default()
            },
            Where::Other(loc) => loc,
        };
        if loc.top {
            for i in 0..w {
                result.write_cell(
                    i,
                    0,
                    Cell {
                        char: horizontal,
                        style,
                        ..Default::default()
                    },
                );
            }
        }
        if loc.bottom {
            for i in 0..w {
                result.write_cell(
                    i,
                    h.saturating_sub(1),
                    Cell {
                        char: horizontal,
                        style,
                        ..Default::default()
                    },
                );
            }
        }
        if loc.left {
            for i in 0..h {
                result.write_cell(
                    0,
                    i,
                    Cell {
                        char: vertical,
                        style,
                        ..Default::default()
                    },
                );
            }
        }
        if loc.right {
            for i in 0..h {
                result.write_cell(
                    w.saturating_sub(1),
                    i,
                    Cell {
                        char: vertical,
                        style,
                        ..Default::default()
                    },
                );
            }
        }
        // draw corners
        if loc.top && loc.left {
            result.write_cell(
                0,
                0,
                Cell {
                    char: top_left,
                    style,
                    ..Default::default()
                },
            );
        }
        if loc.top && loc.right {
            result.write_cell(
                w.saturating_sub(1),
                0,
                Cell {
                    char: top_right,
                    style,
                    ..Default::default()
                },
            );
        }
        if loc.bottom && loc.left {
            result.write_cell(
                0,
                h.saturating_sub(1),
                Cell {
                    char: bottom_left,
                    style,
                    ..Default::default()
                },
            );
        }
        if loc.bottom && loc.right {
            result.write_cell(
                w.saturating_sub(1),
                h.saturating_sub(1),
                Cell {
                    char: bottom_right,
                    style,
                    ..Default::default()
                },
            );
        }

        let x_off: u16 = if loc.left { 1 } else { 0 };
        let y_off: u16 = if loc.top { 1 } else { 0 };
        let h_delt: u16 = if loc.bottom { 1 } else { 0 };
        let w_delt: u16 = if loc.right { 1 } else { 0 };
        let h_ch: u16 = h.saturating_sub(y_off).saturating_sub(h_delt);
        let w_ch: u16 = w.saturating_sub(x_off).saturating_sub(w_delt);
        result.init_child(x_off as i32, y_off as i32, Some(w_ch), Some(h_ch))
    }

    /// writes a cell to the location in the window
    pub fn write_cell(self, col: u16, row: u16, cell: Cell<'a>) {
        if self.height <= row
            || self.width <= col
            || self.x_off + (col as i32) < 0
            || self.y_off + (row as i32) < 0
            || self.parent_x_off + (col as i32) < 0
            || self.parent_y_off + (row as i32) < 0
        {
            return;
        }

        // Mutating — a narrow transient reborrow (§5). The `@intCast` casts are guarded
        // non-negative by the preceding checks (faithful).
        unsafe {
            (*self.screen).write_cell(
                (col as i32 + self.x_off) as u16,
                (row as i32 + self.y_off) as u16,
                cell,
            );
        }
    }

    /// reads a cell at the location in the window
    pub fn read_cell(self, col: u16, row: u16) -> Option<Cell<'a>> {
        if self.height <= row
            || self.width <= col
            || self.x_off + (col as i32) < 0
            || self.y_off + (row as i32) < 0
            || self.parent_x_off + (col as i32) < 0
            || self.parent_y_off + (row as i32) < 0
        {
            return None;
        }
        // Read-only reborrow (§5).
        unsafe {
            (*self.screen).read_cell(
                (col as i32 + self.x_off) as u16,
                (row as i32 + self.y_off) as u16,
            )
        }
    }

    /// fills the window with the default cell
    pub fn clear(self) {
        self.fill(Cell {
            default: true,
            ..Default::default()
        });
    }

    /// returns the width of the grapheme. This depends on the terminal capabilities
    pub fn gwidth(self, str: &[u8]) -> u16 {
        // Read-only deref of a `Copy` field (§5).
        gw::gwidth(str, unsafe { (*self.screen).width_method })
    }

    /// fills the window with the provided cell
    pub fn fill(self, cell: Cell<'a>) {
        // A single mutating reborrow held for the whole body — no re-entrant screen access
        // inside (§5).
        let s = unsafe { &mut *self.screen };
        if self.x_off + (self.width as i32) < 0
            || self.y_off + (self.height as i32) < 0
            || (s.width as i32) < self.x_off
            || (s.height as i32) < self.y_off
        {
            return;
        }
        let first_row: usize = self.y_off.max(0) as usize;
        if self.x_off == 0 && self.width == s.width {
            // we have a full width window, therefore contiguous memory.
            let start = (first_row * self.width as usize).min(s.buf.len());
            let end = (start + (self.height as usize * self.width as usize)).min(s.buf.len());
            s.buf[start..end].fill(cell);
        } else {
            // Non-contiguous. Iterate over rows an memset
            let mut row: usize = first_row;
            let first_col: usize = self.x_off.max(0) as usize;
            let last_row = (self.height as i32 + self.y_off).min(s.height as i32) as usize;
            while row < last_row {
                let start = (first_col + (row * s.width as usize)).min(s.buf.len());
                let mut end =
                    (start + self.width as usize).min(start + (s.width as usize - first_col));
                end = end.min(s.buf.len());
                s.buf[start..end].fill(cell);
                row += 1;
            }
        }
    }

    /// hide the cursor
    pub fn hide_cursor(self) {
        unsafe {
            (*self.screen).cursor_vis = false;
        }
    }

    /// show the cursor at the given coordinates, 0 indexed
    pub fn show_cursor(self, col: u16, row: u16) {
        if self.x_off + (col as i32) < 0
            || self.y_off + (row as i32) < 0
            || row >= self.height
            || col >= self.width
        {
            return;
        }
        unsafe {
            (*self.screen).cursor_vis = true;
            (*self.screen).cursor_row = (row as i32 + self.y_off) as u16;
            (*self.screen).cursor_col = (col as i32 + self.x_off) as u16;
        }
    }

    pub fn set_cursor_shape(self, shape: CursorShape) {
        unsafe {
            (*self.screen).cursor_shape = shape;
        }
    }

    /// prints segments to the window. returns true if the text overflowed with the
    /// given wrap strategy and size.
    pub fn print(self, segments: &[Segment<'a>], opts: PrintOptions) -> PrintResult {
        let mut row = opts.row_offset;
        match opts.wrap {
            Wrap::Grapheme => {
                let mut col: u16 = opts.col_offset;
                let mut overflow: bool = false;
                'blk: for segment in segments {
                    let mut iter = unicode::grapheme_iterator(segment.text);
                    while let Some(grapheme) = iter.next() {
                        if col >= self.width {
                            row += 1;
                            col = 0;
                        }
                        if row >= self.height {
                            overflow = true;
                            break 'blk;
                        }
                        let s = grapheme.bytes(segment.text);
                        if s == b"\n" {
                            row = row.saturating_add(1);
                            col = 0;
                            continue;
                        }
                        let w = self.gwidth(s);
                        if w == 0 {
                            continue;
                        }
                        if opts.commit {
                            self.write_cell(
                                col,
                                row,
                                Cell {
                                    char: Character {
                                        grapheme: s,
                                        width: w as u8,
                                    },
                                    style: segment.style,
                                    link: segment.link,
                                    wrapped: col + w >= self.width,
                                    ..Default::default()
                                },
                            );
                        }
                        col += w;
                    }
                }
                if col >= self.width {
                    row += 1;
                    col = 0;
                }
                PrintResult { row, col, overflow }
            }
            Wrap::Word => {
                let mut col: u16 = opts.col_offset;
                let mut overflow: bool = false;
                let mut soft_wrapped: bool = false;
                'outer: for segment in segments {
                    let mut line_iter = LineIterator {
                        buf: segment.text,
                        index: 0,
                        has_break: true,
                    };
                    while let Some(line) = line_iter.next() {
                        // PORT NOTE (§6): Zig runs a `defer` at each line-body exit — on the
                        // normal path AND while unwinding a `break :outer` through the body.
                        // Rust has no `defer`; we run the `has_break` bump on EVERY exit of
                        // the line body (tracked by `broke_outer`).
                        let mut iter = WhitespaceTokenizer {
                            buf: line,
                            index: 0,
                        };
                        let mut broke_outer = false;
                        while let Some(token) = iter.next() {
                            match token {
                                Token::Whitespace(len) => {
                                    if soft_wrapped {
                                        continue;
                                    }
                                    for _ in 0..len {
                                        if col >= self.width {
                                            col = 0;
                                            row += 1;
                                            break;
                                        }
                                        if opts.commit {
                                            self.write_cell(
                                                col,
                                                row,
                                                Cell {
                                                    char: Character {
                                                        grapheme: b" ",
                                                        width: 1,
                                                    },
                                                    style: segment.style,
                                                    link: segment.link,
                                                    ..Default::default()
                                                },
                                            );
                                        }
                                        col += 1;
                                    }
                                }
                                Token::Word(word) => {
                                    let width = self.gwidth(word);
                                    if width + col > self.width && width < self.width {
                                        row += 1;
                                        col = 0;
                                    }

                                    let mut grapheme_iterator = unicode::grapheme_iterator(word);
                                    while let Some(grapheme) = grapheme_iterator.next() {
                                        soft_wrapped = false;
                                        if row >= self.height {
                                            overflow = true;
                                            broke_outer = true;
                                            break;
                                        }
                                        let s = grapheme.bytes(word);
                                        let w = self.gwidth(s);
                                        if opts.commit {
                                            self.write_cell(
                                                col,
                                                row,
                                                Cell {
                                                    char: Character {
                                                        grapheme: s,
                                                        width: w as u8,
                                                    },
                                                    style: segment.style,
                                                    link: segment.link,
                                                    ..Default::default()
                                                },
                                            );
                                        }
                                        col += w;
                                        if col >= self.width {
                                            row += 1;
                                            col = 0;
                                            soft_wrapped = true;
                                        }
                                    }
                                }
                            }
                            if broke_outer {
                                break;
                            }
                        }
                        // Zig `defer` body:
                        // We only set soft_wrapped to false if a segment actually contains a linebreak
                        if line_iter.has_break {
                            soft_wrapped = false;
                            row += 1;
                            col = 0;
                        }
                        if broke_outer {
                            break 'outer;
                        }
                    }
                }
                PrintResult {
                    // remove last row counter
                    row,
                    col,
                    overflow,
                }
            }
            Wrap::None => {
                let mut col: u16 = opts.col_offset;
                let mut overflow: bool = false;
                'blk: for segment in segments {
                    let mut iter = unicode::grapheme_iterator(segment.text);
                    while let Some(grapheme) = iter.next() {
                        if col >= self.width {
                            overflow = true;
                            break 'blk;
                        }
                        let s = grapheme.bytes(segment.text);
                        if s == b"\n" {
                            overflow = true;
                            break 'blk;
                        }
                        let w = self.gwidth(s);
                        if w == 0 {
                            continue;
                        }
                        if opts.commit {
                            self.write_cell(
                                col,
                                row,
                                Cell {
                                    char: Character {
                                        grapheme: s,
                                        width: w as u8,
                                    },
                                    style: segment.style,
                                    link: segment.link,
                                    ..Default::default()
                                },
                            );
                        }
                        col = col.saturating_add(w);
                    }
                }
                PrintResult { row, col, overflow }
            }
        }
        // PORT NOTE (§6): Zig closes `print` with a dead `return false;` after the
        // exhaustive `switch` (unreachable, wrong type). OMITTED — the Rust `match` is
        // exhaustive and each arm returns `PrintResult`.
    }

    /// print a single segment. This is just a shortcut for print(&.{segment}, opts)
    pub fn print_segment(self, segment: Segment<'a>, opts: PrintOptions) -> PrintResult {
        self.print(&[segment], opts)
    }

    /// scrolls the window down one row (IE inserts a blank row at the bottom of the
    /// screen and shifts all rows up one)
    pub fn scroll(self, n: u16) {
        if n > self.height {
            return;
        }
        {
            // A single mutating reborrow for the row-shift; dropped before `child`/`clear`
            // create their own fresh reborrows (§5).
            let s = unsafe { &mut *self.screen };
            let mut row: u16 = self.y_off.max(0) as u16;
            let first_col: u16 = self.x_off.max(0) as u16;
            while row < self.height - n {
                let dst_start = (row * s.width) + first_col;
                let dst_end = dst_start + self.width;

                let src_start = ((row + n) * s.width) + first_col;
                let src_end = src_start + self.width;
                // @memcpy(dst, src) — non-overlapping row slices; `copy_within` copies the
                // src range into the dst start.
                s.buf
                    .copy_within(src_start as usize..src_end as usize, dst_start as usize);
                let _ = dst_end;
                row += 1;
            }
        }
        let last_row = self.child(ChildOptions {
            y_off: (self.height - n) as i32,
            ..Default::default()
        });
        last_row.clear();
    }

    /// returns the mouse event if the mouse event occurred within the window. If
    /// the mouse event occurred outside the window, null is returned
    pub fn has_mouse(win: Window, mouse: Option<Mouse>) -> Option<Mouse> {
        let event = mouse?;
        if (event.col as i32) >= win.x_off
            && (event.col as i32) < (win.x_off + win.width as i32)
            && (event.row as i32) >= win.y_off
            && (event.row as i32) < (win.y_off + win.height as i32)
        {
            Some(event)
        } else {
            None
        }
    }
}

#[derive(Default)]
pub struct ChildOptions<'g> {
    pub x_off: i32,
    pub y_off: i32,
    /// the width of the resulting child, including any borders
    pub width: Option<u16>,
    /// the height of the resulting child, including any borders
    pub height: Option<u16>,
    pub border: BorderOptions<'g>,
}

pub struct BorderOptions<'g> {
    pub style: Style,
    pub r#where: Where,
    pub glyphs: Glyphs<'g>,
}

impl Default for BorderOptions<'_> {
    fn default() -> Self {
        BorderOptions {
            style: Style::default(),
            r#where: Where::None,
            glyphs: Glyphs::SingleRounded,
        }
    }
}

/// The Zig `where` field's anonymous `union(enum)`.
#[derive(Clone, Copy)]
pub enum Where {
    None,
    All,
    Top,
    Right,
    Bottom,
    Left,
    Other(Locations),
}

#[derive(Default, Clone, Copy)]
pub struct Locations {
    pub top: bool,
    pub right: bool,
    pub bottom: bool,
    pub left: bool,
}

#[derive(Clone, Copy)]
pub enum Glyphs<'g> {
    SingleRounded,
    SingleSquare,
    /// custom border glyphs. each glyph should be one cell wide and the
    /// following indices apply:
    /// [0] = top left
    /// [1] = horizontal
    /// [2] = top right
    /// [3] = vertical
    /// [4] = bottom right
    /// [5] = bottom left
    Custom([&'g [u8]; 6]),
}

// UTF-8 box-drawing byte strings (never a lossy `String`).
const SINGLE_ROUNDED: [&[u8]; 6] = [
    "╭".as_bytes(),
    "─".as_bytes(),
    "╮".as_bytes(),
    "│".as_bytes(),
    "╯".as_bytes(),
    "╰".as_bytes(),
];
const SINGLE_SQUARE: [&[u8]; 6] = [
    "┌".as_bytes(),
    "─".as_bytes(),
    "┐".as_bytes(),
    "│".as_bytes(),
    "┘".as_bytes(),
    "└".as_bytes(),
];

/// Options to use when printing Segments to a window
#[derive(Clone, Copy)]
pub struct PrintOptions {
    /// vertical offset to start printing at
    pub row_offset: u16,
    /// horizontal offset to start printing at
    pub col_offset: u16,

    /// wrap behavior for printing
    pub wrap: Wrap,

    /// when true, print will write to the screen for rendering. When false,
    /// nothing is written. The return value describes the size of the wrapped
    /// text
    pub commit: bool,
}

impl Default for PrintOptions {
    fn default() -> Self {
        PrintOptions {
            row_offset: 0,
            col_offset: 0,
            wrap: Wrap::Grapheme,
            commit: true,
        }
    }
}

#[derive(Clone, Copy)]
pub enum Wrap {
    /// wrap at grapheme boundaries
    Grapheme,
    /// wrap at word boundaries
    Word,
    /// stop printing after one line
    None,
}

pub struct PrintResult {
    pub col: u16,
    pub row: u16,
    pub overflow: bool,
}

/// Iterates a slice of bytes by linebreaks. Lines are split by '\r', '\n', or '\r\n'
struct LineIterator<'b> {
    buf: &'b [u8],
    index: usize,
    has_break: bool,
}

impl<'b> LineIterator<'b> {
    #[allow(clippy::should_implement_trait)]
    fn next(&mut self) -> Option<&'b [u8]> {
        if self.index >= self.buf.len() {
            return None;
        }

        let start = self.index;
        match index_of_any_pos(self.buf, self.index, b"\r\n") {
            None => {
                if start == 0 {
                    self.has_break = false;
                }
                self.index = self.buf.len();
                Some(&self.buf[start..])
            }
            Some(end) => {
                self.index = end;
                self.consume_cr();
                self.consume_lf();
                Some(&self.buf[start..end])
            }
        }
    }

    // consumes a \n byte
    fn consume_lf(&mut self) {
        if self.index >= self.buf.len() {
            return;
        }
        if self.buf[self.index] == b'\n' {
            self.index += 1;
        }
    }

    // consumes a \r byte
    fn consume_cr(&mut self) {
        if self.index >= self.buf.len() {
            return;
        }
        if self.buf[self.index] == b'\r' {
            self.index += 1;
        }
    }
}

/// `std.mem.indexOfAnyPos(u8, buf, start, any)` — the first index at/after `start` whose
/// byte is any of `any`.
fn index_of_any_pos(buf: &[u8], start: usize, any: &[u8]) -> Option<usize> {
    let mut i = start;
    while i < buf.len() {
        if any.contains(&buf[i]) {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// Returns tokens of text and whitespace
struct WhitespaceTokenizer<'b> {
    buf: &'b [u8],
    index: usize,
}

enum Token<'b> {
    // the length of whitespace. Tab = 8
    Whitespace(usize),
    Word(&'b [u8]),
}

impl<'b> WhitespaceTokenizer<'b> {
    #[allow(clippy::should_implement_trait)]
    fn next(&mut self) -> Option<Token<'b>> {
        if self.index >= self.buf.len() {
            return None;
        }
        let first = self.buf[self.index];
        let mode_whitespace = first == b' ' || first == b'\t';
        if mode_whitespace {
            let mut len: usize = 0;
            while self.index < self.buf.len() {
                match self.buf[self.index] {
                    b' ' => len += 1,
                    b'\t' => len += 8,
                    _ => break,
                }
                self.index += 1;
            }
            Some(Token::Whitespace(len))
        } else {
            let start = self.index;
            while self.index < self.buf.len() {
                match self.buf[self.index] {
                    b' ' | b'\t' => break,
                    _ => {}
                }
                self.index += 1;
            }
            Some(Token::Word(&self.buf[start..self.index]))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cell::Hyperlink;
    use crate::gwidth::Method;

    // A terse `Segment` constructor for the tests — `Segment` has no `Default` (its `text`
    // has none), so build one from `text` with default `style`/`link` (Zig `.{ .text = … }`).
    fn seg(text: &[u8]) -> Segment<'_> {
        Segment {
            text,
            style: Style::default(),
            link: Hyperlink::default(),
        }
    }

    fn parent() -> Window<'static> {
        Window {
            x_off: 0,
            y_off: 0,
            parent_x_off: 0,
            parent_y_off: 0,
            width: 20,
            height: 20,
            screen: std::ptr::null_mut(),
        }
    }

    #[test]
    fn window_size_set() {
        let parent = parent();
        let ch = parent.init_child(1, 1, None, None);
        assert_eq!(19, ch.width);
        assert_eq!(19, ch.height);
    }

    #[test]
    fn window_size_set_too_big() {
        let parent = parent();
        let ch = parent.init_child(0, 0, Some(21), Some(21));
        assert_eq!(20, ch.width);
        assert_eq!(20, ch.height);
    }

    #[test]
    fn window_size_set_too_big_with_offset() {
        let parent = parent();
        let ch = parent.init_child(10, 10, Some(21), Some(21));
        assert_eq!(10, ch.width);
        assert_eq!(10, ch.height);
    }

    #[test]
    fn window_size_nested_offsets() {
        let parent = Window {
            x_off: 1,
            y_off: 1,
            parent_x_off: 0,
            parent_y_off: 0,
            width: 20,
            height: 20,
            screen: std::ptr::null_mut(),
        };
        let ch = parent.init_child(10, 10, Some(21), Some(21));
        assert_eq!(11, ch.x_off);
        assert_eq!(11, ch.y_off);
    }

    #[test]
    fn window_offsets() {
        let parent = parent();
        let ch = parent.init_child(10, 10, Some(21), Some(21));
        let ch2 = ch.init_child(-4, -4, None, None);
        // Reading ch2 at row 0 should be null
        assert!(ch2.read_cell(0, 0).is_none());
        // Should not panic us
        ch2.write_cell(0, 0, Cell::default());
    }

    #[test]
    fn print_grapheme() {
        let mut screen = Screen {
            width_method: Method::Unicode,
            ..Default::default()
        };
        let win = Window {
            x_off: 0,
            y_off: 0,
            parent_x_off: 0,
            parent_y_off: 0,
            width: 4,
            height: 2,
            screen: &mut screen as *mut _,
        };
        let opts = PrintOptions {
            commit: false,
            wrap: Wrap::Grapheme,
            ..Default::default()
        };

        {
            let segments = [seg(b"a")];
            let result = win.print(&segments, opts);
            assert_eq!(1, result.col);
            assert_eq!(0, result.row);
            assert!(!result.overflow);
        }
        {
            let segments = [seg(b"abcd")];
            let result = win.print(&segments, opts);
            assert_eq!(0, result.col);
            assert_eq!(1, result.row);
            assert!(!result.overflow);
        }
        {
            let segments = [seg(b"abcde")];
            let result = win.print(&segments, opts);
            assert_eq!(1, result.col);
            assert_eq!(1, result.row);
            assert!(!result.overflow);
        }
        {
            let segments = [seg(b"abcdefgh")];
            let result = win.print(&segments, opts);
            assert_eq!(0, result.col);
            assert_eq!(2, result.row);
            assert!(!result.overflow);
        }
        {
            let segments = [seg(b"abcdefghi")];
            let result = win.print(&segments, opts);
            assert_eq!(0, result.col);
            assert_eq!(2, result.row);
            assert!(result.overflow);
        }
    }

    #[test]
    fn print_word() {
        let mut screen = Screen {
            width_method: Method::Unicode,
            ..Default::default()
        };
        let win = Window {
            x_off: 0,
            y_off: 0,
            parent_x_off: 0,
            parent_y_off: 0,
            width: 4,
            height: 2,
            screen: &mut screen as *mut _,
        };
        let opts = PrintOptions {
            commit: false,
            wrap: Wrap::Word,
            ..Default::default()
        };

        {
            let segments = [seg(b"a")];
            let result = win.print(&segments, opts);
            assert_eq!(1, result.col);
            assert_eq!(0, result.row);
            assert!(!result.overflow);
        }
        {
            let segments = [seg(b" ")];
            let result = win.print(&segments, opts);
            assert_eq!(1, result.col);
            assert_eq!(0, result.row);
            assert!(!result.overflow);
        }
        {
            let segments = [seg(b" a")];
            let result = win.print(&segments, opts);
            assert_eq!(2, result.col);
            assert_eq!(0, result.row);
            assert!(!result.overflow);
        }
        {
            let segments = [seg(b"a b")];
            let result = win.print(&segments, opts);
            assert_eq!(3, result.col);
            assert_eq!(0, result.row);
            assert!(!result.overflow);
        }
        {
            let segments = [seg(b"a b c")];
            let result = win.print(&segments, opts);
            assert_eq!(1, result.col);
            assert_eq!(1, result.row);
            assert!(!result.overflow);
        }
        {
            let segments = [seg(b"hello")];
            let result = win.print(&segments, opts);
            assert_eq!(1, result.col);
            assert_eq!(1, result.row);
            assert!(!result.overflow);
        }
        {
            let segments = [seg(b"hi tim")];
            let result = win.print(&segments, opts);
            assert_eq!(3, result.col);
            assert_eq!(1, result.row);
            assert!(!result.overflow);
        }
        {
            let segments = [seg(b"hello tim")];
            let result = win.print(&segments, opts);
            assert_eq!(0, result.col);
            assert_eq!(2, result.row);
            assert!(result.overflow);
        }
        {
            let segments = [seg(b"hello ti")];
            let result = win.print(&segments, opts);
            assert_eq!(0, result.col);
            assert_eq!(2, result.row);
            assert!(!result.overflow);
        }
        {
            let segments = [seg(b"h"), seg(b"e")];
            let result = win.print(&segments, opts);
            assert_eq!(2, result.col);
            assert_eq!(0, result.row);
            assert!(!result.overflow);
        }
        {
            let segments = [seg(b"h"), seg(b"e"), seg(b"l"), seg(b"l"), seg(b"o")];
            let result = win.print(&segments, opts);
            assert_eq!(1, result.col);
            assert_eq!(1, result.row);
            assert!(!result.overflow);
        }
        {
            let segments = [seg(b"he\n")];
            let result = win.print(&segments, opts);
            assert_eq!(0, result.col);
            assert_eq!(1, result.row);
            assert!(!result.overflow);
        }
        {
            let segments = [seg(b"he\n\n")];
            let result = win.print(&segments, opts);
            assert_eq!(0, result.col);
            assert_eq!(2, result.row);
            assert!(!result.overflow);
        }
        {
            let segments = [seg(b"not now")];
            let result = win.print(&segments, opts);
            assert_eq!(3, result.col);
            assert_eq!(1, result.row);
            assert!(!result.overflow);
        }
        {
            let segments = [seg(b"note now")];
            let result = win.print(&segments, opts);
            assert_eq!(3, result.col);
            assert_eq!(1, result.row);
            assert!(!result.overflow);
        }
        {
            let segments = [seg(b"note"), seg(b" now")];
            let result = win.print(&segments, opts);
            assert_eq!(3, result.col);
            assert_eq!(1, result.row);
            assert!(!result.overflow);
        }
        {
            let segments = [seg(b"note "), seg(b"now")];
            let result = win.print(&segments, opts);
            assert_eq!(3, result.col);
            assert_eq!(1, result.row);
            assert!(!result.overflow);
        }
    }

    #[test]
    fn ref_all_decls() {
        // Zig `test "refAllDecls" { std.testing.refAllDecls(@This()); }` — a smoke test.
        // Constructs a `Window` and touches its public decls.
        let mut screen = Screen::default();
        let win = Window {
            x_off: 0,
            y_off: 0,
            parent_x_off: 0,
            parent_y_off: 0,
            width: 0,
            height: 0,
            screen: &mut screen as *mut _,
        };
        let _ = win.child(ChildOptions::default());
        let _ = Window::has_mouse(win, None);
    }
}
