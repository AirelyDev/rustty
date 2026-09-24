//! Port of `libvaxis` `widgets/TextInput.zig` (0.6.0: 757 LOC, 16 test; the readline-style
//! word motions and their 13 tests are new in 0.6.0) — the single-line
//! text-input widget (the search box used by `list_themes`), built over a **gap buffer**
//! of raw UTF-8 bytes (VAXIS_C4B_DESIGN.md).
//!
//! It is **self-contained over [`Window`]** — it never touches `Vaxis`, `Loop`, or the
//! tty — so there is **no R5 seam**; every method ports LIVE. It is a **pure leaf** →
//! R6 Miri is the primary gate over the gap-buffer / raw-ptr copy paths.
//!
//! PORT NOTE (allocator, crate model): Zig's `Buffer` takes an `std.mem.Allocator` and
//! its `grow`/`insertSliceAtCursor`/`toOwnedSlice` return `std.mem.Allocator.Error!void`.
//! Rust `Vec` owns/frees its own storage and its allocation is infallible (aborts on OOM),
//! so the allocator field is DROPPED and those methods become non-`Result` — matching the
//! crate convention (`Screen`, `InternalScreen`). The `try` at every call site drops.
//! `init`/`deinit`/`clearAndFree` keep their allocator-free shims for API parity.
//!
//! PORT NOTE (zero-filled gap): `grow` builds a fresh `vec![0u8; new_size]`; the gap region
//! is never read (Zig invariant "the gap is never within a grapheme"), so zero-filling it is
//! a benign, Miri-safe divergence from Zig's `undefined` gap bytes (cf. the `GraphemeCache`
//! PORT NOTE).
//!
//! PORT NOTE (grapheme bytes): grapheme text stays `&[u8]`/`Vec<u8>` throughout, NEVER a
//! lossy `String` (zig-byteslice-to-rust-rawbytes).
//!
//! PORT NOTE (`&self` demotions): `sliceToCursor` (Zig `*TextInput`, read-only) and
//! `byteOffsetToCursor` (Zig by-value `TextInput`) demote to `&self` — they never mutate.

use crate::cell::{Cell, Character, Style};
use crate::key::{self, Key, Modifiers};
use crate::unicode::grapheme_iterator;
use crate::uucode;
use crate::uucode::types::GeneralCategory;
use crate::window::Window;

/// The events that this widget handles
///
/// PORT NOTE: Zig `union(enum) { key_press: Key }`. `Event<'a>` carries `'a` because
/// `Key<'a>` borrows its `text`. This is the widget-local event, NOT the crate-level
/// `event::Event`.
pub enum Event<'a> {
    KeyPress(Key<'a>),
}

const ELLIPSIS: Character<'static> = Character {
    grapheme: "…".as_bytes(),
    width: 1,
};

/// Rust-only: the `TextInput` fields `draw_with_style` leaves, returned so a
/// caller can read them while the widget stays borrowed by the screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Drawn {
    pub draw_offset: u16,
    pub prev_cursor_col: u16,
    pub prev_cursor_idx: u16,
}

/// The single-line text-input widget.
pub struct TextInput {
    // Index of our cursor
    pub buf: Buffer,

    /// the number of graphemes to skip when drawing. Used for horizontal scrolling
    pub draw_offset: u16,
    /// the column we placed the cursor the last time we drew
    pub prev_cursor_col: u16,
    /// the grapheme index of the cursor the last time we drew
    pub prev_cursor_idx: u16,
    /// approximate distance from an edge before we scroll
    pub scroll_offset: u16,
}

impl TextInput {
    pub fn init() -> TextInput {
        TextInput {
            buf: Buffer::init(),
            draw_offset: 0,
            prev_cursor_col: 0,
            prev_cursor_idx: 0,
            scroll_offset: 4,
        }
    }

    pub fn deinit(&mut self) {
        self.buf.deinit();
    }

    pub fn update(&mut self, event: Event<'_>) {
        match event {
            Event::KeyPress(key) => {
                if key.matches(key::backspace, Modifiers::default()) {
                    self.delete_before_cursor();
                } else if key.matches(key::delete, Modifiers::default())
                    || key.matches(
                        b'd' as u32,
                        Modifiers {
                            ctrl: true,
                            ..Default::default()
                        },
                    )
                {
                    self.delete_after_cursor();
                } else if key.matches(key::left, Modifiers::default())
                    || key.matches(
                        b'b' as u32,
                        Modifiers {
                            ctrl: true,
                            ..Default::default()
                        },
                    )
                {
                    self.cursor_left();
                } else if key.matches(key::right, Modifiers::default())
                    || key.matches(
                        b'f' as u32,
                        Modifiers {
                            ctrl: true,
                            ..Default::default()
                        },
                    )
                {
                    self.cursor_right();
                } else if key.matches(
                    b'a' as u32,
                    Modifiers {
                        ctrl: true,
                        ..Default::default()
                    },
                ) || key.matches(key::home, Modifiers::default())
                {
                    let n = self.buf.first_half().len();
                    self.buf.move_gap_left(n);
                } else if key.matches(
                    b'e' as u32,
                    Modifiers {
                        ctrl: true,
                        ..Default::default()
                    },
                ) || key.matches(key::end, Modifiers::default())
                {
                    let n = self.buf.second_half().len();
                    self.buf.move_gap_right(n);
                } else if key.matches(
                    b'k' as u32,
                    Modifiers {
                        ctrl: true,
                        ..Default::default()
                    },
                ) {
                    self.delete_to_end();
                } else if key.matches(
                    b'u' as u32,
                    Modifiers {
                        ctrl: true,
                        ..Default::default()
                    },
                ) {
                    self.delete_to_start();
                } else if key.matches(
                    b'b' as u32,
                    Modifiers {
                        alt: true,
                        ..Default::default()
                    },
                ) || key.matches(
                    key::left,
                    Modifiers {
                        alt: true,
                        ..Default::default()
                    },
                ) {
                    self.move_backward_wordwise();
                } else if key.matches(
                    b'f' as u32,
                    Modifiers {
                        alt: true,
                        ..Default::default()
                    },
                ) || key.matches(
                    key::right,
                    Modifiers {
                        alt: true,
                        ..Default::default()
                    },
                ) {
                    self.move_forward_wordwise();
                } else if key.matches(
                    key::backspace,
                    Modifiers {
                        alt: true,
                        ..Default::default()
                    },
                ) {
                    self.delete_word_before();
                } else if key.matches(
                    b'w' as u32,
                    Modifiers {
                        ctrl: true,
                        ..Default::default()
                    },
                ) {
                    self.delete_word_before_whitespace();
                } else if key.matches(
                    b'd' as u32,
                    Modifiers {
                        alt: true,
                        ..Default::default()
                    },
                ) {
                    self.delete_word_after();
                } else if let Some(text) = key.text {
                    self.insert_slice_at_cursor(text);
                }
            }
        }
    }

    /// insert text at the cursor position
    pub fn insert_slice_at_cursor(&mut self, data: &[u8]) {
        let mut iter = grapheme_iterator(data);
        while let Some(text) = iter.next() {
            self.buf.insert_slice_at_cursor(text.bytes(data));
        }
    }

    pub fn slice_to_cursor<'b>(&self, buf: &'b mut [u8]) -> &'b [u8] {
        assert!(buf.len() >= self.buf.cursor);
        buf[0..self.buf.cursor].copy_from_slice(self.buf.first_half());
        &buf[0..self.buf.cursor]
    }

    /// calculates the display width from the draw_offset to the cursor
    pub fn width_to_cursor(&self, win: Window<'_>) -> u16 {
        let mut width: u16 = 0;
        let first_half = self.buf.first_half();
        let mut first_iter = grapheme_iterator(first_half);
        let mut i: usize = 0;
        while let Some(grapheme) = first_iter.next() {
            // defer i += 1
            if i < self.draw_offset as usize {
                i += 1;
                continue;
            }
            let g = grapheme.bytes(first_half);
            width += win.gwidth(g);
            i += 1;
        }
        width
    }

    pub fn cursor_left(&mut self) {
        // We need to find the size of the last grapheme in the first half
        let mut len: usize = 0;
        {
            let first_half = self.buf.first_half();
            let mut iter = grapheme_iterator(first_half);
            while let Some(grapheme) = iter.next() {
                len = grapheme.len;
            }
        }
        self.buf.move_gap_left(len);
    }

    pub fn cursor_right(&mut self) {
        let g_len = {
            let second_half = self.buf.second_half();
            let mut iter = grapheme_iterator(second_half);
            match iter.next() {
                Some(grapheme) => grapheme.len,
                None => return,
            }
        };
        self.buf.move_gap_right(g_len);
    }

    pub fn graphemes_before_cursor(&self) -> u16 {
        let first_half = self.buf.first_half();
        let mut first_iter = grapheme_iterator(first_half);
        let mut i: u16 = 0;
        while first_iter.next().is_some() {
            i += 1;
        }
        i
    }

    pub fn draw<'s>(&'s mut self, win: Window<'s>) {
        let _ = self.draw_with_style(win, Style::default());
    }

    // PORT NOTE (draw record-then-replay, VAXIS_C4B_DESIGN §6): `win.write_cell` stores the
    // grapheme slice BY REFERENCE — `Screen<'a>` keeps `Cell<'a>` with `grapheme: &'a [u8]`,
    // and `Window<'a>` is invariant in `'a`, so the widget must be borrowed for the screen's
    // lifetime (`&'s mut self`, `Window<'s>`). Zig's single loop interleaves `writeCell(g)`
    // (which lends `&'s self.buf` into the screen) with `self.prev_cursor_col = col`
    // (`&mut self`); once a `&'s` shared borrow of `self` escapes into the screen the
    // `&mut self` is dead, so the two cannot interleave in Rust. Resolution: run the *actual*
    // Zig decision loop ONCE (Pass A) recording each draw op into a scratch `Vec<Op>` and
    // completing ALL `self` mutation there, then replay (Pass B) with `self` borrowed shared
    // for `'s`. Behaviorally identical (same cells, order, `prev_cursor_*`/`draw_offset`,
    // `showCursor`); the single decision loop is written once (Pass A). Cost: one small
    // `Vec<Op>` alloc per `draw` — a benign divergence (Zig streams; the tests do not
    // exercise `draw`).
    //
    // PORT NOTE (Rust-only return value, parity S69-F1): Zig's `drawWithStyle`
    // returns `void`, and its caller reads the updated fields afterwards. Here
    // `self` stays borrowed for the screen's `'s`, so a caller cannot read
    // them once this returns; the `Drawn` copy of the three fields Pass A
    // leaves is how a caller that draws a per-frame copy (rustty-cli's
    // `+list-themes` search box) carries them back. It changes nothing drawn.
    pub fn draw_with_style<'s>(&'s mut self, win: Window<'s>, style: Style) -> Drawn {
        // A recorded draw op: an absolute `start/len` range into `self.buf.buffer` (so replay
        // needs no first-/second-half bookkeeping), or the ellipsis marker.
        enum Op {
            Cell {
                col: u16,
                start: usize,
                len: usize,
                width: u8,
            },
            Ellipsis {
                col: u16,
            },
        }

        // ---- Pass A: mutate self, read `buf` only (no `'s` escape) ----
        let cursor_idx = self.graphemes_before_cursor();
        if cursor_idx < self.draw_offset {
            self.draw_offset = cursor_idx;
        }
        if win.width == 0 {
            return self.drawn();
        }
        loop {
            let width = self.width_to_cursor(win);
            if width >= win.width {
                self.draw_offset = self.draw_offset.saturating_add(width - win.width + 1);
                continue;
            } else {
                break;
            }
        }

        self.prev_cursor_idx = cursor_idx;
        let mut prev_cursor_col: u16 = 0;

        let draw_offset = self.draw_offset;
        let mut ops: Vec<Op> = Vec::new();

        // assumption!! the gap is never within a grapheme
        // one way to _ensure_ this is to move the gap... but that's a cost we probably don't want to pay.
        let first_half = self.buf.first_half();
        let second_half = self.buf.second_half();
        // absolute base of the second half within `self.buf.buffer`
        let sh_base = self.buf.cursor + self.buf.gap_size;
        let mut col: u16 = 0;
        let mut i: u16 = 0;
        {
            let mut first_iter = grapheme_iterator(first_half);
            while let Some(grapheme) = first_iter.next() {
                if i < draw_offset {
                    i += 1;
                    continue;
                }
                let g = grapheme.bytes(first_half);
                let w = win.gwidth(g);
                if col + w >= win.width {
                    ops.push(Op::Ellipsis { col: win.width - 1 });
                    break;
                }
                ops.push(Op::Cell {
                    col,
                    start: grapheme.start,
                    len: grapheme.len,
                    width: w as u8,
                });
                col += w;
                i += 1;
                if i == cursor_idx {
                    prev_cursor_col = col;
                }
            }
        }
        {
            let mut second_iter = grapheme_iterator(second_half);
            while let Some(grapheme) = second_iter.next() {
                if i < draw_offset {
                    i += 1;
                    continue;
                }
                let g = grapheme.bytes(second_half);
                let w = win.gwidth(g);
                if col + w > win.width {
                    ops.push(Op::Ellipsis { col: win.width - 1 });
                    break;
                }
                ops.push(Op::Cell {
                    col,
                    start: sh_base + grapheme.start,
                    len: grapheme.len,
                    width: w as u8,
                });
                col += w;
                i += 1;
                if i == cursor_idx {
                    prev_cursor_col = col;
                }
            }
        }
        if draw_offset > 0 {
            ops.push(Op::Ellipsis { col: 0 });
        }
        self.prev_cursor_col = prev_cursor_col;
        let drawn = self.drawn();

        // ---- Pass B: replay, read self shared for `'s` ----
        let this: &'s TextInput = self;
        for op in &ops {
            match *op {
                Op::Cell {
                    col,
                    start,
                    len,
                    width,
                } => {
                    let g: &'s [u8] = &this.buf.buffer[start..start + len];
                    win.write_cell(
                        col,
                        0,
                        Cell {
                            char: Character { grapheme: g, width },
                            style,
                            ..Default::default()
                        },
                    );
                }
                Op::Ellipsis { col } => {
                    win.write_cell(
                        col,
                        0,
                        Cell {
                            char: ELLIPSIS,
                            style,
                            ..Default::default()
                        },
                    );
                }
            }
        }
        win.show_cursor(this.prev_cursor_col, 0);
        drawn
    }

    /// The fields `draw_with_style` updates (see its Rust-only return value).
    fn drawn(&self) -> Drawn {
        Drawn {
            draw_offset: self.draw_offset,
            prev_cursor_col: self.prev_cursor_col,
            prev_cursor_idx: self.prev_cursor_idx,
        }
    }

    pub fn clear_and_free(&mut self) {
        self.buf.clear_and_free();
        self.reset();
    }

    pub fn clear_retaining_capacity(&mut self) {
        self.buf.clear_retaining_capacity();
        self.reset();
    }

    pub fn to_owned_slice(&mut self) -> Vec<u8> {
        // Zig `defer self.reset()` — grab the owned bytes, then reset last.
        let owned = self.buf.to_owned_slice();
        self.reset();
        owned
    }

    pub fn reset(&mut self) {
        self.draw_offset = 0;
        self.prev_cursor_col = 0;
        self.prev_cursor_idx = 0;
    }

    // returns the number of bytes before the cursor
    pub fn byte_offset_to_cursor(&self) -> usize {
        self.buf.cursor
    }

    pub fn delete_to_end(&mut self) {
        let n = self.buf.second_half().len();
        self.buf.grow_gap_right(n);
    }

    pub fn delete_to_start(&mut self) {
        let n = self.buf.cursor;
        self.buf.grow_gap_left(n);
    }

    pub fn delete_before_cursor(&mut self) {
        // We need to find the size of the last grapheme in the first half
        let mut len: usize = 0;
        {
            let first_half = self.buf.first_half();
            let mut iter = grapheme_iterator(first_half);
            while let Some(grapheme) = iter.next() {
                len = grapheme.len;
            }
        }
        self.buf.grow_gap_left(len);
    }

    pub fn delete_after_cursor(&mut self) {
        let g_len = {
            let second_half = self.buf.second_half();
            let mut iter = grapheme_iterator(second_half);
            match iter.next() {
                Some(grapheme) => grapheme.len,
                None => return,
            }
        };
        self.buf.grow_gap_right(g_len);
    }

    /// Moves the cursor backward by one word using character-class boundaries.
    /// Skips non-word characters, then skips word characters (matching readline backward-word).
    pub fn move_backward_wordwise(&mut self) {
        let i = {
            let first_half = self.buf.first_half();
            let mut i: usize = first_half.len();
            // Skip non-word characters
            while i > 0 {
                let decoded = decode_codepoint_before(first_half, i);
                if is_word_codepoint(decoded.cp) {
                    break;
                }
                i = decoded.start;
            }
            // Skip word characters
            while i > 0 {
                let decoded = decode_codepoint_before(first_half, i);
                if !is_word_codepoint(decoded.cp) {
                    break;
                }
                i = decoded.start;
            }
            i
        };
        let n = self.buf.cursor - i;
        self.buf.move_gap_left(n);
    }

    /// Moves the cursor forward by one word using character-class boundaries.
    /// Skips non-word characters, then skips word characters — landing at the end of the next word
    /// (matching readline forward-word).
    pub fn move_forward_wordwise(&mut self) {
        let i = {
            let second_half = self.buf.second_half();
            let mut i: usize = 0;
            // Skip non-word characters
            while i < second_half.len() {
                let decoded = decode_codepoint_at(second_half, i);
                if is_word_codepoint(decoded.cp) {
                    break;
                }
                i += decoded.len;
            }
            // Skip word characters
            while i < second_half.len() {
                let decoded = decode_codepoint_at(second_half, i);
                if !is_word_codepoint(decoded.cp) {
                    break;
                }
                i += decoded.len;
            }
            i
        };
        self.buf.move_gap_right(i);
    }

    /// Deletes the word before the cursor using character-class boundaries
    /// (matching readline backward-kill-word / Alt+Backspace).
    pub fn delete_word_before(&mut self) {
        let pre = self.buf.cursor;
        self.move_backward_wordwise();
        self.buf.grow_gap_right(pre - self.buf.cursor);
    }

    /// Deletes the word before the cursor using whitespace boundaries
    /// (matching readline unix-word-rubout / Ctrl+W).
    pub fn delete_word_before_whitespace(&mut self) {
        let i = {
            let first_half = self.buf.first_half();
            let mut i: usize = first_half.len();
            // Skip trailing whitespace
            while i > 0 {
                let decoded = decode_codepoint_before(first_half, i);
                if !is_whitespace_codepoint(decoded.cp) {
                    break;
                }
                i = decoded.start;
            }
            // Skip non-whitespace
            while i > 0 {
                let decoded = decode_codepoint_before(first_half, i);
                if is_whitespace_codepoint(decoded.cp) {
                    break;
                }
                i = decoded.start;
            }
            i
        };
        let to_delete = self.buf.cursor - i;
        self.buf.move_gap_left(to_delete);
        self.buf.grow_gap_right(to_delete);
    }

    /// Deletes the word after the cursor using character-class boundaries
    /// (matching readline kill-word / Alt+D).
    pub fn delete_word_after(&mut self) {
        let i = {
            let second_half = self.buf.second_half();
            let mut i: usize = 0;
            // Skip non-word characters
            while i < second_half.len() {
                let decoded = decode_codepoint_at(second_half, i);
                if is_word_codepoint(decoded.cp) {
                    break;
                }
                i += decoded.len;
            }
            // Skip word characters
            while i < second_half.len() {
                let decoded = decode_codepoint_at(second_half, i);
                if !is_word_codepoint(decoded.cp) {
                    break;
                }
                i += decoded.len;
            }
            i
        };
        self.buf.grow_gap_right(i);
    }
}

struct DecodedCodepoint {
    cp: u32,
    start: usize,
    len: usize,
}

/// `std.unicode.utf8ByteSequenceLength(first) catch 1`.
fn utf8_byte_sequence_length(first: u8) -> usize {
    match first {
        0b0000_0000..=0b0111_1111 => 1,
        0b1100_0000..=0b1101_1111 => 2,
        0b1110_0000..=0b1110_1111 => 3,
        0b1111_0000..=0b1111_0111 => 4,
        _ => 1, // error.Utf8InvalidStartByte → `catch 1`
    }
}

/// `std.unicode.utf8Decode(slice)`, with `None` for its error returns.
///
/// PORT NOTE: reference defect (R13, UD264). `std.unicode.utf8Decode` is only defined for
/// a slice that is exactly one encoded code point's length: it is
/// `switch (bytes.len) { 1 => bytes[0], 2 => utf8Decode2(..), 3 => ..., 4 => ..., else =>
/// unreachable }`, and `utf8Decode2`/`3`/`4` begin with `assert(bytes[0]` has that
/// length's lead-byte prefix`)`. The 0.6.0 word-motion helpers do not guarantee either:
/// `decodeCodepointBefore` hands it every byte back to the first non-continuation byte (a
/// run of five or more continuation bytes, or `a` followed by one, is not one code point),
/// and `decodeCodepointAt` caps the slice at the end of the buffer (a lead byte cut short).
/// The buffer holds the key text the parser produced from tty input, which can be invalid
/// UTF-8 (typed or pasted bytes: the parser turns each stray `0x80` into a key whose text
/// is that byte, MEASURED with libvaxis 0.6.0), so Ctrl+W, Alt+B, Alt+F, Alt+D or
/// Alt+Backspace then reach `unreachable` (a panic in Debug/ReleaseSafe, MEASURED with Zig
/// 0.16.0 for the four inputs the UD264 test uses; undefined behaviour in ReleaseFast).
/// Both callers already handle
/// a failed decode (`catch`: treat one byte as the code point), so the port returns
/// `None` for those inputs too, and the callers take that path.
fn utf8_decode(bytes: &[u8]) -> Option<u32> {
    match bytes.len() {
        1 => Some(u32::from(bytes[0])),
        2..=4 => {
            if utf8_byte_sequence_length(bytes[0]) != bytes.len() {
                // the reference's `assert` on the lead byte (UD264)
                return None;
            }
            // `utf8Decode2`/`3`/`4`: expected continuation bytes, overlong encodings,
            // surrogate halves (3-byte form) and values above U+10FFFF are errors — the
            // same set `std::str::from_utf8` refuses for one code point of this length.
            let s = std::str::from_utf8(bytes).ok()?;
            let mut chars = s.chars();
            let c = chars.next()?;
            debug_assert!(chars.next().is_none());
            Some(c as u32)
        }
        // `else => unreachable` (UD264); `0` cannot occur (both callers slice at least
        // one byte).
        _ => None,
    }
}

fn decode_codepoint_at(bytes: &[u8], start: usize) -> DecodedCodepoint {
    let first = bytes[start];
    let len = utf8_byte_sequence_length(first);
    let capped_len = len.min(bytes.len() - start);
    let slice = &bytes[start..start + capped_len];
    let Some(cp) = utf8_decode(slice) else {
        return DecodedCodepoint {
            cp: u32::from(first),
            start,
            len: 1,
        };
    };
    DecodedCodepoint {
        cp,
        start,
        len: capped_len,
    }
}

fn is_utf8_continuation_byte(c: u8) -> bool {
    (c & 0b1100_0000) == 0b1000_0000
}

fn decode_codepoint_before(bytes: &[u8], end: usize) -> DecodedCodepoint {
    let mut start = end - 1;
    while start > 0 && is_utf8_continuation_byte(bytes[start]) {
        start -= 1;
    }
    let slice = &bytes[start..end];
    let Some(cp) = utf8_decode(slice) else {
        return DecodedCodepoint {
            cp: u32::from(bytes[end - 1]),
            start: end - 1,
            len: 1,
        };
    };
    DecodedCodepoint {
        cp,
        start,
        len: end - start,
    }
}

/// Returns true if the codepoint is a readline-style word constituent.
fn is_word_codepoint(cp: u32) -> bool {
    if cp == u32::from(b'_') {
        return true;
    }
    matches!(
        uucode::get::general_category(cp),
        GeneralCategory::LetterUppercase
            | GeneralCategory::LetterLowercase
            | GeneralCategory::LetterTitlecase
            | GeneralCategory::LetterModifier
            | GeneralCategory::LetterOther
            | GeneralCategory::NumberDecimalDigit
            | GeneralCategory::NumberLetter
            | GeneralCategory::NumberOther
            | GeneralCategory::MarkNonspacing
            | GeneralCategory::MarkSpacingCombining
            | GeneralCategory::MarkEnclosing
            | GeneralCategory::PunctuationConnector
    )
}

fn is_whitespace_codepoint(cp: u32) -> bool {
    match cp {
        0x20 | 0x09 | 0x0A | 0x0D | 0x0b | 0x0c | 0x85 => true,
        _ => matches!(
            uucode::get::general_category(cp),
            GeneralCategory::SeparatorSpace
                | GeneralCategory::SeparatorLine
                | GeneralCategory::SeparatorParagraph
        ),
    }
}

pub struct Buffer {
    // Zig `allocator: std.mem.Allocator` field DROPPED (Rust global alloc; `Vec` owns/frees).
    pub buffer: Vec<u8>,
    pub cursor: usize,
    pub gap_size: usize,
}

impl Buffer {
    pub fn init() -> Buffer {
        Buffer {
            // Zig `buffer = &.{}` (empty slice) → empty `Vec`.
            buffer: Vec::new(),
            cursor: 0,
            gap_size: 0,
        }
    }

    pub fn deinit(&mut self) {
        // Zig `allocator.free(self.buffer)` → drop the storage (`Vec` `Drop` frees).
        self.buffer = Vec::new();
    }

    pub fn first_half(&self) -> &[u8] {
        &self.buffer[0..self.cursor]
    }

    pub fn second_half(&self) -> &[u8] {
        &self.buffer[self.cursor + self.gap_size..]
    }

    pub fn grow(&mut self, n: usize) {
        // Always grow by 512 bytes
        let new_size = self.buffer.len() + n + 512;
        // Allocate the new memory
        let mut new_memory = vec![0u8; new_size];
        // Copy the first half
        new_memory[0..self.cursor].copy_from_slice(self.first_half());
        // Copy the second half
        let second_half = self.second_half();
        let sh_len = second_half.len();
        new_memory[new_size - sh_len..].copy_from_slice(second_half);
        self.buffer = new_memory;
        self.gap_size = new_size - sh_len - self.cursor;
    }

    pub fn insert_slice_at_cursor(&mut self, slice: &[u8]) {
        if slice.is_empty() {
            return;
        }
        if self.gap_size <= slice.len() {
            self.grow(slice.len());
        }
        self.buffer[self.cursor..self.cursor + slice.len()].copy_from_slice(slice);
        self.cursor += slice.len();
        self.gap_size -= slice.len();
    }

    /// Move the gap n bytes to the left
    pub fn move_gap_left(&mut self, n: usize) {
        let new_idx = self.cursor.saturating_sub(n);
        // dst = self.buffer[new_idx + gap_size ..], src = self.buffer[new_idx .. cursor]
        let dst_start = new_idx + self.gap_size;
        let src_start = new_idx;
        let count = self.cursor - new_idx;
        // faithful std.mem.copyForwards: for i in 0..n { dst[i] = src[i] } (forward order; NOT
        // `copy_within`/memmove — a single `*mut u8` walk is the honest, Miri-clean mirror).
        unsafe {
            let p = self.buffer.as_mut_ptr();
            for i in 0..count {
                *p.add(dst_start + i) = *p.add(src_start + i);
            }
        }
        self.cursor = new_idx;
    }

    pub fn move_gap_right(&mut self, n: usize) {
        let new_idx = self.cursor + n;
        // dst = self.buffer[cursor..], src = self.buffer[cursor+gap_size .. new_idx+gap_size]
        let dst_start = self.cursor;
        let src_start = self.cursor + self.gap_size;
        let count = new_idx - self.cursor;
        unsafe {
            let p = self.buffer.as_mut_ptr();
            for i in 0..count {
                *p.add(dst_start + i) = *p.add(src_start + i);
            }
        }
        self.cursor = new_idx;
    }

    /// grow the gap by moving the cursor n bytes to the left
    pub fn grow_gap_left(&mut self, n: usize) {
        // gap grows by the delta
        self.gap_size += n;
        self.cursor = self.cursor.saturating_sub(n);
    }

    /// grow the gap by removing n bytes after the cursor
    pub fn grow_gap_right(&mut self, n: usize) {
        self.gap_size = (self.gap_size + n).min(self.buffer.len() - self.cursor);
    }

    pub fn clear_and_free(&mut self) {
        self.cursor = 0;
        self.buffer = Vec::new();
        self.gap_size = 0;
    }

    pub fn clear_retaining_capacity(&mut self) {
        self.cursor = 0;
        self.gap_size = self.buffer.len();
    }

    pub fn to_owned_slice(&mut self) -> Vec<u8> {
        let first_half = self.first_half();
        let second_half = self.second_half();
        let mut buf = vec![0u8; first_half.len() + second_half.len()];
        buf[0..first_half.len()].copy_from_slice(first_half);
        buf[first_half.len()..].copy_from_slice(second_half);
        self.clear_and_free();
        buf
    }

    pub fn real_length(&self) -> usize {
        self.first_half().len() + self.second_half().len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assertion() {
        let astronaut = "👩‍🚀".as_bytes();
        let astronaut_emoji: Key = Key {
            text: Some(astronaut),
            codepoint: std::str::from_utf8(&astronaut[0..4])
                .unwrap()
                .chars()
                .next()
                .unwrap() as u32,
            shifted_codepoint: None,
            base_layout_codepoint: None,
            mods: Modifiers::default(),
        };
        let mut input = TextInput::init();
        for _ in 0..6 {
            input.update(Event::KeyPress(astronaut_emoji));
        }
        input.deinit();
    }

    #[test]
    fn slice_to_cursor() {
        let mut input = TextInput::init();
        input.insert_slice_at_cursor(b"hello, world");
        input.cursor_left();
        input.cursor_left();
        input.cursor_left();
        let mut buf: [u8; 32] = [0; 32];
        assert_eq!(input.slice_to_cursor(&mut buf), &b"hello, wo"[..]);
        input.cursor_right();
        assert_eq!(input.slice_to_cursor(&mut buf), &b"hello, wor"[..]);
        input.deinit();
    }

    #[test]
    fn buffer() {
        let mut gap_buf = Buffer::init();

        gap_buf.insert_slice_at_cursor(b"abc");
        assert_eq!(gap_buf.first_half(), &b"abc"[..]);
        assert_eq!(gap_buf.second_half(), &b""[..]);

        gap_buf.move_gap_left(1);
        assert_eq!(gap_buf.first_half(), &b"ab"[..]);
        assert_eq!(gap_buf.second_half(), &b"c"[..]);

        gap_buf.insert_slice_at_cursor(b" ");
        assert_eq!(gap_buf.first_half(), &b"ab "[..]);
        assert_eq!(gap_buf.second_half(), &b"c"[..]);

        gap_buf.grow_gap_left(1);
        assert_eq!(gap_buf.first_half(), &b"ab"[..]);
        assert_eq!(gap_buf.second_half(), &b"c"[..]);
        assert_eq!(gap_buf.cursor, 2);

        gap_buf.grow_gap_right(1);
        assert_eq!(gap_buf.first_half(), &b"ab"[..]);
        assert_eq!(gap_buf.second_half(), &b""[..]);
        assert_eq!(gap_buf.cursor, 2);

        gap_buf.deinit();
    }

    // ---- libvaxis 0.6.0 word-motion tests (TextInput.zig:557-730) ----

    #[test]
    fn move_backward_wordwise_stops_at_word_boundary() {
        let mut input = TextInput::init();
        input.insert_slice_at_cursor(b"hello-world");
        // Cursor is at end: "hello-world|"
        input.move_backward_wordwise();
        // Should stop at start of "world": "hello-|world"
        assert_eq!(input.buf.first_half(), b"hello-");
        assert_eq!(input.buf.second_half(), b"world");
        input.move_backward_wordwise();
        // Should skip "-" and stop at start of "hello": "|hello-world"
        assert_eq!(input.buf.first_half(), b"");
        assert_eq!(input.buf.second_half(), b"hello-world");
        input.deinit();
    }

    #[test]
    fn move_forward_wordwise_stops_at_end_of_word() {
        let mut input = TextInput::init();
        input.insert_slice_at_cursor(b"hello-world");
        // Move cursor to start
        let n = input.buf.first_half().len();
        input.buf.move_gap_left(n);
        // Cursor at start: "|hello-world"
        input.move_forward_wordwise();
        // Should stop at end of "hello": "hello|-world"
        assert_eq!(input.buf.first_half(), b"hello");
        assert_eq!(input.buf.second_half(), b"-world");
        input.move_forward_wordwise();
        // Should skip "-" then stop at end of "world": "hello-world|"
        assert_eq!(input.buf.first_half(), b"hello-world");
        assert_eq!(input.buf.second_half(), b"");
        input.deinit();
    }

    #[test]
    fn move_backward_wordwise_with_path_separators() {
        let mut input = TextInput::init();
        input.insert_slice_at_cursor(b"/usr/local/bin");
        input.move_backward_wordwise();
        assert_eq!(input.buf.first_half(), b"/usr/local/");
        input.move_backward_wordwise();
        assert_eq!(input.buf.first_half(), b"/usr/");
        input.move_backward_wordwise();
        assert_eq!(input.buf.first_half(), b"/");
        input.move_backward_wordwise();
        assert_eq!(input.buf.first_half(), b"");
        input.deinit();
    }

    #[test]
    fn move_forward_wordwise_with_dots() {
        let mut input = TextInput::init();
        input.insert_slice_at_cursor(b"foo.bar.baz");
        let n = input.buf.first_half().len();
        input.buf.move_gap_left(n);
        input.move_forward_wordwise();
        // Stops at end of "foo": "foo|.bar.baz"
        assert_eq!(input.buf.first_half(), b"foo");
        input.move_forward_wordwise();
        // Skips "." then stops at end of "bar": "foo.bar|.baz"
        assert_eq!(input.buf.first_half(), b"foo.bar");
        input.move_forward_wordwise();
        // Skips "." then stops at end of "baz": "foo.bar.baz|"
        assert_eq!(input.buf.first_half(), b"foo.bar.baz");
        input.deinit();
    }

    #[test]
    fn delete_word_before_with_hyphens() {
        let mut input = TextInput::init();
        input.insert_slice_at_cursor(b"hello-world");
        input.delete_word_before();
        // Should delete "world" only: "hello-|"
        assert_eq!(input.buf.first_half(), b"hello-");
        assert_eq!(input.buf.second_half(), b"");
        input.delete_word_before();
        // Should skip "-" and delete "hello": "|"
        assert_eq!(input.buf.first_half(), b"");
        input.deinit();
    }

    #[test]
    fn delete_word_before_whitespace_deletes_to_whitespace() {
        let mut input = TextInput::init();
        input.insert_slice_at_cursor(b"hello-world foo.bar");
        input.delete_word_before_whitespace();
        // Should delete "foo.bar" (entire whitespace-delimited word)
        assert_eq!(input.buf.first_half(), b"hello-world ");
        input.delete_word_before_whitespace();
        // Should delete " hello-world"
        assert_eq!(input.buf.first_half(), b"");
        input.deinit();
    }

    #[test]
    fn delete_word_after_with_mixed_punctuation() {
        let mut input = TextInput::init();
        input.insert_slice_at_cursor(b"foo.bar baz");
        let n = input.buf.first_half().len();
        input.buf.move_gap_left(n);
        input.delete_word_after();
        // kill-word: skip non-word (none), skip word "foo" → delete "foo"
        assert_eq!(input.buf.first_half(), b"");
        assert_eq!(input.buf.second_half(), b".bar baz");
        input.delete_word_after();
        // kill-word: skip "." (non-word), skip "bar" (word) → delete ".bar"
        assert_eq!(input.buf.first_half(), b"");
        assert_eq!(input.buf.second_half(), b" baz");
        input.deinit();
    }

    #[test]
    fn word_motion_with_underscores_treats_them_as_word_chars() {
        let mut input = TextInput::init();
        input.insert_slice_at_cursor(b"hello_world-test");
        input.move_backward_wordwise();
        // "test" is a word, should stop before it: "hello_world-|test"
        assert_eq!(input.buf.first_half(), b"hello_world-");
        input.move_backward_wordwise();
        // "hello_world" is one word (underscore is word char): "|hello_world-test"
        assert_eq!(input.buf.first_half(), b"");
        input.deinit();
    }

    #[test]
    fn word_motion_with_non_ascii_text() {
        let mut input = TextInput::init();
        // "café-latte" — the é is multi-byte UTF-8, should not split inside it
        input.insert_slice_at_cursor("café-latte".as_bytes());
        input.move_backward_wordwise();
        assert_eq!(input.buf.first_half(), "café-".as_bytes());
        assert_eq!(input.buf.second_half(), b"latte");
        input.move_backward_wordwise();
        assert_eq!(input.buf.first_half(), b"");

        // Forward from start
        input.move_forward_wordwise();
        // Should stop at end of "café"
        assert_eq!(input.buf.first_half(), b"caf\xc3\xa9");
        assert_eq!(input.buf.second_half(), b"-latte");
        input.deinit();
    }

    #[test]
    fn non_ascii_punctuation_acts_as_a_separator() {
        let mut input = TextInput::init();
        input.insert_slice_at_cursor("hello\u{2014}world".as_bytes());
        input.move_backward_wordwise();
        assert_eq!(input.buf.first_half(), "hello\u{2014}".as_bytes());
        assert_eq!(input.buf.second_half(), b"world");

        let n = input.buf.first_half().len();
        input.buf.move_gap_left(n);
        input.move_forward_wordwise();
        assert_eq!(input.buf.first_half(), b"hello");
        assert_eq!(input.buf.second_half(), "\u{2014}world".as_bytes());
        input.deinit();
    }

    #[test]
    fn delete_word_before_whitespace_handles_unicode_whitespace() {
        let mut input = TextInput::init();
        input.insert_slice_at_cursor("hello\u{3000}world".as_bytes());
        input.delete_word_before_whitespace();
        assert_eq!(input.buf.first_half(), "hello\u{3000}".as_bytes());
        assert_eq!(input.buf.second_half(), b"");
        input.deinit();
    }

    #[test]
    fn delete_word_before_with_non_ascii_text() {
        let mut input = TextInput::init();
        input.insert_slice_at_cursor("über-cool".as_bytes());
        input.delete_word_before();
        assert_eq!(input.buf.first_half(), "über-".as_bytes());
        input.delete_word_before();
        assert_eq!(input.buf.first_half(), b"");
        input.deinit();
    }

    #[test]
    fn word_motion_with_spaces() {
        let mut input = TextInput::init();
        input.insert_slice_at_cursor(b"hello world");
        input.move_backward_wordwise();
        assert_eq!(input.buf.first_half(), b"hello ");
        input.move_backward_wordwise();
        assert_eq!(input.buf.first_half(), b"");
        input.deinit();
    }

    /// The key bindings moved in 0.6.0: Alt+Backspace is `deleteWordBefore` (word
    /// characters) and Ctrl+W is the new `deleteWordBeforeWhitespace`.
    #[test]
    fn ctrl_w_and_alt_backspace_are_different_words() {
        let key = |codepoint: u32, mods: Modifiers| Key {
            codepoint,
            text: None,
            shifted_codepoint: None,
            base_layout_codepoint: None,
            mods,
        };
        // Alt+Backspace: word characters ("bar" only).
        let mut input = TextInput::init();
        input.insert_slice_at_cursor(b"a foo.bar");
        input.update(Event::KeyPress(key(
            key::backspace,
            Modifiers {
                alt: true,
                ..Default::default()
            },
        )));
        assert_eq!(input.buf.first_half(), b"a foo.");
        input.deinit();

        // Ctrl+W: back to the whitespace ("foo.bar"; 0.5.1 bound it to deleteWordBefore).
        let mut input = TextInput::init();
        input.insert_slice_at_cursor(b"a foo.bar");
        input.update(Event::KeyPress(key(
            b'w' as u32,
            Modifiers {
                ctrl: true,
                ..Default::default()
            },
        )));
        assert_eq!(input.buf.first_half(), b"a ");
        input.deinit();
    }

    /// UD264 (R13): invalid UTF-8 in the buffer (key text from tty input) reaches
    /// `std.unicode.utf8Decode` with a slice that is not one code point: the reference
    /// hits `assert`/`unreachable` there. The port takes the helpers' own decode-failure
    /// path (one byte is the code point).
    #[test]
    fn word_motion_over_invalid_utf8_does_not_crash() {
        // Three continuation bytes after a space: `decodeCodepointBefore` hands utf8Decode
        // the space and all three (the reference asserts ` ` is a 4-byte lead byte).
        let mut input = TextInput::init();
        input.insert_slice_at_cursor(b"x \x80\x80\x80");
        input.delete_word_before_whitespace();
        assert_eq!(input.buf.first_half(), b"x ");
        input.deinit();

        // Five continuation bytes: `utf8Decode`'s `else => unreachable`.
        let mut input = TextInput::init();
        input.insert_slice_at_cursor(b"ab \x80\x80\x80\x80\x80");
        // Each continuation byte is one non-word code point; then the space, then "ab".
        input.delete_word_before();
        assert_eq!(input.buf.first_half(), b"");
        input.deinit();

        // An ASCII letter followed by a continuation byte: a 2-byte slice whose first
        // byte is not a 2-byte lead byte. The continuation byte is not a word
        // character (U+0080 is a control), the `a` is.
        let mut input = TextInput::init();
        input.insert_slice_at_cursor(b"- a\x80");
        input.move_backward_wordwise();
        assert_eq!(input.buf.first_half(), b"- ");
        input.deinit();

        // A lead byte cut short by the end of the buffer (`decodeCodepointAt` caps the
        // slice at two bytes; the reference asserts `0xE2` is a 2-byte lead byte). The
        // lone lead byte decodes as U+00E2 (a letter), the orphan `0x82` as U+0082.
        let mut input = TextInput::init();
        input.insert_slice_at_cursor(b"\xe2\x82");
        let n = input.buf.first_half().len();
        input.buf.move_gap_left(n);
        input.move_forward_wordwise();
        assert_eq!(input.buf.first_half(), b"\xe2");
        input.delete_word_after();
        assert_eq!(input.buf.second_half(), b"");
        input.deinit();
    }
}
