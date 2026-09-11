//! C header enum reader — the `@import("ghostty.h")` half of
//! [`crate::r#enum::check_ghostty_henum`].
//!
//! PORT NOTE: this module has NO reference counterpart, and is port support
//! rather than a port of any `src/lib/*.zig` file. It exists because of a
//! structural gap: the reference's `lib.checkGhosttyHEnum` does
//! `const c = @import("ghostty.h")` and reflects over the REAL generated C
//! header, so its "C side" is, by construction, the shipped header. Rust has no
//! `@import` for a C header, so `check_ghostty_henum` takes the C declarations
//! as a lookup closure — and a caller that *synthesizes* that closure from the
//! very descriptor under test writes a tautology that cannot fail. (That is
//! exactly what finding `S45-F1` recorded across 21 `ghostty_h_*` tests.)
//!
//! So: every `check_ghostty_henum` caller parses its declarations out of the
//! checked-in header text with the functions below, and the header — never the
//! descriptor — is the authority.
//!
//! The parser is deliberately strict. Anything it does not understand is an
//! `Err`, never a silent skip, so header drift cannot quietly disappear from a
//! check that exists to detect header drift.

use std::collections::HashMap;

/// Strip `//` and `/* */` comments from a C fragment.
pub fn strip_c_comments(src: &str) -> String {
    let b = src.as_bytes();
    let mut out = String::with_capacity(src.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'/' && i + 1 < b.len() && b[i + 1] == b'/' {
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
        } else if b[i] == b'/' && i + 1 < b.len() && b[i + 1] == b'*' {
            i += 2;
            while i + 1 < b.len() && !(b[i] == b'*' && b[i + 1] == b'/') {
                i += 1;
            }
            i = (i + 2).min(b.len());
            out.push(' ');
        } else {
            out.push(b[i] as char);
            i += 1;
        }
    }
    out
}

/// Parse `typedef enum [<tag or macro>] { ... } <enum_name>;` out of C header
/// text into its declared `(name, value)` pairs, in declaration order.
/// Enumerators may be bare (value = previous + 1, starting at 0) or
/// `NAME = <int>` (decimal, optionally negative, or `0x`-prefixed hex);
/// anything else is an error rather than a silent skip.
///
/// Comments are stripped from the WHOLE source before anything is located, so a
/// prose comment reproducing an enum declaration (the curated `include/rustty`
/// headers open with exactly that) cannot be mistaken for the real one.
///
/// The optional token between `enum` and `{` is what the curated vt headers use
/// (`typedef enum RUSTTY_ENUM_TYPED {`); `include/rustty-embed.h` uses the bare
/// form. Both are accepted.
pub fn parse_header_enum(src: &str, enum_name: &str) -> Result<Vec<(String, i64)>, String> {
    let src = strip_c_comments(src);
    let close = format!("}} {};", enum_name);
    let close_at = src
        .find(&close)
        .ok_or_else(|| format!("enum `{}` not found in header", enum_name))?;
    let head = &src[..close_at];
    let open_pat = "typedef enum";
    let open_at = head
        .rfind(open_pat)
        .ok_or_else(|| format!("no `{}` opening for `{}`", open_pat, enum_name))?;
    let after = &head[open_at + open_pat.len()..];
    let brace_at = after
        .find('{')
        .ok_or_else(|| format!("no `{{` after `{}` for `{}`", open_pat, enum_name))?;
    // Anything between `enum` and `{` must be a single identifier (a tag or a
    // macro); anything else means we located the wrong region.
    let between = after[..brace_at].trim();
    if !between.is_empty()
        && !between
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        return Err(format!(
            "unexpected `{}` between `typedef enum` and `{{` for `{}`",
            between, enum_name
        ));
    }
    let body = after[brace_at + 1..].to_string();
    if body.contains('}') || body.contains('{') {
        return Err(format!("unbalanced braces in enum `{}`", enum_name));
    }

    let mut out: Vec<(String, i64)> = Vec::new();
    let mut next: i64 = 0;
    for raw in body.split(',') {
        let entry = raw.trim();
        if entry.is_empty() {
            continue;
        }
        let (name, value) = match entry.split_once('=') {
            Some((n, v)) => {
                let v = v.trim();
                let parsed = match v.strip_prefix("0x").or_else(|| v.strip_prefix("0X")) {
                    Some(hex) => i64::from_str_radix(hex, 16),
                    None => v.parse::<i64>(),
                }
                .map_err(|_| format!("unparseable value `{}` in enum `{}`", v, enum_name))?;
                (n.trim(), parsed)
            }
            None => (entry, next),
        };
        if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return Err(format!(
                "unparseable enumerator `{}` in enum `{}`",
                entry, enum_name
            ));
        }
        next = value + 1;
        out.push((name.to_string(), value));
    }
    if out.is_empty() {
        return Err(format!("enum `{}` declares no enumerators", enum_name));
    }
    Ok(out)
}

/// The `@hasDecl`/`@field` stand-in: the named header enum's declarations as a
/// lookup suitable for [`crate::r#enum::check_ghostty_henum`]'s `c` closure.
///
/// Panics (rather than returning) on a malformed header, because every caller
/// is a test and a header that cannot be parsed is a test failure, not a
/// reason to fall back to a weaker check.
pub fn header_decls(src: &str, enum_name: &str) -> HashMap<String, i64> {
    parse_header_enum(src, enum_name)
        .unwrap()
        .into_iter()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
// apprt.Target.Key
typedef enum {
  RUSTTY_TARGET_APP,
  RUSTTY_TARGET_SURFACE,
} rustty_target_tag_e;

// apprt.action.GotoTab
typedef enum {
  RUSTTY_GOTO_TAB_PREVIOUS = -1, /* prev */
  RUSTTY_GOTO_TAB_NEXT = -2,
  RUSTTY_GOTO_TAB_LAST = -3,
} rustty_action_goto_tab_e;
"#;

    #[test]
    fn parses_bare_enumerators() {
        assert_eq!(
            vec![
                ("RUSTTY_TARGET_APP".to_string(), 0i64),
                ("RUSTTY_TARGET_SURFACE".to_string(), 1i64),
            ],
            parse_header_enum(SAMPLE, "rustty_target_tag_e").unwrap()
        );
    }

    #[test]
    fn parses_explicit_and_negative_values() {
        assert_eq!(
            vec![
                ("RUSTTY_GOTO_TAB_PREVIOUS".to_string(), -1i64),
                ("RUSTTY_GOTO_TAB_NEXT".to_string(), -2i64),
                ("RUSTTY_GOTO_TAB_LAST".to_string(), -3i64),
            ],
            parse_header_enum(SAMPLE, "rustty_action_goto_tab_e").unwrap()
        );
    }

    #[test]
    fn parses_hex_values() {
        let src = "typedef enum {\n  A = 0x10,\n  B,\n} e_t;";
        assert_eq!(
            vec![("A".to_string(), 16i64), ("B".to_string(), 17i64)],
            parse_header_enum(src, "e_t").unwrap()
        );
    }

    /// Self-test arms: each malformed shape is an `Err`, never a silent skip.
    #[test]
    fn rejects_malformed_headers() {
        // Unknown enum name.
        assert!(parse_header_enum(SAMPLE, "rustty_nope_e").is_err());
        // No `typedef enum` opening.
        assert!(parse_header_enum("} e_t;", "e_t").is_err());
        // Junk between `enum` and `{`.
        assert!(parse_header_enum("typedef enum a b {\n A,\n} e_t;", "e_t").is_err());
        // A value the parser does not understand (a shift expression).
        assert!(parse_header_enum("typedef enum {\n A = 1 << 3,\n} e_t;", "e_t").is_err());
        // A non-identifier enumerator.
        assert!(parse_header_enum("typedef enum {\n A B,\n} e_t;", "e_t").is_err());
        // A brace inside the body means we grabbed the wrong region.
        assert!(
            parse_header_enum("typedef enum {\n A,\n} x; struct { int a; }\n} e_t;", "e_t")
                .is_err()
        );
        // An empty enum body.
        assert!(parse_header_enum("typedef enum {\n} e_t;", "e_t").is_err());
    }

    #[test]
    fn strips_comments() {
        assert_eq!("a   b", strip_c_comments("a /* x */ b"));
        assert_eq!("a \n b", strip_c_comments("a // x\n b"));
    }

    /// The curated `include/rustty` headers open their enums with a MACRO
    /// (`typedef enum RUSTTY_ENUM_TYPED {`), and their file header carries a
    /// prose comment reproducing that same declaration. Both are pinned here:
    /// the macro form parses, and the comment does NOT shadow the real one.
    #[test]
    fn parses_the_tagged_form_and_ignores_a_comment_that_mimics_it() {
        let src = concat!(
            "/*\n * typedef enum RUSTTY_ENUM_TYPED {\n *   DECOY = 99,\n * } RusttyThing;\n */\n",
            "typedef enum RUSTTY_ENUM_TYPED {\n  RUSTTY_THING_A,\n  RUSTTY_THING_B = 5,\n",
            "} RusttyThing;\n"
        );
        assert_eq!(
            vec![
                ("RUSTTY_THING_A".to_string(), 0i64),
                ("RUSTTY_THING_B".to_string(), 5i64),
            ],
            parse_header_enum(src, "RusttyThing").unwrap()
        );
    }

    #[test]
    fn header_decls_is_a_lookup() {
        let d = header_decls(SAMPLE, "rustty_target_tag_e");
        assert_eq!(Some(&0), d.get("RUSTTY_TARGET_APP"));
        assert_eq!(Some(&1), d.get("RUSTTY_TARGET_SURFACE"));
        assert_eq!(None, d.get("RUSTTY_TARGET_NOPE"));
    }
}
