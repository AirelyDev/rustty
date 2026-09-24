//! zf.zig
//! The zf fuzzy finding algorithm
//! Inspired by https://github.com/garybernhardt/selecta
//!
//! The public façade over `filter` — `list_themes` only needs `rank` / `highlight`.

// const filter = @import("filter.zig");
// const std = @import("std");
// const testing = std.testing;
use crate::filter;

// The anonymous `test { _ = @import("clib.zig"); _ = @import("filter.zig"); }`
// refAllDecls compile-touch is ported as `ref_all_decls` in lib.rs.

pub struct RankOptions {
    /// Set to false for case insensitive ranking.
    pub case_sensitive: bool,

    /// If true, the zf filepath algorithms are disabled (useful for matching arbitrary strings)
    pub plain: bool,
}

impl Default for RankOptions {
    // Zig field defaults: `case_sensitive = true`, `plain = false` (not derivable —
    // `case_sensitive` defaults to `true`, so the `.{}` call sites map here).
    //
    // PORT NOTE (zf 0.10.3 -> 0.11.0, NON_PORTS.md §G2): 0.10.3 had
    // `to_lower: bool = true` and passed `!opts.to_lower` as `case_sensitive`, so the
    // DEFAULT POLARITY FLIPPED with the rename: `.{}` was case-INsensitive and is now
    // case-SENSITIVE. The one caller, `+list-themes`, passes `.case_sensitive = false`
    // explicitly at the reference pin (list_themes.zig:377-386), so its behaviour does
    // not depend on this default.
    fn default() -> Self {
        Self {
            case_sensitive: true,
            plain: false,
        }
    }
}

/// rank a given haystack against a slice of needles
pub fn rank(haystack: &[u8], needles: &[&[u8]], opts: RankOptions) -> Option<f64> {
    let filename = if opts.plain {
        None
    } else {
        Some(filter::basename(haystack))
    };

    // the haystack must contain all of the characters (in order) in each needle.
    // each needle's rank is summed. if any needle does not match the haystack is ignored.
    let mut sum: f64 = 0.0;
    for &needle in needles {
        let strict_path = !opts.plain && filter::has_separator(needle);
        if let Some(r) =
            filter::rank_needle(haystack, filename, needle, opts.case_sensitive, strict_path)
        {
            sum += r;
        } else {
            return None;
        }
    }

    // all needles matched and the best ranks for each needle are summed
    Some(sum)
}

pub struct RankNeedleOptions<'a> {
    /// Set to false for case insensitive ranking.
    pub case_sensitive: bool,

    /// Set to true when the needle has path separators in it
    pub strict_path: bool,

    /// Set to the filename (basename) of the haystack for filepath matching
    pub filename: Option<&'a [u8]>,
}

impl Default for RankNeedleOptions<'_> {
    // Zig field defaults `case_sensitive = true`, `strict_path = false`,
    // `filename = null`. (0.10.3's `RankTokenOptions.to_lower = false` was ALSO
    // case-sensitive by default, so this one did not flip.)
    fn default() -> Self {
        Self {
            case_sensitive: true,
            strict_path: false,
            filename: None,
        }
    }
}

/// rank a given haystack against a single needle
pub fn rank_needle(haystack: &[u8], needle: &[u8], opts: RankNeedleOptions) -> Option<f64> {
    filter::rank_needle(
        haystack,
        opts.filename,
        needle,
        opts.case_sensitive,
        opts.strict_path,
    )
}

#[test]
fn rank_library_interface() {
    assert!(rank(b"abcdefg", &[b"a", b"z"], RankOptions::default()).is_none());
    assert!(rank(b"abcdefg", &[b"a", b"b"], RankOptions::default()).is_some());
    assert!(rank(b"abcdefg", &[b"a", b"B"], RankOptions::default()).is_none());
    assert!(rank(b"aBcdefg", &[b"a", b"B"], RankOptions::default()).is_some());
    assert!(rank(b"a/path/to/file", &[b"zig"], RankOptions::default()).is_none());
    assert!(rank(
        b"a/path/to/file",
        &[b"path", b"file"],
        RankOptions::default()
    )
    .is_some());

    assert!(rank_needle(b"abcdefg", b"a", RankNeedleOptions::default()).is_some());
    assert!(rank_needle(b"abcdefg", b"z", RankNeedleOptions::default()).is_none());
    assert!(rank_needle(b"abcdefG", b"G", RankNeedleOptions::default()).is_some());
    assert!(rank_needle(b"abcdefg", b"A", RankNeedleOptions::default()).is_none());
    assert!(rank_needle(
        b"a/path/to/file",
        b"file",
        RankNeedleOptions {
            filename: Some(b"file"),
            ..Default::default()
        }
    )
    .is_some());
    assert!(rank_needle(
        b"a/path/to/file",
        b"zig",
        RankNeedleOptions {
            filename: Some(b"file"),
            ..Default::default()
        }
    )
    .is_none());

    // zero length haystacks and needles
    assert!(rank(b"", &[b"a"], RankOptions::default()).is_none());
    assert!(rank_needle(b"", b"a", RankNeedleOptions::default()).is_none());
    assert!(rank(b"a", &[b""], RankOptions::default()).is_none());
    assert!(rank_needle(b"a", b"", RankNeedleOptions::default()).is_none());
}

// PORT NOTE (W13B-02 / NON_PORTS.md §G2, discharged by parity tail S-10b): zf moved
// 0.10.3 -> 0.11.0 with the reference pin. `RankOptions.to_lower: bool = true` became
// `RankOptions.case_sensitive: bool = true`, so the public `rank` DEFAULT flipped from
// case-INsensitive to case-SENSITIVE, and case-insensitive mode now lowers the NEEDLE
// as well as the haystack (filter.zig `indexOfInsensitive`), so an uppercase needle
// byte matches too. This test binds both, so neither can drift silently.
#[test]
fn rank_options_default_is_case_sensitive() {
    // Under the 0.11.0 default an uppercase haystack byte does not match a lowercase
    // needle (0.10.3's default lowered the haystack, so both were `is_some()` there).
    assert!(rank(b"aBcdefg", &[b"a", b"b"], RankOptions::default()).is_none());
    assert!(rank(b"ABCDEFG", &[b"abc"], RankOptions::default()).is_none());
    // The exact input pair 0.10.3's default refused (`is_none()` there).
    assert!(rank(b"aBcdefg", &[b"a", b"B"], RankOptions::default()).is_some());
    // Case-insensitive mode is now insensitive in both directions: an uppercase
    // NEEDLE matches a lowercase haystack (0.10.3 never lowered the needle).
    let insensitive = || RankOptions {
        case_sensitive: false,
        ..Default::default()
    };
    assert!(rank(b"abcdefg", &[b"A", b"B"], insensitive()).is_some());
    assert!(rank(b"ABCDEFG", &[b"abc"], insensitive()).is_some());
}

// Maybe all that needs to be done is to sort the highlight integers? That would probably save some work in implementation
// Or maybe could sort and then make ranges out of the pairs? Return a list of ranges?
// for the Zig api that could be reasonable... but the C api maybe not
// sorting as a minimum for sure

/// Remove duplicate match indexes. Assumes input is sorted.
fn deduplicate_matches(matches: &mut [usize]) -> &mut [usize] {
    if matches.len() < 2 {
        return matches;
    }

    let mut end: usize = 1;
    for i in 1..matches.len() {
        let value = matches[i];
        if value != matches[end - 1] {
            matches[end] = value;
            end += 1;
        }
    }
    &mut matches[0..end]
}

#[test]
fn deduplicate_matches_test() {
    {
        let mut matches: [usize; 0] = [];
        assert_eq!(&[] as &[usize], deduplicate_matches(&mut matches));
    }
    {
        let mut matches: [usize; 1] = [1];
        assert_eq!(&[1][..], deduplicate_matches(&mut matches));
    }
    {
        let mut matches: [usize; 2] = [1, 2];
        assert_eq!(&[1, 2][..], deduplicate_matches(&mut matches));
    }
    {
        let mut matches: [usize; 2] = [1, 1];
        assert_eq!(&[1][..], deduplicate_matches(&mut matches));
    }
    {
        let mut matches: [usize; 4] = [1, 2, 3, 3];
        assert_eq!(&[1, 2, 3][..], deduplicate_matches(&mut matches));
    }
    {
        let mut matches: [usize; 4] = [1, 2, 2, 3];
        assert_eq!(&[1, 2, 3][..], deduplicate_matches(&mut matches));
    }
    {
        let mut matches: [usize; 8] = [0, 1, 1, 2, 2, 2, 3, 4];
        assert_eq!(&[0, 1, 2, 3, 4][..], deduplicate_matches(&mut matches));
    }
}

/// compute matching ranges given a haystack and a slice of needles
pub fn highlight<'a>(
    haystack: &[u8],
    needles: &[&[u8]],
    matches: &'a mut [usize],
    opts: RankOptions,
) -> &'a [usize] {
    let filename = if opts.plain {
        None
    } else {
        Some(filter::basename(haystack))
    };

    let mut index: usize = 0;
    for &needle in needles {
        let strict_path = !opts.plain && filter::has_separator(needle);
        let matched = filter::highlight_needle(
            haystack,
            filename,
            needle,
            opts.case_sensitive,
            strict_path,
            &mut matches[index..],
        );
        index += matched.len();
    }

    if needles.len() == 1 {
        return &matches[0..index];
    }

    // When there is more than one needle, matches may overlap. The matched indices are what
    // matter so we sort and deduplicate. This extra computation is fine because highlighting
    // is usually only done for a small number of haystacks.
    // (`std.mem.sortUnstable(usize, .., std.sort.asc(usize))`: the result of sorting
    // integers does not depend on the algorithm.)
    matches[0..index].sort_unstable();
    deduplicate_matches(&mut matches[0..index])
}

/// compute matching ranges given a haystack and a single needle
/// Matches are guaranteed to be in ascending order.
pub fn highlight_needle<'a>(
    haystack: &[u8],
    needle: &[u8],
    matches: &'a mut [usize],
    opts: RankNeedleOptions,
) -> &'a [usize] {
    filter::highlight_needle(
        haystack,
        opts.filename,
        needle,
        opts.case_sensitive,
        opts.strict_path,
        matches,
    )
}

#[test]
fn highlight_library_interface() {
    let mut matches_buf: [usize; 128] = [0; 128];

    assert_eq!(
        highlight(
            b"abcdef",
            &[b"a", b"f"],
            &mut matches_buf,
            RankOptions::default()
        ),
        &[0, 5][..]
    );
    assert_eq!(
        highlight(
            b"abcdeF",
            &[b"a", b"F"],
            &mut matches_buf,
            RankOptions::default()
        ),
        &[0, 5][..]
    );
    assert_eq!(
        highlight(
            b"a/path/to/file",
            &[b"path", b"file"],
            &mut matches_buf,
            RankOptions::default()
        ),
        &[2, 3, 4, 5, 10, 11, 12, 13][..]
    );
    assert_eq!(
        highlight(
            b"lib/ziglyph/zig.mod",
            &[b"ziglyph"],
            &mut matches_buf,
            RankOptions::default()
        ),
        &[4, 5, 6, 7, 8, 9, 10][..]
    );

    assert_eq!(
        highlight_needle(
            b"abcdef",
            b"a",
            &mut matches_buf,
            RankNeedleOptions::default()
        ),
        &[0][..]
    );
    assert_eq!(
        highlight_needle(
            b"abcdeF",
            b"F",
            &mut matches_buf,
            RankNeedleOptions::default()
        ),
        &[5][..]
    );
    assert_eq!(
        highlight_needle(
            b"a/path/to/file",
            b"file",
            &mut matches_buf,
            RankNeedleOptions {
                filename: Some(b"file"),
                ..Default::default()
            }
        ),
        &[10, 11, 12, 13][..]
    );

    // highlights with basename trailing slashes
    assert_eq!(
        highlight_needle(
            b"s/",
            b"s",
            &mut matches_buf,
            RankNeedleOptions {
                filename: Some(b"s"),
                ..Default::default()
            }
        ),
        &[0][..]
    );
    assert_eq!(
        highlight_needle(
            b"/this/is/path/not/a/file/",
            b"file",
            &mut matches_buf,
            RankNeedleOptions {
                filename: Some(b"file"),
                ..Default::default()
            }
        ),
        &[20, 21, 22, 23][..]
    );

    // disconnected highlights
    assert_eq!(
        highlight(
            b"ababab",
            &[b"aab"],
            &mut matches_buf,
            RankOptions::default()
        ),
        &[0, 2, 3][..]
    );
    assert_eq!(
        highlight(
            b"abbbbbabab",
            &[b"aab"],
            &mut matches_buf,
            RankOptions::default()
        ),
        &[6, 8, 9][..]
    );
    assert_eq!(
        highlight(
            b"abcdefg",
            &[b"acg"],
            &mut matches_buf,
            RankOptions::default()
        ),
        &[0, 2, 6][..]
    );
    assert_eq!(
        highlight(
            b"__init__.py",
            &[b"initpy"],
            &mut matches_buf,
            RankOptions::default()
        ),
        &[2, 3, 4, 5, 9, 10][..]
    );

    // small buffer to ensure highlighting doesn't go out of range when the needles overflow
    let mut small_buf: [usize; 4] = [0; 4];
    assert_eq!(
        highlight(
            b"abcd",
            &[b"ab", b"cd", b"abcd"],
            &mut small_buf,
            RankOptions::default()
        ),
        &[0, 1, 2, 3][..]
    );
    assert_eq!(
        highlight(
            b"wxyz",
            &[b"wxy", b"xyz"],
            &mut small_buf,
            RankOptions::default()
        ),
        &[0, 1, 2][..]
    );

    // zero length haystacks and needles
    assert_eq!(
        highlight(b"", &[b"a"], &mut matches_buf, RankOptions::default()),
        &[] as &[usize]
    );
    assert_eq!(
        highlight_needle(b"", b"a", &mut matches_buf, RankNeedleOptions::default()),
        &[] as &[usize]
    );
    assert_eq!(
        highlight(b"a", &[b""], &mut matches_buf, RankOptions::default()),
        &[] as &[usize]
    );
    assert_eq!(
        highlight_needle(b"a", b"", &mut matches_buf, RankNeedleOptions::default()),
        &[] as &[usize]
    );
}
