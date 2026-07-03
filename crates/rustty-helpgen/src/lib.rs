//! rustty-helpgen — build-time `help_strings` doc-table extraction primitives.
//!
//! PORTS: ghostty `src/helpgen.zig` (the `extractDocComments` + `findCommonPrefix`
//! doc-scan) and the build-graph role of `src/build/HelpStrings.zig`.
//
// PORT STATUS: done
//
// -----------------------------------------------------------------------------
// The reference (src/helpgen.zig, run by src/build/HelpStrings.zig):
//
//   //! This program is used to generate the help strings from the configuration
//   //! file and CLI actions for Ghostty. These can then be used to generate
//   //! help, docs, website, etc.
//
// ...`@embedFile`s three sources (config/Config.zig, cli/<action>.zig,
// input/Binding.zig), parses each with `std.zig.Ast`, walks the token stream for
// a `.doc_comment` run immediately above the target `.identifier`/`fn run`,
// dedents it (drop `///`, strip the common leading-space prefix), and emits three
// namespaces: `help_strings.Config`, `help_strings.Action`, `help_strings.KeybindAction`.
//
// PORT NOTE (R1/R4): ghostty's ONE `helpgen` program becomes three `build.rs`
// codegen steps — one in each crate that OWNS a table (rustty-config,
// rustty-input, rustty-cli), each scanning ITS OWN ported `.rs` source and
// emitting the table into `OUT_DIR`. This is the faithful analog of the reference
// build step ("parse the source → emit a generated module"): `HelpStrings.zig`'s
// `addExecutable(helpgen)` + `addRunArtifact` + `addWriteFiles("helpgen.zig")` is
// the Cargo `build.rs` + `OUT_DIR` include. Splitting per-crate keeps each table
// next to the `///` source it extracts and avoids a runtime crate cycle (config
// and cli would otherwise both need a shared help crate at runtime). The shared
// leaf here holds the two primitives all three build steps use — the exact
// analog of `helpgen.zig`'s shared `extractDocComments`/`findCommonPrefix`.
//
// PORT NOTE (HelpStrings.zig): the reference's build step copies `helpgen`'s
// stdout into "helpgen.zig" before importing it as the `help_strings` module
// (Generated Zig files have to end with .zig). Here the `OUT_DIR` include
// (`config_help.rs` / `keybind_help.rs` / `action_help.rs`) has no such filename
// rule; the generated Rust is `include!`d directly by each owning crate.
//
// The reference walks a real Zig AST token stream ("identifiers preceded by
// doc_comment tokens"). The ported sources are Rust, so this is a LEXER-LEVEL
// LINE SCAN (mirroring the reference's token-level walk, NOT semantic analysis):
// a contiguous run of `///` lines immediately above a declaration line, with any
// intervening `//` reference-comment / `#[..]` attribute lines skipped (the port
// renders the reference declaration as a `//` comment and may carry a lint
// `#[allow]`, neither of which exists in the reference token stream). The
// `[3..]` `///`-strip + `findCommonPrefix` dedent is reproduced byte-for-byte.
// -----------------------------------------------------------------------------

/// Port of `helpgen.findCommonPrefix`: the minimum number of leading spaces
/// across all NON-empty lines (empty lines contribute nothing, matching the
/// reference's `n = maxInt` skip). `lines` are the per-line contents AFTER the
/// `///` (i.e. the reference's `tokenSlice(i)[3..]`).
// fn findCommonPrefix(lines: std.ArrayList([]const u8)) usize {
pub fn find_common_prefix(lines: &[&[u8]]) -> usize {
    // var m: usize = std.math.maxInt(usize);
    let mut m: usize = usize::MAX;
    // for (lines.items) |line| {
    for line in lines {
        // var n: usize = std.math.maxInt(usize);
        let mut n: usize = usize::MAX;
        // for (line, 0..) |c, i| { if (c != ' ') { n = i; break; } }
        for (i, &c) in line.iter().enumerate() {
            if c != b' ' {
                n = i;
                break;
            }
        }
        // m = @min(m, n);
        m = m.min(n);
    }
    // return m;
    m
}

/// Port of the rendering half of `helpgen.extractDocComments`: given the per-line
/// contents (each already stripped of its leading `///`), strip the common
/// leading-space prefix from every line and join with `\n`. The result is the
/// VALUE of the reference's generated `[:0]const u8` (a Zig multiline string of
/// the dedented lines joined by newline — no trailing newline unless the block
/// ends with a blank `///` line, exactly as the reference emits).
// fn extractDocComments(...) -> the `\\<line[min(prefix,len)..]>` multiline string.
pub fn extract_doc_block(contents: &[&[u8]]) -> Vec<u8> {
    // const prefix = findCommonPrefix(lines);
    let prefix = find_common_prefix(contents);
    let mut out: Vec<u8> = Vec::new();
    // for (lines.items) |line| { writeAll(line[@min(prefix, line.len)..]); writeAll("\n"); }
    //   (rendered as the multiline-string value: lines joined by '\n'.)
    for (idx, line) in contents.iter().enumerate() {
        if idx > 0 {
            out.push(b'\n');
        }
        let start = prefix.min(line.len());
        out.extend_from_slice(&line[start..]);
    }
    out
}

/// Collect the contiguous `///` doc block immediately above `decl_idx`, returning
/// each line's content AFTER the `///` (the reference `tokenSlice(i)[3..]`), in
/// top-to-bottom order. Blank / `//`-reference-comment / `#[..]`-attribute lines
/// between the declaration and the doc block are skipped (they do not exist in
/// the reference token stream — see the module note). Returns `None` when the
/// declaration carries no doc block (the reference's "no `.doc_comment` above the
/// identifier" case → `@hasDecl == false`).
pub fn doc_contents_above<'a>(lines: &[&'a str], decl_idx: usize) -> Option<Vec<&'a [u8]>> {
    // We only care about identifiers that are preceded by doc comments.
    //
    // (In the reference the identifier may have `@""` so it strips that; here the
    // declaration is located by its Rust name by the caller.)
    //
    // Find the first index of the doc comments. The doc comments are
    // always stacked on top of each other so we can just go backwards.
    // Walk up to the bottom-most `///` line, skipping blanks / `//` / attributes.
    let mut i = decl_idx;
    let bottom = loop {
        if i == 0 {
            return None;
        }
        i -= 1;
        let t = lines[i].trim_start();
        if t.starts_with("///") {
            break i;
        }
        if t.is_empty() || t.starts_with('#') || (t.starts_with("//") && !t.starts_with("///")) {
            continue;
        }
        // A code line before any `///` → no doc block.
        return None;
    };

    // Extend up over the contiguous `///` run.
    let mut top = bottom;
    while top > 0 && lines[top - 1].trim_start().starts_with("///") {
        top -= 1;
    }

    // Per-line content after the `///` (== reference `tokenSlice(i)[3..]`).
    let mut out: Vec<&[u8]> = Vec::with_capacity(bottom - top + 1);
    for line in &lines[top..=bottom] {
        let bytes = line.as_bytes();
        let pos = find_slashes(bytes);
        out.push(&bytes[pos + 3..]);
    }
    Some(out)
}

/// Convenience: `doc_contents_above` + `extract_doc_block` — the full analog of
/// `genConfigField`/`genActions`' per-target extraction. `None` when the
/// declaration has no doc block.
pub fn doc_above(lines: &[&str], decl_idx: usize) -> Option<Vec<u8>> {
    let contents = doc_contents_above(lines, decl_idx)?;
    Some(extract_doc_block(&contents))
}

/// Index of the `///` marker within a (already-known-to-be-`///`) doc line.
fn find_slashes(bytes: &[u8]) -> usize {
    let mut i = 0;
    while i + 2 < bytes.len() {
        if bytes[i] == b'/' && bytes[i + 1] == b'/' && bytes[i + 2] == b'/' {
            return i;
        }
        i += 1;
    }
    // Only ever called on a verified `///` line; leading whitespace only.
    i
}

/// Escape a doc value (valid UTF-8) into the body of a Rust `"..."` string
/// literal, so a `build.rs` can emit `Some("<escaped>".as_bytes())`. Content is
/// preserved byte-for-byte (the UTF-8 of the literal == the doc bytes).
pub fn escape_rust_string(bytes: &[u8]) -> String {
    let s = std::str::from_utf8(bytes).expect("doc comment must be valid UTF-8");
    let mut out = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{{{:x}}}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // Parity of `findCommonPrefix`: min leading-space over non-empty lines,
    // empty lines skipped.
    #[test]
    fn common_prefix() {
        assert_eq!(find_common_prefix(&[b" a", b" b"]), 1);
        assert_eq!(find_common_prefix(&[b" a", b"   b"]), 1);
        assert_eq!(find_common_prefix(&[b" a", b"", b"   b"]), 1); // empty skipped
        assert_eq!(find_common_prefix(&[b"a"]), 0);
    }

    // Parity of the dedent+join: a `/// ` prefix (1 space) is stripped, indented
    // example lines keep their RELATIVE indent, blank `///` lines stay blank.
    #[test]
    fn dedent_and_join() {
        // Doc block contents (after `///`) for:
        //   /// Font size.
        //   ///
        //   ///     example
        let contents: Vec<&[u8]> = vec![b" Font size.", b"", b"     example"];
        let out = extract_doc_block(&contents);
        assert_eq!(out, b"Font size.\n\n    example".to_vec());
    }

    // A trailing blank `///` line yields a trailing '\n' (consumers drop it).
    #[test]
    fn trailing_blank_line() {
        let contents: Vec<&[u8]> = vec![b" text", b""];
        assert_eq!(extract_doc_block(&contents), b"text\n".to_vec());
    }

    // The line scanner skips `//` reference comments and `#[..]` attributes
    // between the doc block and the declaration (the fn-run / lint-allow cases).
    #[test]
    fn scanner_skips_ref_comments_and_attrs() {
        let src = "\
    /// Line one.
    /// Line two.
    // @\"font-size\": f32 = ...,
    #[allow(clippy::doc_lazy_continuation)]
    pub font_size: f32,";
        let lines: Vec<&str> = src.lines().collect();
        let decl = lines
            .iter()
            .position(|l| l.contains("pub font_size:"))
            .unwrap();
        let doc = doc_above(&lines, decl).unwrap();
        assert_eq!(doc, b"Line one.\nLine two.".to_vec());
    }

    // No doc block → None (the reference's `@hasDecl == false`).
    #[test]
    fn no_doc_is_none() {
        let src = "\
    pub font_family: RepeatableString,
    pub font_family_bold: RepeatableString,";
        let lines: Vec<&str> = src.lines().collect();
        let decl = lines
            .iter()
            .position(|l| l.contains("font_family_bold"))
            .unwrap();
        assert!(doc_above(&lines, decl).is_none());
    }

    #[test]
    fn escape_roundtrip() {
        assert_eq!(escape_rust_string(b"a\"b\\c\nd"), "a\\\"b\\\\c\\nd");
    }
}
