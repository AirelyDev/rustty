//! `uucode/src/utf8.zig` (9d555245; unchanged from 0.1.0 except its tests, and from 2826a37a) — Björn
//! Höhrmann's UTF-8 decoder DFA, ported verbatim.
//!
//! See https://bjoern.hoehrmann.de/utf-8/decoder/dfa/
//! and licenses/LICENSE_Bjoern_Hoehrmann

const UTF8_ACCEPT: usize = 0;
const UTF8_REJECT: usize = 12;

// The first part of the table maps bytes to character classes to reduce the
// size of the transition table and create bitmasks.
#[rustfmt::skip]
const UTF8D: [u8; 256] = [
   0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,  0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,
   0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,  0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,
   0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,  0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,
   0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,  0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,
   1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,  9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,
   7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,  7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,
   8,8,2,2,2,2,2,2,2,2,2,2,2,2,2,2,  2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,
  10,3,3,3,3,3,3,3,3,3,3,3,3,4,3,3, 11,6,6,6,5,8,8,8,8,8,8,8,8,8,8,8,
];

// The second part is a transition table that maps a combination of a state of
// the automaton and a character class to a state.
#[rustfmt::skip]
const STATE_UTF8D: [u8; 108] = [
   0,12,24,36,60,96,84,12,12,12,48,72, 12,12,12,12,12,12,12,12,12,12,12,12,
  12, 0,12,12,12,12,12, 0,12, 0,12,12, 12,24,12,12,12,12,12,24,12,24,12,12,
  12,12,12,12,12,12,12,24,12,12,12,12, 12,24,12,12,12,12,12,12,12,24,12,12,
  12,12,12,12,12,12,12,36,12,36,12,12, 12,36,12,12,12,12,12,36,12,36,12,12,
  12,36,12,12,12,12,12,12,12,12,12,12,
];

fn decode_byte(state: &mut usize, cp: &mut u32, byte: u8) {
    let class = UTF8D[byte as usize] as u32;
    let mask: u32 = 0xff;

    *cp = if *state != UTF8_ACCEPT {
        (byte as u32 & 0x3f) | (*cp << 6)
    } else {
        (mask >> class) & byte as u32
    };

    *state = STATE_UTF8D[*state + class as usize] as usize;
}

fn is_done_decoding(state: usize) -> bool {
    state == UTF8_ACCEPT || state == UTF8_REJECT
}

/// The UTF-8 code-point iterator over a byte slice (`&[u8]`, never a lossy `String`).
#[derive(Clone)]
pub struct Iterator<'a> {
    pub bytes: &'a [u8],
    pub i: usize,
}

impl<'a> Iterator<'a> {
    pub fn init(bytes: &'a [u8]) -> Self {
        Self { bytes, i: 0 }
    }

    // Faithful port of Zig `utf8.Iterator.next` (returns 0xFFFD on reject, not the
    // std `Iterator` trait); also exposes `peek`.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Option<u32> {
        if self.i >= self.bytes.len() {
            return None;
        }

        let mut cp: u32 = 0;
        let mut state: usize = UTF8_ACCEPT;

        loop {
            decode_byte(&mut state, &mut cp, self.bytes[self.i]);
            self.i += 1;
            if is_done_decoding(state) || self.i >= self.bytes.len() {
                break;
            }
        }

        if state == UTF8_ACCEPT {
            Some(cp)
        } else {
            Some(0xFFFD) // Replacement character
        }
    }

    pub fn peek(&self) -> Option<u32> {
        let mut it = self.clone();
        it.next()
    }
}

// The grapheme `Iterator` is generic over any code-point source; `utf8::Iterator`
// is the one vaxis uses. `CodePointIterator` exposes the `.next()`/`.i` surface the
// Zig `CustomIterator` reads directly off the iterator.
impl<'a> super::grapheme::CodePointIterator for Iterator<'a> {
    fn next_cp(&mut self) -> Option<u32> {
        self.next()
    }
    fn index(&self) -> usize {
        self.i
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iterator_for_ascii() {
        let mut it = Iterator::init(b"abc");
        assert_eq!(it.next(), Some(b'a' as u32));
        assert_eq!(it.i, 1);
        assert_eq!(it.peek(), Some(b'b' as u32));
        assert_eq!(it.next(), Some(b'b' as u32));
        assert_eq!(it.next(), Some(b'c' as u32));
        assert_eq!(it.peek(), None);
        assert_eq!(it.next(), None);
        assert_eq!(it.next(), None);
    }

    #[test]
    fn iterator_for_emoji() {
        let mut it = Iterator::init("😀😅😻👺".as_bytes());
        assert_eq!(it.next(), Some(0x1F600));
        assert_eq!(it.i, 4);
        assert_eq!(it.peek(), Some(0x1F605));
        assert_eq!(it.i, 4);
        assert_eq!(it.next(), Some(0x1F605));
        assert_eq!(it.i, 8);
        assert_eq!(it.next(), Some(0x1F63B));
        assert_eq!(it.i, 12);
        assert_eq!(it.next(), Some(0x1F47A));
        assert_eq!(it.i, 16);
        assert_eq!(it.next(), None);
        assert_eq!(it.i, 16);
    }

    #[test]
    fn iterator_overlong_utf8() {
        let mut it = Iterator::init(b"\xf0\x80\x80\xaf");
        assert_eq!(it.next(), Some(0xFFFD));
        assert_eq!(it.next(), Some(0xFFFD));
        assert_eq!(it.next(), Some(0xFFFD));
        assert_eq!(it.next(), None);
        assert_eq!(it.next(), None);
    }
}
