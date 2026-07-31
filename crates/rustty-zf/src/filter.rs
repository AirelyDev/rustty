//! filter.zig — the core zf scorer (a Smith-Waterman-ish greedy gap/bonus walk).
//!
//! Ports natecraddock/zf 0.10.3 `src/zf/filter.zig` line-for-line: `indexOf`,
//! `IndexIterator`, `hasSeparator`, `PathIterator`, `segmentLen`, `rankToken`,
//! `FixedArrayList`, `highlightToken`, `isStartOfWord`, `ScanResult`, `scanToEnd`.
//!
//! `std.fs.path.{sep,basenamePosix}` are inlined here (pure leaf ⇒ no std shim).
//!
//! PORT NOTE (deferral re-audit, 2026-07-31): `SEP` used to be hardcoded to
//! `b'/'` with the note "Windows `'\\'` is out of scope". That was a real
//! behavioural divergence on a target the reference supports — upstream zf uses
//! the comptime-native `const sep = std.fs.path.sep` (`zf/filter.zig:4`), which
//! is `'\\'` on Windows, and it feeds `hasSeparator`/`PathIterator`/
//! `isStartOfWord`, so a Windows build scored path segments wrongly. `SEP` is
//! now target-native, matching the reference exactly.

// const std = @import("std");
// const testing = std.testing;

// const sep = std.fs.path.sep;
#[cfg(not(windows))]
pub const SEP: u8 = b'/';
#[cfg(windows)]
pub const SEP: u8 = b'\\';

/// `std.fs.path.basenamePosix` — the trailing path component (no trailing slashes).
/// `basename("a/path/to/file") = "file"`, `basename("s/") = "s"`,
/// `basename("/this/is/path/not/a/file/") = "file"`, `basename("abcdefg") = "abcdefg"`.
pub fn basename(path: &[u8]) -> &[u8] {
    if path.is_empty() {
        return &[];
    }

    // Ignore trailing slashes; an all-slashes path has an empty basename.
    let mut end = path.len() - 1;
    while path[end] == SEP {
        if end == 0 {
            return &[];
        }
        end -= 1;
    }

    // Walk left to the separator (or the start) preceding the component.
    let mut start = end;
    end += 1;
    while path[start] != SEP {
        if start == 0 {
            return &path[0..end];
        }
        start -= 1;
    }

    &path[start + 1..end]
}

// `std.ascii.toLower` — ASCII-only: uppercase A-Z fold to lowercase, all other
// bytes pass through unchanged (bytes, never `char` — non-ASCII paths stay raw).
fn to_lower(byte: u8) -> u8 {
    if byte.is_ascii_uppercase() {
        byte | 0b0010_0000
    } else {
        byte
    }
}

// `indexOf(comptime T, …, comptime case_sensitive)` — only `T = u8` is
// instantiated, so this is monomorphized to `u8` with a runtime `case_sensitive`
// branch (behavior identical to the two comptime instantiations). Note the
// haystack byte is lowered, NEVER the search `value` — this is why
// `RankOptions.to_lower` "does not convert the tokens" (the caller hands a
// lowercase token) and why `case_sensitive = !to_lower`.
fn index_of(slice: &[u8], start_index: usize, value: u8, case_sensitive: bool) -> Option<usize> {
    let mut i: usize = start_index;
    while i < slice.len() {
        if case_sensitive {
            if slice[i] == value {
                return Some(i);
            }
        } else if to_lower(slice[i]) == value {
            return Some(i);
        }
        i += 1;
    }
    None
}

struct IndexIterator<'a> {
    str: &'a [u8],
    char: u8,
    index: usize,
    case_sensitive: bool,
}

impl<'a> IndexIterator<'a> {
    fn init(str: &'a [u8], char: u8, case_sensitive: bool) -> Self {
        Self {
            str,
            char,
            index: 0,
            case_sensitive,
        }
    }

    // Kept as an explicit `next` (not `impl Iterator`) to mirror Zig's
    // `while (it.next()) |x|` control flow 1:1.
    #[allow(clippy::should_implement_trait)]
    fn next(&mut self) -> Option<usize> {
        let index = index_of(self.str, self.index, self.char, self.case_sensitive);
        if let Some(i) = index {
            self.index = i + 1;
        }
        index
    }
}

pub fn has_separator(str: &[u8]) -> bool {
    for &byte in str {
        if byte == SEP {
            return true;
        }
    }
    false
}

struct PathIterator<'a> {
    str: &'a [u8],
    index: usize,
}

impl<'a> PathIterator<'a> {
    fn init(str: &'a [u8]) -> Self {
        Self { str, index: 0 }
    }

    #[allow(clippy::should_implement_trait)]
    fn next(&mut self) -> Option<&'a [u8]> {
        if self.index >= self.str.len() {
            return None;
        }

        let start = self.index;
        if self.str[self.index] == SEP {
            self.index += 1;
            return Some(&self.str[start..self.index]);
        }

        while self.index < self.str.len() {
            if self.str[self.index] == SEP {
                return Some(&self.str[start..self.index]);
            }
            self.index += 1;
        }

        Some(&self.str[start..])
    }
}

#[test]
fn path_iterator() {
    let mut iter = PathIterator::init(b"");
    assert!(iter.next().is_none());

    iter = PathIterator::init(b"/");
    assert_eq!(iter.next().unwrap(), b"/");
    assert!(iter.next().is_none());

    iter = PathIterator::init(b"a");
    assert_eq!(iter.next().unwrap(), b"a");
    assert!(iter.next().is_none());

    iter = PathIterator::init(b"/a");
    assert_eq!(iter.next().unwrap(), b"/");
    assert_eq!(iter.next().unwrap(), b"a");
    assert!(iter.next().is_none());

    iter = PathIterator::init(b"a/");
    assert_eq!(iter.next().unwrap(), b"a");
    assert_eq!(iter.next().unwrap(), b"/");
    assert!(iter.next().is_none());

    iter = PathIterator::init(b"a/b/");
    assert_eq!(iter.next().unwrap(), b"a");
    assert_eq!(iter.next().unwrap(), b"/");
    assert_eq!(iter.next().unwrap(), b"b");
    assert_eq!(iter.next().unwrap(), b"/");
    assert!(iter.next().is_none());

    iter = PathIterator::init(b"src/data/b.zig");
    assert_eq!(iter.next().unwrap(), b"src");
    assert_eq!(iter.next().unwrap(), b"/");
    assert_eq!(iter.next().unwrap(), b"data");
    assert_eq!(iter.next().unwrap(), b"/");
    assert_eq!(iter.next().unwrap(), b"b.zig");
    assert!(iter.next().is_none());
}

/// Scan left and right for the length of the current path segment
pub fn segment_len(str: &[u8], index: usize) -> usize {
    if str[index] == SEP {
        return 1;
    }

    let mut start = index;
    let mut end = index;
    while start > 0 {
        if str[start - 1] == SEP {
            break;
        }
        start -= 1;
    }
    while end < str.len() && str[end] != SEP {
        end += 1;
    }
    end - start
}

#[test]
fn segment_len_test() {
    assert_eq!(segment_len(b"a", 0), 1);
    assert_eq!(segment_len(b"/a", 1), 1);
    assert_eq!(segment_len(b"/a/", 1), 1);
    assert_eq!(segment_len(b"src/main.zig", 0), 3);
    assert_eq!(segment_len(b"src/main.zig", 2), 3);
    assert_eq!(segment_len(b"src/main.zig", 5), 8);
    assert_eq!(segment_len(b"a/b", 1), 1);
}

pub fn rank_token(
    str: &[u8],
    filename_or_null: Option<&[u8]>,
    token: &[u8],
    case_sensitive: bool,
    strict_path: bool,
) -> Option<f64> {
    if str.is_empty() || token.is_empty() {
        return None;
    }

    // iterates over the string performing a match starting at each possible index
    // the best (minimum) overall ranking is kept and returned
    let mut best_rank: Option<f64> = None;

    if strict_path {
        let mut iter = PathIterator::init(token);
        let mut start: usize = 0;
        while let Some(segment) = iter.next() {
            let mut it = IndexIterator::init(str, segment[0], case_sensitive);
            it.index = start;
            let mut matched = false;
            while let Some(start_index) = it.next() {
                if let Some(scan) = scan_to_end(
                    str,
                    &segment[1..],
                    start_index,
                    0,
                    None,
                    case_sensitive,
                    strict_path,
                ) {
                    // how much of the query token segment matched the path segment?
                    // "mod" would match "module" better than "modules" for example
                    let path_segment_len = segment_len(str, start_index);
                    let coverage = 1.0 - (segment.len() as f64 / path_segment_len as f64);
                    let rank = coverage * scan.rank;

                    best_rank = Some(best_rank.map_or(rank, |br| br + rank));

                    start = scan.index;
                    matched = true;
                    break;
                }
            }
            // the `while … else` — if the IndexIterator exhausts without a match
            // for this segment, no candidate matched the whole token.
            if !matched {
                return None;
            }
        }
        return best_rank;
    }

    // perform search on the filename only if requested
    if let Some(filename) = filename_or_null {
        let mut it = IndexIterator::init(filename, token[0], case_sensitive);
        while let Some(start_index) = it.next() {
            if let Some(scan) = scan_to_end(
                filename,
                &token[1..],
                start_index,
                0,
                None,
                case_sensitive,
                false,
            ) {
                if best_rank.is_none_or(|br| scan.rank < br) {
                    best_rank = Some(scan.rank);
                }
            } else {
                break;
            }
        }

        if let Some(mut br) = best_rank {
            // was a filename match, give priority
            br /= 2.0;

            // how much of the token matched the filename?
            if token.len() == filename.len() {
                br /= 2.0;
            } else {
                let coverage = 1.0 - (token.len() as f64 / filename.len() as f64);
                br *= coverage;
            }

            return Some(br);
        }
    }

    // perform search on the full string if requested or if no match was found on the filename
    let mut it = IndexIterator::init(str, token[0], case_sensitive);
    while let Some(start_index) = it.next() {
        if let Some(scan) = scan_to_end(
            str,
            &token[1..],
            start_index,
            0,
            None,
            case_sensitive,
            false,
        ) {
            if best_rank.is_none_or(|br| scan.rank < br) {
                best_rank = Some(scan.rank);
            }
        } else {
            break;
        }
    }

    best_rank
}

#[test]
fn rank_token_test() {
    // plain string matching
    assert_eq!(rank_token(b"", None, b"", false, false), None);
    assert_eq!(rank_token(b"", None, b"b", false, false), None);
    assert_eq!(rank_token(b"a", None, b"", false, false), None);
    assert_eq!(rank_token(b"a", None, b"b", false, false), None);
    assert_eq!(rank_token(b"aaa", None, b"aab", false, false), None);
    assert_eq!(rank_token(b"abbba", None, b"abab", false, false), None);

    assert!(rank_token(b"a", None, b"a", false, false).is_some());
    assert!(rank_token(b"abc", None, b"abc", false, false).is_some());
    assert!(rank_token(b"aaabbbccc", None, b"abc", false, false).is_some());
    assert!(rank_token(b"azbycx", None, b"x", false, false).is_some());
    assert!(rank_token(b"azbycx", None, b"ax", false, false).is_some());
    assert!(rank_token(b"a", None, b"a", false, false).is_some());

    // file name matching
    assert_eq!(rank_token(b"", Some(b""), b"", false, false), None);
    assert_eq!(rank_token(b"/a", Some(b"a"), b"b", false, false), None);
    assert_eq!(rank_token(b"c/a", Some(b"a"), b"b", false, false), None);
    assert_eq!(
        rank_token(b"/file.ext", Some(b"file.ext"), b"z", false, false),
        None
    );
    assert_eq!(
        rank_token(b"/file.ext", Some(b"file.ext"), b"fext.", false, false),
        None
    );
    assert_eq!(rank_token(b"/a/b/c", Some(b"c"), b"d", false, false), None);

    assert!(rank_token(b"/b", Some(b"b"), b"b", false, false).is_some());
    assert!(rank_token(b"/a/b/c", Some(b"c"), b"c", false, false).is_some());
    assert!(rank_token(b"/file.ext", Some(b"file.ext"), b"ext", false, false).is_some());
    assert!(rank_token(
        b"path/to/file.ext",
        Some(b"file.ext"),
        b"file",
        false,
        false
    )
    .is_some());
    assert!(rank_token(b"path/to/file.ext", Some(b"file.ext"), b"to", false, false).is_some());
    assert!(rank_token(
        b"path/to/file.ext",
        Some(b"file.ext"),
        b"path",
        false,
        false
    )
    .is_some());
    assert!(rank_token(
        b"path/to/file.ext",
        Some(b"file.ext"),
        b"pfile",
        false,
        false
    )
    .is_some());
    assert!(rank_token(b"path/to/file.ext", Some(b"file.ext"), b"ptf", false, false).is_some());
    assert!(rank_token(
        b"path/to/file.ext",
        Some(b"file.ext"),
        b"p/t/f",
        false,
        false
    )
    .is_some());

    // strict path matching
    assert_eq!(rank_token(b"a/b", Some(b"b"), b"ab", false, true), None);
    assert_eq!(rank_token(b"a/b/c", Some(b"c"), b"abc", false, true), None);
    assert_eq!(
        rank_token(
            b"app/monsters/dungeon/foo/bar/baz.rb",
            Some(b"baz.rb"),
            b"mod/",
            false,
            true
        ),
        None
    );
    assert_eq!(
        rank_token(
            b"app/models/foo/bar/baz.rb",
            Some(b"baz.rb"),
            b"mod/barbaz",
            false,
            true
        ),
        None
    );
    assert_eq!(
        rank_token(b"/some/path/here", Some(b"here"), b"/somepath", false, true),
        None
    );

    assert!(rank_token(b"a/b/c", Some(b"c"), b"a/c", false, true).is_some());
    assert!(rank_token(b"a/b/c", Some(b"c"), b"//", false, true).is_some());
    assert!(rank_token(
        b"src/config/__init__.py",
        Some(b"__init__.py"),
        b"con/i",
        false,
        true
    )
    .is_some());
    assert!(rank_token(b"a/b/c/d", Some(b"d"), b"a/b/c", false, true).is_some());
    assert!(rank_token(
        b"./app/models/foo/bar/baz.rb",
        Some(b"baz.rb"),
        b"a/m/f/b/baz",
        false,
        true
    )
    .is_some());
    assert!(rank_token(
        b"/app/monsters/dungeon/foo/bar/baz.rb",
        Some(b"baz.rb"),
        b"a/m/f/b/baz",
        false,
        true
    )
    .is_some());
    assert!(rank_token(
        b"app/models/foo/bar/baz.rb",
        Some(b"baz.rb"),
        b"mod/baz.rb",
        false,
        true
    )
    .is_some());
}

/// A simple, append-only array list backed by a fixed buffer. `append` is a
/// SILENT no-op when full (the small-buffer overflow highlights depend on this).
/// Only `usize` is ever instantiated ⇒ no generic needed.
pub struct FixedArrayList<'a> {
    buffer: &'a mut [usize],
    len: usize,
}

impl<'a> FixedArrayList<'a> {
    pub fn init(buffer: &'a mut [usize]) -> Self {
        Self { buffer, len: 0 }
    }

    pub fn append(&mut self, data: usize) {
        if self.len >= self.buffer.len() {
            return;
        }
        self.buffer[self.len] = data;
        self.len += 1;
    }

    pub fn clear(&mut self) {
        self.len = 0;
    }

    pub fn slice(&self) -> &[usize] {
        &self.buffer[0..self.len]
    }
}

#[test]
fn fixed_array_list() {
    let mut buffer: [usize; 4] = [0; 4];
    let mut list = FixedArrayList::init(&mut buffer);

    list.append(1);
    list.append(2);
    list.append(3);
    list.append(4);
    list.append(5);
    assert_eq!(list.slice(), &[1, 2, 3, 4][..]);

    list.clear();
    assert_eq!(list.slice(), &[] as &[usize]);
}

pub fn highlight_token<'a>(
    str: &[u8],
    filename_or_null: Option<&[u8]>,
    token: &[u8],
    case_sensitive: bool,
    strict_path: bool,
    matches: &'a mut [usize],
) -> &'a [usize] {
    if str.is_empty() || token.is_empty() {
        return &[];
    }

    // The `best_matched` list is backed by the caller's `matches`; return the
    // filled prefix once the mutable-borrow phase is done (reborrow as shared).
    let len = highlight_token_len(
        str,
        filename_or_null,
        token,
        case_sensitive,
        strict_path,
        matches,
    );
    &matches[0..len]
}

fn highlight_token_len(
    str: &[u8],
    filename_or_null: Option<&[u8]>,
    token: &[u8],
    case_sensitive: bool,
    strict_path: bool,
    matches: &mut [usize],
) -> usize {
    let mut best_rank: Option<f64> = None;

    // Working memory for computing matches
    let mut buf: [usize; 1024] = [0; 1024];
    let mut matched = FixedArrayList::init(&mut buf);
    let mut best_matched = FixedArrayList::init(matches);

    if strict_path {
        let mut iter = PathIterator::init(token);
        let mut start: usize = 0;
        while let Some(segment) = iter.next() {
            let mut it = IndexIterator::init(str, segment[0], case_sensitive);
            it.index = start;
            let mut found = false;
            while let Some(start_index) = it.next() {
                matched.append(start_index);

                if let Some(scan) = scan_to_end(
                    str,
                    &segment[1..],
                    start_index,
                    0,
                    Some(&mut matched),
                    case_sensitive,
                    strict_path,
                ) {
                    // how much of the query token segment matched the path segment?
                    // "mod" would match "module" better than "modules" for example
                    let path_segment_len = segment_len(str, start_index);
                    let coverage = 1.0 - (segment.len() as f64 / path_segment_len as f64);
                    let rank = coverage * scan.rank;

                    best_rank = Some(best_rank.map_or(rank, |br| br + rank));

                    start = scan.index;
                    for &index in matched.slice() {
                        best_matched.append(index);
                    }
                    found = true;
                    break;
                } else {
                    matched.clear();
                }
            }
            // `while … else return &.{}` — a segment with no match ⇒ no highlights.
            if !found {
                return 0;
            }

            matched.clear();
        }
        return best_matched.len;
    }

    // highlight on the filename if requested
    if let Some(filename) = filename_or_null {
        // The basename doesn't include trailing slashes so if the string ends in a slash the offset will be off by one
        let offset = str.len() - filename.len() - usize::from(str[str.len() - 1] == SEP);

        let mut it = IndexIterator::init(filename, token[0], case_sensitive);
        while let Some(start_index) = it.next() {
            matched.append(start_index + offset);

            if let Some(scan) = scan_to_end(
                filename,
                &token[1..],
                start_index,
                offset,
                Some(&mut matched),
                case_sensitive,
                false,
            ) {
                if best_rank.is_none_or(|br| scan.rank < br) {
                    best_rank = Some(scan.rank);
                    best_matched.clear();
                    for &index in matched.slice() {
                        best_matched.append(index);
                    }
                }
            } else {
                break;
            }
            matched.clear();
        }
        if best_rank.is_some() {
            return best_matched.len;
        }
    }

    matched.clear();

    // highlight the full string if requested or if no match was found on the filename
    let mut it = IndexIterator::init(str, token[0], case_sensitive);
    while let Some(start_index) = it.next() {
        matched.append(start_index);

        if let Some(scan) = scan_to_end(
            str,
            &token[1..],
            start_index,
            0,
            Some(&mut matched),
            case_sensitive,
            false,
        ) {
            if best_rank.is_none_or(|br| scan.rank < br) {
                best_rank = Some(scan.rank);
                best_matched.clear();
                for &index in matched.slice() {
                    best_matched.append(index);
                }
            }
        } else {
            break;
        }
        matched.clear();
    }

    best_matched.len
}

fn is_start_of_word(byte: u8) -> bool {
    matches!(byte, SEP | b'_' | b'-' | b'.' | b' ')
}

struct ScanResult {
    rank: f64,
    index: usize,
}

/// this is the core of the ranking algorithm. special precedence is given to
/// filenames. if a match is found on a filename the candidate is ranked higher
fn scan_to_end(
    str: &[u8],
    token: &[u8],
    start_index: usize,
    offset: usize,
    mut matched_indices: Option<&mut FixedArrayList<'_>>,
    case_sensitive: bool,
    strict_path: bool,
) -> Option<ScanResult> {
    let mut rank: f64 = 1.0;
    let mut last_index = start_index;
    let mut last_sequential = false;

    // penalty for not starting on a word boundary
    if start_index > 0 && !is_start_of_word(str[start_index - 1]) {
        rank += 2.0;
    }

    for &c in token {
        let index = index_of(str, last_index + 1, c, case_sensitive);

        if let Some(idx) = index {
            // did the match span a slash in strict path mode?
            if strict_path && has_separator(&str[last_index..idx + 1]) {
                return None;
            }

            if let Some(mi) = matched_indices.as_mut() {
                mi.append(idx + offset);
            }

            if idx == last_index + 1 {
                // sequential matches only count the first character
                if !last_sequential {
                    last_sequential = true;
                    rank += 1.0;
                }
            } else {
                // penalty for not starting on a word boundary
                if !is_start_of_word(str[idx - 1]) {
                    rank += 2.0;
                }

                // normal match
                last_sequential = false;
                rank += (idx - last_index) as f64;
            }

            last_index = idx;
        } else {
            return None;
        }
    }

    Some(ScanResult {
        rank,
        index: last_index + 1,
    })
}
