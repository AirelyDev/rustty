//! rustty-crash::json
//!
//! PORTS: the slice of zig 0.16.0 `std.json` that `crash/sentry_envelope.zig`
//! uses — `std.json.Value` / `std.json.ObjectMap` (`std/json/dynamic.zig`),
//! `std.json.parseFromSliceLeaky(std.json.Value, ..)` with default options
//! (`dynamic.zig` `jsonParse` over `std/json/Scanner.zig`), and
//! `std.json.fmt(value, .{ .whitespace = .minified })` (`std/json/Stringify.zig`).
//!
//! PORT NOTE (LEDGER S79-F3, S79-F4): the envelope code used `serde_json`, whose
//! value model and printer differ from `std.json` in ways an envelope can reach:
//! a duplicate object key (`std.json`: `error.DuplicateField`; serde: last one
//! wins), an integer outside `i64` (`std.json`: `.number_string`, kept verbatim;
//! serde: `u64` or a rounded `f64`), a float that overflows `f64` (`std.json`:
//! `.number_string`; serde: a parse error), float printing (`std.json`: `2.0` ->
//! `2`, `1e100` -> all 101 digits; serde: `2.0`, `1e100`), and nesting depth
//! (serde: a parse error past 128 levels; `std.json`: see below). This module
//! is a direct port of those semantics instead. Every behaviour here is pinned
//! against a golden file MEASURED from zig 0.16.0 (`testdata/std_json_golden.tsv`).
//!
//! Nesting depth. CORRECTION: this header used to say `std.json` has no
//! nesting limit. Its parser has none (`Scanner` keeps an allocator-backed
//! `BitStack`; MEASURED on zig 0.16.0 ReleaseSafe: 100,000 levels parse), but
//! `Stringify` is documented to a maximum output depth of 256
//! (`std/json/Stringify.zig:74` `.checked_to_fixed_depth = 256`, `:572`).
//!
//! PORT NOTE: reference defect (R13), UD182. `std.json.fmt` of a value nested
//! 257 or more levels deep does not return an error: in safe builds it panics
//! (MEASURED, ReleaseSafe: `index out of bounds: index 32, len 32` at
//! `BitStack.zig:50`, exit 134, for arrays and objects alike), and in
//! ReleaseFast the check is compiled out and the fixed 32-byte stack is
//! written out of bounds. Here [`stringify`] and [`stringify_object`] check
//! the depth first and return an [`std::io::ErrorKind::InvalidInput`] error
//! without writing anything, so the documented 256-level limit is enforced as
//! an error. Parsing stays unlimited, as in the reference, so an envelope
//! whose headers nest deeper than 256 still parses (the reference's
//! `sendInternal` only parses, then writes the raw bytes). Reachability: the
//! reference's `Envelope.serialize` is called only by its tests; the input
//! is sentry-native's own envelope.
//!
//! The port's own depth bound: parsing is iterative and [`Value`]'s `Drop` is
//! iterative, so neither overflows the stack at any depth; [`stringify`]
//! recurses at most 256 levels. The derived `Clone`, `PartialEq` and `Debug`
//! still recurse per level; nothing outside tests calls them on parsed input.

use std::io::Write;

/// `std.json.ObjectMap` — `StringArrayHashMap(Value)`: insertion-ordered;
/// `put` on an existing key keeps its position, `swapRemove` moves the last
/// entry into the hole (`IndexMap::swap_remove`).
pub type ObjectMap = indexmap::IndexMap<String, Value>;

/// `std.json.Value`.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Integer(i64),
    Float(f64),
    /// A number `parseFromNumberSlice` could not hold: an integer outside
    /// `i64`, or a float that is not finite as an `f64`. Kept verbatim.
    NumberString(String),
    String(String),
    Array(Vec<Value>),
    Object(ObjectMap),
}

/// Dropping a deeply nested value must not recurse once per level (a
/// 20,000-level array overflowed the 2 MB test-thread stack with the derived
/// drop glue). The children are moved onto a heap stack instead, so every
/// `Value` is dropped with no nested container left in it.
impl Drop for Value {
    fn drop(&mut self) {
        fn take_children(v: &mut Value, stack: &mut Vec<Value>) {
            match v {
                Value::Array(items) => stack.append(items),
                Value::Object(map) => stack.extend(map.drain(..).map(|(_, v)| v)),
                _ => {}
            }
        }
        let mut stack = Vec::new();
        take_children(self, &mut stack);
        while let Some(mut v) = stack.pop() {
            take_children(&mut v, &mut stack);
        }
    }
}

/// The maximum nesting depth `std.json.Stringify` supports
/// (`Stringify.zig:74`, `:572`).
pub const MAX_STRINGIFY_DEPTH: usize = 256;

/// The nesting depth of `value` as `Stringify` counts it (`[[[]]]` is 3; a
/// scalar is 0), computed without recursion.
fn nesting_depth(value: &Value) -> usize {
    let mut max = 0;
    let mut stack: Vec<(&Value, usize)> = vec![(value, 0)];
    while let Some((v, depth)) = stack.pop() {
        match v {
            Value::Array(items) => {
                max = max.max(depth + 1);
                stack.extend(items.iter().map(|c| (c, depth + 1)));
            }
            Value::Object(map) => {
                max = max.max(depth + 1);
                stack.extend(map.values().map(|c| (c, depth + 1)));
            }
            _ => {}
        }
    }
    max
}

/// The error [`stringify`] returns where the reference panics (UD182).
fn too_deep() -> std::io::Error {
    std::io::Error::new(
        std::io::ErrorKind::InvalidInput,
        "JSON nesting depth exceeds std.json.Stringify's maximum of 256",
    )
}

/// The `std.json` parse errors reachable from `parseFromSliceLeaky` on a
/// complete slice (`OutOfMemory` aborts in Rust; `ValueTooLong` cannot occur
/// because the default `max_value_len` is the input length).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseError {
    SyntaxError,
    UnexpectedEndOfInput,
    DuplicateField,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            ParseError::SyntaxError => "SyntaxError",
            ParseError::UnexpectedEndOfInput => "UnexpectedEndOfInput",
            ParseError::DuplicateField => "DuplicateField",
        };
        write!(f, "error.{name}")
    }
}

impl std::error::Error for ParseError {}

/// `Value.parseFromNumberSlice` (`dynamic.zig:30-45`).
fn value_from_number_slice(s: &str) -> Value {
    // `Scanner.isNumberFormattedLikeAnInteger`: `-0` is NOT integer-like.
    let integer_like = s != "-0" && !s.contains(['.', 'e', 'E']);
    if !integer_like {
        // `std.fmt.parseFloat` and Rust's `f64::from_str` are both correctly
        // rounded, so they agree on every JSON float literal.
        let f: f64 = s
            .parse()
            .expect("the scanner only yields valid JSON numbers");
        return if f.is_finite() {
            Value::Float(f)
        } else {
            Value::NumberString(s.to_string())
        };
    }
    match s.parse::<i64>() {
        Ok(i) => Value::Integer(i),
        // `error.Overflow => .number_string`
        Err(_) => Value::NumberString(s.to_string()),
    }
}

/// A container being built (`jsonParse`'s explicit stack; no recursion, so no
/// parse depth limit, as in the reference's parser).
enum Frame {
    Array(Vec<Value>),
    /// The object so far and the key whose value is being parsed.
    Object(ObjectMap, String),
}

struct Scanner<'a> {
    input: &'a [u8],
    i: usize,
}

impl Scanner<'_> {
    fn skip_whitespace(&mut self) {
        while let Some(b' ' | b'\t' | b'\n' | b'\r') = self.input.get(self.i) {
            self.i += 1;
        }
    }

    fn peek(&self) -> Option<u8> {
        self.input.get(self.i).copied()
    }

    /// The next non-whitespace byte, or `UnexpectedEndOfInput`.
    fn next_significant(&mut self) -> Result<u8, ParseError> {
        self.skip_whitespace();
        self.peek().ok_or(ParseError::UnexpectedEndOfInput)
    }

    /// After a number or literal the next byte must end the value.
    fn expect_value_end(&self) -> Result<(), ParseError> {
        match self.peek() {
            None | Some(b' ' | b'\t' | b'\n' | b'\r' | b',' | b']' | b'}') => Ok(()),
            Some(_) => Err(ParseError::SyntaxError),
        }
    }

    fn literal(&mut self, word: &[u8]) -> Result<(), ParseError> {
        for &want in word {
            match self.peek() {
                None => return Err(ParseError::UnexpectedEndOfInput),
                Some(b) if b == want => self.i += 1,
                Some(_) => return Err(ParseError::SyntaxError),
            }
        }
        self.expect_value_end()
    }

    fn digits(&mut self) -> usize {
        let start = self.i;
        while let Some(b'0'..=b'9') = self.peek() {
            self.i += 1;
        }
        self.i - start
    }

    /// One JSON number, per the Scanner's grammar.
    fn number(&mut self) -> Result<Value, ParseError> {
        let start = self.i;
        if self.peek() == Some(b'-') {
            self.i += 1;
        }
        match self.peek() {
            None => return Err(ParseError::UnexpectedEndOfInput),
            Some(b'0') => self.i += 1,
            Some(b'1'..=b'9') => {
                self.digits();
            }
            Some(_) => return Err(ParseError::SyntaxError),
        }
        if self.peek() == Some(b'.') {
            self.i += 1;
            match self.peek() {
                None => return Err(ParseError::UnexpectedEndOfInput),
                Some(b'0'..=b'9') => {
                    self.digits();
                }
                Some(_) => return Err(ParseError::SyntaxError),
            }
        }
        if let Some(b'e' | b'E') = self.peek() {
            self.i += 1;
            if let Some(b'+' | b'-') = self.peek() {
                self.i += 1;
            }
            match self.peek() {
                None => return Err(ParseError::UnexpectedEndOfInput),
                Some(b'0'..=b'9') => {
                    self.digits();
                }
                Some(_) => return Err(ParseError::SyntaxError),
            }
        }
        self.expect_value_end()?;
        let text = std::str::from_utf8(&self.input[start..self.i]).expect("ASCII");
        Ok(value_from_number_slice(text))
    }

    fn hex4(&mut self) -> Result<u16, ParseError> {
        let mut v: u16 = 0;
        for _ in 0..4 {
            let b = self.peek().ok_or(ParseError::UnexpectedEndOfInput)?;
            let d = (b as char).to_digit(16).ok_or(ParseError::SyntaxError)?;
            v = (v << 4) | d as u16;
            self.i += 1;
        }
        Ok(v)
    }

    /// One JSON string (the opening `"` already consumed). Raw bytes must be
    /// valid UTF-8 and not control characters; escapes are decoded, and a
    /// `\u` surrogate must be a complete high+low pair.
    fn string(&mut self) -> Result<String, ParseError> {
        let mut out: Vec<u8> = Vec::new();
        loop {
            let b = self.peek().ok_or(ParseError::UnexpectedEndOfInput)?;
            match b {
                b'"' => {
                    self.i += 1;
                    return String::from_utf8(out).map_err(|_| ParseError::SyntaxError);
                }
                0x00..=0x1F => return Err(ParseError::SyntaxError),
                b'\\' => {
                    self.i += 1;
                    let e = self.peek().ok_or(ParseError::UnexpectedEndOfInput)?;
                    self.i += 1;
                    let c: char = match e {
                        b'"' => '"',
                        b'\\' => '\\',
                        b'/' => '/',
                        b'b' => '\u{8}',
                        b'f' => '\u{c}',
                        b'n' => '\n',
                        b'r' => '\r',
                        b't' => '\t',
                        b'u' => {
                            let hi = self.hex4()?;
                            match hi {
                                0xD800..=0xDBFF => {
                                    for want in [b'\\', b'u'] {
                                        match self.peek() {
                                            None => return Err(ParseError::UnexpectedEndOfInput),
                                            Some(got) if got == want => self.i += 1,
                                            Some(_) => return Err(ParseError::SyntaxError),
                                        }
                                    }
                                    let lo = self.hex4()?;
                                    if !(0xDC00..=0xDFFF).contains(&lo) {
                                        return Err(ParseError::SyntaxError);
                                    }
                                    let cp = 0x10000
                                        + ((u32::from(hi) - 0xD800) << 10)
                                        + (u32::from(lo) - 0xDC00);
                                    char::from_u32(cp).ok_or(ParseError::SyntaxError)?
                                }
                                0xDC00..=0xDFFF => return Err(ParseError::SyntaxError),
                                _ => {
                                    char::from_u32(u32::from(hi)).ok_or(ParseError::SyntaxError)?
                                }
                            }
                        }
                        _ => return Err(ParseError::SyntaxError),
                    };
                    let mut buf = [0u8; 4];
                    out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
                }
                0x80..=0xFF => {
                    // The Scanner validates UTF-8 as it goes: a byte that
                    // cannot continue a valid sequence is a `SyntaxError` at
                    // once, while input ending mid-sequence is
                    // `UnexpectedEndOfInput` (MEASURED: `"\xff` vs `"\xe2\x82`).
                    let (len, lo2, hi2): (usize, u8, u8) = match b {
                        0xC2..=0xDF => (2, 0x80, 0xBF),
                        0xE0 => (3, 0xA0, 0xBF),
                        0xE1..=0xEC | 0xEE..=0xEF => (3, 0x80, 0xBF),
                        0xED => (3, 0x80, 0x9F),
                        0xF0 => (4, 0x90, 0xBF),
                        0xF1..=0xF3 => (4, 0x80, 0xBF),
                        0xF4 => (4, 0x80, 0x8F),
                        _ => return Err(ParseError::SyntaxError),
                    };
                    out.push(b);
                    self.i += 1;
                    for k in 1..len {
                        let c = self.peek().ok_or(ParseError::UnexpectedEndOfInput)?;
                        let (lo, hi) = if k == 1 { (lo2, hi2) } else { (0x80, 0xBF) };
                        if c < lo || c > hi {
                            return Err(ParseError::SyntaxError);
                        }
                        out.push(c);
                        self.i += 1;
                    }
                }
                _ => {
                    out.push(b);
                    self.i += 1;
                }
            }
        }
    }
}

/// `std.json.parseFromSliceLeaky(std.json.Value, alloc, input, .{})`.
pub fn parse_from_slice(input: &[u8]) -> Result<Value, ParseError> {
    let mut s = Scanner { input, i: 0 };
    let mut stack: Vec<Frame> = Vec::new();

    'value: loop {
        // Parse one value (or open a container).
        let b = s.next_significant()?;
        let mut value = match b {
            b'{' => {
                s.i += 1;
                match s.next_significant()? {
                    b'}' => {
                        s.i += 1;
                        Value::Object(ObjectMap::new())
                    }
                    b'"' => {
                        s.i += 1;
                        let key = s.string()?;
                        if s.next_significant()? != b':' {
                            return Err(ParseError::SyntaxError);
                        }
                        s.i += 1;
                        stack.push(Frame::Object(ObjectMap::new(), key));
                        continue 'value;
                    }
                    _ => return Err(ParseError::SyntaxError),
                }
            }
            b'[' => {
                s.i += 1;
                if s.next_significant()? == b']' {
                    s.i += 1;
                    Value::Array(Vec::new())
                } else {
                    stack.push(Frame::Array(Vec::new()));
                    continue 'value;
                }
            }
            b'"' => {
                s.i += 1;
                Value::String(s.string()?)
            }
            b't' => {
                s.literal(b"true")?;
                Value::Bool(true)
            }
            b'f' => {
                s.literal(b"false")?;
                Value::Bool(false)
            }
            b'n' => {
                s.literal(b"null")?;
                Value::Null
            }
            b'-' | b'0'..=b'9' => s.number()?,
            _ => return Err(ParseError::SyntaxError),
        };

        // `handleCompleteValue`: attach the finished value to its parent,
        // closing every container that ends here.
        loop {
            match stack.last_mut() {
                None => {
                    s.skip_whitespace();
                    if s.peek().is_some() {
                        return Err(ParseError::SyntaxError);
                    }
                    return Ok(value);
                }
                Some(Frame::Array(items)) => {
                    items.push(value);
                    match s.next_significant()? {
                        b',' => {
                            s.i += 1;
                            continue 'value;
                        }
                        b']' => {
                            s.i += 1;
                            let Some(Frame::Array(items)) = stack.pop() else {
                                unreachable!()
                            };
                            value = Value::Array(items);
                        }
                        _ => return Err(ParseError::SyntaxError),
                    }
                }
                Some(Frame::Object(map, key)) => {
                    // `getOrPut`; `duplicate_field_behavior` defaults to `.@"error"`.
                    if map.contains_key(key.as_str()) {
                        return Err(ParseError::DuplicateField);
                    }
                    map.insert(std::mem::take(key), value);
                    match s.next_significant()? {
                        b',' => {
                            s.i += 1;
                            if s.next_significant()? != b'"' {
                                return Err(ParseError::SyntaxError);
                            }
                            s.i += 1;
                            *key = s.string()?;
                            if s.next_significant()? != b':' {
                                return Err(ParseError::SyntaxError);
                            }
                            s.i += 1;
                            continue 'value;
                        }
                        b'}' => {
                            s.i += 1;
                            let Some(Frame::Object(map, _)) = stack.pop() else {
                                unreachable!()
                            };
                            value = Value::Object(map);
                        }
                        _ => return Err(ParseError::SyntaxError),
                    }
                }
            }
        }
    }
}

/// `Stringify.encodeJsonString` with default options (no `escape_unicode`):
/// only `"`, `\` and bytes below 0x20 are escaped.
fn write_string<W: Write>(w: &mut W, s: &str) -> std::io::Result<()> {
    w.write_all(b"\"")?;
    let bytes = s.as_bytes();
    let mut cursor = 0;
    for (i, &c) in bytes.iter().enumerate() {
        let esc: Option<&[u8]> = match c {
            b'\\' => Some(b"\\\\"),
            b'"' => Some(b"\\\""),
            0x08 => Some(b"\\b"),
            0x0C => Some(b"\\f"),
            b'\n' => Some(b"\\n"),
            b'\r' => Some(b"\\r"),
            b'\t' => Some(b"\\t"),
            0x00..=0x1F => None,
            _ => continue,
        };
        w.write_all(&bytes[cursor..i])?;
        match esc {
            Some(e) => w.write_all(e)?,
            // `outputUnicodeEscape`: `\u` + four lowercase hex digits.
            None => write!(w, "\\u{c:0>4x}")?,
        }
        cursor = i + 1;
    }
    w.write_all(&bytes[cursor..])?;
    w.write_all(b"\"")
}

/// `std.json.fmt(value, .{ .whitespace = .minified })`.
///
/// PORT NOTE: reference defect (R13), UD182: a value nested deeper than
/// [`MAX_STRINGIFY_DEPTH`] is an `InvalidInput` error, with nothing written;
/// the reference panics (see the module header).
pub fn stringify<W: Write>(w: &mut W, value: &Value) -> std::io::Result<()> {
    if nesting_depth(value) > MAX_STRINGIFY_DEPTH {
        return Err(too_deep());
    }
    write_value(w, value)
}

/// The printer behind [`stringify`]. The caller has checked the depth, so this
/// recurses at most [`MAX_STRINGIFY_DEPTH`] levels.
fn write_value<W: Write>(w: &mut W, value: &Value) -> std::io::Result<()> {
    match value {
        Value::Null => w.write_all(b"null"),
        Value::Bool(b) => w.write_all(if *b { b"true" } else { b"false" }),
        Value::Integer(i) => write!(w, "{i}"),
        // `writer.print("{}", .{f64})`: MEASURED identical to Rust's `Display`
        // for every finite value in the golden file (`2.0` -> `2`, `-0.0` ->
        // `-0`, `1e100` -> 101 digits, `5e-324` -> the full decimal). A parsed
        // value is always finite (non-finite floats become `.number_string`).
        Value::Float(f) => write!(w, "{f}"),
        Value::NumberString(s) => w.write_all(s.as_bytes()),
        Value::String(s) => write_string(w, s),
        Value::Array(items) => {
            w.write_all(b"[")?;
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    w.write_all(b",")?;
                }
                write_value(w, item)?;
            }
            w.write_all(b"]")
        }
        Value::Object(map) => write_object(w, map),
    }
}

/// `std.json.fmt(std.json.Value{ .object = map }, minified)`, with the same
/// depth check as [`stringify`] (the object itself is one level; UD182).
pub fn stringify_object<W: Write>(w: &mut W, map: &ObjectMap) -> std::io::Result<()> {
    let children = map.values().map(nesting_depth).max().unwrap_or(0);
    if 1 + children > MAX_STRINGIFY_DEPTH {
        return Err(too_deep());
    }
    write_object(w, map)
}

fn write_object<W: Write>(w: &mut W, map: &ObjectMap) -> std::io::Result<()> {
    w.write_all(b"{")?;
    for (i, (k, v)) in map.iter().enumerate() {
        if i > 0 {
            w.write_all(b",")?;
        }
        write_string(w, k)?;
        w.write_all(b":")?;
        write_value(w, v)?;
    }
    w.write_all(b"}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unhex(s: &str) -> Vec<u8> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }

    /// Every case in the golden file: the input, and what zig 0.16.0's
    /// `parseFromSliceLeaky(std.json.Value, ..)` + `std.json.fmt(.., minified)`
    /// produced for it (`ok <hex output>` or `err <error name>`). Generated by
    /// running those two calls over the inputs (scratch probe, task T-E).
    #[test]
    fn matches_zig_std_json_golden() {
        let golden = include_str!("testdata/std_json_golden.tsv");
        let mut n = 0;
        for line in golden.lines() {
            let mut parts = line.split('\t');
            let input = unhex(parts.next().unwrap());
            let kind = parts.next().unwrap();
            let want = parts.next().unwrap();
            let got = match parse_from_slice(&input) {
                Ok(v) => {
                    let mut out = Vec::new();
                    stringify(&mut out, &v).unwrap();
                    ("ok", out)
                }
                Err(e) => ("err", format!("{e:?}").into_bytes()),
            };
            let want_bytes = if kind == "ok" {
                unhex(want)
            } else {
                want.as_bytes().to_vec()
            };
            assert_eq!(
                (kind, want_bytes.as_slice()),
                (got.0, got.1.as_slice()),
                "input {:?}",
                String::from_utf8_lossy(&input)
            );
            n += 1;
        }
        assert_eq!(143, n, "the whole golden file was checked");
    }

    fn nested(depth: usize, object: bool) -> String {
        let (open, close) = if object { ("{\"a\":", "}") } else { ("[", "]") };
        format!("{}0{}", open.repeat(depth), close.repeat(depth))
    }

    /// MEASURED on zig 0.16.0 ReleaseSafe (`parseFromSliceLeaky` +
    /// `std.json.fmt(.., minified)`): 256 levels round-trip, for arrays and
    /// objects.
    #[test]
    fn stringify_round_trips_the_documented_256_levels() {
        for object in [false, true] {
            let input = nested(MAX_STRINGIFY_DEPTH, object);
            let v = parse_from_slice(input.as_bytes()).unwrap();
            assert_eq!(MAX_STRINGIFY_DEPTH, nesting_depth(&v));
            let mut out = Vec::new();
            stringify(&mut out, &v).unwrap();
            assert_eq!(input.as_bytes(), out.as_slice());
            if let Value::Object(map) = &v {
                let mut out = Vec::new();
                stringify_object(&mut out, map).unwrap();
                assert_eq!(input.as_bytes(), out.as_slice());
            }
        }
    }

    /// Reference defect (R13), UD182: at 257 levels zig 0.16.0's
    /// `std.json.fmt` panics (MEASURED, ReleaseSafe, exit 134, arrays and
    /// objects). The port returns an `InvalidInput` error and writes nothing.
    /// Negative control: drop the depth check in `stringify` (then in
    /// `stringify_object`) and this fails on `unwrap_err`.
    #[test]
    fn stringify_rejects_depth_257_with_an_error() {
        let mut objects_checked = 0;
        for object in [false, true] {
            let input = nested(MAX_STRINGIFY_DEPTH + 1, object);
            // The parser has no limit (MEASURED: zig parses it too).
            let v = parse_from_slice(input.as_bytes()).unwrap();
            let mut out = Vec::new();
            let err = stringify(&mut out, &v).unwrap_err();
            assert_eq!(std::io::ErrorKind::InvalidInput, err.kind());
            assert!(out.is_empty(), "nothing is written before the error");

            // The object entry point counts the object itself as a level.
            if let Value::Object(map) = &v {
                let mut out = Vec::new();
                let err = stringify_object(&mut out, map).unwrap_err();
                assert_eq!(std::io::ErrorKind::InvalidInput, err.kind());
                assert!(out.is_empty());
                objects_checked += 1;
            }
        }
        assert_eq!(1, objects_checked);
    }

    /// CORRECTION: this test was `deep_nesting_has_no_limit` and claimed
    /// `std.json` has no nesting limit. Its parser has none (MEASURED on zig
    /// 0.16.0: 100,000 levels parse), so the port parses any depth, and a
    /// parsed value must drop without overflowing the stack. Negative
    /// control: remove `impl Drop for Value` and this test aborts the test
    /// binary with "has overflowed its stack".
    #[test]
    fn parse_and_drop_any_depth_without_stack_overflow() {
        for object in [false, true] {
            let depth = 200_000;
            let input = nested(depth, object);
            let v = parse_from_slice(input.as_bytes()).unwrap();
            assert_eq!(depth, nesting_depth(&v));
            // Stringify refuses it rather than recursing 200,000 levels.
            let mut out = Vec::new();
            let err = stringify(&mut out, &v).unwrap_err();
            assert_eq!(std::io::ErrorKind::InvalidInput, err.kind());
            drop(v);
        }
    }
}
