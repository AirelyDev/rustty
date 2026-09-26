//! `vaxis/src/unicode.zig` — the Old-API grapheme-iteration wrapper over the vendored
//! `uucode` grapheme `Iterator` (VAXIS_C1B_DESIGN.md §9). `&[u8]` throughout. The
//! reference has no own tests here; the one below pins the UD260 (R13) repair.

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
        loop {
            // The inner iterator's `i` before `nextCodePoint` is where the code point it
            // returns starts. Used by the UD260 repair below.
            let cp_start = self.inner.i;
            let Some(res) = self.inner.next_code_point() else {
                break;
            };

            // When leaving a break and entering a non-break, set the start of a cluster
            if self.prev_break && !res.is_break {
                // PORT NOTE: reference defect (R13, UD260). The reference computes
                //     const cp_len: usize = std.unicode.utf8CodepointSequenceLength(res.code_point) catch 1;
                //     self.start = self.inner.i - cp_len;
                // which uses the re-encoded length of the decoded code point: an invalid
                // byte decodes to U+FFFD (3 bytes) after consuming 1, so at the start of
                // the input this underflows (a trap in Debug/ReleaseSafe) and later it
                // yields a grapheme that re-includes the two bytes before it. The start of
                // the cluster is where its first code point started: `cp_start` (the same
                // value for valid UTF-8).
                self.start = cp_start;
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

/// creates a grapheme iterator based on str
pub fn grapheme_iterator(str: &[u8]) -> GraphemeIterator<'_> {
    GraphemeIterator::init(str)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn collect(s: &[u8]) -> Vec<(usize, usize)> {
        let mut it = grapheme_iterator(s);
        let mut out = Vec::new();
        while let Some(g) = it.next() {
            out.push((g.start, g.len));
        }
        out
    }

    /// UD260 (R13): an invalid byte (decoded as U+FFFD, one byte consumed) that starts a
    /// multi-code-point cluster. The reference's `self.inner.i - cp_len` uses the
    /// re-encoded length of U+FFFD (3): at the start of the input that underflows (a
    /// panic in Debug/ReleaseSafe); later in the input it moves the cluster start two
    /// bytes back, so the reference yields `(0, 5)` for the third grapheme below and
    /// re-emits "ab" (measured with Zig 0.16.0 against libvaxis 0.6.0 + uucode 2826a37a).
    /// The port starts the cluster where its first code point started.
    #[test]
    fn invalid_byte_starting_a_cluster() {
        assert_eq!(collect(b"\xFF\xCC\x81"), vec![(0, 3)]);
        assert_eq!(collect(b"ab\xFF\xCC\x81"), vec![(0, 1), (1, 1), (2, 3)]);
        // Valid input: a combining cluster after a multi-byte code point.
        assert_eq!(
            collect("中e\u{301}x".as_bytes()),
            vec![(0, 3), (3, 3), (6, 1)]
        );
    }
}
