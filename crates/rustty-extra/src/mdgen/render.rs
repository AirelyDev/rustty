//! rustty-extra::mdgen::render — the `pandoc` stand-in.
//!
//! PORTS (by substitution, not transcription): ghostty `src/build/GhosttyDocs.zig:60-88`
//! shells out to `pandoc --standalone --from markdown --to man` and
//! `--to html` to turn `mdgen`'s markdown into the installed `share/man/**` roff
//! and `share/ghostty/doc/*.html`. This module does that conversion in Rust.
//!
//! **Why not shell out to `pandoc` (the tradeoff, recorded so it is not
//! re-litigated):** `pandoc` is present on NEITHER build host — not the macOS
//! workstation and not the Fedora VM. Three options were weighed.
//!
//!   a. *Require `pandoc`.* Matches the reference exactly, but makes
//!      `xtask resources` — a step every packaging run needs — fail on a stock
//!      machine, and adds a heavyweight host build dependency. WS9 faced the same
//!      shape of choice for `libintl` and deliberately went the other way
//!      (vendor, do not depend).
//!   b. *Emit roff directly from `mdgen`, skipping markdown.* Fewer moving parts,
//!      but it discards the `.md` and `.html` artifacts the reference installs
//!      into `share/{app}/doc/`, and makes the generator's output un-diffable
//!      against the reference's markdown.
//!   c. *Generate the reference's markdown verbatim (modulo R2), then convert in
//!      Rust.* Keeps `mdgen`'s output byte-comparable to the reference's,
//!      produces all three installed formats, adds no host dependency, and the
//!      conversion is unit-testable.
//!
//! **(c) is chosen.** The markdown subset that must be supported is small and
//! CLOSED, because `mdgen` is the only producer: the `%`-title block, `#`/`##`
//! headings, blank-line-separated paragraphs, pandoc definition lists (a one-line
//! term followed by a `:`-introduced body), `**bold**` / `_italic_` /
//! `` `code` ``, `-` bullets, 4-space indented code blocks, `<url>` autolinks and
//! `[text](url)` links. Anything outside that subset is passed through as text
//! rather than silently dropped.
//!
//! Everything is `&[u8]`/`Vec<u8>`: markdown in, roff/HTML bytes out (memory:
//! zig-byteslice-to-rust-rawbytes). The generated pages contain UTF-8 prose
//! (e.g. `⚠️`-free but accented) and must never round-trip through a lossy
//! `String` conversion.
//!
//! ## Deviations from `pandoc --to man`, and why
//!
//!   * **The `.TH` date field is populated** ([`MAN_DATE`]) rather than left
//!     empty. pandoc emits `.TH "RUSTTY" "1" "" …` when the markdown title block
//!     carries no date, and `mandoc -Tlint` then reports
//!     `WARNING: missing date, using "": TH` on every generated page. A warning
//!     that fires unconditionally is a gate that can never go green, so the date
//!     is supplied. It is a committed constant, not a clock read, so the output
//!     stays byte-deterministic across builds.
//!   * **The man-page section is taken from the title block, not from a
//!     parameter.** The title block (`% RUSTTY(1) …`) is the single source of
//!     truth for both the name and the section; accepting a separate `section`
//!     argument would create two sources that can disagree.
//!   * **Filled text is wrapped at [`WRAP_COLUMNS`] bytes.** A newline inside
//!     roff filled text is exactly equivalent to a space, so this changes nothing
//!     semantically, but it keeps `mandoc -Tlint` from reporting
//!     `STYLE: input text line longer than 80 bytes` on the long generated
//!     option documentation. Code blocks (`.EX`/`.EE`, no-fill) are never
//!     wrapped, because there a newline IS significant.

use std::collections::BTreeSet;

/// Documentation revision date stamped into the generated `.TH` line.
///
/// A committed constant rather than a clock read: the man pages are a build
/// artifact and must be byte-reproducible. Bump it when the prose fixtures in
/// `src/mdgen/*.md` change.
pub const MAN_DATE: &[u8] = b"2026-07-30";

/// Column at which filled roff text is wrapped. `mandoc -Tlint`'s
/// `input text line longer than 80 bytes` STYLE check is the reason this exists;
/// 78 leaves headroom for the `\&` line-start guard.
const WRAP_COLUMNS: usize = 78;

// ---------------------------------------------------------------------------
// Block model
// ---------------------------------------------------------------------------

/// One parsed markdown block. The variants are exactly the constructs `mdgen`
/// and its four prose fixtures can produce; there is deliberately no catch-all
/// "raw markdown" variant, so an unsupported construct shows up as a `Para` of
/// literal text rather than vanishing.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Block {
    /// `% RUSTTY(1) Version 1.2.3 | Rustty terminal emulator`
    Title {
        name: Vec<u8>,
        section: Vec<u8>,
        source: Vec<u8>,
        manual: Vec<u8>,
    },
    /// `# NAME` (level 1) / `## Logging` (level 2).
    Heading { level: u8, text: Vec<u8> },
    /// A blank-line-delimited run of text lines.
    Para(Vec<Vec<u8>>),
    /// A 4-space-indented literal block, with the indent already stripped.
    Code(Vec<Vec<u8>>),
    /// A `-` bullet list; each item is its own line run.
    Bullets(Vec<Vec<Vec<u8>>>),
    /// A pandoc definition list entry: a one-line term plus a `:`-introduced
    /// body, which is itself a block sequence (a definition can hold several
    /// paragraphs, a bullet list, or an indented code example).
    Def { term: Vec<u8>, body: Vec<Block> },
}

/// True for a line that pandoc treats as blank — including a line that is only
/// whitespace, which `mdgen`'s help-block emitter produces whenever a doc
/// comment contains an empty line (`write_help_block` prefixes it with 4
/// spaces).
fn is_blank(line: &[u8]) -> bool {
    line.iter()
        .all(|b| *b == b' ' || *b == b'\t' || *b == b'\r')
}

/// Drop a trailing `\r` so CRLF input parses identically to LF input.
fn strip_cr(line: &[u8]) -> &[u8] {
    line.strip_suffix(b"\r").unwrap_or(line)
}

/// Strip one level of definition-body indentation from a line.
///
/// pandoc's definition-list body column is 4: either `:` plus up to three
/// spaces, or four spaces on a continuation line. A "lazy" continuation (no
/// indent at all — the fixtures use these, e.g. `rustty_1_footer.md`'s
/// `for configuration files.`) keeps its bytes.
fn strip_def_indent(line: &[u8]) -> &[u8] {
    if let Some(rest) = line.strip_prefix(b":") {
        let mut n = 0;
        while n < 3 && rest.get(n) == Some(&b' ') {
            n += 1;
        }
        &rest[n..]
    } else if let Some(rest) = line.strip_prefix(b"    ") {
        rest
    } else {
        line
    }
}

/// Parse a markdown document into blocks. The `%` title block is only
/// recognised as the very first non-blank line, exactly as pandoc requires.
fn parse_doc(md: &[u8]) -> Vec<Block> {
    let lines: Vec<&[u8]> = md.split(|b| *b == b'\n').map(strip_cr).collect();
    let mut out = Vec::new();
    let mut start = 0usize;

    while start < lines.len() && is_blank(lines[start]) {
        start += 1;
    }
    if start < lines.len() {
        if let Some(rest) = lines[start].strip_prefix(b"% ") {
            out.push(parse_title(rest));
            start += 1;
        }
    }

    out.extend(parse_blocks(&lines[start.min(lines.len())..]));
    out
}

/// `RUSTTY(1) Version 1.2.3 | Rustty terminal emulator` → the `.TH` fields.
fn parse_title(rest: &[u8]) -> Block {
    // Split off the `| manual` tail first: the name/version half never contains
    // a `|`.
    let (head, manual) = match rest.windows(3).position(|w| w == b" | ") {
        Some(i) => (&rest[..i], rest[i + 3..].to_vec()),
        None => (rest, Vec::new()),
    };
    // `NAME(SECTION)` then optionally a space and the source string.
    let (name, section, source) = match (
        head.iter().position(|b| *b == b'('),
        head.iter().position(|b| *b == b')'),
    ) {
        (Some(o), Some(c)) if o < c => {
            let name = head[..o].to_vec();
            let section = head[o + 1..c].to_vec();
            let source = head[c + 1..]
                .strip_prefix(b" ")
                .unwrap_or(&head[c + 1..])
                .to_vec();
            (name, section, source)
        }
        _ => (head.to_vec(), Vec::new(), Vec::new()),
    };
    Block::Title {
        name,
        section,
        source,
        manual,
    }
}

/// The block-level pass. Recursion happens through definition bodies only.
fn parse_blocks(lines: &[&[u8]]) -> Vec<Block> {
    let mut out: Vec<Block> = Vec::new();
    let mut i = 0usize;

    while i < lines.len() {
        if is_blank(lines[i]) {
            i += 1;
            continue;
        }
        let line = lines[i];

        // `#`/`##` headings.
        if line.starts_with(b"#") {
            let level = line.iter().take_while(|b| **b == b'#').count();
            let text = line[level..]
                .strip_prefix(b" ")
                .unwrap_or(&line[level..])
                .to_vec();
            out.push(Block::Heading {
                level: u8::try_from(level).unwrap_or(u8::MAX),
                text,
            });
            i += 1;
            continue;
        }

        // 4-space indented code block.
        if line.starts_with(b"    ") {
            let mut body: Vec<Vec<u8>> = Vec::new();
            while i < lines.len() && (is_blank(lines[i]) || lines[i].starts_with(b"    ")) {
                body.push(if is_blank(lines[i]) {
                    Vec::new()
                } else {
                    lines[i][4..].to_vec()
                });
                i += 1;
            }
            while body.last().is_some_and(std::vec::Vec::is_empty) {
                body.pop();
            }
            out.push(Block::Code(body));
            continue;
        }

        // `-` bullet list.
        if line.starts_with(b"- ") {
            let mut items: Vec<Vec<Vec<u8>>> = Vec::new();
            while i < lines.len() && !is_blank(lines[i]) {
                if lines[i].starts_with(b"- ") {
                    items.push(vec![lines[i][2..].to_vec()]);
                } else if let Some(last) = items.last_mut() {
                    // Lazy continuation of the previous item.
                    let cont = lines[i].strip_prefix(b"  ").unwrap_or(lines[i]);
                    last.push(cont.to_vec());
                } else {
                    break;
                }
                i += 1;
            }
            out.push(Block::Bullets(items));
            continue;
        }

        // Otherwise a paragraph run, which may turn out to be a definition term.
        let para_start = i;
        while i < lines.len() && !is_blank(lines[i]) {
            i += 1;
        }
        let para: Vec<Vec<u8>> = lines[para_start..i].iter().map(|l| (*l).to_vec()).collect();

        // Peek past the blank run: a `:` line makes the LAST paragraph line a
        // definition term.
        let mut j = i;
        while j < lines.len() && is_blank(lines[j]) {
            j += 1;
        }
        let is_def = j < lines.len() && lines[j].starts_with(b":");
        if !is_def {
            out.push(Block::Para(para));
            continue;
        }

        let mut para = para;
        let term = para.pop().unwrap_or_default();
        if !para.is_empty() {
            out.push(Block::Para(para));
        }

        // Gather the definition body: `:`-introduced paragraphs, plus any
        // 4-space-indented paragraph that follows one (pandoc's continuation
        // rule). Paragraphs are kept separated by one blank line so the
        // recursive parse sees them as distinct blocks.
        let mut body_lines: Vec<&[u8]> = Vec::new();
        let mut k = j;
        let mut first = true;
        loop {
            while k < lines.len() && is_blank(lines[k]) {
                k += 1;
            }
            if k >= lines.len() {
                break;
            }
            let starts_body =
                lines[k].starts_with(b":") || (!first && lines[k].starts_with(b"    "));
            if !starts_body {
                break;
            }
            if !first {
                body_lines.push(b"");
            }
            while k < lines.len() && !is_blank(lines[k]) {
                body_lines.push(lines[k]);
                k += 1;
            }
            first = false;
        }
        i = k;

        let stripped: Vec<&[u8]> = body_lines.iter().map(|l| strip_def_indent(l)).collect();
        out.push(Block::Def {
            term,
            body: parse_blocks(&stripped),
        });
    }

    out
}

// ---------------------------------------------------------------------------
// Inline pass — shared shape, two back ends
// ---------------------------------------------------------------------------

/// The inline constructs the subset supports, produced once and rendered by
/// either back end. Keeping one parser means the roff and HTML outputs can never
/// disagree about what the markdown MEANT.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Inline {
    Text(Vec<u8>),
    Bold(Vec<Inline>),
    Italic(Vec<Inline>),
    Code(Vec<u8>),
    /// `[text](url)` and `<url>` both land here; an autolink has `text == url`.
    Link {
        text: Vec<Inline>,
        url: Vec<u8>,
    },
}

fn is_word_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b >= 0x80
}

/// Parse one logical line (or run of lines already joined) of markdown inline
/// markup.
fn parse_inline(src: &[u8]) -> Vec<Inline> {
    let mut out: Vec<Inline> = Vec::new();
    let mut text: Vec<u8> = Vec::new();
    let mut i = 0usize;

    macro_rules! flush {
        () => {
            if !text.is_empty() {
                out.push(Inline::Text(std::mem::take(&mut text)));
            }
        };
    }

    while i < src.len() {
        let b = src[i];

        // Markdown backslash escape: `\$` is a literal `$`. Resolved HERE, before
        // any roff/HTML escaping, so the escape character never leaks downstream.
        if b == b'\\' && i + 1 < src.len() {
            text.push(src[i + 1]);
            i += 2;
            continue;
        }

        // `` `code` ``
        if b == b'`' {
            if let Some(end) = src[i + 1..].iter().position(|c| *c == b'`') {
                flush!();
                out.push(Inline::Code(src[i + 1..i + 1 + end].to_vec()));
                i += end + 2;
                continue;
            }
        }

        // `**bold**`
        if src[i..].starts_with(b"**") {
            if let Some(end) = find_sub(&src[i + 2..], b"**") {
                flush!();
                out.push(Inline::Bold(parse_inline(&src[i + 2..i + 2 + end])));
                i += end + 4;
                continue;
            }
        }

        // `_italic_` / `*italic*`. The `_` form is only emphasis when it is NOT
        // intra-word — otherwise `RUSTTY_LOG` and `XDG_CONFIG_HOME` would be
        // shredded into italics.
        if b == b'_' || b == b'*' {
            let prev_word = i > 0 && is_word_byte(src[i - 1]);
            let next_ok = src.get(i + 1).is_some_and(|c| *c != b' ' && *c != b'\t');
            let intraword_ok = b == b'*' || !prev_word;
            if next_ok && intraword_ok {
                if let Some(end) = find_emphasis_close(&src[i + 1..], b) {
                    flush!();
                    out.push(Inline::Italic(parse_inline(&src[i + 1..i + 1 + end])));
                    i += end + 2;
                    continue;
                }
            }
        }

        // `[text](url)`
        if b == b'[' {
            if let Some(close) = src[i..].iter().position(|c| *c == b']') {
                if src.get(i + close + 1) == Some(&b'(') {
                    if let Some(paren) = src[i + close + 2..].iter().position(|c| *c == b')') {
                        flush!();
                        out.push(Inline::Link {
                            text: parse_inline(&src[i + 1..i + close]),
                            url: src[i + close + 2..i + close + 2 + paren].to_vec(),
                        });
                        i += close + 2 + paren + 1;
                        continue;
                    }
                }
            }
        }

        // `<url>` / `<mail@host>` autolink. Requires no whitespace inside, so a
        // stray `<` in prose stays literal.
        if b == b'<' {
            if let Some(end) = src[i + 1..].iter().position(|c| *c == b'>') {
                let inner = &src[i + 1..i + 1 + end];
                if !inner.is_empty()
                    && !inner.iter().any(|c| c.is_ascii_whitespace())
                    && (inner.contains(&b'@') || inner.contains(&b':') || inner.contains(&b'.'))
                {
                    flush!();
                    out.push(Inline::Link {
                        text: vec![Inline::Text(inner.to_vec())],
                        url: inner.to_vec(),
                    });
                    i += end + 2;
                    continue;
                }
            }
        }

        text.push(b);
        i += 1;
    }
    flush!();
    out
}

fn find_sub(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

/// Find the closing emphasis delimiter, honouring the intra-word rule for `_`.
fn find_emphasis_close(rest: &[u8], delim: u8) -> Option<usize> {
    let mut k = 0usize;
    while k < rest.len() {
        if rest[k] == b'\\' {
            k += 2;
            continue;
        }
        if rest[k] == delim {
            let prev_ok = k > 0 && rest[k - 1] != b' ';
            let next_word = rest.get(k + 1).is_some_and(|c| is_word_byte(*c));
            let closes = prev_ok && (delim == b'*' || !next_word);
            if closes && k > 0 {
                return Some(k);
            }
        }
        k += 1;
    }
    None
}

// ---------------------------------------------------------------------------
// roff back end
// ---------------------------------------------------------------------------

/// Escape a literal text run for roff.
///
/// Order matters: `\` must become `\e` FIRST (otherwise the backslashes this
/// function itself introduces would be re-escaped), then `-` becomes `\-` so an
/// option name stays copy-pasteable instead of rendering as a Unicode hyphen.
/// The line-start `.`/`'` guard is applied later, after wrapping, because
/// wrapping can move a `.` to the start of a physical line.
fn roff_escape(src: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(src.len());
    let mut i = 0usize;
    while i < src.len() {
        let b = src[i];
        match b {
            b'\\' => {
                out.extend_from_slice(b"\\e");
                i += 1;
            }
            b'-' => {
                out.extend_from_slice(b"\\-");
                i += 1;
            }
            0x00..=0x7F => {
                out.push(b);
                i += 1;
            }
            // Non-ASCII: emit the roff `\[uXXXX]` escape rather than the raw
            // UTF-8 bytes. Classic troff reads input as single bytes, so
            // `groff -man -Tutf8` (without the `preconv` front end, which a
            // packager will not necessarily invoke) reports
            // `warning: invalid input character code 134` on every continuation
            // byte and drops the character. mandoc performs the same rewrite
            // internally, so this is what it measures for its 80-byte line
            // check too — escaping here makes the whole page pure ASCII and the
            // wrap arithmetic exact instead of approximate.
            _ => {
                let (scalar, len) = decode_utf8(&src[i..]);
                out.extend_from_slice(format!("\\[u{scalar:04X}]").as_bytes());
                i += len;
            }
        }
    }
    out
}

/// Decode one UTF-8 scalar, returning `(codepoint, byte length)`. A malformed
/// sequence decodes as U+FFFD consuming one byte, so no input can wedge the
/// loop or produce a truncated escape.
fn decode_utf8(src: &[u8]) -> (u32, usize) {
    match std::str::from_utf8(&src[..src.len().min(4)]) {
        Ok(s) => match s.chars().next() {
            Some(c) => (c as u32, c.len_utf8()),
            None => (0xFFFD, 1),
        },
        Err(e) => {
            let valid = e.valid_up_to();
            if valid > 0 {
                let c = std::str::from_utf8(&src[..valid])
                    .ok()
                    .and_then(|s| s.chars().next());
                match c {
                    Some(c) => (c as u32, c.len_utf8()),
                    None => (0xFFFD, 1),
                }
            } else {
                // A prefix of a longer sequence truncated by the 4-byte window,
                // or genuinely invalid input.
                match std::str::from_utf8(src) {
                    Ok(s) => match s.chars().next() {
                        Some(c) => (c as u32, c.len_utf8()),
                        None => (0xFFFD, 1),
                    },
                    Err(_) => (0xFFFD, 1),
                }
            }
        }
    }
}

/// Roff font-selection escapes. `\fP` ("previous font") is deliberately NOT
/// used: it keeps only ONE level of history, so a nested run such as
/// `**`--font-family`**` (bold wrapping code, which is exactly what `mdgen`
/// emits for every option term) would leave the formatter in the wrong font
/// after the inner close. Restoring to an EXPLICIT font instead is unambiguous
/// in both `mandoc` and `groff`.
const FONT_ROMAN: &[u8] = b"\\fR";
const FONT_BOLD: &[u8] = b"\\fB";
const FONT_ITALIC: &[u8] = b"\\fI";
const FONT_CONST: &[u8] = b"\\f(CR";

fn inline_roff(items: &[Inline]) -> Vec<u8> {
    inline_roff_in(items, FONT_ROMAN)
}

/// `enclosing` is the font escape to restore to when a nested run closes.
fn inline_roff_in(items: &[Inline], enclosing: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    for it in items {
        match it {
            Inline::Text(t) => out.extend_from_slice(&roff_escape(t)),
            Inline::Bold(inner) => {
                out.extend_from_slice(FONT_BOLD);
                out.extend_from_slice(&inline_roff_in(inner, FONT_BOLD));
                out.extend_from_slice(enclosing);
            }
            Inline::Italic(inner) => {
                out.extend_from_slice(FONT_ITALIC);
                out.extend_from_slice(&inline_roff_in(inner, FONT_ITALIC));
                out.extend_from_slice(enclosing);
            }
            Inline::Code(t) => {
                out.extend_from_slice(FONT_CONST);
                out.extend_from_slice(&roff_escape(t));
                out.extend_from_slice(enclosing);
            }
            Inline::Link { text, url } => {
                let t = inline_roff_in(text, enclosing);
                let u = roff_escape(url);
                if t == u {
                    out.extend_from_slice(b"\\(la");
                    out.extend_from_slice(&u);
                    out.extend_from_slice(b"\\(ra");
                } else {
                    out.extend_from_slice(&t);
                    out.extend_from_slice(b" \\(la");
                    out.extend_from_slice(&u);
                    out.extend_from_slice(b"\\(ra");
                }
            }
        }
    }
    out
}

/// Wrap one logical filled-text line at [`WRAP_COLUMNS`] **bytes**, breaking
/// only at spaces. A single token longer than the limit is left intact rather
/// than split, because splitting a URL or a `\f(CR...\fR` escape sequence would
/// corrupt it.
///
/// Bytes, not display columns, because `mandoc -Tlint`'s
/// `input text line longer than 80 bytes` check counts INPUT bytes — and a font
/// escape such as `\f(CR` is five input bytes but zero display columns, so a
/// column-based wrap leaves heavily-marked-up lines over the limit. Wrapping on
/// bytes is strictly more conservative, and a roff newline in filled text is
/// just a space, so nothing is lost.
fn wrap_roff_line(line: &[u8], out: &mut Vec<Vec<u8>>) {
    if line.len() <= WRAP_COLUMNS {
        out.push(line.to_vec());
        return;
    }
    let mut cur: Vec<u8> = Vec::new();
    for word in line.split(|b| *b == b' ') {
        if word.is_empty() {
            continue;
        }
        if cur.is_empty() {
            cur.extend_from_slice(word);
        } else if cur.len() + 1 + word.len() <= WRAP_COLUMNS {
            cur.push(b' ');
            cur.extend_from_slice(word);
        } else {
            out.push(std::mem::take(&mut cur));
            cur.extend_from_slice(word);
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
}

/// Render one filled-text run: join the source lines, run the inline pass over
/// the WHOLE run, then wrap and control-guard the result.
///
/// Joining first is load-bearing, not tidiness: markdown inline markup wraps
/// freely across source lines, and the ported fixtures do exactly that (a
/// `[text](url)` link in the `class` doc comment is split over three lines). A
/// per-line inline pass leaves such a construct unmatched and emits the raw
/// markdown into the man page. Joining is also semantically free — in roff
/// filled text a newline IS a space.
fn render_text_run(out: &mut Vec<u8>, lines: &[Vec<u8>]) {
    let mut joined: Vec<u8> = Vec::new();
    for (n, l) in lines.iter().enumerate() {
        if n > 0 {
            joined.push(b' ');
        }
        joined.extend_from_slice(l);
    }
    push_text_lines(out, &[inline_roff(&parse_inline(&joined))]);
}

/// Emit a run of already-inlined text lines as wrapped, control-guarded roff.
fn push_text_lines(out: &mut Vec<u8>, lines: &[Vec<u8>]) {
    let mut physical: Vec<Vec<u8>> = Vec::new();
    for l in lines {
        wrap_roff_line(l, &mut physical);
    }
    for l in &physical {
        // A line that begins with `.` or `'` would be parsed as a roff request
        // and the whole line silently swallowed. `\&` is the zero-width guard.
        if l.first() == Some(&b'.') || l.first() == Some(&b'\'') {
            out.extend_from_slice(b"\\&");
        }
        out.extend_from_slice(l);
        out.push(b'\n');
    }
}

/// `nested` is true inside a definition body — i.e. inside an `.RS`/`.RE`
/// region. It matters because `.SH`/`.SS` implicitly CLOSE any open `.RS`
/// block, which would leave this renderer's matching `.RE` orphaned
/// (`mandoc: ERROR: skipping end of block that is not open: RE`). Several
/// config doc comments carry `##` headings, so this is not hypothetical.
/// Nested headings are therefore rendered as a bold lead paragraph, the
/// conventional roff idiom for a sub-heading inside an indented block.
fn render_blocks_roff(out: &mut Vec<u8>, blocks: &[Block], nested: bool) {
    // `.PP` immediately after `.SH`/`.SS` is a no-op that mandoc reports as
    // `WARNING: skipping paragraph macro`, so track whether a heading was the
    // last thing emitted.
    let mut after_heading = false;
    for b in blocks {
        let mut emitted_heading = false;
        match b {
            Block::Title { .. } => {}
            Block::Heading { level, text } => {
                let inlined = inline_roff(&parse_inline(text));
                if nested {
                    out.extend_from_slice(b".PP\n");
                    out.extend_from_slice(FONT_BOLD);
                    out.extend_from_slice(&inlined);
                    out.extend_from_slice(FONT_ROMAN);
                    out.push(b'\n');
                } else {
                    let macro_name: &[u8] = if *level <= 1 { b".SH \"" } else { b".SS \"" };
                    out.extend_from_slice(macro_name);
                    out.extend_from_slice(&inlined);
                    out.extend_from_slice(b"\"\n");
                    emitted_heading = true;
                }
            }
            Block::Para(lines) => {
                if !after_heading {
                    out.extend_from_slice(b".PP\n");
                }
                render_text_run(out, lines);
            }
            Block::Code(lines) => {
                out.extend_from_slice(b".RS 4\n.EX\n");
                for l in lines {
                    // No inline markup and no wrapping inside a no-fill block:
                    // a newline is significant here.
                    let esc = roff_escape(l);
                    if esc.first() == Some(&b'.') || esc.first() == Some(&b'\'') {
                        out.extend_from_slice(b"\\&");
                    }
                    out.extend_from_slice(&esc);
                    out.push(b'\n');
                }
                out.extend_from_slice(b".EE\n.RE\n");
            }
            Block::Bullets(items) => {
                for item in items {
                    out.extend_from_slice(b".IP \\(bu 2\n");
                    render_text_run(out, item);
                }
            }
            Block::Def { term, body } => {
                out.extend_from_slice(b".TP\n");
                let t = inline_roff(&parse_inline(term));
                if t.first() == Some(&b'.') || t.first() == Some(&b'\'') {
                    out.extend_from_slice(b"\\&");
                }
                out.extend_from_slice(&t);
                out.push(b'\n');

                // The first body paragraph is the `.TP` body proper; anything
                // further is a nested `.RS`/`.RE` region, which is what
                // `pandoc --to man` emits for a multi-paragraph definition.
                let tail = match body.split_first() {
                    Some((Block::Para(lines), rest)) => {
                        render_text_run(out, lines);
                        rest
                    }
                    _ => &body[..],
                };
                if !tail.is_empty() {
                    out.extend_from_slice(b".RS\n");
                    render_blocks_roff(out, tail, true);
                    out.extend_from_slice(b".RE\n");
                }
            }
        }
        after_heading = emitted_heading;
    }
}

/// markdown → `man(7)` roff, using [`MAN_DATE`] for the `.TH` date field.
#[must_use]
pub fn markdown_to_roff(md: &[u8]) -> Vec<u8> {
    markdown_to_roff_with_title(md, None)
}

/// markdown → `man(7)` roff with an optional `.TH` title override.
///
/// The override exists for the `identity-oracle` parity profile (R2/Task 14):
/// there the installed FILENAME becomes `ghostty.1`, so the `.TH` title must
/// follow it. Without the override the title comes from the markdown's own
/// `%`-block, which is the normal path.
#[must_use]
pub fn markdown_to_roff_with_title(md: &[u8], title: Option<&[u8]>) -> Vec<u8> {
    let blocks = parse_doc(md);
    let mut out = Vec::with_capacity(md.len() * 2);

    if let Some(Block::Title {
        name,
        section,
        source,
        manual,
    }) = blocks.first()
    {
        let name = title.unwrap_or(name);
        out.extend_from_slice(b".TH \"");
        out.extend_from_slice(name);
        out.extend_from_slice(b"\" \"");
        out.extend_from_slice(section);
        out.extend_from_slice(b"\" \"");
        out.extend_from_slice(MAN_DATE);
        out.extend_from_slice(b"\" \"");
        out.extend_from_slice(source);
        out.extend_from_slice(b"\" \"");
        out.extend_from_slice(manual);
        out.extend_from_slice(b"\"\n");
    }
    render_blocks_roff(&mut out, &blocks, false);
    out
}

// ---------------------------------------------------------------------------
// HTML back end
// ---------------------------------------------------------------------------

fn html_escape(src: &[u8], out: &mut Vec<u8>) {
    for b in src {
        match b {
            b'&' => out.extend_from_slice(b"&amp;"),
            b'<' => out.extend_from_slice(b"&lt;"),
            b'>' => out.extend_from_slice(b"&gt;"),
            b'"' => out.extend_from_slice(b"&quot;"),
            _ => out.push(*b),
        }
    }
}

fn inline_html(items: &[Inline], out: &mut Vec<u8>) {
    for it in items {
        match it {
            Inline::Text(t) => html_escape(t, out),
            Inline::Bold(inner) => {
                out.extend_from_slice(b"<strong>");
                inline_html(inner, out);
                out.extend_from_slice(b"</strong>");
            }
            Inline::Italic(inner) => {
                out.extend_from_slice(b"<em>");
                inline_html(inner, out);
                out.extend_from_slice(b"</em>");
            }
            Inline::Code(t) => {
                out.extend_from_slice(b"<code>");
                html_escape(t, out);
                out.extend_from_slice(b"</code>");
            }
            Inline::Link { text, url } => {
                out.extend_from_slice(b"<a href=\"");
                html_escape(url, out);
                out.extend_from_slice(b"\">");
                inline_html(text, out);
                out.extend_from_slice(b"</a>");
            }
        }
    }
}

/// The HTML twin of [`render_text_run`]: the source lines of one filled run are
/// joined before the inline pass so a construct split across lines (a
/// `[text](url)` link, a `**bold**` phrase) is still recognised.
fn inline_html_lines(lines: &[Vec<u8>], out: &mut Vec<u8>) {
    let mut joined: Vec<u8> = Vec::new();
    for (n, l) in lines.iter().enumerate() {
        if n > 0 {
            joined.push(b'\n');
        }
        joined.extend_from_slice(l);
    }
    inline_html(&parse_inline(&joined), out);
}

fn render_blocks_html(out: &mut Vec<u8>, blocks: &[Block]) {
    let mut i = 0usize;
    while i < blocks.len() {
        match &blocks[i] {
            Block::Title { .. } => {}
            Block::Heading { level, text } => {
                let tag = if *level <= 1 { b"h1" } else { b"h2" };
                out.extend_from_slice(b"<");
                out.extend_from_slice(tag);
                out.extend_from_slice(b">");
                inline_html(&parse_inline(text), out);
                out.extend_from_slice(b"</");
                out.extend_from_slice(tag);
                out.extend_from_slice(b">\n");
            }
            Block::Para(lines) => {
                out.extend_from_slice(b"<p>");
                inline_html_lines(lines, out);
                out.extend_from_slice(b"</p>\n");
            }
            Block::Code(lines) => {
                out.extend_from_slice(b"<pre><code>");
                for l in lines {
                    html_escape(l, out);
                    out.push(b'\n');
                }
                out.extend_from_slice(b"</code></pre>\n");
            }
            Block::Bullets(items) => {
                out.extend_from_slice(b"<ul>\n");
                for item in items {
                    out.extend_from_slice(b"<li>");
                    inline_html_lines(item, out);
                    out.extend_from_slice(b"</li>\n");
                }
                out.extend_from_slice(b"</ul>\n");
            }
            // Consecutive definitions form ONE `<dl>`, matching pandoc.
            Block::Def { .. } => {
                out.extend_from_slice(b"<dl>\n");
                while let Some(Block::Def { term, body }) = blocks.get(i) {
                    out.extend_from_slice(b"<dt>");
                    inline_html(&parse_inline(term), out);
                    out.extend_from_slice(b"</dt>\n<dd>\n");
                    render_blocks_html(out, body);
                    out.extend_from_slice(b"</dd>\n");
                    i += 1;
                }
                out.extend_from_slice(b"</dl>\n");
                continue;
            }
        }
        i += 1;
    }
}

/// markdown → a standalone HTML document (`pandoc --standalone --to html`).
#[must_use]
pub fn markdown_to_html(md: &[u8]) -> Vec<u8> {
    let blocks = parse_doc(md);
    let mut title: Vec<u8> = b"Documentation".to_vec();
    if let Some(Block::Title {
        name,
        section,
        manual,
        ..
    }) = blocks.first()
    {
        title.clear();
        title.extend_from_slice(name);
        if !section.is_empty() {
            title.push(b'(');
            title.extend_from_slice(section);
            title.push(b')');
        }
        if !manual.is_empty() {
            // " — " (U+2014 EM DASH) as raw UTF-8 bytes; a byte string literal
            // cannot carry a `\u{}` escape.
            title.extend_from_slice(" \u{2014} ".as_bytes());
            title.extend_from_slice(manual);
        }
    }

    let mut out: Vec<u8> = Vec::with_capacity(md.len() * 3);
    out.extend_from_slice(b"<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n");
    out.extend_from_slice(b"<meta charset=\"utf-8\">\n");
    out.extend_from_slice(
        b"<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n<title>",
    );
    html_escape(&title, &mut out);
    out.extend_from_slice(b"</title>\n</head>\n<body>\n");
    render_blocks_html(&mut out, &blocks);
    out.extend_from_slice(b"</body>\n</html>\n");
    out
}

/// Every `<dt>` term the document defines — used by `xtask` and the tests to
/// prove the generated catalogs actually reached the rendered artifact rather
/// than being silently dropped by the block parser.
#[must_use]
pub fn definition_terms(md: &[u8]) -> BTreeSet<Vec<u8>> {
    fn walk(blocks: &[Block], acc: &mut BTreeSet<Vec<u8>>) {
        for b in blocks {
            if let Block::Def { term, body } = b {
                acc.insert(term.clone());
                walk(body, acc);
            }
        }
    }
    let mut acc = BTreeSet::new();
    walk(&parse_doc(md), &mut acc);
    acc
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roff(md: &[u8]) -> String {
        String::from_utf8(markdown_to_roff(md)).unwrap()
    }

    /// The `%`-title block becomes `.TH` (`pandoc --to man` behavior), with the
    /// date field POPULATED — see the module docs for why this deviates.
    #[test]
    fn title_block_becomes_th() {
        let out = roff(b"% RUSTTY(1) Version 1.2.3 | Rustty terminal emulator\n\n# NAME\n\nx\n");
        assert!(
            out.starts_with(
                ".TH \"RUSTTY\" \"1\" \"2026-07-30\" \"Version 1.2.3\" \"Rustty terminal emulator\"\n"
            ),
            "got: {}",
            out.lines().next().unwrap_or("")
        );
        // Control: the raw `%` line must NOT survive into the output.
        assert!(!out.contains("% RUSTTY"));
        // Control: an empty date field would resurrect mandoc's `missing date`
        // warning, which is the whole reason MAN_DATE exists.
        assert!(!out.contains("\"1\" \"\""));
    }

    /// The title override drives the `.TH` name (the `identity-oracle` profile),
    /// and nothing else.
    #[test]
    fn title_override_renames_only_the_th() {
        let md = b"% RUSTTY(1) Version 1 | desc\n\n# NAME\n\n**rustty** - x\n";
        let out = String::from_utf8(markdown_to_roff_with_title(md, Some(b"GHOSTTY"))).unwrap();
        assert!(out.starts_with(".TH \"GHOSTTY\" \"1\" "), "got {out}");
        // Control: the body prose is untouched by the override.
        assert!(out.contains("\\fBrustty\\fR"), "got {out}");
    }

    /// A definition list becomes `.TP`; the term keeps its inline formatting.
    #[test]
    fn definition_list_becomes_tp() {
        let out = roff(b"% T(1) v | t\n\n# X\n\n**`font-family`**\n\n:   The font families.\n    Second line.\n");
        assert!(
            out.contains(".TP\n\\fB\\f(CRfont\\-family\\fB\\fR\n"),
            "no .TP term: {out}"
        );
        // `\fP` must never be emitted: it keeps one level of history and would
        // leave the nested bold-around-code term in the wrong font.
        assert!(!out.contains("\\fP"), "used the one-level \\fP: {out}");
        assert!(out.contains("The font families."), "no body: {out}");
        assert!(out.contains("Second line."), "no continuation: {out}");
        // Control: the raw pandoc definition marker must not survive.
        assert!(!out.contains(":   "), "literal ':   ' leaked: {out}");
    }

    /// A multi-paragraph definition nests the extra paragraphs in `.RS`/`.RE`,
    /// and a bullet list inside a definition survives.
    #[test]
    fn multi_paragraph_definition_nests_and_keeps_bullets() {
        let md = b"% T(1) v | t\n\n# X\n\n**RUSTTY_LOG**\n\n: First para.\n\n: - `stderr` - to stderr.\n: - `macos` - to the unified log.\n\n# Y\n\nz\n";
        let out = roff(md);
        assert!(out.contains(".TP\n\\fBRUSTTY_LOG\\fR\n"), "term: {out}");
        assert!(out.contains(".RS\n"), "no nested region: {out}");
        assert!(out.contains(".IP \\(bu 2\n"), "bullets lost: {out}");
        assert_eq!(out.matches(".RS\n").count(), out.matches(".RE\n").count());
        // Control: `_` inside RUSTTY_LOG must NOT become italics.
        assert!(
            !out.contains("RUSTTY\\fILOG"),
            "intraword _ emphasised: {out}"
        );
    }

    /// Roff control characters at line start are neutered — an unescaped leading
    /// `.` or `'` would be parsed as a macro and silently swallow the line.
    #[test]
    fn leading_control_chars_are_escaped() {
        let out = roff(b"% T(1) v | t\n\n# X\n\n.tricky line\n");
        assert!(out.contains("\\&.tricky line"), "got {out}");
        // Control: the unguarded form must not appear anywhere.
        assert!(!out.contains("\n.tricky"), "unguarded control line: {out}");
    }

    /// A literal hyphen becomes `\-` (an unescaped `-` renders as a Unicode
    /// hyphen and breaks copy-paste of option names).
    #[test]
    fn hyphens_are_escaped() {
        let out = roff(b"% T(1) v | t\n\n# X\n\nRun --font-family now.\n");
        assert!(out.contains("\\-\\-font\\-family"), "got {out}");
        // Control: no bare `--font-family` survives.
        assert!(!out.contains(" --font-family"), "bare hyphens: {out}");
    }

    /// A code block is emitted no-fill and is never re-wrapped, because there a
    /// newline is significant.
    #[test]
    fn code_block_is_nofill_and_unwrapped() {
        let long = format!("% T(1) v | t\n\n# X\n\n    {}\n", "a".repeat(120));
        let out = roff(long.as_bytes());
        assert!(out.contains(".RS 4\n.EX\n"), "no .EX: {out}");
        assert!(out.contains(".EE\n.RE\n"), "no .EE: {out}");
        assert!(out.contains(&"a".repeat(120)), "code block was wrapped");
    }

    /// Long filled prose IS wrapped, so `mandoc -Tlint` stays free of
    /// `input text line longer than 80 bytes`.
    #[test]
    fn long_prose_is_wrapped_under_eighty_columns() {
        let para = "word ".repeat(60);
        let md = format!("% T(1) v | t\n\n# X\n\n{para}\n");
        let out = roff(md.as_bytes());
        for line in out.lines() {
            assert!(
                line.len() <= 80,
                "line longer than 80 bytes ({}): {line}",
                line.len()
            );
        }
        // Control: the unwrapped source line WAS longer than the limit, so the
        // assertion above is not vacuous.
        assert!(para.len() > 80);
    }

    /// Links and autolinks become the `\(la … \(ra` angle-bracket form.
    #[test]
    fn links_become_angle_brackets() {
        let out = roff(
            b"% T(1) v | t\n\n# X\n\nMail <gael@airely.io> or [docs](https://example.invalid/d).\n",
        );
        assert!(out.contains("\\(lagael@airely.io\\(ra"), "autolink: {out}");
        assert!(
            out.contains("docs \\(lahttps://example.invalid/d\\(ra"),
            "link: {out}"
        );
        // Control: the markdown syntax must not survive.
        assert!(!out.contains("]("), "raw markdown link leaked: {out}");
    }

    /// Markdown backslash escapes resolve to the literal character, so the
    /// fixtures' `_\$XDG_CONFIG_HOME…_` renders a real `$`.
    #[test]
    fn markdown_backslash_escapes_resolve() {
        let out = roff(b"% T(1) v | t\n\n# X\n\n_\\$XDG_CONFIG_HOME/rustty_\n");
        assert!(out.contains("$XDG_CONFIG_HOME/rustty"), "got {out}");
        assert!(!out.contains("\\e$"), "escape leaked as roff \\e: {out}");
    }

    /// Non-ASCII text becomes a `\[uXXXX]` roff escape, so the page is pure
    /// ASCII. Raw UTF-8 makes `groff -man -Tutf8` emit
    /// `warning: invalid input character code 134` per continuation byte and
    /// DROP the character — measured on the real generated pages.
    #[test]
    fn non_ascii_becomes_a_unicode_escape() {
        let out = roff("% T(1) v | t\n\n# X\n\ncolors (0\u{2013}15) \u{2014} ok\n".as_bytes());
        assert!(out.contains("(0\\[u2013]15)"), "en dash not escaped: {out}");
        assert!(out.contains("\\[u2014]"), "em dash not escaped: {out}");
        // Control: the whole rendered page is ASCII.
        assert!(out.is_ascii(), "non-ASCII byte survived into the roff");
    }

    /// Inline markup that WRAPS across source lines is still recognised: the
    /// filled run is joined before the inline pass. The real fixtures do this —
    /// the `class` doc comment splits a `[text](url)` link over three lines,
    /// and a per-line pass emitted the raw markdown into the man page.
    #[test]
    fn inline_markup_spanning_source_lines_is_matched() {
        let md = b"% T(1) v | t\n\n# X\n\nfollow the rules defined [in the GTK\ndocumentation](https://docs.gtk.org/x.html)\nor it is ignored.\n";
        let out = roff(md);
        assert!(
            out.contains("\\(lahttps://docs.gtk.org/x.html\\(ra"),
            "multi-line link not matched: {out}"
        );
        // Control: no raw markdown syntax survives.
        assert!(!out.contains("]("), "raw markdown link leaked: {out}");
        assert!(!out.contains("[in the GTK"), "raw link text leaked: {out}");
        // The HTML back end must agree.
        let html = String::from_utf8(markdown_to_html(md)).unwrap();
        assert!(
            html.contains("<a href=\"https://docs.gtk.org/x.html\">"),
            "{html}"
        );
    }

    /// HTML is well-formed, standalone, and escapes `<`/`>`/`&` in text.
    #[test]
    fn html_is_standalone_and_escaped() {
        let md = b"% T(1) v | Manual\n\n# X\n\nUse <script>alert(1)</script> & 5 > 3.\n\n**term**\n\n: body\n";
        let s = String::from_utf8(markdown_to_html(md)).unwrap();
        assert!(s.starts_with("<!DOCTYPE html>"), "not standalone: {s}");
        assert!(
            s.contains("<title>T(1) \u{2014} Manual</title>"),
            "title: {s}"
        );
        assert!(s.contains("&lt;script&gt;"), "no escaping: {s}");
        assert!(s.contains("&amp;"), "ampersand not escaped: {s}");
        assert!(s.contains("&gt; 3"), "gt not escaped: {s}");
        // Control: no executable markup survives from the input.
        assert!(!s.to_lowercase().contains("<script"), "raw script tag: {s}");
        // Definition lists round-trip.
        assert!(s.contains("<dl>") && s.contains("<dt>") && s.contains("</dl>"));
    }

    /// `definition_terms` sees every term the roff back end emits — the shared
    /// block parse is what makes the two artifacts agree.
    #[test]
    fn definition_terms_match_the_tp_count() {
        let md = b"% T(1) v | t\n\n# X\n\n**a**\n\n: one\n\n**b**\n\n: two\n\n**c**\n\n: three\n";
        let terms = definition_terms(md);
        assert_eq!(terms.len(), 3, "{terms:?}");
        let out = roff(md);
        assert_eq!(out.matches(".TP\n").count(), 3, "{out}");
        // Control: a document with no definitions yields none.
        assert!(definition_terms(b"% T(1) v | t\n\n# X\n\nplain\n").is_empty());
    }
}
