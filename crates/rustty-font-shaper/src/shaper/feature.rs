//! rustty-font-shaper::shape::feature
//!
//! PORTS: ghostty `src/font/shaper/feature.zig` (354 loc, 2 tests) — OpenType
//! font-feature settings parsing (a tolerant subset of HarfBuzz's feature-string
//! syntax).

// const std = @import("std");
// const assert = @import("../../quirks.zig").inlineAssert;
// const Allocator = std.mem.Allocator;
use rustty_util::quirks::inlineAssert as assert;

// const log = std.log.scoped(.font_shaper);
// PORT NOTE: this crate has no logger wired; `log.warn` sites are preserved as
// comments (matching `collection.rs` / `codepoint_resolver.rs`).

/// A minimal byte reader mirroring the reference's
/// `std.io.fixedBufferStream(str).reader()`. `read_byte` yields the next byte or
/// `,` at end-of-stream (the reference `reader.readByte() catch ','`), leaving
/// `pos` at the end so the caller can observe exhaustion. `pos` is public so
/// `FeatureList.appendFromString` can track boundaries exactly like `fbs.pos`.
pub struct ByteReader<'a> {
    pub buf: &'a [u8],
    pub pos: usize,
}

impl<'a> ByteReader<'a> {
    pub fn new(buf: &'a [u8]) -> ByteReader<'a> {
        ByteReader { buf, pos: 0 }
    }

    /// `reader.readByte() catch ','` — next byte, or `,` on EOF (without
    /// advancing past the end).
    fn read_byte(&mut self) -> u8 {
        if self.pos < self.buf.len() {
            let b = self.buf[self.pos];
            self.pos += 1;
            b
        } else {
            b','
        }
    }

    /// `reader.skipUntilDelimiterOrEof(delim)` — advance past the next `delim`
    /// or to end-of-stream.
    fn skip_until_delimiter_or_eof(&mut self, delim: u8) {
        while self.pos < self.buf.len() {
            let b = self.buf[self.pos];
            self.pos += 1;
            if b == delim {
                break;
            }
        }
    }
}

/// Represents an OpenType font feature setting, which consists of a tag and
/// a numeric parameter >= 0. Most features are boolean, so only parameters
/// of 0 and 1 make sense for them, but some (e.g. 'cv01'..'cv99') can take
/// parameters to choose between multiple variants of a given character or
/// characters.
///
/// Ref:
/// - https://learn.microsoft.com/en-us/typography/opentype/spec/chapter2#features-and-lookups
/// - https://harfbuzz.github.io/shaping-opentype-features.html
// pub const Feature = struct { tag: [4]u8, value: u32, ... };
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Feature {
    pub tag: [u8; 4],
    pub value: u32,
}

/// The parser state (the reference's inline `enum` on the labeled `switch`).
enum State {
    /// Initial state.
    Start,
    /// Parsing the tag.
    Tag,
    /// In the space between the tag and the value.
    Space,
    /// Parsing an integer parameter directly in to `value`.
    Int,
    /// Parsing a boolean keyword parameter ("on"/"off").
    Bool,
    /// Encountered an unrecoverable syntax error, advancing to boundary.
    Err,
    /// Done parsing feature, skip whitespace until end.
    Done,
}

impl Feature {
    // pub fn fromString(str: []const u8) ?Feature {
    //     var fbs = std.io.fixedBufferStream(str);
    //     const reader = fbs.reader();
    //     return .fromReader(reader);
    // }
    pub fn from_string(str: &[u8]) -> Option<Feature> {
        let mut reader = ByteReader::new(str);
        Feature::from_reader(&mut reader)
    }

    /// Parse a single font feature setting from a std.io.Reader, with a version
    /// of the syntax of HarfBuzz's font feature strings. Stops at end of stream
    /// or when a ',' is encountered.
    ///
    /// This parsing aims to be as error-tolerant as possible while avoiding any
    /// assumptions in ambiguous scenarios. When invalid syntax is encountered,
    /// the reader is advanced to the next boundary (end-of-stream or ',') so
    /// that further features may be read.
    ///
    /// Ref: https://harfbuzz.github.io/harfbuzz-hb-common.html#hb-feature-from-string
    // pub fn fromReader(reader: anytype) ?Feature {
    pub fn from_reader(reader: &mut ByteReader) -> Option<Feature> {
        let mut tag_buf: [u8; 4] = [0; 4];
        // PORT NOTE: the reference tracks the tag as a slice `tag = tag_buf[0..0]`
        // whose `.len` grows; here we track the length explicitly.
        let mut tag_len: usize = 0;
        let mut value: Option<u32> = None;

        // state: switch (enum { ... }.start) { ... }
        //
        // PORT NOTE: Zig's labeled `switch` with `continue :state .X` becomes a
        // `'state` loop over the `State` enum. `continue :state .X` -> `{ state =
        // X; continue 'state; }`; a bare inner `break` (which exits the labeled
        // switch and falls through to the final asserts+return) -> `break 'state`;
        // `return null` -> `return None`; a bare `continue` -> `continue` the
        // inner loop.
        let mut state = State::Start;
        'state: loop {
            match state {
                // If we're done then we skip whitespace until we see a ','.
                State::Done => {
                    loop {
                        match reader.read_byte() {
                            b' ' | b'\t' => continue,
                            b',' => break,
                            // If we see something other than whitespace or a ','
                            // then this is an error since the intent is unclear.
                            _ => {
                                state = State::Err;
                                continue 'state;
                            }
                        }
                    }
                    break 'state;
                }

                // If we're fast-forwarding from an error we just wanna
                // stop at the first boundary and ignore all other bytes.
                State::Err => {
                    reader.skip_until_delimiter_or_eof(b',');
                    return None;
                }

                State::Start => loop {
                    match reader.read_byte() {
                        // Ignore leading whitespace.
                        b' ' | b'\t' => continue,
                        // Empty feature string.
                        b',' => return None,
                        // '+' prefix to explicitly enable feature.
                        b'+' => {
                            value = Some(1);
                            state = State::Tag;
                            continue 'state;
                        }
                        // '-' prefix to explicitly disable feature.
                        b'-' => {
                            value = Some(0);
                            state = State::Tag;
                            continue 'state;
                        }
                        // Quote mark introducing a tag.
                        b'"' | b'\'' => {
                            state = State::Tag;
                            continue 'state;
                        }
                        // First letter of tag.
                        byte => {
                            tag_len = 1;
                            tag_buf[0] = byte;
                            state = State::Tag;
                            continue 'state;
                        }
                    }
                },

                State::Tag => loop {
                    match reader.read_byte() {
                        // If the tag is interrupted by a comma it's invalid.
                        b',' => return None,
                        // Ignore quote marks. This does technically ignore cases
                        // like "'k'e'r'n' = 0", but it's unambiguous so if someone
                        // really wants to do that in their config then... sure why
                        // not.
                        b'"' | b'\'' => continue,
                        // In all other cases we add the byte to our tag.
                        byte => {
                            tag_len += 1;
                            tag_buf[tag_len - 1] = byte;
                            if tag_len == 4 {
                                state = State::Space;
                                continue 'state;
                            }
                        }
                    }
                },

                State::Space => loop {
                    match reader.read_byte() {
                        b' ' | b'\t' => continue,
                        // Ignore quote marks since we might have a
                        // closing quote from the tag still ahead.
                        b'"' | b'\'' => continue,
                        // Allow an '=' (which we can safely ignore)
                        // only if we don't already have a value due
                        // to a '+' or '-' prefix.
                        b'=' => {
                            if value.is_some() {
                                state = State::Err;
                                continue 'state;
                            }
                        }
                        b',' => {
                            // Specifying only a tag turns a feature on.
                            if value.is_none() {
                                value = Some(1);
                            }
                            break 'state;
                        }
                        byte @ b'0'..=b'9' => {
                            // If we already have value because of a
                            // '+' or '-' prefix then this is an error.
                            if value.is_some() {
                                state = State::Err;
                                continue 'state;
                            }
                            value = Some((byte - b'0') as u32);
                            state = State::Int;
                            continue 'state;
                        }
                        b'o' | b'O' => {
                            // If we already have value because of a
                            // '+' or '-' prefix then this is an error.
                            if value.is_some() {
                                state = State::Err;
                                continue 'state;
                            }
                            state = State::Bool;
                            continue 'state;
                        }
                        _ => {
                            state = State::Err;
                            continue 'state;
                        }
                    }
                    // (fall through: `'=' with value == null` continues the loop)
                },

                State::Int => loop {
                    match reader.read_byte() {
                        b',' => break 'state,
                        byte @ b'0'..=b'9' => {
                            // If our value gets too big while
                            // parsing we consider it an error.
                            let m = match value.unwrap().checked_mul(10) {
                                Some(v) => v,
                                None => {
                                    state = State::Err;
                                    continue 'state;
                                }
                            };
                            value = Some(m + (byte - b'0') as u32);
                        }
                        _ => {
                            state = State::Err;
                            continue 'state;
                        }
                    }
                },

                State::Bool => loop {
                    match reader.read_byte() {
                        b',' => return None,
                        b'n' | b'N' => {
                            // "ofn"
                            if value.is_some() {
                                assert(value == Some(0));
                                state = State::Err;
                                continue 'state;
                            }
                            value = Some(1);
                            state = State::Done;
                            continue 'state;
                        }
                        b'f' | b'F' => {
                            // To make sure we consume two 'f's.
                            if value.is_none() {
                                value = Some(0);
                            } else {
                                assert(value == Some(0));
                                state = State::Done;
                                continue 'state;
                            }
                        }
                        _ => {
                            state = State::Err;
                            continue 'state;
                        }
                    }
                },
            }
        }

        assert(value.is_some());
        assert(tag_len == 4);

        Some(Feature {
            tag: tag_buf,
            value: value.unwrap(),
        })
    }

    /// Serialize this feature to the provided buffer.
    /// The string that this produces should be valid to parse.
    // pub fn toString(self: *const Feature, buf: []u8) !void {
    //     var fbs = std.io.fixedBufferStream(buf);
    //     try self.format("", .{}, fbs.writer());
    // }
    pub fn to_string(&self, buf: &mut [u8]) -> Result<(), ToStringError> {
        let mut fbw = FixedBufWriter { buf, pos: 0 };
        self.write_to(&mut fbw)
            .map_err(|_| ToStringError::NoSpaceLeft)
    }

    /// Formatter core (the reference `format`). Writes the feature to any
    /// `core::fmt::Write` sink, shared by `to_string` and `Display`.
    // pub fn format(self, comptime layout, opts, writer) !void { ... }
    fn write_to<W: core::fmt::Write>(&self, writer: &mut W) -> core::fmt::Result {
        // _ = layout; _ = opts;
        if self.value <= 1 {
            // Format boolean options as "+tag" for on and "-tag" for off.
            // try std.fmt.format(writer, "{c}{s}", .{ "-+"[self.value], self.tag });
            let sign = [b'-', b'+'][self.value as usize];
            writer.write_char(sign as char)?;
            // {s} over `self.tag`: font tags are ASCII, written byte-by-byte.
            for &b in &self.tag {
                writer.write_char(b as char)?;
            }
        } else {
            // Format non-boolean tags as "tag=value".
            // try std.fmt.format(writer, "{s}={d}", .{ self.tag, self.value });
            for &b in &self.tag {
                writer.write_char(b as char)?;
            }
            write!(writer, "={}", self.value)?;
        }
        Ok(())
    }
}

/// Formatter for logging.
impl core::fmt::Display for Feature {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        self.write_to(f)
    }
}

/// Error from `Feature.toString` / `FixedBufWriter` (the reference `!void` is a
/// `NoSpaceLeft` from the fixed-buffer stream).
#[derive(Debug, PartialEq, Eq)]
pub enum ToStringError {
    NoSpaceLeft,
}

/// A `core::fmt::Write` over a fixed byte buffer (mirrors
/// `std.io.fixedBufferStream(buf).writer()`), erroring when out of space.
struct FixedBufWriter<'a> {
    buf: &'a mut [u8],
    pos: usize,
}

impl core::fmt::Write for FixedBufWriter<'_> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let bytes = s.as_bytes();
        if self.pos + bytes.len() > self.buf.len() {
            return Err(core::fmt::Error);
        }
        self.buf[self.pos..self.pos + bytes.len()].copy_from_slice(bytes);
        self.pos += bytes.len();
        Ok(())
    }
}

/// A list of font feature settings (see `Feature` for more documentation).
// pub const FeatureList = struct { features: std.ArrayListUnmanaged(Feature) = .{}, ... };
#[derive(Clone, Debug, Default)]
pub struct FeatureList {
    pub features: Vec<Feature>,
}

impl FeatureList {
    // pub fn deinit(self: *FeatureList, alloc: Allocator) void {
    //     self.features.deinit(alloc);
    // }
    // PORT NOTE: `deinit` frees the `ArrayListUnmanaged`; in Rust `Vec` frees on
    // drop, so this is a no-op kept for parity with the reference API.
    pub fn deinit(&mut self) {
        self.features.clear();
    }

    /// Parse a comma separated list of features.
    /// See `Feature.fromReader` for more docs.
    // pub fn fromString(alloc: Allocator, str: []const u8) !FeatureList {
    pub fn from_string(str: &[u8]) -> FeatureList {
        let mut self_ = FeatureList::default();
        self_.append_from_string(str);
        self_
    }

    /// Append features to this list from a string with a comma separated list.
    /// See `Feature.fromReader` for more docs.
    // pub fn appendFromString(self, alloc, str) !void {
    pub fn append_from_string(&mut self, str: &[u8]) {
        let mut fbs = ByteReader::new(str);
        while fbs.pos < fbs.buf.len() {
            let i = fbs.pos;
            if let Some(feature) = Feature::from_reader(&mut fbs) {
                self.features.push(feature);
            } else {
                // log.warn(
                //     "failed to parse font feature setting: \"{s}\"",
                //     .{fbs.buffer[i..fbs.pos]},
                // );
                let _ = i;
            }
        }
    }
}

/// Formatter for logging.
impl core::fmt::Display for FeatureList {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // for (self.features.items, 0..) |feature, i| {
        //     try feature.format(layout, opts, writer);
        //     if (i != std.features.items.len - 1) try writer.writeAll(", ");
        // }
        for (i, feature) in self.features.iter().enumerate() {
            core::fmt::Display::fmt(feature, f)?;
            if i != self.features.len() - 1 {
                f.write_str(", ")?;
            }
        }
        // PORT NOTE: the reference `FeatureList.format` then contains a
        // copy-paste of `Feature.format`'s body referencing `self.value` /
        // `self.tag` / `std.features` — none of which exist on `FeatureList`.
        // Zig never compiles it because format methods are analyzed lazily and
        // this one is never called; it is dead, un-analyzable code. Rust compiles
        // every item eagerly, so it CANNOT be ported as live code and is
        // preserved verbatim here as a comment (R1: no loss):
        //
        //     if (self.value <= 1) {
        //         // Format boolean options as "+tag" for on and "-tag" for off.
        //         try std.fmt.format(writer, "{c}{s}", .{
        //             "-+"[self.value],
        //             self.tag,
        //         });
        //     } else {
        //         // Format non-boolean tags as "tag=value".
        //         try std.fmt.format(writer, "{s}={d}", .{
        //             self.tag,
        //             self.value,
        //         });
        //     }
        Ok(())
    }
}

/// These features are hardcoded to always be on by default. Users
/// can turn them off by setting the features to "-liga" for example.
// pub const default_features = [_]Feature{ .{ .tag = "liga".*, .value = 1 } };
pub const DEFAULT_FEATURES: [Feature; 1] = [Feature {
    tag: *b"liga",
    value: 1,
}];

#[cfg(test)]
mod tests {
    use super::*;

    // test "Feature.fromString" {
    #[test]
    fn feature_from_string() {
        // This is not *complete* coverage of every possible
        // combination of syntax, but it covers quite a few.

        // Boolean settings (on)
        let kern_on = Feature {
            tag: *b"kern",
            value: 1,
        };
        assert_eq!(Some(kern_on), Feature::from_string(b"kern"));
        assert_eq!(Some(kern_on), Feature::from_string(b"kern, "));
        assert_eq!(Some(kern_on), Feature::from_string(b"kern on"));
        assert_eq!(Some(kern_on), Feature::from_string(b"kern on, "));
        assert_eq!(Some(kern_on), Feature::from_string(b"+kern"));
        assert_eq!(Some(kern_on), Feature::from_string(b"+kern, "));
        assert_eq!(Some(kern_on), Feature::from_string(b"\"kern\" = 1"));
        assert_eq!(Some(kern_on), Feature::from_string(b"\"kern\" = 1, "));

        // Boolean settings (off)
        let kern_off = Feature {
            tag: *b"kern",
            value: 0,
        };
        assert_eq!(Some(kern_off), Feature::from_string(b"kern off"));
        assert_eq!(Some(kern_off), Feature::from_string(b"kern off, "));
        assert_eq!(Some(kern_off), Feature::from_string(b"-'kern'"));
        assert_eq!(Some(kern_off), Feature::from_string(b"-'kern', "));
        assert_eq!(Some(kern_off), Feature::from_string(b"\"kern\" = 0"));
        assert_eq!(Some(kern_off), Feature::from_string(b"\"kern\" = 0, "));

        // Non-boolean settings
        let aalt_2 = Feature {
            tag: *b"aalt",
            value: 2,
        };
        assert_eq!(Some(aalt_2), Feature::from_string(b"aalt=2"));
        assert_eq!(Some(aalt_2), Feature::from_string(b"aalt=2, "));
        assert_eq!(Some(aalt_2), Feature::from_string(b"'aalt' 2"));
        assert_eq!(Some(aalt_2), Feature::from_string(b"'aalt' 2, "));

        // Various ambiguous/error cases which should be null
        assert_eq!(None, Feature::from_string(b"aalt=2x")); // bad number
        assert_eq!(None, Feature::from_string(b"toolong")); // tag too long
        assert_eq!(None, Feature::from_string(b"sht")); // tag too short
        assert_eq!(None, Feature::from_string(b"-kern 1")); // redundant/conflicting
        assert_eq!(None, Feature::from_string(b"-kern on")); // redundant/conflicting
        assert_eq!(None, Feature::from_string(b"aalt=o,")); // bad keyword
        assert_eq!(None, Feature::from_string(b"aalt=ofn,")); // bad keyword
    }

    // test "FeatureList.fromString" {
    #[test]
    fn feature_list_from_string() {
        let str: &[u8] = concat!(
            "  kern, kern on , +kern, \"kern\"  = 1,", // Boolean settings (on)
            "kern    off, -'kern' , \"kern\"=0,",      // Boolean settings (off)
            "aalt=2,  'aalt'\t2,",                     // Non-boolean settings
            // Invalid cases
            "aalt=2x, toolong, sht, -kern 1, -kern on, aalt=o, aalt=ofn,",
            "last", // To ensure final element is included correctly.
        )
        .as_bytes();
        let mut feats = FeatureList::from_string(str);
        // PORT NOTE: `defer feats.deinit(...)` — Rust drops `feats` at scope end;
        // `deinit` is called explicitly at the end to mirror the reference.

        // &(.{kern×4} ++ .{kern_off×3} ++ .{aalt×2} ++ .{last})
        let mut expected: Vec<Feature> = Vec::new();
        expected.extend(
            [Feature {
                tag: *b"kern",
                value: 1,
            }; 4],
        );
        expected.extend(
            [Feature {
                tag: *b"kern",
                value: 0,
            }; 3],
        );
        expected.extend(
            [Feature {
                tag: *b"aalt",
                value: 2,
            }; 2],
        );
        expected.push(Feature {
            tag: *b"last",
            value: 1,
        });
        assert_eq!(expected.as_slice(), feats.features.as_slice());

        feats.deinit();
    }
}
