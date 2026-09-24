//! rustty-vaxis::vxfw::text — the `Text` widget.
//!
//! PORTS: `vaxis/src/vxfw/Text.zig` — a wrapping/aligning text widget over the
//! gwidth/grapheme surface, plus its `LineIterator` (hard line breaks) and
//! `SoftwrapIterator` (word wrapping). `cli +boo` uses it for the too-small-screen
//! warning. All 10 reference tests are ported below.

use std::os::raw::c_void;

use super::{DrawContext, Error, Size, Surface, Widget};
use crate::cell::{Cell, Character, Style};

/// `text_align: enum { left, center, right } = .left,`
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TextAlign {
    Left,
    Center,
    Right,
}

/// `overflow: enum { ellipsis, clip } = .ellipsis,`
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Overflow {
    Ellipsis,
    Clip,
}

/// `width_basis: enum { parent, longest_line } = .longest_line,`
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum WidthBasis {
    Parent,
    LongestLine,
}

/// The `Text` struct (`const Text = @This();` — the file's fields).
#[derive(Clone, Copy)]
pub struct Text<'a> {
    pub text: &'a [u8],
    pub style: Style,
    pub text_align: TextAlign,
    pub softwrap: bool,
    pub overflow: Overflow,
    pub width_basis: WidthBasis,
}

impl Default for Text<'_> {
    // The Zig field defaults (`text` has no Zig default; `b""` is a convenient,
    // harmless Rust `Default` so callers may use `..Text::default()`).
    fn default() -> Self {
        Text {
            text: b"",
            style: Style::default(),
            text_align: TextAlign::Left,
            softwrap: true,
            overflow: Overflow::Ellipsis,
            width_basis: WidthBasis::LongestLine,
        }
    }
}

impl<'a> Text<'a> {
    pub fn widget(&self) -> Widget<'a> {
        Widget {
            userdata: self as *const Text<'a> as *mut c_void,
            capture_handler: None,
            event_handler: None,
            draw_fn: Text::type_erased_draw_fn,
        }
    }

    fn type_erased_draw_fn(ptr: *mut c_void, ctx: DrawContext) -> Result<Surface<'a>, Error> {
        // SAFETY: `ptr` is the `*const Text` stored by `widget()`; read-only.
        let self_ref: &Text<'a> = unsafe { &*(ptr as *const Text<'a>) };
        self_ref.draw(ctx)
    }

    pub fn draw(&self, ctx: DrawContext) -> Result<Surface<'a>, Error> {
        if ctx.max.width.is_some() && ctx.max.width.unwrap() == 0 {
            return Ok(Surface {
                size: ctx.min,
                widget: self.widget(),
                cursor: None,
                buffer: super::empty_cells(),
                children: super::empty_children(),
            });
        }
        let container_size = self.find_container_size(ctx);

        // Create a surface of target width and max height. We'll trim the result after drawing
        let surface = Surface::init(ctx.arena, self.widget(), container_size);
        let base_style = Style {
            fg: self.style.fg,
            bg: self.style.bg,
            reverse: self.style.reverse,
            ..Style::default()
        };
        let base = Cell {
            style: base_style,
            ..Cell::default()
        };
        // @memset(surface.buffer, base);
        {
            let n = surface.buffer.len();
            // SAFETY: `surface.buffer` is `n` initialized cells from the arena.
            let ptr = surface.buffer.cast::<Cell<'a>>();
            for i in 0..n {
                unsafe { ptr.add(i).write(base) };
            }
        }

        let mut row: u16 = 0;
        if self.softwrap {
            let mut iter = SoftwrapIterator::init(self.text, ctx);
            while let Some(line) = iter.next() {
                if row >= container_size.height {
                    break;
                }
                let mut col: u16 = match self.text_align {
                    TextAlign::Left => 0,
                    TextAlign::Center => (container_size.width - line.width) / 2,
                    TextAlign::Right => container_size.width - line.width,
                };
                let mut char_iter = ctx.grapheme_iterator(line.bytes);
                while let Some(char) = char_iter.next() {
                    let grapheme = char.bytes(line.bytes);
                    if grapheme == b"\t" {
                        for i in 0..8u16 {
                            surface.write_cell(
                                col + i,
                                row,
                                Cell {
                                    char: Character {
                                        grapheme: b" ",
                                        width: 1,
                                    },
                                    style: self.style,
                                    ..Cell::default()
                                },
                            );
                        }
                        col += 8;
                        continue;
                    }
                    let grapheme_width = ctx.string_width(grapheme) as u8;
                    surface.write_cell(
                        col,
                        row,
                        Cell {
                            char: Character {
                                grapheme,
                                width: grapheme_width,
                            },
                            style: self.style,
                            ..Cell::default()
                        },
                    );
                    col += grapheme_width as u16;
                }
                row += 1;
            }
        } else {
            let mut line_iter = LineIterator {
                buf: self.text,
                index: 0,
            };
            while let Some(line) = line_iter.next() {
                if row >= container_size.height {
                    break;
                }
                // \t is default 1 wide. We add 7x the count of tab characters to get the full width
                let line_width = ctx.string_width(line) + 7 * count(line, b'\t');
                let resolved_line_width = (container_size.width as usize).min(line_width) as u16;
                let mut col: u16 = match self.text_align {
                    TextAlign::Left => 0,
                    TextAlign::Center => (container_size.width - resolved_line_width) / 2,
                    TextAlign::Right => container_size.width - resolved_line_width,
                };
                let mut char_iter = ctx.grapheme_iterator(line);
                while let Some(char) = char_iter.next() {
                    if col >= container_size.width {
                        break;
                    }
                    let grapheme = char.bytes(line);
                    let grapheme_width = ctx.string_width(grapheme) as u8;

                    if col + grapheme_width as u16 >= container_size.width
                        && line_width > container_size.width as usize
                        && self.overflow == Overflow::Ellipsis
                    {
                        surface.write_cell(
                            col,
                            row,
                            Cell {
                                char: Character {
                                    grapheme: "…".as_bytes(),
                                    width: 1,
                                },
                                style: self.style,
                                ..Cell::default()
                            },
                        );
                        col = container_size.width;
                    } else {
                        surface.write_cell(
                            col,
                            row,
                            Cell {
                                char: Character {
                                    grapheme,
                                    width: grapheme_width,
                                },
                                style: self.style,
                                ..Cell::default()
                            },
                        );
                        col += grapheme_width as u16;
                    }
                }
                row += 1;
            }
        }
        Ok(surface.trim_height(row.max(ctx.min.height)))
    }

    /// Determines the container size by finding the widest line in the viewable area
    fn find_container_size(&self, ctx: DrawContext) -> Size {
        let mut row: u16 = 0;
        let mut max_width: u16 = ctx.min.width;
        if self.softwrap {
            let mut iter = SoftwrapIterator::init(self.text, ctx);
            while let Some(line) = iter.next() {
                if ctx.max.outside_height(row) {
                    break;
                }
                max_width = max_width.max(line.width);
                row += 1;
            }
        } else {
            let mut line_iter = LineIterator {
                buf: self.text,
                index: 0,
            };
            while let Some(line) = line_iter.next() {
                if ctx.max.outside_height(row) {
                    break;
                }
                let line_width: u16 = ctx.string_width(line) as u16;
                let resolved_line_width = if let Some(max) = ctx.max.width {
                    max.min(line_width)
                } else {
                    line_width
                };
                max_width = max_width.max(resolved_line_width);
                row += 1;
            }
        }
        let result_width = match self.width_basis {
            WidthBasis::LongestLine => {
                if let Some(max) = ctx.max.width {
                    max.min(max_width)
                } else {
                    max_width
                }
            }
            WidthBasis::Parent => {
                assert!(ctx.max.width.is_some());
                ctx.max.width.unwrap()
            }
        };
        Size {
            width: result_width,
            height: row.max(ctx.min.height),
        }
    }
}

/// Iterates a slice of bytes by linebreaks. Lines are split by '\r', '\n', or '\r\n'
pub struct LineIterator<'a> {
    pub buf: &'a [u8],
    pub index: usize,
}

impl<'a> LineIterator<'a> {
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Option<&'a [u8]> {
        if self.index >= self.buf.len() {
            return None;
        }

        let start = self.index;
        let Some(end) = index_of_any_pos(self.buf, self.index, b"\r\n") else {
            self.index = self.buf.len();
            return Some(&self.buf[start..]);
        };

        self.index = end;
        self.consume_cr();
        self.consume_lf();
        Some(&self.buf[start..end])
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

pub struct SoftwrapIterator<'a> {
    pub ctx: DrawContext,
    pub line: &'a [u8],
    pub index: usize,
    pub hard_iter: LineIterator<'a>,
}

/// `pub const Line = struct { width: u16, bytes: []const u8 };`
pub struct Line<'a> {
    pub width: u16,
    pub bytes: &'a [u8],
}

const SOFT_BREAKS: &[u8] = b" \t";

impl<'a> SoftwrapIterator<'a> {
    fn init(buf: &'a [u8], ctx: DrawContext) -> SoftwrapIterator<'a> {
        SoftwrapIterator {
            ctx,
            line: b"",
            index: 0,
            hard_iter: LineIterator { buf, index: 0 },
        }
    }

    #[allow(clippy::should_implement_trait)]
    fn next(&mut self) -> Option<Line<'a>> {
        // Advance the hard iterator
        if self.index == self.line.len() {
            self.line = self.hard_iter.next()?;
            self.line = trim_right(self.line, b" \t");
            self.index = 0;
        }

        let start = self.index;
        let mut cur_width: u16 = 0;
        while self.index < self.line.len() {
            let idx = self.next_wrap();
            let word = &self.line[self.index..idx];
            let next_width = self.ctx.string_width(word);

            if let Some(max) = self.ctx.max.width {
                if cur_width as usize + next_width > max as usize {
                    // Trim the word to see if it can fit on a line by itself
                    //
                    // PORT NOTE: reference defect (R13, UD265). libvaxis 0.6.0 has
                    //     const trimmed = std.mem.trimEnd(u8, word, " \t");
                    // where 0.5.1 had `std.mem.trimLeft` (Zig 0.15 renamed `trimLeft` /
                    // `trimRight` to `trimStart` / `trimEnd`, and both became `trimEnd`
                    // here). `word` is the separators BEFORE the next word plus the word
                    // (`nextWrap` stops at the first separator after it), so it never ends
                    // in whitespace: `trimEnd` removes nothing, `trimmed_bytes` is always
                    // 0, and a word that fits a line by itself once its leading space is
                    // dropped is judged too long and split mid-word. MEASURED (Zig 0.16.0,
                    // libvaxis 0.6.0, max width 5): "ab hello" wraps as "ab he" / "llo"
                    // instead of "ab" / "hello". The comment above and the byte count
                    // below both describe removing the LEADING separators, so the port keeps
                    // trimming the start.
                    let trimmed = trim_left(word, b" \t");
                    let trimmed_bytes = word.len() - trimmed.len();
                    // The number of bytes we trimmed is equal to the reduction in length
                    let trimmed_width = next_width - trimmed_bytes;
                    if trimmed_width > max as usize {
                        // Won't fit on line by itself, so fit as much on this line as we can
                        let mut iter = self.ctx.grapheme_iterator(word);
                        while let Some(item) = iter.next() {
                            let grapheme = item.bytes(word);
                            let w = self.ctx.string_width(grapheme);
                            if cur_width as usize + w > max as usize {
                                let end = self.index;
                                return Some(Line {
                                    width: cur_width,
                                    bytes: &self.line[start..end],
                                });
                            }
                            cur_width += w as u16;
                            self.index += grapheme.len();
                        }
                    }
                    // We are softwrapping, advance index to the start of the next word
                    let end = self.index;
                    self.index = index_of_none_pos(self.line, self.index, SOFT_BREAKS)
                        .unwrap_or(self.line.len());
                    return Some(Line {
                        width: cur_width,
                        bytes: &self.line[start..end],
                    });
                }
            }

            self.index = idx;
            cur_width += next_width as u16;
        }
        Some(Line {
            width: cur_width,
            bytes: &self.line[start..],
        })
    }

    /// Determines the index of the end of the next word
    fn next_wrap(&self) -> usize {
        // Find the first linear whitespace char
        let Some(start_pos) = index_of_none_pos(self.line, self.index, SOFT_BREAKS) else {
            return self.line.len();
        };
        if let Some(idx) = index_of_any_pos(self.line, start_pos, SOFT_BREAKS) {
            return idx;
        }
        self.line.len()
    }
}

// ---------------------------------------------------------------------------
// `std.mem` helpers used by the iterators (byte-slice, never lossy `String`).
// ---------------------------------------------------------------------------

/// `std.mem.indexOfAnyPos(u8, buf, start, set)` — first index >= start whose byte
/// is IN `set`.
fn index_of_any_pos(buf: &[u8], start: usize, set: &[u8]) -> Option<usize> {
    (start..buf.len()).find(|&i| set.contains(&buf[i]))
}

/// `std.mem.indexOfNonePos(u8, buf, start, set)` — first index >= start whose byte
/// is NOT in `set`.
fn index_of_none_pos(buf: &[u8], start: usize, set: &[u8]) -> Option<usize> {
    (start..buf.len()).find(|&i| !set.contains(&buf[i]))
}

/// `std.mem.trimEnd(u8, buf, set)` (0.5.1: `trimRight`)
fn trim_right<'b>(buf: &'b [u8], set: &[u8]) -> &'b [u8] {
    let mut end = buf.len();
    while end > 0 && set.contains(&buf[end - 1]) {
        end -= 1;
    }
    &buf[..end]
}

/// `std.mem.trimStart(u8, buf, set)` (0.5.1: `trimLeft`; see UD265)
fn trim_left<'b>(buf: &'b [u8], set: &[u8]) -> &'b [u8] {
    let mut start = 0;
    while start < buf.len() && set.contains(&buf[start]) {
        start += 1;
    }
    &buf[start..]
}

/// `std.mem.count(u8, buf, needle_byte)`
fn count(buf: &[u8], needle: u8) -> usize {
    buf.iter().filter(|&&b| b == needle).count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gwidth::Method;
    use crate::vxfw::{MaxSize, VxfwArena};

    fn ctx_with(arena: &VxfwArena, max: MaxSize) -> DrawContext {
        DrawContext {
            arena,
            min: Size {
                width: 0,
                height: 0,
            },
            max,
            cell_size: Size {
                width: 10,
                height: 20,
            },
        }
    }

    #[test]
    fn softwrap_lf_breaks() {
        DrawContext::init(Method::Unicode);
        let arena = VxfwArena::new();
        let ctx = ctx_with(
            &arena,
            MaxSize {
                width: Some(20),
                height: Some(10),
            },
        );
        let mut iter = SoftwrapIterator::init(b"Hello, \n world", ctx);
        let first = iter.next();
        assert!(first.is_some());
        assert_eq!(first.as_ref().unwrap().bytes, b"Hello,");
        assert_eq!(first.as_ref().unwrap().width, 6);

        let second = iter.next();
        assert!(second.is_some());
        assert_eq!(second.as_ref().unwrap().bytes, b" world");
        assert_eq!(second.as_ref().unwrap().width, 6);

        assert!(iter.next().is_none());
    }

    #[test]
    fn softwrap_soft_breaks_that_fit() {
        DrawContext::init(Method::Unicode);
        let arena = VxfwArena::new();
        let ctx = ctx_with(
            &arena,
            MaxSize {
                width: Some(6),
                height: Some(10),
            },
        );
        let mut iter = SoftwrapIterator::init(b"Hello, \nworld", ctx);
        let first = iter.next();
        assert_eq!(first.as_ref().unwrap().bytes, b"Hello,");
        assert_eq!(first.as_ref().unwrap().width, 6);

        let second = iter.next();
        assert_eq!(second.as_ref().unwrap().bytes, b"world");
        assert_eq!(second.as_ref().unwrap().width, 5);

        assert!(iter.next().is_none());
    }

    #[test]
    fn softwrap_soft_breaks_longer_than_width() {
        DrawContext::init(Method::Unicode);
        let arena = VxfwArena::new();
        let ctx = ctx_with(
            &arena,
            MaxSize {
                width: Some(6),
                height: Some(10),
            },
        );
        let mut iter = SoftwrapIterator::init(b"very-long-word \nworld", ctx);
        let first = iter.next();
        assert_eq!(first.as_ref().unwrap().bytes, b"very-l");
        assert_eq!(first.as_ref().unwrap().width, 6);

        let second = iter.next();
        assert_eq!(second.as_ref().unwrap().bytes, b"ong-wo");
        assert_eq!(second.as_ref().unwrap().width, 6);

        let third = iter.next();
        assert_eq!(third.as_ref().unwrap().bytes, b"rd");
        assert_eq!(third.as_ref().unwrap().width, 2);

        let fourth = iter.next();
        assert_eq!(fourth.as_ref().unwrap().bytes, b"world");
        assert_eq!(fourth.as_ref().unwrap().width, 5);

        assert!(iter.next().is_none());
    }

    #[test]
    fn softwrap_soft_breaks_with_leading_spaces() {
        DrawContext::init(Method::Unicode);
        let arena = VxfwArena::new();
        let ctx = ctx_with(
            &arena,
            MaxSize {
                width: Some(6),
                height: Some(10),
            },
        );
        let mut iter = SoftwrapIterator::init(b"Hello,        \n world", ctx);
        let first = iter.next();
        assert_eq!(first.as_ref().unwrap().bytes, b"Hello,");
        assert_eq!(first.as_ref().unwrap().width, 6);

        let second = iter.next();
        assert_eq!(second.as_ref().unwrap().bytes, b" world");
        assert_eq!(second.as_ref().unwrap().width, 6);

        assert!(iter.next().is_none());
    }

    /// UD265 (R13): a word that fits a line by itself once its leading space is dropped
    /// wraps whole. libvaxis 0.6.0 (`trimEnd` where `trimLeft` was meant) splits it:
    /// "ab he" / "llo" (MEASURED with Zig 0.16.0).
    #[test]
    fn softwrap_word_that_fits_alone_after_a_space_wraps_whole() {
        DrawContext::init(Method::Unicode);
        let arena = VxfwArena::new();
        let ctx = ctx_with(
            &arena,
            MaxSize {
                width: Some(5),
                height: Some(10),
            },
        );
        let mut iter = SoftwrapIterator::init(b"ab hello", ctx);
        let first = iter.next();
        assert_eq!(first.as_ref().unwrap().bytes, b"ab");
        assert_eq!(first.as_ref().unwrap().width, 2);

        let second = iter.next();
        assert_eq!(second.as_ref().unwrap().bytes, b"hello");
        assert_eq!(second.as_ref().unwrap().width, 5);

        assert!(iter.next().is_none());
    }

    #[test]
    fn lineiterator_lf_breaks() {
        let input = b"Hello, \n world";
        let mut iter = LineIterator {
            buf: input,
            index: 0,
        };
        assert_eq!(iter.next().unwrap(), b"Hello, ");
        assert_eq!(iter.next().unwrap(), b" world");
        assert!(iter.next().is_none());
    }

    #[test]
    fn lineiterator_cr_breaks() {
        let input = b"Hello, \r world";
        let mut iter = LineIterator {
            buf: input,
            index: 0,
        };
        assert_eq!(iter.next().unwrap(), b"Hello, ");
        assert_eq!(iter.next().unwrap(), b" world");
        assert!(iter.next().is_none());
    }

    #[test]
    fn lineiterator_crlf_breaks() {
        let input = b"Hello, \r\n world";
        let mut iter = LineIterator {
            buf: input,
            index: 0,
        };
        assert_eq!(iter.next().unwrap(), b"Hello, ");
        assert_eq!(iter.next().unwrap(), b" world");
        assert!(iter.next().is_none());
    }

    #[test]
    fn lineiterator_crlf_breaks_with_empty_line() {
        let input = b"Hello, \r\n\r\n world";
        let mut iter = LineIterator {
            buf: input,
            index: 0,
        };
        assert_eq!(iter.next().unwrap(), b"Hello, ");
        assert_eq!(iter.next().unwrap(), b"");
        assert_eq!(iter.next().unwrap(), b" world");
        assert!(iter.next().is_none());
    }

    #[test]
    fn text_doctest() {
        let arena = VxfwArena::new();
        DrawContext::init(Method::Unicode);
        let ctx = ctx_with(
            &arena,
            MaxSize {
                width: Some(7),
                height: Some(2),
            },
        );

        let mut text = Text {
            text: b"Hello, world",
            ..Text::default()
        };
        {
            // Text softwraps by default
            let text_widget = text.widget();
            let surface = text_widget.draw(ctx).unwrap();
            assert_eq!(
                surface.size,
                Size {
                    width: 6,
                    height: 2
                }
            );
        }

        {
            text.softwrap = false;
            text.overflow = Overflow::Ellipsis;
            let text_widget = text.widget();
            let surface = text_widget.draw(ctx).unwrap();
            assert_eq!(
                surface.size,
                Size {
                    width: 7,
                    height: 1
                }
            );
            // The last character will be an ellipsis
            let last = surface.read_cell(surface.size.width as usize - 1, 0);
            assert_eq!(last.char.grapheme, "…".as_bytes());
        }
    }
}
