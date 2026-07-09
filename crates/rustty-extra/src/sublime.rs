//! Sublime Text syntax generation. Port of ghostty's `src/extra/sublime.zig`.
//! `syntax()` = header ++ keywords (Config field NAMES joined by `|`) ++ footer.

use rustty_config::config_struct::config_fields;

// const Template = struct { const header = \\...; const footer = \\...; };
struct Template;

impl Template {
    // const header = \\...  (ends at `- match: '\b(`, no trailing newline).
    const HEADER: &'static [u8] = br#"%YAML 1.2
---
# See http://www.sublimetext.com/docs/syntax.html
name: Ghostty Config
file_extensions:
  - ghostty
scope: source.ghostty

contexts:
  main:
    # Comments
    - match: '^\s*#.*$'
      scope: comment.line.number-sign.ghostty

    # Keywords
    - match: '\b("#;

    // const footer = \\...
    const FOOTER: &'static [u8] = br#")\b'
      scope: keyword.other.ghostty
"#;
}

// /// Check if a field is internal (starts with underscore)
// fn isInternal(name: []const u8) bool {
//   PORT NOTE: `config_fields()` is already the `_`-filtered decl-order table, so
//   the reference's per-field `isInternal` guard is subsumed by the iteration
//   source (kept as a comment for R1 provenance).

/// Generate keywords from Config fields
// fn generateKeywords() []const u8 {
fn generate_keywords() -> Vec<u8> {
    // var keywords: []const u8 = "";
    let mut keywords: Vec<u8> = Vec::new();
    // const config_fields = @typeInfo(Config).@"struct".fields;
    // for (config_fields) |field| {
    //     if (isInternal(field.name)) continue;
    //     if (keywords.len > 0) keywords = keywords ++ "|";
    //     keywords = keywords ++ field.name;
    // }
    for (name, _key) in config_fields() {
        if !keywords.is_empty() {
            keywords.push(b'|');
        }
        keywords.extend_from_slice(name);
    }
    keywords
}

/// Complete Sublime syntax file content
// pub const syntax = Template.header ++ generateKeywords() ++ Template.footer;
//
// PORT NOTE: Rust cannot comptime-concatenate a `const`; `syntax` is a runtime
// pure fn returning the same bytes.
pub fn syntax() -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(Template::HEADER);
    out.extend_from_slice(&generate_keywords());
    out.extend_from_slice(Template::FOOTER);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // Byte-exact golden diff against ghostty's actually-generated file (R1 gate).
    #[test]
    fn syntax_matches_golden() {
        assert_eq!(
            syntax(),
            include_bytes!("../testdata/ghostty.sublime-syntax").as_slice()
        );
    }
}
