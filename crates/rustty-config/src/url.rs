//! rustty-config::url
//!
//! PORTS: ghostty src/config/url.zig — the default URL/path detection regex (the
//! non-regex string-assembly logic; the single oniguruma-backed test is gated
//! behind the off-by-default `oniguruma` feature). C1 leaf value-type (1 test,
//! deferred behind `oniguruma`).
//!
//! PORT STATUS: ported (every line/comment; the 1 oniguruma test gated, R1).

// const std = @import("std");
// const oni = @import("oniguruma");
//
// PORT NOTE: `oniguruma` is only used by the one `url regex` test. oniguruma-sys
// is not built yet, so that test is gated behind the off-by-default `oniguruma`
// cargo feature (a SkipZigTest-style defer, documented in DEFERRALS.md). The
// regex string-assembly logic below has no runtime dependency on oniguruma and
// is ported normally.

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

const TRAILING_SPACES_AT_EOL: &str = r"(?: +(?= *$))?";

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
//     trailing_spaces_at_eol ++
//     "|" ++
//     non_dotted_path_lookahead ++
//     path_chars ++ "+" ++
//     any_path_space_segments ++
//     no_trailing_colon ++
//     trailing_spaces_at_eol ++
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
    s.push_str(TRAILING_SPACES_AT_EOL);
    s.push('|');
    s.push_str(NON_DOTTED_PATH_LOOKAHEAD);
    s.push_str(PATH_CHARS);
    s.push('+');
    s.push_str(ANY_PATH_SPACE_SEGMENTS);
    s.push_str(NO_TRAILING_COLON);
    s.push_str(TRAILING_SPACES_AT_EOL);
    s.push(')');
    s
}

// Branch 3: Bare relative paths such as src/config/url.zig.
const BARE_RELATIVE_PATH_PREFIX: &str = r"(?<!\$\d*)(?<!\w)[\w][\w\-.]*\/";

// const bare_relative_path_branch =
//     dotted_path_lookahead ++
//     bare_relative_path_prefix ++
//     path_chars ++ "+" ++
//     no_trailing_colon ++
//     trailing_spaces_at_eol;
fn bare_relative_path_branch() -> String {
    let mut s = String::new();
    s.push_str(DOTTED_PATH_LOOKAHEAD);
    s.push_str(BARE_RELATIVE_PATH_PREFIX);
    s.push_str(PATH_CHARS);
    s.push('+');
    s.push_str(NO_TRAILING_COLON);
    s.push_str(TRAILING_SPACES_AT_EOL);
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

// test "url regex" {
//
// PORT NOTE (DEFERRED, R1): this is the single oniguruma-backed test in
// `url.zig`. oniguruma-sys is not built yet, so the test is gated behind the
// off-by-default `oniguruma` cargo feature (a SkipZigTest-style defer). It is
// NOT dropped — it is scheduled, recorded in `.port/phase-2/DEFERRALS.md`. The
// full reference body (all match/no-match cases) is preserved below verbatim as
// a comment so the assertion inventory is intact; it will be enabled and ported
// assertion-for-assertion once `oniguruma-sys` lands.
//
//     const testing = std.testing;
//
//     try oni.testing.ensureInit();
//     var re = try oni.Regex.init(
//         regex,
//         .{},
//         oni.Encoding.utf8,
//         oni.Syntax.default,
//         null,
//     );
//     defer re.deinit();
//
//     // The URL cases to test what our regex matches. Feel free to add to this
//     // as we find bugs or just want more coverage.
//     const cases = [_]struct {
//         input: []const u8,
//         expect: []const u8,
//         num_matches: usize = 1,
//     }{
//         .{ .input = "hello https://example.com world", .expect = "https://example.com" },
//         .{ .input = "https://example.com/foo(bar) more", .expect = "https://example.com/foo(bar)" },
//         .{ .input = "https://example.com/foo(bar)baz more", .expect = "https://example.com/foo(bar)baz" },
//         .{ .input = "Link inside (https://example.com) parens", .expect = "https://example.com" },
//         .{ .input = "Link period https://example.com. More text.", .expect = "https://example.com" },
//         .{ .input = "Link trailing comma https://example.com, more text.", .expect = "https://example.com" },
//         .{ .input = "Link in double quotes \"https://example.com\" and more", .expect = "https://example.com" },
//         .{ .input = "Link in single quotes 'https://example.com' and more", .expect = "https://example.com" },
//         .{ .input = "some file with https://google.com https://duckduckgo.com links.", .expect = "https://google.com" },
//         .{ .input = "and links in it. links https://yahoo.com mailto:test@example.com ssh://1.2.3.4", .expect = "https://yahoo.com" },
//         .{ .input = "also match http://example.com non-secure links", .expect = "http://example.com" },
//         .{ .input = "match tel://+12123456789 phone numbers", .expect = "tel://+12123456789" },
//         .{ .input = "match with query url https://example.com?query=1&other=2 and more text.", .expect = "https://example.com?query=1&other=2" },
//         .{ .input = "url with dashes [mode 2027](https://github.com/contour-terminal/terminal-unicode-core) for better unicode support", .expect = "https://github.com/contour-terminal/terminal-unicode-core" },
//         .{ .input = "dot.http://example.com", .expect = "http://example.com" },
//         // weird characters in URL
//         .{ .input = "weird characters https://example.com/~user/?query=1&other=2#hash and more", .expect = "https://example.com/~user/?query=1&other=2#hash" },
//         // square brackets in URL
//         .{ .input = "square brackets https://example.com/[foo] and more", .expect = "https://example.com/[foo]" },
//         // square bracket following url
//         .{ .input = "[13]:TooManyStatements: TempFile#assign_temp_file_to_entity has approx 7 statements [https://example.com/docs/Too-Many-Statements.md]", .expect = "https://example.com/docs/Too-Many-Statements.md" },
//         // remaining URL schemes tests
//         .{ .input = "match ftp://example.com ftp links", .expect = "ftp://example.com" },
//         .{ .input = "match file://example.com file links", .expect = "file://example.com" },
//         .{ .input = "match ssh://example.com ssh links", .expect = "ssh://example.com" },
//         .{ .input = "match git://example.com git links", .expect = "git://example.com" },
//         .{ .input = "/tmp/test.txt http://www.google.com", .expect = "/tmp/test.txt" },
//         .{ .input = "match tel:+18005551234 tel links", .expect = "tel:+18005551234" },
//         .{ .input = "match magnet:?xt=urn:btih:1234567890 magnet links", .expect = "magnet:?xt=urn:btih:1234567890" },
//         .{ .input = "match ipfs://QmSomeHashValue ipfs links", .expect = "ipfs://QmSomeHashValue" },
//         .{ .input = "match ipns://QmSomeHashValue ipns links", .expect = "ipns://QmSomeHashValue" },
//         .{ .input = "match gemini://example.com gemini links", .expect = "gemini://example.com" },
//         .{ .input = "match gopher://example.com gopher links", .expect = "gopher://example.com" },
//         .{ .input = "match news:comp.infosystems.www.servers.unix news links", .expect = "news:comp.infosystems.www.servers.unix" },
//         .{ .input = "/Users/ghostty.user/code/example.py", .expect = "/Users/ghostty.user/code/example.py" },
//         .{ .input = "/Users/ghostty.user/code/../example.py", .expect = "/Users/ghostty.user/code/../example.py" },
//         .{ .input = "/Users/ghostty.user/code/../example.py hello world", .expect = "/Users/ghostty.user/code/../example.py" },
//         .{ .input = "../example.py", .expect = "../example.py" },
//         .{ .input = "../example.py ", .expect = "../example.py " },
//         .{ .input = "first time ../example.py contributor ", .expect = "../example.py" },
//         .{ .input = "[link](/home/user/ghostty.user/example)", .expect = "/home/user/ghostty.user/example" },
//         // IPv6 URL tests - Basic tests
//         .{ .input = "Serving HTTP on :: port 8000 (http://[::]:8000/)", .expect = "http://[::]:8000/" },
//         .{ .input = "IPv6 address https://[2001:db8::1]:8080/path", .expect = "https://[2001:db8::1]:8080/path" },
//         .{ .input = "IPv6 localhost http://[::1]:3000", .expect = "http://[::1]:3000" },
//         .{ .input = "Complex IPv6 https://[2001:db8:85a3:8d3:1319:8a2e:370:7348]:443/", .expect = "https://[2001:db8:85a3:8d3:1319:8a2e:370:7348]:443/" },
//         // IPv6 URL tests - URLs with paths and query parameters
//         .{ .input = "IPv6 with path https://[2001:db8::1]/path/to/resource", .expect = "https://[2001:db8::1]/path/to/resource" },
//         .{ .input = "IPv6 with query https://[2001:db8::1]:8080/api?param=value&other=123", .expect = "https://[2001:db8::1]:8080/api?param=value&other=123" },
//         // IPv6 URL tests - Compressed forms
//         .{ .input = "IPv6 compressed http://[2001:db8::]:80/", .expect = "http://[2001:db8::]:80/" },
//         .{ .input = "IPv6 multiple zeros http://[2001:0:0:0:0:0:0:1]", .expect = "http://[2001:0:0:0:0:0:0:1]" },
//         // IPv6 URL tests - Special cases
//         .{ .input = "IPv6 link-local https://[fe80::1234:5678:9abc]", .expect = "https://[fe80::1234:5678:9abc]" },
//         .{ .input = "IPv6 multicast http://[ff02::1]/stream", .expect = "http://[ff02::1]/stream" },
//         // IPv6 URL tests - Mixed scenarios
//         .{ .input = "IPv6 in markdown [link](http://[2001:db8::1]/docs)", .expect = "http://[2001:db8::1]/docs" },
//         // File paths with spaces
//         .{ .input = "./spaces-end.   ", .expect = "./spaces-end.   " },
//         .{ .input = "./space middle", .expect = "./space middle" },
//         .{ .input = "../test folder/file.txt", .expect = "../test folder/file.txt" },
//         .{ .input = "/tmp/test folder/file.txt", .expect = "/tmp/test folder/file.txt" },
//         .{ .input = "/tmp/test  folder/file.txt", .expect = "/tmp/test" },
//         // unified diff lines
//         .{ .input = "diff --git a/src/font/shaper/harfbuzz.zig b/src/font/shaper/harfbuzz.zig", .expect = "a/src/font/shaper/harfbuzz.zig" },
//         // Two space-separated absolute paths should match only the first
//         .{ .input = "/tmp/foo /tmp/bar", .expect = "/tmp/foo" },
//         .{ .input = "/tmp/foo.txt /tmp/bar.txt", .expect = "/tmp/foo.txt" },
//         // Bare relative file paths (no ./ or ../ prefix)
//         .{ .input = "src/config/url.zig", .expect = "src/config/url.zig" },
//         .{ .input = "app/folder/file.rb:1", .expect = "app/folder/file.rb:1" },
//         .{ .input = "modified:   src/config/url.zig", .expect = "src/config/url.zig" },
//         .{ .input = "lib/ghostty/terminal.zig:42:10", .expect = "lib/ghostty/terminal.zig:42:10" },
//         .{ .input = "some-pkg/src/file.txt more text", .expect = "some-pkg/src/file.txt" },
//         // comma should match substrings
//         .{ .input = "src/foo.c,baz.txt", .expect = "src/foo.c" },
//         .{ .input = "~/foo/bar.txt", .expect = "~/foo/bar.txt" },
//         .{ .input = "open ~/Documents/notes.md please", .expect = "~/Documents/notes.md" },
//         .{ .input = "~/.config/ghostty/config", .expect = "~/.config/ghostty/config" },
//         .{ .input = "directory: ~/src/ghostty-org/ghostty", .expect = "~/src/ghostty-org/ghostty" },
//         .{ .input = "$HOME/src/config/url.zig", .expect = "$HOME/src/config/url.zig" },
//         .{ .input = "project dir: $PWD/src/ghostty/main.zig", .expect = "$PWD/src/ghostty/main.zig" },
//         // $VAR mid-path should match fully, not partially from the $
//         .{ .input = "foo/$BAR/baz", .expect = "foo/$BAR/baz" },
//         .{ .input = ".foo/bar/$VAR", .expect = ".foo/bar/$VAR" },
//         .{ .input = ".config/ghostty/config", .expect = ".config/ghostty/config" },
//         .{ .input = "loaded from .local/share/ghostty/state.db now", .expect = ".local/share/ghostty/state.db" },
//         .{ .input = "../some/where", .expect = "../some/where" },
//         // comma-separated file paths
//         .{ .input = "  - shared/src/foo/SomeItem.m:12, shared/src/", .expect = "shared/src/foo/SomeItem.m:12" },
//         // mid-string dot should not partially match but fully
//         .{ .input = "foo.local/share", .expect = "foo.local/share" },
//         // numeric directory should match fully
//         .{ .input = "2024/report.txt", .expect = "2024/report.txt" },
//         // comma should stop matching in spaced path segments
//         .{ .input = "./foo bar,baz", .expect = "./foo bar" },
//         .{ .input = "/tmp/foo bar,baz", .expect = "/tmp/foo bar" },
//         // trailing colon should not be part of the path
//         .{ .input = "./.config/ghostty: Needs upstream (main)", .expect = "./.config/ghostty" },
//         .{ .input = "./Downloads: Operation not permitted", .expect = "./Downloads" },
//     };
//
//     for (cases) |case| {
//         //std.debug.print("input: {s}\n", .{case.input});
//         //std.debug.print("match: {s}\n", .{case.expect});
//         var reg = try re.search(case.input, .{});
//         //std.debug.print("count: {d}\n", .{@as(usize, reg.count())});
//         //std.debug.print("starts: {d}\n", .{reg.starts()});
//         //std.debug.print("ends: {d}\n", .{reg.ends()});
//         defer reg.deinit();
//         try testing.expectEqual(@as(usize, case.num_matches), reg.count());
//         const match = case.input[@intCast(reg.starts()[0])..@intCast(reg.ends()[0])];
//         try testing.expectEqualStrings(case.expect, match);
//     }
//
//     const no_match_cases = [_][]const u8{
//         // bare relative paths without any dot should not match as file paths
//         "input/output",
//         "foo/bar",
//         // $-numeric character should not match
//         "$10/bar",
//         "$10/$20",
//         "$10/bar.txt",
//         // comma should not let dot detection look past it
//         "foo/bar,baz.txt",
//         // $VAR should not match mid-word
//         "foo$BAR/baz.txt",
//         // ~ should not match mid-word
//         "foo~/bar.txt",
//         // double-slash comments are not paths
//         "// foo bar",
//         "//foo",
//     };
//     for (no_match_cases) |input| {
//         var result = re.search(input, .{});
//         if (result) |*reg| {
//             reg.deinit();
//             return error.TestUnexpectedResult;
//         } else |_| {}
//     }
// }
#[cfg(all(test, feature = "oniguruma"))]
mod tests {
    // The oniguruma-backed `url regex` test will be ported here once
    // oniguruma-sys is built. See the verbatim reference body above.
}
