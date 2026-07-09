//! `vaxis/src/unicode.zig` — the Old-API grapheme-iteration wrapper over the vendored
//! `uucode` grapheme `Iterator` (VAXIS_C1B_DESIGN.md §9). `&[u8]` throughout. No own
//! tests (uucode's grapheme tests + gwidth exercise the same path).

use crate::uucode;

/// Old API-compatible Grapheme value
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Grapheme {
    pub start: usize,
    pub len: usize,
}

impl Grapheme {
    pub fn bytes<'a>(&self, str: &'a [u8]) -> &'a [u8] {
        &str[self.start..self.start + self.len]
    }
}

/// Old API-compatible iterator that yields Grapheme with .len and .bytes()
pub struct GraphemeIterator<'a> {
    str: &'a [u8],
    inner: uucode::grapheme::Iterator<uucode::utf8::Iterator<'a>>,
    start: usize,
    prev_break: bool,
}

impl<'a> GraphemeIterator<'a> {
    pub fn init(str: &'a [u8]) -> Self {
        Self {
            str,
            inner: uucode::grapheme::Iterator::init(uucode::utf8::Iterator::init(str)),
            start: 0,
            prev_break: true,
        }
    }

    // Faithful port of Zig `GraphemeIterator.next` (Old-API), not the std `Iterator` trait.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Option<Grapheme> {
        while let Some(res) = self.inner.next() {
            // When leaving a break and entering a non-break, set the start of a cluster
            if self.prev_break && !res.is_break {
                let cp_len: usize = utf8_codepoint_sequence_length(res.cp);
                self.start = self.inner.i - cp_len;
            }

            // A break marks the end of the current grapheme
            if res.is_break {
                let end = self.inner.i;
                let s = self.start;
                self.start = end;
                self.prev_break = true;
                return Some(Grapheme {
                    start: s,
                    len: end - s,
                });
            }

            self.prev_break = false;
        }

        // Flush the last grapheme if we ended mid-cluster
        if !self.prev_break && self.start < self.str.len() {
            let s = self.start;
            let len = self.str.len() - s;
            self.start = self.str.len();
            self.prev_break = true;
            return Some(Grapheme { start: s, len });
        }

        None
    }
}

/// `std.unicode.utf8CodepointSequenceLength` (with the `catch 1` fallback).
fn utf8_codepoint_sequence_length(cp: u32) -> usize {
    if cp < 0x80 {
        1
    } else if cp < 0x800 {
        2
    } else if cp < 0x10000 {
        3
    } else if cp <= 0x10FFFF {
        4
    } else {
        1
    }
}

/// creates a grapheme iterator based on str
pub fn grapheme_iterator(str: &[u8]) -> GraphemeIterator<'_> {
    GraphemeIterator::init(str)
}
