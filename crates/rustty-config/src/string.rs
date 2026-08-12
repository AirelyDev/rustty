//! rustty-config::string
//!
//! PORTS: ghostty src/config/string.zig — string-literal parsing helpers
//! (`parse`, `codepointIterator`/`CodepointIterator`). C1 leaf value-type.
//!
//! PORT STATUS: ported (every line/comment/test{}, R1).

// const std = @import("std");

/// PORTS: Zig stdlib `std.zig.string_literal.ParsedCharLiteral` — the result of
/// parsing a single escape sequence.
//
// PORT NOTE: Zig's `parseEscapeSequence` returns a `union(enum) { success: u21,
// failure: Error }`. The `string.zig` callers only distinguish success (with the
// decoded codepoint) from failure, so we model the result as `Option<u21>`-free
// dedicated enum to keep the faithful arm structure; the rich `Error` payloads
// are not observed by `string.zig` (it maps any `.failure` to one error), so they
// are not reproduced here.
enum ParsedCharLiteral {
    Success(u32),
    Failure,
}

/// PORTS: Zig stdlib `std.zig.string_literal.parseEscapeSequence`.
///
/// Parse an escape sequence from `slice[offset..]`. If parsing is successful,
/// offset is updated to reflect the characters consumed.
//
// PORT NOTE: faithful port of Zig 0.15's `std.zig.string_literal.parseEscapeSequence`
// (lib/std/zig/string_literal.zig). `string.zig` relies on this stdlib function;
// since rustty has no Zig stdlib, the function is reproduced here verbatim
// (control flow + arms identical). The `Error` variant payloads are collapsed to
// a single `Failure` because `string.zig` observes only success/failure.
fn parse_escape_sequence(slice: &[u8], offset: &mut usize) -> ParsedCharLiteral {
    // assert(slice.len > offset.*);
    assert!(slice.len() > *offset);
    // assert(slice[offset.*] == '\\');
    assert!(slice[*offset] == b'\\');

    // if (slice.len == offset.* + 1)
    //     return .{ .failure = .{ .invalid_escape_character = offset.* + 1 } };
    if slice.len() == *offset + 1 {
        return ParsedCharLiteral::Failure;
    }

    // offset.* += 2;
    *offset += 2;
    // switch (slice[offset.* - 1]) {
    match slice[*offset - 1] {
        // 'n' => return .{ .success = '\n' },
        b'n' => ParsedCharLiteral::Success('\n' as u32),
        // 'r' => return .{ .success = '\r' },
        b'r' => ParsedCharLiteral::Success('\r' as u32),
        // '\\' => return .{ .success = '\\' },
        b'\\' => ParsedCharLiteral::Success('\\' as u32),
        // 't' => return .{ .success = '\t' },
        b't' => ParsedCharLiteral::Success('\t' as u32),
        // '\'' => return .{ .success = '\'' },
        b'\'' => ParsedCharLiteral::Success('\'' as u32),
        // '"' => return .{ .success = '"' },
        b'"' => ParsedCharLiteral::Success('"' as u32),
        // 'x' => {
        b'x' => {
            // var value: u8 = 0;
            let mut value: u8 = 0;
            // var i: usize = offset.*;
            let mut i: usize = *offset;
            // while (i < offset.* + 2) : (i += 1) {
            while i < *offset + 2 {
                // if (i == slice.len) return .{ .failure = .{ .expected_hex_digit = i } };
                if i == slice.len() {
                    return ParsedCharLiteral::Failure;
                }

                // const c = slice[i];
                let c = slice[i];
                // switch (c) {
                match c {
                    // '0'...'9' => { value *= 16; value += c - '0'; },
                    b'0'..=b'9' => {
                        value = value.wrapping_mul(16);
                        value = value.wrapping_add(c - b'0');
                    }
                    // 'a'...'f' => { value *= 16; value += c - 'a' + 10; },
                    b'a'..=b'f' => {
                        value = value.wrapping_mul(16);
                        value = value.wrapping_add(c - b'a' + 10);
                    }
                    // 'A'...'F' => { value *= 16; value += c - 'A' + 10; },
                    b'A'..=b'F' => {
                        value = value.wrapping_mul(16);
                        value = value.wrapping_add(c - b'A' + 10);
                    }
                    // else => { return .{ .failure = .{ .expected_hex_digit = i } }; },
                    _ => {
                        return ParsedCharLiteral::Failure;
                    }
                }
                i += 1;
            }
            // offset.* = i;
            *offset = i;
            // return .{ .success = value };
            ParsedCharLiteral::Success(value as u32)
        }
        // 'u' => {
        b'u' => {
            // var i: usize = offset.*;
            let mut i: usize = *offset;
            // if (i >= slice.len or slice[i] != '{') return .{ .failure = .{ .expected_lbrace = i } };
            if i >= slice.len() || slice[i] != b'{' {
                return ParsedCharLiteral::Failure;
            }
            // i += 1;
            i += 1;
            // if (i >= slice.len) return .{ .failure = .{ .expected_hex_digit_or_rbrace = i } };
            if i >= slice.len() {
                return ParsedCharLiteral::Failure;
            }
            // if (slice[i] == '}') return .{ .failure = .{ .empty_unicode_escape_sequence = i } };
            if slice[i] == b'}' {
                return ParsedCharLiteral::Failure;
            }

            // var value: u32 = 0;
            let mut value: u32 = 0;
            // while (i < slice.len) : (i += 1) {
            // PORT NOTE: the Zig `while ... else` runs the `else` block when the
            // loop exits without `break`. We track that with a `found_rbrace`
            // flag and emit the `.failure = .{ .expected_rbrace = i }` arm when
            // the loop falls through without seeing a `}`.
            let mut found_rbrace = false;
            while i < slice.len() {
                // const c = slice[i];
                let c = slice[i];
                // switch (c) {
                match c {
                    // '0'...'9' => { value *= 16; value += c - '0'; },
                    b'0'..=b'9' => {
                        value = value.wrapping_mul(16);
                        value = value.wrapping_add((c - b'0') as u32);
                    }
                    // 'a'...'f' => { value *= 16; value += c - 'a' + 10; },
                    b'a'..=b'f' => {
                        value = value.wrapping_mul(16);
                        value = value.wrapping_add((c - b'a' + 10) as u32);
                    }
                    // 'A'...'F' => { value *= 16; value += c - 'A' + 10; },
                    b'A'..=b'F' => {
                        value = value.wrapping_mul(16);
                        value = value.wrapping_add((c - b'A' + 10) as u32);
                    }
                    // '}' => { i += 1; break; },
                    b'}' => {
                        i += 1;
                        found_rbrace = true;
                        break;
                    }
                    // else => return .{ .failure = .{ .expected_hex_digit_or_rbrace = i } },
                    _ => return ParsedCharLiteral::Failure,
                }
                // if (value > 0x10ffff) {
                //     return .{ .failure = .{ .invalid_unicode_codepoint = i } };
                // }
                if value > 0x10ffff {
                    return ParsedCharLiteral::Failure;
                }
                i += 1;
            }
            // } else {
            //     return .{ .failure = .{ .expected_rbrace = i } };
            // }
            if !found_rbrace {
                return ParsedCharLiteral::Failure;
            }
            // offset.* = i;
            *offset = i;
            // return .{ .success = @as(u21, @intCast(value)) };
            ParsedCharLiteral::Success(value)
        }
        // else => return .{ .failure = .{ .invalid_escape_character = offset.* - 1 } },
        _ => ParsedCharLiteral::Failure,
    }
}

/// PORTS: Zig stdlib `std.unicode.utf8ByteSequenceLength`.
//
// PORT NOTE: returns the number of bytes in the UTF-8 sequence beginning with
// `first_byte`, or `None` (the reference's `error.Utf8InvalidStartByte`) when the
// leading byte is not a valid sequence start.
fn utf8_byte_sequence_length(first_byte: u8) -> Option<usize> {
    // The switch on the leading-byte ranges in `std.unicode.utf8ByteSequenceLength`.
    match first_byte {
        0b0000_0000..=0b0111_1111 => Some(1),
        0b1100_0000..=0b1101_1111 => Some(2),
        0b1110_0000..=0b1110_1111 => Some(3),
        0b1111_0000..=0b1111_0111 => Some(4),
        _ => None,
    }
}

/// Parse a string literal into a byte array. The string can contain
/// any valid Zig string literal escape sequences.
///
/// The output buffer never needs to be larger than the input buffer.
/// The buffers may alias.
// pub fn parse(out: []u8, bytes: []const u8) ![]u8 {
//
// PORT NOTE: `out` and `bytes` may alias (the reference notes this). The Rust
// signature takes `out: &mut [u8]` and `bytes: &[u8]`; the `string.zig` callers
// that alias the two buffers do so by copying first (see `io.zig` `cloneParsed`),
// so the safe split-borrow signature suffices for every call site. Returns the
// written prefix length; callers slice `out[..len]` to reproduce `out[0..dst_i]`.
pub fn parse(out: &mut [u8], bytes: &[u8]) -> Result<usize, ParseError> {
    // var dst_i: usize = 0;
    let mut dst_i: usize = 0;
    // var src_i: usize = 0;
    let mut src_i: usize = 0;
    // while (src_i < bytes.len) {
    while src_i < bytes.len() {
        // if (dst_i >= out.len) return error.OutOfMemory;
        if dst_i >= out.len() {
            return Err(ParseError::OutOfMemory);
        }

        // If this byte is not beginning an escape sequence we copy.
        // const b = bytes[src_i];
        let b = bytes[src_i];
        // if (b != '\\') {
        if b != b'\\' {
            // out[dst_i] = b;
            out[dst_i] = b;
            // dst_i += 1;
            dst_i += 1;
            // src_i += 1;
            src_i += 1;
            // continue;
            continue;
        }

        // Parse the escape sequence
        // switch (std.zig.string_literal.parseEscapeSequence(bytes, &src_i)) {
        match parse_escape_sequence(bytes, &mut src_i) {
            // .failure => return error.InvalidString,
            ParsedCharLiteral::Failure => return Err(ParseError::InvalidString),
            // .success => |cp| dst_i += try std.unicode.utf8Encode(cp, out[dst_i..]),
            ParsedCharLiteral::Success(cp) => {
                dst_i += utf8_encode(cp, &mut out[dst_i..])?;
            }
        }
    }

    // return out[0..dst_i];
    Ok(dst_i)
}

/// PORTS: Zig stdlib `std.unicode.utf8Encode` (the subset `string.zig` reaches).
//
// PORT NOTE: encodes codepoint `cp` into `out`, returning the byte count.
//
// The reference's `utf8Encode` error set is
// `error{ Utf8CannotEncodeSurrogateHalf, CodepointTooLarge }`, and `parse`'s
// inferred error set propagates it verbatim (`try std.unicode.utf8Encode(...)`).
// A surrogate IS reachable here: `parse_escape_sequence`'s `\u{...}` arm rejects
// only `value > 0x10ffff` (exactly as `std.zig.string_literal` does), so
// `\u{D800}` parses successfully and reaches this encode, where the reference
// returns `error.Utf8CannotEncodeSurrogateHalf`. `ParseError` therefore carries
// that class instead of folding it into `OutOfMemory` — which the reference's
// caller (`cli/args.zig` `parseIntoField`) treats as a FATAL config-load error,
// while any other error becomes a recoverable "unknown error {}" diagnostic.
// `CodepointTooLarge` stays unreachable (the `\u{...}` arm caps at `0x10ffff`),
// so it is not part of this error set.
//
// The `out`-too-small arm has no reference analogue (Zig asserts the buffer is
// large enough); it keeps `OutOfMemory`, the class `parse` itself returns when
// the destination buffer is exhausted.
fn utf8_encode(cp: u32, out: &mut [u8]) -> Result<usize, ParseError> {
    // `char::from_u32` rejects surrogates (D800..DFFF) and > 0x10FFFF; only the
    // surrogate half is reachable from `parse_escape_sequence` (see above), so
    // failure here is the reference's `error.Utf8CannotEncodeSurrogateHalf`.
    let ch = char::from_u32(cp).ok_or(ParseError::Utf8CannotEncodeSurrogateHalf)?;
    let len = ch.len_utf8();
    if len > out.len() {
        return Err(ParseError::OutOfMemory);
    }
    ch.encode_utf8(&mut out[..len]);
    Ok(len)
}

/// PORTS: the error set of `parse` — `error.OutOfMemory || error.InvalidString`
/// plus the `std.unicode.utf8Encode` errors its `try` propagates (of which only
/// `Utf8CannotEncodeSurrogateHalf` is reachable; see `utf8_encode`).
#[derive(Debug, PartialEq, Eq)]
pub enum ParseError {
    OutOfMemory,
    InvalidString,
    /// `error.Utf8CannotEncodeSurrogateHalf` — an escaped surrogate half
    /// (`\u{D800}`..`\u{DFFF}`) cannot be UTF-8 encoded.
    Utf8CannotEncodeSurrogateHalf,
}

/// Creates an iterator that requires no allocation to extract codepoints
/// from the string literal, parsing escape sequences as it goes.
// pub fn codepointIterator(bytes: []const u8) CodepointIterator {
//     return .{ .bytes = bytes, .i = 0 };
// }
pub fn codepoint_iterator(bytes: &[u8]) -> CodepointIterator<'_> {
    CodepointIterator { bytes, i: 0 }
}

// pub const CodepointIterator = struct {
pub struct CodepointIterator<'a> {
    // bytes: []const u8,
    bytes: &'a [u8],
    // i: usize,
    i: usize,
}

/// PORTS: the error set of `CodepointIterator.next` — `error{InvalidString}`.
#[derive(Debug, PartialEq, Eq)]
pub struct InvalidString;

impl CodepointIterator<'_> {
    // pub fn next(self: *CodepointIterator) error{InvalidString}!?u21 {
    //
    // PORT NOTE: the reference's `error{InvalidString}!?u21` (an error union of an
    // optional codepoint) maps to `Result<Option<u32>, InvalidString>`. `u21` is a
    // codepoint; we carry it as `u32` (no narrower native int).
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Result<Option<u32>, InvalidString> {
        // if (self.i >= self.bytes.len) return null;
        if self.i >= self.bytes.len() {
            return Ok(None);
        }
        // switch (self.bytes[self.i]) {
        match self.bytes[self.i] {
            // An escape sequence
            // '\\' => return switch (std.zig.string_literal.parseEscapeSequence(
            //     self.bytes,
            //     &self.i,
            // )) {
            //     .failure => error.InvalidString,
            //     .success => |cp| cp,
            // },
            b'\\' => match parse_escape_sequence(self.bytes, &mut self.i) {
                ParsedCharLiteral::Failure => Err(InvalidString),
                ParsedCharLiteral::Success(cp) => Ok(Some(cp)),
            },

            // Not an escape, parse as UTF-8
            // else => |start| {
            start => {
                // const cp_len = std.unicode.utf8ByteSequenceLength(start) catch
                //     return error.InvalidString;
                let cp_len = match utf8_byte_sequence_length(start) {
                    Some(n) => n,
                    None => return Err(InvalidString),
                };
                // defer self.i += cp_len;
                // return std.unicode.utf8Decode(self.bytes[self.i..][0..cp_len]) catch
                //     return error.InvalidString;
                // PORT NOTE: the `defer self.i += cp_len` advances the index after
                // decoding regardless of success/failure; on the failure path the
                // reference still runs the defer before propagating the error.
                if self.i + cp_len > self.bytes.len() {
                    self.i += cp_len;
                    return Err(InvalidString);
                }
                let seq = &self.bytes[self.i..self.i + cp_len];
                let result = match std::str::from_utf8(seq) {
                    Ok(s) => match s.chars().next() {
                        Some(ch) => Ok(Some(ch as u32)),
                        None => Err(InvalidString),
                    },
                    Err(_) => Err(InvalidString),
                };
                self.i += cp_len;
                result
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // test "parse: empty" {
    #[test]
    fn parse_empty() {
        // var buf: [128]u8 = undefined;
        let mut buf = [0u8; 128];
        // const result = try parse(&buf, "");
        let len = parse(&mut buf, b"").unwrap();
        // try testing.expectEqualStrings("", result);
        assert_eq!(b"", &buf[..len]);
    }

    // test "parse: no escapes" {
    #[test]
    fn parse_no_escapes() {
        // var buf: [128]u8 = undefined;
        let mut buf = [0u8; 128];
        // const result = try parse(&buf, "hello world");
        let len = parse(&mut buf, b"hello world").unwrap();
        // try testing.expectEqualStrings("hello world", result);
        assert_eq!(b"hello world", &buf[..len]);
    }

    // test "parse: escapes" {
    #[test]
    fn parse_escapes() {
        // var buf: [128]u8 = undefined;
        let mut buf = [0u8; 128];
        {
            // const result = try parse(&buf, "hello\\nworld");
            let len = parse(&mut buf, b"hello\\nworld").unwrap();
            // try testing.expectEqualStrings("hello\nworld", result);
            assert_eq!("hello\nworld".as_bytes(), &buf[..len]);
        }
        {
            // const result = try parse(&buf, "hello\\u{1F601}world");
            let len = parse(&mut buf, b"hello\\u{1F601}world").unwrap();
            // try testing.expectEqualStrings("hello\u{1F601}world", result);
            assert_eq!("hello\u{1F601}world".as_bytes(), &buf[..len]);
        }
    }

    // test "codepointIterator: empty" {
    #[test]
    fn codepoint_iterator_empty() {
        // var it = codepointIterator("");
        let mut it = codepoint_iterator(b"");
        // try std.testing.expectEqual(null, try it.next());
        assert_eq!(None, it.next().unwrap());
    }

    // test "codepointIterator: ascii no escapes" {
    #[test]
    fn codepoint_iterator_ascii_no_escapes() {
        // var it = codepointIterator("abc");
        let mut it = codepoint_iterator(b"abc");
        // try std.testing.expectEqual(@as(u21, 'a'), (try it.next()).?);
        assert_eq!('a' as u32, it.next().unwrap().unwrap());
        // try std.testing.expectEqual(@as(u21, 'b'), (try it.next()).?);
        assert_eq!('b' as u32, it.next().unwrap().unwrap());
        // try std.testing.expectEqual(@as(u21, 'c'), (try it.next()).?);
        assert_eq!('c' as u32, it.next().unwrap().unwrap());
        // try std.testing.expectEqual(null, try it.next());
        assert_eq!(None, it.next().unwrap());
    }

    // test "codepointIterator: multibyte utf8" {
    #[test]
    fn codepoint_iterator_multibyte_utf8() {
        // │ is U+2502 (3 bytes in UTF-8)
        // var it = codepointIterator("a│b");
        let mut it = codepoint_iterator("a│b".as_bytes());
        // try std.testing.expectEqual(@as(u21, 'a'), (try it.next()).?);
        assert_eq!('a' as u32, it.next().unwrap().unwrap());
        // try std.testing.expectEqual(@as(u21, '│'), (try it.next()).?);
        assert_eq!('│' as u32, it.next().unwrap().unwrap());
        // try std.testing.expectEqual(@as(u21, 'b'), (try it.next()).?);
        assert_eq!('b' as u32, it.next().unwrap().unwrap());
        // try std.testing.expectEqual(null, try it.next());
        assert_eq!(None, it.next().unwrap());
    }

    // test "codepointIterator: escape sequences" {
    #[test]
    fn codepoint_iterator_escape_sequences() {
        // var it = codepointIterator("a\\tb\\n\\\\");
        let mut it = codepoint_iterator(b"a\\tb\\n\\\\");
        // try std.testing.expectEqual(@as(u21, 'a'), (try it.next()).?);
        assert_eq!('a' as u32, it.next().unwrap().unwrap());
        // try std.testing.expectEqual(@as(u21, '\t'), (try it.next()).?);
        assert_eq!('\t' as u32, it.next().unwrap().unwrap());
        // try std.testing.expectEqual(@as(u21, 'b'), (try it.next()).?);
        assert_eq!('b' as u32, it.next().unwrap().unwrap());
        // try std.testing.expectEqual(@as(u21, '\n'), (try it.next()).?);
        assert_eq!('\n' as u32, it.next().unwrap().unwrap());
        // try std.testing.expectEqual(@as(u21, '\\'), (try it.next()).?);
        assert_eq!('\\' as u32, it.next().unwrap().unwrap());
        // try std.testing.expectEqual(null, try it.next());
        assert_eq!(None, it.next().unwrap());
    }

    // test "codepointIterator: unicode escape" {
    #[test]
    fn codepoint_iterator_unicode_escape() {
        // var it = codepointIterator("\\u{2502}x");
        let mut it = codepoint_iterator(b"\\u{2502}x");
        // try std.testing.expectEqual(@as(u21, '│'), (try it.next()).?);
        assert_eq!('│' as u32, it.next().unwrap().unwrap());
        // try std.testing.expectEqual(@as(u21, 'x'), (try it.next()).?);
        assert_eq!('x' as u32, it.next().unwrap().unwrap());
        // try std.testing.expectEqual(null, try it.next());
        assert_eq!(None, it.next().unwrap());
    }

    // test "codepointIterator: emoji unicode escape" {
    #[test]
    fn codepoint_iterator_emoji_unicode_escape() {
        // var it = codepointIterator("\\u{1F601}");
        let mut it = codepoint_iterator(b"\\u{1F601}");
        // try std.testing.expectEqual(@as(u21, 0x1F601), (try it.next()).?);
        assert_eq!(0x1F601, it.next().unwrap().unwrap());
        // try std.testing.expectEqual(null, try it.next());
        assert_eq!(None, it.next().unwrap());
    }

    // PORT NOTE: no reference test. `\u{D800}` parses as an escape sequence
    // (`std.zig.string_literal` rejects only `> 0x10ffff`) and then fails to
    // UTF-8 encode: the reference's `try std.unicode.utf8Encode` propagates
    // `error.Utf8CannotEncodeSurrogateHalf`, a class `cli/args.zig` turns into a
    // recoverable "unknown error {}" diagnostic. Reporting it as `OutOfMemory`
    // instead — as the port did before — is the one class `parseIntoField`
    // treats as FATAL.
    #[test]
    fn parse_escaped_surrogate_half() {
        let mut buf = [0u8; 128];
        assert_eq!(
            Err(ParseError::Utf8CannotEncodeSurrogateHalf),
            parse(&mut buf, b"\\u{D800}")
        );
        // The whole surrogate range, low and high halves alike.
        assert_eq!(
            Err(ParseError::Utf8CannotEncodeSurrogateHalf),
            parse(&mut buf, b"pre\\u{DFFF}post")
        );

        // Controls: the neighbours of the surrogate range and the largest legal
        // codepoint still encode, and a genuinely too-small output buffer is
        // still `OutOfMemory`.
        let len = parse(&mut buf, b"\\u{D7FF}").unwrap();
        assert_eq!("\u{D7FF}".as_bytes(), &buf[..len]);
        let len = parse(&mut buf, b"\\u{E000}").unwrap();
        assert_eq!("\u{E000}".as_bytes(), &buf[..len]);
        let len = parse(&mut buf, b"\\u{10FFFF}").unwrap();
        assert_eq!("\u{10FFFF}".as_bytes(), &buf[..len]);
        let mut small = [0u8; 2];
        assert_eq!(Err(ParseError::OutOfMemory), parse(&mut small, b"abc"));
    }
}
