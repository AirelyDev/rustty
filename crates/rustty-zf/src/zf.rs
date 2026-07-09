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
    /// Converts the string to lowercase while ranking if set to true. Does not convert the tokens to lowercase.
    pub to_lower: bool,

    /// If true, the zf filepath algorithms are disabled (useful for matching arbitrary strings)
    pub plain: bool,
}

impl Default for RankOptions {
    // Zig field defaults: `to_lower = true`, `plain = false` (not derivable —
    // `to_lower` defaults to `true`, so the `.{}` call sites map here).
    fn default() -> Self {
        Self {
            to_lower: true,
            plain: false,
        }
    }
}

/// rank a given string against a slice of tokens
pub fn rank(str: &[u8], tokens: &[&[u8]], opts: RankOptions) -> Option<f64> {
    let filename = if opts.plain {
        None
    } else {
        Some(filter::basename(str))
    };

    // the candidate must contain all of the characters (in order) in each token.
    // each tokens rank is summed. if any token does not match the candidate is ignored
    let mut sum: f64 = 0.0;
    for &token in tokens {
        let strict_path = !opts.plain && filter::has_separator(token);
        if let Some(r) = filter::rank_token(str, filename, token, !opts.to_lower, strict_path) {
            sum += r;
        } else {
            return None;
        }
    }

    // all tokens matched and the best ranks for each tokens are summed
    Some(sum)
}

// Zig field defaults `to_lower = false`, `strict_path = false`, `filename = null`
// — all `Default`-derivable. NOTE `rankToken` passes `!opts.to_lower` as
// `case_sensitive` with default `to_lower = false` ⇒ default case_sensitive =
// TRUE (differs from `rank`'s default `to_lower = true`); both are pinned.
#[derive(Default)]
pub struct RankTokenOptions<'a> {
    /// Converts the string to lowercase while ranking if set to true. Does not convert the token to lowercase.
    pub to_lower: bool,

    /// Set to true when the token has path separators in it
    pub strict_path: bool,

    /// Set to the filename (basename) of the string for filepath matching
    pub filename: Option<&'a [u8]>,
}

/// rank a given string against a single token
pub fn rank_token(str: &[u8], token: &[u8], opts: RankTokenOptions) -> Option<f64> {
    filter::rank_token(str, opts.filename, token, !opts.to_lower, opts.strict_path)
}

#[test]
fn rank_library_interface() {
    assert!(rank(b"abcdefg", &[b"a", b"z"], RankOptions::default()).is_none());
    assert!(rank(b"abcdefg", &[b"a", b"b"], RankOptions::default()).is_some());
    assert!(rank(
        b"abcdefg",
        &[b"a", b"B"],
        RankOptions {
            to_lower: false,
            ..Default::default()
        }
    )
    .is_none());
    assert!(rank(
        b"aBcdefg",
        &[b"a", b"B"],
        RankOptions {
            to_lower: false,
            ..Default::default()
        }
    )
    .is_some());
    assert!(rank(b"a/path/to/file", &[b"zig"], RankOptions::default()).is_none());
    assert!(rank(
        b"a/path/to/file",
        &[b"path", b"file"],
        RankOptions::default()
    )
    .is_some());

    assert!(rank_token(b"abcdefg", b"a", RankTokenOptions::default()).is_some());
    assert!(rank_token(b"abcdefg", b"z", RankTokenOptions::default()).is_none());
    assert!(rank_token(
        b"abcdefG",
        b"G",
        RankTokenOptions {
            to_lower: false,
            ..Default::default()
        }
    )
    .is_some());
    assert!(rank_token(
        b"abcdefg",
        b"A",
        RankTokenOptions {
            to_lower: false,
            ..Default::default()
        }
    )
    .is_none());
    assert!(rank_token(
        b"a/path/to/file",
        b"file",
        RankTokenOptions {
            filename: Some(b"file"),
            ..Default::default()
        }
    )
    .is_some());
    assert!(rank_token(
        b"a/path/to/file",
        b"zig",
        RankTokenOptions {
            filename: Some(b"file"),
            ..Default::default()
        }
    )
    .is_none());

    // zero length strings and tokens
    assert!(rank(b"", &[b"a"], RankOptions::default()).is_none());
    assert!(rank_token(b"", b"a", RankTokenOptions::default()).is_none());
    assert!(rank(b"a", &[b""], RankOptions::default()).is_none());
    assert!(rank_token(b"a", b"", RankTokenOptions::default()).is_none());
}

// Maybe all that needs to be done is to sort the highlight integers? That would probably save some work in implementation
// Or maybe could sort and then make ranges out of the pairs? Return a list of ranges?
// for the Zig api that could be reasonable... but the C api maybe not
// sorting as a minimum for sure

/// compute matching ranges given a string and a slice of tokens
pub fn highlight<'a>(
    str: &[u8],
    tokens: &[&[u8]],
    matches: &'a mut [usize],
    opts: RankOptions,
) -> &'a [usize] {
    let filename = if opts.plain {
        None
    } else {
        Some(filter::basename(str))
    };

    let mut index: usize = 0;
    for &token in tokens {
        let strict_path = !opts.plain && filter::has_separator(token);
        let matched = filter::highlight_token(
            str,
            filename,
            token,
            !opts.to_lower,
            strict_path,
            &mut matches[index..],
        );
        index += matched.len();
    }

    &matches[0..index]
}

/// compute matching ranges given a string and a single token
pub fn highlight_token<'a>(
    str: &[u8],
    token: &[u8],
    matches: &'a mut [usize],
    opts: RankTokenOptions,
) -> &'a [usize] {
    filter::highlight_token(
        str,
        opts.filename,
        token,
        !opts.to_lower,
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
            RankOptions {
                to_lower: false,
                ..Default::default()
            }
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
        highlight_token(
            b"abcdef",
            b"a",
            &mut matches_buf,
            RankTokenOptions::default()
        ),
        &[0][..]
    );
    assert_eq!(
        highlight_token(
            b"abcdeF",
            b"F",
            &mut matches_buf,
            RankTokenOptions {
                to_lower: false,
                ..Default::default()
            }
        ),
        &[5][..]
    );
    assert_eq!(
        highlight_token(
            b"a/path/to/file",
            b"file",
            &mut matches_buf,
            RankTokenOptions {
                filename: Some(b"file"),
                ..Default::default()
            }
        ),
        &[10, 11, 12, 13][..]
    );

    // highlights with basename trailing slashes
    assert_eq!(
        highlight_token(
            b"s/",
            b"s",
            &mut matches_buf,
            RankTokenOptions {
                filename: Some(b"s"),
                ..Default::default()
            }
        ),
        &[0][..]
    );
    assert_eq!(
        highlight_token(
            b"/this/is/path/not/a/file/",
            b"file",
            &mut matches_buf,
            RankTokenOptions {
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

    // small buffer to ensure highlighting doesn't go out of range when the tokens overflow
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
        &[0, 1, 2, 1][..]
    );

    // zero length strings and tokens
    assert_eq!(
        highlight(b"", &[b"a"], &mut matches_buf, RankOptions::default()),
        &[] as &[usize]
    );
    assert_eq!(
        highlight_token(b"", b"a", &mut matches_buf, RankTokenOptions::default()),
        &[] as &[usize]
    );
    assert_eq!(
        highlight(b"a", &[b""], &mut matches_buf, RankOptions::default()),
        &[] as &[usize]
    );
    assert_eq!(
        highlight_token(b"a", b"", &mut matches_buf, RankTokenOptions::default()),
        &[] as &[usize]
    );
}
