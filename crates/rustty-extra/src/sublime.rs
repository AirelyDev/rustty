//! Sublime Text syntax generation. Port of ghostty's `src/extra/sublime.zig`.
//! `syntax()` = header ++ keywords (Config field NAMES joined by `|`) ++ footer.
//!
//! R2 IDENTITY NOTE (Task 16): the emitted syntax file now carries the rustty
//! product identity (`Rustty Config`, `file_extensions: - rustty`,
//! `scope: source.rustty`) matching `config.rustty`
//! (`rustty_identity::CONFIG_FILE`). Previously this hardcoded `ghostty`/
//! `source.ghostty`/`file_extensions: - ghostty`, which meant bat's
//! `file_extensions:` match never fired for the actual shipped config file
//! (bat matches by the file's own `file_extensions:`/`name:` YAML fields, not
//! by the syntax file's basename) — a real leak, same class as the Task 18
//! vim/nvim fix. No `identity-oracle` dual-profile exists for this content
//! (matching `res_shell_integration.rs`'s / `rustty_extra::vim`'s precedent);
//! there is exactly one rustty-branded generator output.

use rustty_config::config_struct::config_fields;

// const Template = struct { const header = \\...; const footer = \\...; };
struct Template;

impl Template {
    // const header = \\...  (ends at `- match: '\b(`, no trailing newline).
    const HEADER: &'static [u8] = br#"%YAML 1.2
---
# See http://www.sublimetext.com/docs/syntax.html
name: Rustty Config
file_extensions:
  - rustty
scope: source.rustty

contexts:
  main:
    # Comments
    - match: '^\s*#.*$'
      scope: comment.line.number-sign.rustty

    # Keywords
    - match: '\b("#;

    // const footer = \\...
    const FOOTER: &'static [u8] = br#")\b'
      scope: keyword.other.rustty
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

    // Byte-exact golden diff against the committed rustty-branded fixture
    // (R1 gate; the fixture itself was derived from ghostty's actually-generated
    // file with only the R2 brand tokens flipped — see the module doc comment).
    #[test]
    fn syntax_matches_golden() {
        assert_eq!(
            syntax(),
            include_bytes!("../testdata/rustty.sublime-syntax").as_slice()
        );
    }
}
