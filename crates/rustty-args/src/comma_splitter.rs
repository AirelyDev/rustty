//! Iterator to split a string into fields by commas, taking into account
//! quotes and escapes.
//!
//! Supports the same escapes as in Zig literal strings.
//!
//! Quotes must begin and end with a double quote (`"`). It is an error to not
//! end a quote that was begun. To include a double quote inside a quote (or to
//! not have a double quote start a quoted section) escape it with a backslash.
//!
//! Single quotes (`'`) are not special, they do not begin a quoted block.
//!
//! Zig multiline string literals are NOT supported.
//!
//! Quotes and escapes are not stripped or decoded, that must be handled as a
//! separate step!
//!
//! On Windows, backslash is only treated as an escape character inside quoted
//! strings. Outside quotes, backslash is a literal character (path separator).
//
// PORT NOTE: the reference is `CommaSplitter = @This()` — a file-as-struct. The
// Rust port is the `CommaSplitter` struct below. Byte-slices (`[]const u8`) map
// to `&[u8]` per the fidelity rule (never lossy `String`); the splitter operates
// over raw bytes and returns sub-slices of the original input, exactly as the
// reference returns sub-slices of `str`.

// const CommaSplitter = @This();

// const builtin = @import("builtin");

/// Whether backslash acts as an escape character outside quoted strings.
/// On Windows, backslash is the path separator so it is always literal
/// outside quotes.
// const escape_outside_quotes = builtin.os.tag != .windows;
const ESCAPE_OUTSIDE_QUOTES: bool = !cfg!(target_os = "windows");

/// PORTS: ghostty `Error`.
// pub const Error = error{
//     UnclosedQuote,
//     UnfinishedEscape,
//     IllegalEscape,
// };
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    UnclosedQuote,
    UnfinishedEscape,
    IllegalEscape,
}

/// PORTS: ghostty `CommaSplitter`.
pub struct CommaSplitter<'a> {
    /// the string that we are splitting
    // str: []const u8,
    str: &'a [u8],
    /// how much of the string has been consumed so far
    // index: usize,
    index: usize,
}

// PORT NOTE: the reference state machine uses Zig's labeled `switch`
// (`continue :loop .state`). The Rust port reproduces it with a `State` enum
// and a `loop` that re-dispatches on the current state, preserving the exact
// transitions, byte-by-byte index advancement, and error cases.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Normal,
    Quoted,
    Escape,
    Hexescape,
    Unicodeescape,
}

// sub-state of parsing hex escapes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UnicodeescapeState {
    Start,
    Digits,
}

impl<'a> CommaSplitter<'a> {
    /// initialize a splitter with the given string
    // pub fn init(str: []const u8) CommaSplitter {
    pub fn init(str: &'a [u8]) -> CommaSplitter<'a> {
        CommaSplitter { str, index: 0 }
    }

    /// return the next field, null if no more fields
    // pub fn next(self: *CommaSplitter) Error!?[]const u8 {
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Result<Option<&'a [u8]>, Error> {
        // if (self.index >= self.str.len) return null;
        if self.index >= self.str.len() {
            return Ok(None);
        }

        // where the current field starts
        let start = self.index;
        // state of state machine
        // (State enum defined above)
        // keep track of the state to return to when done processing an escape
        // sequence.
        let mut last: State = State::Normal;
        // used to count number of digits seen in a hex escape
        let mut hexescape_digits: usize = 0;
        // sub-state of parsing hex escapes
        let mut unicodeescape_state: UnicodeescapeState = UnicodeescapeState::Start;
        // number of digits in a unicode escape seen so far
        let mut unicodeescape_digits: usize = 0;
        // accumulator for value of unicode escape
        let mut unicodeescape_value: usize = 0;

        // loop: switch (State.normal) {
        let mut state = State::Normal;
        loop {
            match state {
                State::Normal => {
                    // if (self.index >= self.str.len) return self.str[start..];
                    if self.index >= self.str.len() {
                        return Ok(Some(&self.str[start..]));
                    }
                    match self.str[self.index] {
                        b',' => {
                            self.index += 1;
                            return Ok(Some(&self.str[start..self.index - 1]));
                        }
                        b'"' => {
                            self.index += 1;
                            state = State::Quoted;
                            continue;
                        }
                        b'\\' => {
                            self.index += 1;
                            if ESCAPE_OUTSIDE_QUOTES {
                                last = State::Normal;
                                state = State::Escape;
                                continue;
                            }
                            state = State::Normal;
                            continue;
                        }
                        _ => {
                            self.index += 1;
                            state = State::Normal;
                            continue;
                        }
                    }
                }
                State::Quoted => {
                    // if (self.index >= self.str.len) return error.UnclosedQuote;
                    if self.index >= self.str.len() {
                        return Err(Error::UnclosedQuote);
                    }
                    match self.str[self.index] {
                        b'"' => {
                            self.index += 1;
                            state = State::Normal;
                            continue;
                        }
                        b'\\' => {
                            self.index += 1;
                            last = State::Quoted;
                            state = State::Escape;
                            continue;
                        }
                        _ => {
                            self.index += 1;
                            state = State::Quoted;
                            continue;
                        }
                    }
                }
                State::Escape => {
                    // if (self.index >= self.str.len) return error.UnfinishedEscape;
                    if self.index >= self.str.len() {
                        return Err(Error::UnfinishedEscape);
                    }
                    match self.str[self.index] {
                        b'n' | b'r' | b't' | b'\\' | b'\'' | b'"' => {
                            self.index += 1;
                            state = last;
                            continue;
                        }
                        b'x' => {
                            self.index += 1;
                            hexescape_digits = 0;
                            state = State::Hexescape;
                            continue;
                        }
                        b'u' => {
                            self.index += 1;
                            unicodeescape_state = UnicodeescapeState::Start;
                            unicodeescape_digits = 0;
                            unicodeescape_value = 0;
                            state = State::Unicodeescape;
                            continue;
                        }
                        _ => return Err(Error::IllegalEscape),
                    }
                }
                State::Hexescape => {
                    // if (self.index >= self.str.len) return error.UnfinishedEscape;
                    if self.index >= self.str.len() {
                        return Err(Error::UnfinishedEscape);
                    }
                    match self.str[self.index] {
                        b'0'..=b'9' | b'a'..=b'f' | b'A'..=b'F' => {
                            self.index += 1;
                            hexescape_digits += 1;
                            if hexescape_digits == 2 {
                                state = last;
                                continue;
                            }
                            state = State::Hexescape;
                            continue;
                        }
                        _ => return Err(Error::IllegalEscape),
                    }
                }
                State::Unicodeescape => {
                    // if (self.index >= self.str.len) return error.UnfinishedEscape;
                    if self.index >= self.str.len() {
                        return Err(Error::UnfinishedEscape);
                    }
                    match unicodeescape_state {
                        UnicodeescapeState::Start => match self.str[self.index] {
                            b'{' => {
                                self.index += 1;
                                unicodeescape_value = 0;
                                unicodeescape_state = UnicodeescapeState::Digits;
                                state = State::Unicodeescape;
                                continue;
                            }
                            _ => return Err(Error::IllegalEscape),
                        },
                        UnicodeescapeState::Digits => {
                            match self.str[self.index] {
                                b'}' => {
                                    self.index += 1;
                                    if unicodeescape_digits == 0 {
                                        return Err(Error::IllegalEscape);
                                    }
                                    state = last;
                                    continue;
                                }
                                d @ b'0'..=b'9' => {
                                    self.index += 1;
                                    unicodeescape_digits += 1;
                                    unicodeescape_value <<= 4;
                                    unicodeescape_value += (d - b'0') as usize;
                                }
                                d @ b'a'..=b'f' => {
                                    self.index += 1;
                                    unicodeescape_digits += 1;
                                    unicodeescape_value <<= 4;
                                    unicodeescape_value += (d - b'a') as usize;
                                }
                                d @ b'A'..=b'F' => {
                                    self.index += 1;
                                    unicodeescape_digits += 1;
                                    unicodeescape_value <<= 4;
                                    unicodeescape_value += (d - b'A') as usize;
                                }
                                _ => return Err(Error::IllegalEscape),
                            }
                            // if (unicodeescape_value > 0x10ffff) return error.IllegalEscape;
                            if unicodeescape_value > 0x10ffff {
                                return Err(Error::IllegalEscape);
                            }
                            state = State::Unicodeescape;
                            continue;
                        }
                    }
                }
            }
        }
    }

    /// Return any remaining string data, whether it has a comma or not.
    // pub fn rest(self: *CommaSplitter) ?[]const u8 {
    pub fn rest(&mut self) -> Option<&'a [u8]> {
        // if (self.index >= self.str.len) return null;
        if self.index >= self.str.len() {
            return None;
        }
        // defer self.index = self.str.len;
        // return self.str[self.index..];
        let result = &self.str[self.index..];
        self.index = self.str.len();
        Some(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // PORT NOTE: the reference's `escape_outside_quotes`-guarded tests
    // (`if (comptime !escape_outside_quotes) return error.SkipZigTest;` and the
    // Windows `if (comptime escape_outside_quotes) ...`) are reproduced as a
    // compile-time guard at the top of each ported test, returning early on the
    // platform where the reference would `SkipZigTest`. This keeps the test count
    // (assertion-for-assertion) identical across platforms.

    // PORT NOTE: `expectEqualStrings("x", (try s.next()).?)` -> assert the
    // returned byte-slice equals the byte-literal. `try testing.expect(null ==
    // try s.next())` -> assert `next()` is `Ok(None)`.

    #[test]
    fn splitter_1() {
        let mut s = CommaSplitter::init(b"a,b,c");
        assert_eq!(b"a".as_slice(), s.next().unwrap().unwrap());
        assert_eq!(b"b".as_slice(), s.next().unwrap().unwrap());
        assert_eq!(b"c".as_slice(), s.next().unwrap().unwrap());
        assert!(s.next().unwrap().is_none());
    }

    #[test]
    fn splitter_2() {
        let mut s = CommaSplitter::init(b"");
        assert!(s.next().unwrap().is_none());
    }

    #[test]
    fn splitter_3() {
        let mut s = CommaSplitter::init(b"a");
        assert_eq!(b"a".as_slice(), s.next().unwrap().unwrap());
        assert!(s.next().unwrap().is_none());
    }

    #[test]
    fn splitter_4() {
        let mut s = CommaSplitter::init(b"\\x5a");
        assert_eq!(b"\\x5a".as_slice(), s.next().unwrap().unwrap());
        assert!(s.next().unwrap().is_none());
    }

    #[test]
    fn splitter_5() {
        let mut s = CommaSplitter::init(b"'a',b");
        assert_eq!(b"'a'".as_slice(), s.next().unwrap().unwrap());
        assert_eq!(b"b".as_slice(), s.next().unwrap().unwrap());
        assert!(s.next().unwrap().is_none());
    }

    #[test]
    fn splitter_6() {
        let mut s = CommaSplitter::init(b"'a,b',c");
        assert_eq!(b"'a".as_slice(), s.next().unwrap().unwrap());
        assert_eq!(b"b'".as_slice(), s.next().unwrap().unwrap());
        assert_eq!(b"c".as_slice(), s.next().unwrap().unwrap());
        assert!(s.next().unwrap().is_none());
    }

    #[test]
    fn splitter_7() {
        let mut s = CommaSplitter::init(b"\"a,b\",c");
        assert_eq!(b"\"a,b\"".as_slice(), s.next().unwrap().unwrap());
        assert_eq!(b"c".as_slice(), s.next().unwrap().unwrap());
        assert!(s.next().unwrap().is_none());
    }

    #[test]
    fn splitter_8() {
        let mut s = CommaSplitter::init(b" a , b ");
        assert_eq!(b" a ".as_slice(), s.next().unwrap().unwrap());
        assert_eq!(b" b ".as_slice(), s.next().unwrap().unwrap());
        assert!(s.next().unwrap().is_none());
    }

    #[test]
    fn splitter_9() {
        if !ESCAPE_OUTSIDE_QUOTES {
            return;
        }
        let mut s = CommaSplitter::init(b"\\x");
        assert_eq!(Err(Error::UnfinishedEscape), s.next());
    }

    #[test]
    fn splitter_10() {
        if !ESCAPE_OUTSIDE_QUOTES {
            return;
        }
        let mut s = CommaSplitter::init(b"\\x5");
        assert_eq!(Err(Error::UnfinishedEscape), s.next());
    }

    #[test]
    fn splitter_11() {
        if !ESCAPE_OUTSIDE_QUOTES {
            return;
        }
        let mut s = CommaSplitter::init(b"\\u");
        assert_eq!(Err(Error::UnfinishedEscape), s.next());
    }

    #[test]
    fn splitter_12() {
        if !ESCAPE_OUTSIDE_QUOTES {
            return;
        }
        let mut s = CommaSplitter::init(b"\\u{");
        assert_eq!(Err(Error::UnfinishedEscape), s.next());
    }

    #[test]
    fn splitter_13() {
        if !ESCAPE_OUTSIDE_QUOTES {
            return;
        }
        let mut s = CommaSplitter::init(b"\\u{}");
        assert_eq!(Err(Error::IllegalEscape), s.next());
    }

    #[test]
    fn splitter_14() {
        if !ESCAPE_OUTSIDE_QUOTES {
            return;
        }
        let mut s = CommaSplitter::init(b"\\u{h1}");
        assert_eq!(Err(Error::IllegalEscape), s.next());
    }

    #[test]
    fn splitter_15() {
        let mut s = CommaSplitter::init(b"\\u{10ffff}");
        assert_eq!(b"\\u{10ffff}".as_slice(), s.next().unwrap().unwrap());
        assert!(s.next().unwrap().is_none());
    }

    #[test]
    fn splitter_16() {
        if !ESCAPE_OUTSIDE_QUOTES {
            return;
        }
        let mut s = CommaSplitter::init(b"\\u{110000}");
        assert_eq!(Err(Error::IllegalEscape), s.next());
    }

    #[test]
    fn splitter_17() {
        if !ESCAPE_OUTSIDE_QUOTES {
            return;
        }
        let mut s = CommaSplitter::init(b"\\d");
        assert_eq!(Err(Error::IllegalEscape), s.next());
    }

    #[test]
    fn splitter_18() {
        if !ESCAPE_OUTSIDE_QUOTES {
            return;
        }
        let mut s = CommaSplitter::init(b"\\n\\r\\t\\\"\\'\\\\");
        assert_eq!(
            b"\\n\\r\\t\\\"\\'\\\\".as_slice(),
            s.next().unwrap().unwrap()
        );
        assert!(s.next().unwrap().is_none());
    }

    #[test]
    fn splitter_19() {
        let mut s = CommaSplitter::init(b"\"abc'def'ghi\"");
        assert_eq!(b"\"abc'def'ghi\"".as_slice(), s.next().unwrap().unwrap());
        assert!(s.next().unwrap().is_none());
    }

    #[test]
    fn splitter_20() {
        let mut s = CommaSplitter::init(b"\",\",abc");
        assert_eq!(b"\",\"".as_slice(), s.next().unwrap().unwrap());
        assert_eq!(b"abc".as_slice(), s.next().unwrap().unwrap());
        assert!(s.next().unwrap().is_none());
    }

    #[test]
    fn splitter_21() {
        let mut s = CommaSplitter::init(b"'a','b', 'c'");
        assert_eq!(b"'a'".as_slice(), s.next().unwrap().unwrap());
        assert_eq!(b"'b'".as_slice(), s.next().unwrap().unwrap());
        assert_eq!(b" 'c'".as_slice(), s.next().unwrap().unwrap());
        assert!(s.next().unwrap().is_none());
    }

    #[test]
    fn splitter_22() {
        let mut s = CommaSplitter::init(b"abc\"def");
        assert_eq!(Err(Error::UnclosedQuote), s.next());
    }

    #[test]
    fn splitter_23() {
        let mut s = CommaSplitter::init(b"title:\"Focus Split: Up\",description:\"Focus the split above, if it exists.\",action:goto_split:up");
        assert_eq!(
            b"title:\"Focus Split: Up\"".as_slice(),
            s.next().unwrap().unwrap()
        );
        assert_eq!(
            b"description:\"Focus the split above, if it exists.\"".as_slice(),
            s.next().unwrap().unwrap()
        );
        assert_eq!(
            b"action:goto_split:up".as_slice(),
            s.next().unwrap().unwrap()
        );
        assert!(s.next().unwrap().is_none());
    }

    #[test]
    fn splitter_24() {
        let mut s = CommaSplitter::init(b"a,b,c,def");
        assert_eq!(b"a".as_slice(), s.next().unwrap().unwrap());
        assert_eq!(b"b".as_slice(), s.next().unwrap().unwrap());
        assert_eq!(b"c,def".as_slice(), s.rest().unwrap());
        assert!(s.next().unwrap().is_none());
    }

    #[test]
    fn splitter_25() {
        if !ESCAPE_OUTSIDE_QUOTES {
            return;
        }
        let mut s = CommaSplitter::init(b"a,\\u{10,df}");
        assert_eq!(b"a".as_slice(), s.next().unwrap().unwrap());
        assert_eq!(Err(Error::IllegalEscape), s.next());
    }

    // Windows-specific tests: backslash is literal outside quotes.

    #[test]
    fn splitter_windows_paths() {
        if ESCAPE_OUTSIDE_QUOTES {
            return;
        }
        let mut s = CommaSplitter::init(b"light:C:\\Users\\foo\\theme,dark:C:\\Users\\bar\\theme");
        assert_eq!(
            b"light:C:\\Users\\foo\\theme".as_slice(),
            s.next().unwrap().unwrap()
        );
        assert_eq!(
            b"dark:C:\\Users\\bar\\theme".as_slice(),
            s.next().unwrap().unwrap()
        );
        assert!(s.next().unwrap().is_none());
    }

    #[test]
    fn splitter_backslash_literal_outside_quotes_on_windows() {
        if ESCAPE_OUTSIDE_QUOTES {
            return;
        }
        // Backslash followed by characters that would be escapes on Unix
        // are treated as literal on Windows outside quotes.
        let mut s = CommaSplitter::init(b"\\n\\r\\t");
        assert_eq!(b"\\n\\r\\t".as_slice(), s.next().unwrap().unwrap());
        assert!(s.next().unwrap().is_none());
    }

    #[test]
    fn splitter_backslash_still_escapes_inside_quotes_on_windows() {
        if ESCAPE_OUTSIDE_QUOTES {
            return;
        }
        // Inside quotes, backslash escapes work on all platforms.
        let mut s = CommaSplitter::init(b"\"hello\\nworld\"");
        assert_eq!(b"\"hello\\nworld\"".as_slice(), s.next().unwrap().unwrap());
        assert!(s.next().unwrap().is_none());
    }
}
