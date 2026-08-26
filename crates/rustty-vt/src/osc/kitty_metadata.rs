//! Helpers for parsing metadata shared by Kitty OSC protocols.
//!
//! Kitty OSC 99 and OSC 5522 encode metadata as colon-separated `key=value`
//! fields. The iterator in this module lazily searches that metadata for one
//! key, preserving the order of repeated values without allocating.
//!
//! Parsing is intentionally tolerant. Whitespace around keys and values is
//! trimmed, while malformed fields, non-matching keys, and invalid values are
//! skipped. Returned values are slices of the original metadata and remain
//! valid only as long as that input remains valid.
//!
//! PORTS: ghostty src/terminal/osc/kitty_metadata.zig
//!
//! PORT STATUS: ported — `ValueIterator` + its 2 tests (P5-W5 / B-2
//! `ca9e5b130`; the transient `osc/lib.zig` intermediate form of `07f33ad91`
//! was deleted upstream and is deliberately NOT ported).

// PORT NOTE: `const std = @import("std");` — std facilities are used inline.

/// `std.ascii.whitespace` — space, `\t`, `\n`, `\r`, VT (0x0B), FF (0x0C).
/// (Rust's `u8::is_ascii_whitespace` omits VT, so the exact Zig set is spelled
/// out for fidelity.)
const ASCII_WHITESPACE: &[u8] = &[b' ', b'\t', b'\n', b'\r', 0x0B, 0x0C];

// PORT NOTE: `std.mem.trim(u8, slice, &std.ascii.whitespace)` — strip leading
// and trailing ASCII whitespace bytes from the slice.
fn trim_whitespace(s: &[u8]) -> &[u8] {
    let mut start = 0;
    let mut end = s.len();
    while start < end && ASCII_WHITESPACE.contains(&s[start]) {
        start += 1;
    }
    while end > start && ASCII_WHITESPACE.contains(&s[end - 1]) {
        end -= 1;
    }
    &s[start..end]
}

/// Return an iterator over values whose key exactly matches `key`.
///
/// If `valid_value_characters` is non-null, every byte in a returned value must
/// appear in that character set. Passing null disables value validation. Both
/// arguments are comptime-known so each protocol can specialize the iterator
/// for its metadata grammar without storing a key or validator at runtime.
//
// pub fn ValueIterator(
//     comptime key: []const u8,
//     comptime valid_value_characters: ?[]const u8,
// ) type { ... }
//
// PORT NOTE (R8/D4 written argument): the reference type-constructor takes the
// key and validator as COMPTIME parameters and specializes a struct per
// protocol option. Rust cannot carry `&'static [u8]` const generics on stable,
// and a per-key marker-trait ladder would force each consumer's dependent
// `Option.Type()` dispatch (already runtime-modeled per the
// kitty_clipboard_protocol.rs precedent) to erase the types again. The port
// therefore carries both parameters as `'static` runtime fields: every call
// site supplies fixed `&'static` byte-string constants (the option enums'
// `tag_name()` tables and the protocols' `valid_*_characters` consts), so no
// runtime bytes can alias the compile-time grammar; behavior is byte-identical
// and locked by this module's 2 tests plus the 57 OSC 99 parser tests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValueIterator<'a> {
    /// `comptime key: []const u8` (see the PORT NOTE above).
    key: &'static [u8],
    /// `comptime valid_value_characters: ?[]const u8` (see the PORT NOTE
    /// above).
    valid_value_characters: Option<&'static [u8]>,

    pub metadata: &'a [u8],
    pub pos: usize,
}

impl<'a> ValueIterator<'a> {
    /// Initialize an iterator borrowing `metadata`.
    // pub fn init(metadata: []const u8) Self {
    //     return .{ .metadata = metadata, .pos = 0 };
    // }
    pub fn init(
        key: &'static [u8],
        valid_value_characters: Option<&'static [u8]>,
        metadata: &'a [u8],
    ) -> ValueIterator<'a> {
        ValueIterator {
            key,
            valid_value_characters,
            metadata,
            pos: 0,
        }
    }

    /// Return the next valid matching value, or null when none remain.
    /// The returned slice borrows the metadata passed to `init`.
    // PORT NOTE: named `next` for fidelity with the reference (`pub fn next`);
    // deliberately a plain method rather than an `Iterator` impl (same
    // convention as the other ported `next` iterators — see sgr.rs).
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Option<&'a [u8]> {
        while self.pos < self.metadata.len() {
            // const end = std.mem.indexOfScalarPos(u8, self.metadata, self.pos, ':')
            //     orelse self.metadata.len;
            let end = match self.metadata[self.pos..].iter().position(|&b| b == b':') {
                Some(rel) => self.pos + rel,
                None => self.metadata.len(),
            };
            let field = &self.metadata[self.pos..end];
            self.pos = if end < self.metadata.len() {
                end + 1
            } else {
                end
            };

            // const equals = std.mem.indexOfScalar(u8, field, '=') orelse continue;
            let equals = match field.iter().position(|&b| b == b'=') {
                Some(i) => i,
                None => continue,
            };
            let field_key = trim_whitespace(&field[0..equals]);
            if field_key != self.key {
                continue;
            }

            let value = trim_whitespace(&field[equals + 1..]);
            if let Some(valid) = self.valid_value_characters {
                // if (std.mem.indexOfNone(u8, value, valid) != null) continue;
                if value.iter().any(|b| !valid.contains(b)) {
                    continue;
                }
            }

            return Some(value);
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // test "ValueIterator skips malformed and prefix-matching keys" {
    #[test]
    fn value_iterator_skips_malformed_and_prefix_matching_keys() {
        // var it: ValueIterator("id", null) = .init(
        //     "id-extra=wrong:id: id = first :id=second",
        // );
        let mut it = ValueIterator::init(b"id", None, b"id-extra=wrong:id: id = first :id=second");

        // try testing.expectEqualStrings("first", it.next().?);
        assert_eq!(b"first".as_slice(), it.next().unwrap());
        // try testing.expectEqualStrings("second", it.next().?);
        assert_eq!(b"second".as_slice(), it.next().unwrap());
        // try testing.expect(it.next() == null);
        assert!(it.next().is_none());
    }

    // test "ValueIterator skips values containing disallowed characters" {
    #[test]
    fn value_iterator_skips_values_containing_disallowed_characters() {
        // var it: ValueIterator("i", "abc") = .init("i=a?:i=abc");
        let mut it = ValueIterator::init(b"i", Some(b"abc"), b"i=a?:i=abc");

        // try testing.expectEqualStrings("abc", it.next().?);
        assert_eq!(b"abc".as_slice(), it.next().unwrap());
        // try testing.expect(it.next() == null);
        assert!(it.next().is_none());
    }
}
