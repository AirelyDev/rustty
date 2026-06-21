//! PORT NOTE: Ports ghostty `src/lib/string.zig`.

// This is a copy of std.mem.cutPrefix from 0.16. Once Ghostty has been ported
// to 0.16 this can be removed.

/// If slice starts with prefix, returns the rest of slice starting at
/// prefix.len.
pub fn cut_prefix<'a, T: PartialEq>(slice: &'a [T], prefix: &[T]) -> Option<&'a [T]> {
    if slice.starts_with(prefix) {
        Some(&slice[prefix.len()..])
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn cut_prefix() {
        // PORT NOTE: Zig's `expectEqualStrings("foo", cutPrefix(...).?)` — the
        // returned slice is compared as a string.
        assert_eq!(
            "foo".as_bytes(),
            super::cut_prefix(b"--example=foo", b"--example=").unwrap()
        );
        assert_eq!(
            None,
            super::cut_prefix::<u8>(b"--example=foo", b"-example=")
        );
    }
}
