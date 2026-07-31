//! rustty-config::url
//!
//! PORTS: ghostty src/config/url.zig — the default URL/path detection regex (the
//! non-regex string-assembly logic plus the single oniguruma-backed test, gated
//! behind the off-by-default `oniguruma` feature). C1 leaf value-type (1 test,
//! ported behind `oniguruma`).
//!
//! PORT STATUS: ported (every line/comment; the 1 oniguruma test ported behind
//! the `oniguruma` feature, R1).

// const std = @import("std");
// const oni = @import("oniguruma");
//
// PORT NOTE: `oniguruma` is only used by the one `url regex` test. That test is
// ported (see the `#[cfg(all(test, feature = "oniguruma"))]` module below) and
// gated behind the off-by-default `oniguruma` cargo feature, which pulls in the
// `oni` safe wrapper crate (`rustty-oniguruma`) — exactly as vt's string_map
// search surface is gated. The feature keeps the C Oniguruma dependency out of
// the default build (documented in DEFERRALS.md, RESOLVED). The regex
// string-assembly logic below has no runtime dependency on oniguruma and is
// ported normally.

/// Default URL/path regex. This is used to detect URLs and file paths in
/// terminal output.
///
/// This is here in the config package because one day the matchers will be
/// configurable and this will be a default.
///
/// For scheme URLs, this regex is liberal in what it accepts after the scheme,
/// with exceptions for URLs ending with . or ). Although such URLs are
/// perfectly valid, it is common for text to contain URLs surrounded by
/// parentheses (such as in Markdown links) or at the end of sentences.
/// Therefore, this regex excludes them as follows:
///
/// 1. Do not match regexes ending with .
/// 2. Do not match regexes ending with ), except for ones which contain a (
///    without a subsequent )
///
/// Rule 2 means that we handle the following two cases:
///
///   "https://en.wikipedia.org/wiki/Rust_(video_game)" (include parens)
///   "(https://example.com)" (do not include the parens)
///
/// There are many complicated cases where these heuristics break down, but
/// handling them well requires a non-regex approach.
//
// PORT NOTE: each `const X = \\...;` below is a Zig multiline string literal
// (the `\\` prefix is Zig raw-string syntax; the literal CONTENT begins after it
// and is reproduced byte-for-byte as a Rust raw string `r"..."`). The final
// `regex` is the `++`-concatenation of the branches, preserved exactly.
const URL_SCHEMES: &str = r"https?://|mailto:|ftp://|file:|ssh:|git://|ssh://|tel:|magnet:|ipfs://|ipns://|gemini://|gopher://|news:";

const IPV6_URL_PATTERN: &str = r"(?:\[[:0-9a-fA-F]+(?:[:0-9a-fA-F]*)+\](?::[0-9]+)?)";

const SCHEME_URL_CHARS: &str = r"[\w\-.~:/?#@!$&*+,;=%]";

const PATH_CHARS: &str = r"[\w\-.~:\/?#@!$&*+;=%]";

const OPTIONAL_BRACKETED_WORD_SUFFIX: &str = r"(?:[\(\[]\w*[\)\]])?";

const NO_TRAILING_PUNCTUATION: &str = r"(?<![,.])";

const NO_TRAILING_COLON: &str = r"(?<!:)";

const DOTTED_PATH_LOOKAHEAD: &str = r"(?=[\w\-.~:\/?#@!$&*+;=%]*\.)";

const NON_DOTTED_PATH_LOOKAHEAD: &str = r"(?![\w\-.~:\/?#@!$&*+;=%]*\.)";

const DOTTED_PATH_SPACE_SEGMENTS: &str =
    r"(?:(?<!:) (?!\w+:\/\/)(?!\.{0,2}\/)(?!~\/)[\w\-.~:\/?#@!$&*+;=%]*[\/.])*";

const ANY_PATH_SPACE_SEGMENTS: &str =
    r"(?:(?<!:) (?!\w+:\/\/)(?!\.{0,2}\/)(?!~\/)[\w\-.~:\/?#@!$&*+;=%]+)*";

// Branch 1: URLs with explicit schemes (http, mailto, ftp, etc.).
// const scheme_url_branch =
//     "(?:" ++ url_schemes ++ ")" ++
//     "(?:" ++ ipv6_url_pattern ++ "|" ++ scheme_url_chars ++ "+" ++ optional_bracketed_word_suffix ++ ")+" ++
//     no_trailing_punctuation;
fn scheme_url_branch() -> String {
    let mut s = String::new();
    s.push_str("(?:");
    s.push_str(URL_SCHEMES);
    s.push(')');
    s.push_str("(?:");
    s.push_str(IPV6_URL_PATTERN);
    s.push('|');
    s.push_str(SCHEME_URL_CHARS);
    s.push('+');
    s.push_str(OPTIONAL_BRACKETED_WORD_SUFFIX);
    s.push_str(")+");
    s.push_str(NO_TRAILING_PUNCTUATION);
    s
}

const ROOTED_OR_RELATIVE_PATH_PREFIX: &str = r"(?:\.\.\/|\.\/|(?<!\w)~\/|(?:[\w][\w\-.]*\/)*(?<!\w)\$[A-Za-z_]\w*\/|\.[\w][\w\-.]*\/|(?<![\w~\/])\/(?!\/))";

// Branch 2: Absolute paths and dot-relative paths (/, ./, ../).
// A dotted segment is treated as file-like, while the undotted case stays
// broad to capture directory-like paths with spaces.
// const rooted_or_relative_path_branch =
//     rooted_or_relative_path_prefix ++
//     "(?:" ++
//     dotted_path_lookahead ++
//     path_chars ++ "+" ++
//     dotted_path_space_segments ++
//     no_trailing_colon ++
//     "|" ++
//     non_dotted_path_lookahead ++
//     path_chars ++ "+" ++
//     any_path_space_segments ++
//     no_trailing_colon ++
//     ")";
fn rooted_or_relative_path_branch() -> String {
    let mut s = String::new();
    s.push_str(ROOTED_OR_RELATIVE_PATH_PREFIX);
    s.push_str("(?:");
    s.push_str(DOTTED_PATH_LOOKAHEAD);
    s.push_str(PATH_CHARS);
    s.push('+');
    s.push_str(DOTTED_PATH_SPACE_SEGMENTS);
    s.push_str(NO_TRAILING_COLON);
    s.push('|');
    s.push_str(NON_DOTTED_PATH_LOOKAHEAD);
    s.push_str(PATH_CHARS);
    s.push('+');
    s.push_str(ANY_PATH_SPACE_SEGMENTS);
    s.push_str(NO_TRAILING_COLON);
    s.push(')');
    s
}

// Branch 3: Bare relative paths such as src/config/url.zig.
const BARE_RELATIVE_PATH_PREFIX: &str = r"(?<!\$\d*)(?<!\w)[\w][\w\-.]*\/";

// const bare_relative_path_branch =
//     dotted_path_lookahead ++
//     bare_relative_path_prefix ++
//     path_chars ++ "+" ++
//     no_trailing_colon;
fn bare_relative_path_branch() -> String {
    let mut s = String::new();
    s.push_str(DOTTED_PATH_LOOKAHEAD);
    s.push_str(BARE_RELATIVE_PATH_PREFIX);
    s.push_str(PATH_CHARS);
    s.push('+');
    s.push_str(NO_TRAILING_COLON);
    s
}

/// PORTS: `pub const regex` — the full URL/path detection regex.
// pub const regex =
//     scheme_url_branch ++
//     "|" ++
//     rooted_or_relative_path_branch ++
//     "|" ++
//     bare_relative_path_branch;
//
// PORT NOTE: the reference `regex` is a comptime-concatenated `const`. Here it is
// assembled at runtime by `regex()` (a pure string build, no oniguruma). The
// produced string is byte-for-byte identical to the reference's `regex`.
pub fn regex() -> String {
    let mut s = String::new();
    s.push_str(&scheme_url_branch());
    s.push('|');
    s.push_str(&rooted_or_relative_path_branch());
    s.push('|');
    s.push_str(&bare_relative_path_branch());
    s
}

// PORT NOTE (NOT a reference test — area G OQ-4 / `P3-W9 T1 Step 2`): the oracle
// keeps ONE copy of this regex (`src/config/url.zig`). rustty has TWO, because
// `rustty-vt` cannot depend on `rustty-config` (R4: config is ABOVE vt, so a
// vt->config edge is a cycle) and vt's `StringMap` URL tests need the same
// fixture. The second copy is `rustty_vt::string_map::url`.
//
// Nothing detected a one-sided edit. Oracle `9fc2a3085` (delete
// `trailing_spaces_at_eol`) is exactly the shape that hazard produces: a
// one-line upstream deletion that must be applied at two rustty sites. This gate
// converts that into a one-line-per-site fix WITH a guarantee.
//
// Deliberately NOT behind the `oniguruma` feature: `.port/tools/ci.sh` never
// passes `--features oniguruma` (`grep -rn oniguruma .port/tools/ci.sh` -> 0
// hits), so the `url regex` behaviour table below is exercised by NO CI lane.
// The duplication gate must not inherit that hole, so it runs on default
// features — which is possible because both regex assemblies are pure string
// builds with no oniguruma dependency.
#[cfg(test)]
mod duplication_gate {
    #[test]
    fn regex_is_byte_identical_to_the_vt_copy() {
        let config_copy = super::regex();
        let vt_copy = rustty_vt::string_map::url::regex();
        // `String` is UTF-8 bytes, so `==` here IS byte identity; comparing the
        // strings (rather than `as_bytes()`) only makes the failure readable.
        assert_eq!(
            config_copy, vt_copy,
            "rustty-config::url::regex() and rustty_vt::string_map::url::regex() \
             have diverged. They are two transcriptions of ONE oracle constant \
             (src/config/url.zig `pub const regex`); every edit must be applied \
             to BOTH."
        );
    }
}

// test "url regex" {
//
// PORT NOTE (R1): the single oniguruma-backed test in `url.zig`, ported below
// assertion-for-assertion into the `#[cfg(all(test, feature = "oniguruma"))]`
// module. It compiles the assembled `regex()` with Oniguruma (the `oni` safe
// wrapper) and asserts every match case's span + every no-match case's mismatch,
// exactly as the reference does. Gated behind the off-by-default `oniguruma`
// feature (which pulls in `rustty-oniguruma`), documented in DEFERRALS.md
// (RESOLVED).
#[cfg(all(test, feature = "oniguruma"))]
mod tests {
    use super::regex;

    // PORT NOTE: the reference `cases` array element is an anonymous struct with a
    // `num_matches: usize = 1` default field. The faithful mirror is `Case` + the
    // `case(input, expect)` helper supplying that same `num_matches = 1` default
    // (no reference case overrides it). `input`/`expect` are `&[u8]` mirroring the
    // reference `[]const u8` (byte-faithful, not lossy `String`).
    struct Case {
        input: &'static [u8],
        expect: &'static [u8],
        num_matches: usize,
    }

    fn case(input: &'static [u8], expect: &'static [u8]) -> Case {
        Case {
            input,
            expect,
            num_matches: 1,
        }
    }

    // test "url regex"
    #[test]
    fn url_regex() {
        oni::testing::ensure_init().unwrap();
        let pattern = regex();
        let mut re = oni::Regex::init(
            pattern.as_bytes(),
            oni::RegexOptions::default(),
            oni::Encoding::utf8(),
            oni::Syntax::default(),
            None,
        )
        .unwrap();
        // defer re.deinit();

        // The URL cases to test what our regex matches. Feel free to add to this
        // as we find bugs or just want more coverage.
        let cases = [
            case(b"hello https://example.com world", b"https://example.com"),
            case(
                b"https://example.com/foo(bar) more",
                b"https://example.com/foo(bar)",
            ),
            case(
                b"https://example.com/foo(bar)baz more",
                b"https://example.com/foo(bar)baz",
            ),
            case(
                b"Link inside (https://example.com) parens",
                b"https://example.com",
            ),
            case(
                b"Link period https://example.com. More text.",
                b"https://example.com",
            ),
            case(
                b"Link trailing comma https://example.com, more text.",
                b"https://example.com",
            ),
            case(
                b"Link in double quotes \"https://example.com\" and more",
                b"https://example.com",
            ),
            case(
                b"Link in single quotes 'https://example.com' and more",
                b"https://example.com",
            ),
            case(
                b"some file with https://google.com https://duckduckgo.com links.",
                b"https://google.com",
            ),
            case(
                b"and links in it. links https://yahoo.com mailto:test@example.com ssh://1.2.3.4",
                b"https://yahoo.com",
            ),
            case(
                b"also match http://example.com non-secure links",
                b"http://example.com",
            ),
            case(
                b"match tel://+12123456789 phone numbers",
                b"tel://+12123456789",
            ),
            case(
                b"match with query url https://example.com?query=1&other=2 and more text.",
                b"https://example.com?query=1&other=2",
            ),
            case(
                b"url with dashes [mode 2027](https://github.com/contour-terminal/terminal-unicode-core) for better unicode support",
                b"https://github.com/contour-terminal/terminal-unicode-core",
            ),
            case(b"dot.http://example.com", b"http://example.com"),
            // weird characters in URL
            case(
                b"weird characters https://example.com/~user/?query=1&other=2#hash and more",
                b"https://example.com/~user/?query=1&other=2#hash",
            ),
            // square brackets in URL
            case(
                b"square brackets https://example.com/[foo] and more",
                b"https://example.com/[foo]",
            ),
            // square bracket following url
            case(
                b"[13]:TooManyStatements: TempFile#assign_temp_file_to_entity has approx 7 statements [https://example.com/docs/Too-Many-Statements.md]",
                b"https://example.com/docs/Too-Many-Statements.md",
            ),
            // remaining URL schemes tests
            case(b"match ftp://example.com ftp links", b"ftp://example.com"),
            case(b"match file://example.com file links", b"file://example.com"),
            case(b"match ssh://example.com ssh links", b"ssh://example.com"),
            case(b"match git://example.com git links", b"git://example.com"),
            case(b"/tmp/test.txt http://www.google.com", b"/tmp/test.txt"),
            case(b"match tel:+18005551234 tel links", b"tel:+18005551234"),
            case(
                b"match magnet:?xt=urn:btih:1234567890 magnet links",
                b"magnet:?xt=urn:btih:1234567890",
            ),
            case(
                b"match ipfs://QmSomeHashValue ipfs links",
                b"ipfs://QmSomeHashValue",
            ),
            case(
                b"match ipns://QmSomeHashValue ipns links",
                b"ipns://QmSomeHashValue",
            ),
            case(
                b"match gemini://example.com gemini links",
                b"gemini://example.com",
            ),
            case(
                b"match gopher://example.com gopher links",
                b"gopher://example.com",
            ),
            case(
                b"match news:comp.infosystems.www.servers.unix news links",
                b"news:comp.infosystems.www.servers.unix",
            ),
            case(
                b"/Users/ghostty.user/code/example.py",
                b"/Users/ghostty.user/code/example.py",
            ),
            case(
                b"/Users/ghostty.user/code/../example.py",
                b"/Users/ghostty.user/code/../example.py",
            ),
            case(
                b"/Users/ghostty.user/code/../example.py hello world",
                b"/Users/ghostty.user/code/../example.py",
            ),
            case(b"../example.py", b"../example.py"),
            case(b"../example.py ", b"../example.py"),
            case(b"first time ../example.py contributor ", b"../example.py"),
            case(
                b"[link](/home/user/ghostty.user/example)",
                b"/home/user/ghostty.user/example",
            ),
            // IPv6 URL tests - Basic tests
            case(
                b"Serving HTTP on :: port 8000 (http://[::]:8000/)",
                b"http://[::]:8000/",
            ),
            case(
                b"IPv6 address https://[2001:db8::1]:8080/path",
                b"https://[2001:db8::1]:8080/path",
            ),
            case(b"IPv6 localhost http://[::1]:3000", b"http://[::1]:3000"),
            case(
                b"Complex IPv6 https://[2001:db8:85a3:8d3:1319:8a2e:370:7348]:443/",
                b"https://[2001:db8:85a3:8d3:1319:8a2e:370:7348]:443/",
            ),
            // IPv6 URL tests - URLs with paths and query parameters
            case(
                b"IPv6 with path https://[2001:db8::1]/path/to/resource",
                b"https://[2001:db8::1]/path/to/resource",
            ),
            case(
                b"IPv6 with query https://[2001:db8::1]:8080/api?param=value&other=123",
                b"https://[2001:db8::1]:8080/api?param=value&other=123",
            ),
            // IPv6 URL tests - Compressed forms
            case(
                b"IPv6 compressed http://[2001:db8::]:80/",
                b"http://[2001:db8::]:80/",
            ),
            case(
                b"IPv6 multiple zeros http://[2001:0:0:0:0:0:0:1]",
                b"http://[2001:0:0:0:0:0:0:1]",
            ),
            // IPv6 URL tests - Special cases
            case(
                b"IPv6 link-local https://[fe80::1234:5678:9abc]",
                b"https://[fe80::1234:5678:9abc]",
            ),
            case(
                b"IPv6 multicast http://[ff02::1]/stream",
                b"http://[ff02::1]/stream",
            ),
            // IPv6 URL tests - Mixed scenarios
            case(
                b"IPv6 in markdown [link](http://[2001:db8::1]/docs)",
                b"http://[2001:db8::1]/docs",
            ),
            // Trailing whitespace isn't part of a detected file path.
            case(b"./spaces-end.   ", b"./spaces-end."),
            // File paths with internal spaces
            case(b"./space middle", b"./space middle"),
            case(b"../test folder/file.txt", b"../test folder/file.txt"),
            case(b"/tmp/test folder/file.txt", b"/tmp/test folder/file.txt"),
            case(b"/tmp/test  folder/file.txt", b"/tmp/test"),
            // unified diff lines
            case(
                b"diff --git a/src/font/shaper/harfbuzz.zig b/src/font/shaper/harfbuzz.zig",
                b"a/src/font/shaper/harfbuzz.zig",
            ),
            // Two space-separated absolute paths should match only the first
            case(b"/tmp/foo /tmp/bar", b"/tmp/foo"),
            case(b"/tmp/foo.txt /tmp/bar.txt", b"/tmp/foo.txt"),
            // Bare relative file paths (no ./ or ../ prefix)
            case(b"src/config/url.zig", b"src/config/url.zig"),
            case(b"app/folder/file.rb:1", b"app/folder/file.rb:1"),
            case(b"modified:   src/config/url.zig", b"src/config/url.zig"),
            case(
                b"lib/ghostty/terminal.zig:42:10",
                b"lib/ghostty/terminal.zig:42:10",
            ),
            case(b"some-pkg/src/file.txt more text", b"some-pkg/src/file.txt"),
            // comma should match substrings
            case(b"src/foo.c,baz.txt", b"src/foo.c"),
            case(b"~/foo/bar.txt", b"~/foo/bar.txt"),
            case(b"open ~/Documents/notes.md please", b"~/Documents/notes.md"),
            case(b"~/.config/ghostty/config", b"~/.config/ghostty/config"),
            case(
                b"directory: ~/src/ghostty-org/ghostty",
                b"~/src/ghostty-org/ghostty",
            ),
            case(b"$HOME/src/config/url.zig", b"$HOME/src/config/url.zig"),
            case(
                b"project dir: $PWD/src/ghostty/main.zig",
                b"$PWD/src/ghostty/main.zig",
            ),
            // $VAR mid-path should match fully, not partially from the $
            case(b"foo/$BAR/baz", b"foo/$BAR/baz"),
            case(b".foo/bar/$VAR", b".foo/bar/$VAR"),
            case(b".config/ghostty/config", b".config/ghostty/config"),
            case(
                b"loaded from .local/share/ghostty/state.db now",
                b".local/share/ghostty/state.db",
            ),
            case(b"../some/where", b"../some/where"),
            // comma-separated file paths
            case(
                b"  - shared/src/foo/SomeItem.m:12, shared/src/",
                b"shared/src/foo/SomeItem.m:12",
            ),
            // mid-string dot should not partially match but fully
            case(b"foo.local/share", b"foo.local/share"),
            // numeric directory should match fully
            case(b"2024/report.txt", b"2024/report.txt"),
            // comma should stop matching in spaced path segments
            case(b"./foo bar,baz", b"./foo bar"),
            case(b"/tmp/foo bar,baz", b"/tmp/foo bar"),
            // trailing colon should not be part of the path
            case(b"./.config/ghostty: Needs upstream (main)", b"./.config/ghostty"),
            case(b"./Downloads: Operation not permitted", b"./Downloads"),
        ];

        for c in &cases {
            //std.debug.print("input: {s}\n", .{case.input});
            //std.debug.print("match: {s}\n", .{case.expect});
            let mut reg = re.search(c.input, oni::SearchOptions::default()).unwrap();
            //std.debug.print("count: {d}\n", .{@as(usize, reg.count())});
            //std.debug.print("starts: {d}\n", .{reg.starts()});
            //std.debug.print("ends: {d}\n", .{reg.ends()});
            // defer reg.deinit();
            assert_eq!(c.num_matches, reg.count());
            let matched = &c.input[reg.starts()[0] as usize..reg.ends()[0] as usize];
            assert_eq!(
                c.expect,
                matched,
                "input={:?}",
                String::from_utf8_lossy(c.input),
            );
            reg.deinit();
        }

        let no_match_cases: [&[u8]; 10] = [
            // bare relative paths without any dot should not match as file paths
            b"input/output",
            b"foo/bar",
            // $-numeric character should not match
            b"$10/bar",
            b"$10/$20",
            b"$10/bar.txt",
            // comma should not let dot detection look past it
            b"foo/bar,baz.txt",
            // $VAR should not match mid-word
            b"foo$BAR/baz.txt",
            // ~ should not match mid-word
            b"foo~/bar.txt",
            // double-slash comments are not paths
            b"// foo bar",
            b"//foo",
        ];
        for &input in &no_match_cases {
            // PORT NOTE: the reference `if (result) |*reg| { reg.deinit();
            // return error.TestUnexpectedResult; } else |_| {}` — a match is a
            // failure (a no-match search returns `Error.Mismatch`).
            if let Ok(mut reg) = re.search(input, oni::SearchOptions::default()) {
                reg.deinit();
                panic!("TestUnexpectedResult");
            }
        }

        re.deinit();
    }
}
