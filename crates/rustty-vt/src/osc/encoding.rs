//! Specialized encodings used in some OSC protocols.
//!
//! PORTS: ghostty src/terminal/osc/encoding.zig
//!
//! PORT STATUS: done

/// Kitty defines "Escape code safe UTF-8" as valid UTF-8 with the
/// additional requirement of not containing any C0 escape codes
/// (0x00-0x1f), DEL (0x7f) and C1 escape codes (0x80-0x9f).
///
/// Used by OSC 66 (text sizing) and OSC 99 (Kitty notifications).
///
/// See: https://sw.kovidgoyal.net/kitty/desktop-notifications/#safe-utf8
pub fn is_safe_utf8(s: &[u8]) -> bool {
    // PORT NOTE: `std.unicode.Utf8View.init(s)` validates the slice is
    // well-formed UTF-8 (returning on the cold path otherwise); `str::from_utf8`
    // is the faithful equivalent. The `@branchHint(.cold)` markers carry no
    // observable behavior and are dropped per R2 (retarget-only).
    let utf8 = match std::str::from_utf8(s) {
        Ok(v) => v,
        Err(_) => {
            return false;
        }
    };

    for cp in utf8.chars() {
        match cp as u32 {
            0x00..=0x1f | 0x7f | 0x80..=0x9f => {
                return false;
            }
            _ => {}
        }
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_safe_utf8_test() {
        assert!(is_safe_utf8("Hello world!".as_bytes()));
        assert!(is_safe_utf8("安全的ユニコード☀️".as_bytes()));
        assert!(!is_safe_utf8("No linebreaks\nallowed".as_bytes()));
        assert!(!is_safe_utf8("\x07no bells".as_bytes()));
        assert!(!is_safe_utf8("\x1b]9;no OSCs\x1b\\\x1b[m".as_bytes()));
        // PORT NOTE: the reference's `"\x9f8-bit escapes..."` is a Zig string
        // literal, i.e. the RAW BYTE 0x9F, which is not well-formed UTF-8 — so
        // this case exercises the `Utf8View.init` rejection branch, NOT the
        // 0x80...0x9f codepoint arm. A Rust `"\u{9f}"` would encode as C2 9F
        // (valid UTF-8) and take the other branch, so the bytes are spelled out
        // with a byte-string literal to keep the reference's coverage.
        assert!(!is_safe_utf8(b"\x9f8-bit escapes are clever, but no"));
    }

    /// PORT NOTE: NOT a reference test. The reference's own `isSafeUtf8` test
    /// never reaches the `0x7f` or `0x80...0x9f` codepoint arms (its 8-bit-escape
    /// case is invalid UTF-8 and returns at the `Utf8View.init` check), so these
    /// pin the two arms that would otherwise be uncovered.
    #[test]
    fn is_safe_utf8_del_and_c1_codepoints() {
        assert!(!is_safe_utf8("del\u{7f}here".as_bytes()));
        assert!(!is_safe_utf8("c1\u{80}here".as_bytes()));
        assert!(!is_safe_utf8("c1\u{9f}here".as_bytes()));
        // U+00A0 is just past the C1 range and is safe.
        assert!(is_safe_utf8("nbsp\u{a0}here".as_bytes()));
    }
}
