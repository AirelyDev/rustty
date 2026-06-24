//! rustty-vt string_map
//!
//! A string along with the mapping of each individual byte in the string
//! to the point in the screen.
//!
//! PORTS: ghostty src/terminal/StringMap.zig
//!
//! PORT STATUS: complete (vt-11g). `StringMap` + `deinit` always; the oniguruma
//! search surface (`SearchIterator`/`Match`/`searchIterator`/`oni::*`) plus its
//! 3 tests are gated behind `#[cfg(feature = "oniguruma")]`, off by default.
//! The `oni` binding crate does not exist yet, so the gated code is NOT compiled
//! in the default build, faithful to the Zig `SkipZigTest`.
// PORT NOTE: `search_iterator`/`shallow_copy` and the gated test module are only
// reachable under `#[cfg(feature = "oniguruma")]`; `should_implement_trait`
// suppresses the `SearchIterator::next`/`Iterator` advisory (the reference is a
// fallible `next` returning `Result<Option<Match>>`, not `std::iter::Iterator`).
#![cfg_attr(feature = "oniguruma", allow(clippy::should_implement_trait))]

// const std = @import("std");
// const build_options = @import("terminal_options");
// const oni = @import("oniguruma");
// const point = @import("point.zig");
// const Selection = @import("Selection.zig");
// const Screen = @import("Screen.zig");
// const Pin = @import("PageList.zig").Pin;
// const Allocator = std.mem.Allocator;
use crate::pagelist::Pin;

// PORT NOTE: the reference allocator maps to `allocator_api2::alloc::Global`
// (the crate's allocator seam).
use allocator_api2::alloc::Global as Allocator;

// Retry budget for StringMap regex searches.
//
// Units are Oniguruma retry steps (internal backtracking/retry counter),
// not bytes/characters/time.
#[cfg(feature = "oniguruma")]
const ONI_SEARCH_RETRY_LIMIT: usize = 100_000;

/// A string along with the mapping of each individual byte in the string
/// to the point in the screen.
pub struct StringMap {
    // PORT NOTE: `string: [:0]const u8` -> `Vec<u8>` carrying the NUL-terminated
    // invariant (never a lossy `String`).
    pub string: Vec<u8>,
    // PORT NOTE: `map: []Pin` -> `Vec<Pin>`.
    pub map: Vec<Pin>,
}

impl StringMap {
    pub fn deinit(self, alloc: Allocator) {
        // alloc.free(self.string);
        // alloc.free(self.map);
        // PORT NOTE: the reference frees the two `alloc`-owned slices
        // (`string`, `map`). In Rust both are owning `Vec`s, so taking `self`
        // by value and letting them drop here is the faithful equivalent; the
        // `alloc` parameter is preserved for signature parity but the `Global`
        // seam is the implicit `Vec` allocator.
        let _ = alloc;
        drop(self.string);
        drop(self.map);
    }

    /// Returns an iterator that yields the next match of the given regex.
    /// Requires Ghostty to be compiled with regex support.
    // PORT NOTE: the reference `searchIterator` is
    //   `if (build_options.oniguruma) searchIteratorOni else void`.
    // The oniguruma surface is gated behind `#[cfg(feature = "oniguruma")]`; the
    // `oni::*` binding crate is wired in vt-11 (off by default).
    #[cfg(feature = "oniguruma")]
    pub fn search_iterator(self, regex: oni::Regex) -> SearchIterator {
        SearchIterator {
            map: self,
            regex,
            offset: 0,
        }
    }

    // PORT NOTE: faithful mirror of Zig's by-value copy of the `StringMap`
    // value type for the `oni` search surface (`SearchIterator.next` copies
    // `self.map` into each `Match`). In Zig this is a shallow copy of the
    // `string`/`map` slice headers — a non-owning view whose backing data is
    // only freed by `StringMap.deinit`. The faithful non-owning mirror lands
    // with the `oni` binding crate; this whole surface is gated off by default
    // (the binding crate does not exist yet) and is not compiled in the default
    // build, so the precise ownership model is finalized when the crate arrives.
    #[cfg(feature = "oniguruma")]
    fn shallow_copy(&self) -> StringMap {
        StringMap {
            string: self.string.clone(),
            map: self.map.clone(),
        }
    }
}

/// Iterates over the regular expression matches of the string.
#[cfg(feature = "oniguruma")]
pub struct SearchIterator {
    pub map: StringMap,
    pub regex: oni::Regex,
    pub offset: usize,
}

#[cfg(feature = "oniguruma")]
impl SearchIterator {
    /// Returns the next regular expression match or null if there are
    /// no more matches.
    pub fn next(&mut self) -> Result<Option<Match>, oni::Error> {
        if self.offset >= self.map.string.len() {
            return Ok(None);
        }

        // Use per-search match params so we can bound regex retry steps
        // (Oniguruma's internal backtracking work counter).
        let mut match_param = oni::MatchParam::init()?;
        // defer match_param.deinit();
        match_param.set_retry_limit_in_search(ONI_SEARCH_RETRY_LIMIT)?;

        let region = match self.regex.search_with_param(
            &self.map.string[self.offset..],
            oni::SearchOptions::default(),
            &mut match_param,
        ) {
            Ok(region) => region,
            // Retry/stack-limit errors mean we hit our work budget and
            // aborted matching.
            // For iterator callers this is equivalent to "no further matches".
            Err(
                oni::Error::Mismatch
                | oni::Error::RetryLimitInMatchOver
                | oni::Error::RetryLimitInSearchOver
                | oni::Error::MatchStackLimitOver
                | oni::Error::SubexpCallLimitInSearchOver,
            ) => {
                self.offset = self.map.string.len();
                match_param.deinit();
                return Ok(None);
            }

            Err(err) => {
                match_param.deinit();
                return Err(err);
            }
        };
        // errdefer region.deinit();
        match_param.deinit();

        // Increment our offset by the number of bytes in the match.
        // We defer this so that we can return the match before
        // modifying the offset.
        let end_idx: usize = region.ends()[0] as usize;

        // PORT NOTE: the reference `Match.map` is a by-value copy of the
        // `StringMap` value type, which in Zig is a *shallow* copy of the two
        // slice headers (`string`, `map`) — `Match` does NOT own the backing
        // data (only `StringMap.deinit` frees it). The faithful Rust mirror of
        // this non-owning header copy lands with the `oni` binding crate; this
        // whole `#[cfg(feature = "oniguruma")]` surface is off by default (the
        // binding crate does not exist yet) and is not compiled in the default
        // build.
        let result = Match {
            map: self.map.shallow_copy(),
            offset: self.offset,
            region,
        };
        self.offset += end_idx;
        Ok(Some(result))
    }
}

/// A single regular expression match.
#[cfg(feature = "oniguruma")]
pub struct Match {
    pub map: StringMap,
    pub offset: usize,
    pub region: oni::Region,
}

#[cfg(feature = "oniguruma")]
impl Match {
    pub fn deinit(&mut self) {
        self.region.deinit();
    }

    /// Returns the selection containing the full match.
    pub fn selection(self) -> crate::selection::Selection {
        let start_idx: usize = self.region.starts()[0] as usize;
        let end_idx: usize = (self.region.ends()[0] - 1) as usize;
        let start_pt = self.map.map[self.offset + start_idx];
        let end_pt = self.map.map[self.offset + end_idx];
        crate::selection::Selection::init(start_pt, end_pt, false)
    }
}

// PORT NOTE: the reference's three `test "StringMap searchIterator …"` blocks
// each open with `if (comptime !build_options.oniguruma) return error.SkipZigTest;`
// — they are no-ops unless Ghostty is compiled with regex support. The faithful
// mirror is to gate the whole test module behind `#[cfg(feature = "oniguruma")]`
// (off by default); with the feature off the tests are not compiled, exactly as
// the Zig `SkipZigTest` returns before touching `oni`. The `oni` binding crate
// does not exist yet, so this surface lands compiled only once it does.
#[cfg(all(test, feature = "oniguruma"))]
mod tests {
    use super::*;
    use crate::point;
    use crate::screen::{Options, Screen, SelectLine, SelectionString};
    use allocator_api2::alloc::Global;

    // test "StringMap searchIterator"
    #[test]
    fn string_map_search_iterator() {
        let alloc = Global;

        // Initialize our regex
        oni::testing::ensure_init().unwrap();
        let mut re = oni::Regex::init(
            b"[A-B]{2}",
            oni::RegexOptions::default(),
            oni::Encoding::utf8(),
            oni::Syntax::default(),
            None,
        )
        .unwrap();

        // Initialize our screen
        let mut s = Screen::init(
            alloc,
            Options {
                cols: 5,
                rows: 5,
                max_scrollback: 0,
                kitty_image_storage_limit: 0,
                kitty_image_loading_limits: crate::kitty::graphics_image::Limits::DIRECT,
            },
        )
        .unwrap();
        let str_ = b"1ABCD2EFGH\n3IJKL";
        s.test_write_string(str_).unwrap();
        let line = s
            .select_line(SelectLine {
                pin: s
                    .pages
                    .pin(point::Point::Active(point::Coordinate { x: 2, y: 1 }))
                    .unwrap(),
                whitespace: None,
                semantic_prompt_boundary: false,
            })
            .unwrap();
        let mut map: StringMap = StringMap {
            string: Vec::new(),
            map: Vec::new(),
        };
        let sel_str = s
            .selection_string(
                alloc,
                SelectionString {
                    sel: line,
                    trim: false,
                    map: Some(&mut map),
                },
            )
            .unwrap();
        drop(sel_str);

        // Get our iterator
        let mut it = map.search_iterator(re);
        {
            let mut match_ = it.next().unwrap().unwrap();

            let sel = match_.selection();
            assert_eq!(
                point::Point::Screen(point::Coordinate { x: 1, y: 0 }),
                s.pages
                    .point_from_pin(point::Tag::Screen, sel.start())
                    .unwrap(),
            );
            assert_eq!(
                point::Point::Screen(point::Coordinate { x: 2, y: 0 }),
                s.pages
                    .point_from_pin(point::Tag::Screen, sel.end())
                    .unwrap(),
            );
            match_.deinit();
        }

        assert!(it.next().unwrap().is_none());

        // map.deinit consumes the map; the regex outlives the iterator.
        it.map.deinit(alloc);
        re.deinit();
        s.deinit();
    }

    // test "StringMap searchIterator URL detection"
    #[test]
    fn string_map_search_iterator_url_detection() {
        let alloc = Global;
        use crate::config::url;

        // Initialize URL regex
        oni::testing::ensure_init().unwrap();
        let mut re = oni::Regex::init(
            url::REGEX,
            oni::RegexOptions::default(),
            oni::Encoding::utf8(),
            oni::Syntax::default(),
            None,
        )
        .unwrap();

        // Initialize our screen with text containing a URL
        let mut s = Screen::init(
            alloc,
            Options {
                cols: 40,
                rows: 5,
                max_scrollback: 0,
                kitty_image_storage_limit: 0,
                kitty_image_loading_limits: crate::kitty::graphics_image::Limits::DIRECT,
            },
        )
        .unwrap();
        s.test_write_string(b"hello https://example.com/path world")
            .unwrap();

        // Get the line
        let line = s
            .select_line(SelectLine {
                pin: s
                    .pages
                    .pin(point::Point::Active(point::Coordinate { x: 10, y: 0 }))
                    .unwrap(),
                whitespace: None,
                semantic_prompt_boundary: false,
            })
            .unwrap();
        let mut map: StringMap = StringMap {
            string: Vec::new(),
            map: Vec::new(),
        };
        let sel_str = s
            .selection_string(
                alloc,
                SelectionString {
                    sel: line,
                    trim: false,
                    map: Some(&mut map),
                },
            )
            .unwrap();
        drop(sel_str);

        // Search for URL match
        let mut it = map.search_iterator(re);
        {
            let mut match_ = it.next().unwrap().unwrap();

            let sel = match_.selection();
            // URL should start at x=6 ("https://example.com/path" starts after "hello ")
            assert_eq!(
                point::Point::Screen(point::Coordinate { x: 6, y: 0 }),
                s.pages
                    .point_from_pin(point::Tag::Screen, sel.start())
                    .unwrap(),
            );
            // URL should end at x=29 (end of "/path")
            assert_eq!(
                point::Point::Screen(point::Coordinate { x: 29, y: 0 }),
                s.pages
                    .point_from_pin(point::Tag::Screen, sel.end())
                    .unwrap(),
            );
            match_.deinit();
        }

        assert!(it.next().unwrap().is_none());

        it.map.deinit(alloc);
        re.deinit();
        s.deinit();
    }

    // test "StringMap searchIterator URL with click position"
    #[test]
    fn string_map_search_iterator_url_with_click_position() {
        let alloc = Global;
        use crate::config::url;

        // Initialize URL regex
        oni::testing::ensure_init().unwrap();
        let mut re = oni::Regex::init(
            url::REGEX,
            oni::RegexOptions::default(),
            oni::Encoding::utf8(),
            oni::Syntax::default(),
            None,
        )
        .unwrap();

        // Initialize our screen with text containing a URL
        let mut s = Screen::init(
            alloc,
            Options {
                cols: 40,
                rows: 5,
                max_scrollback: 0,
                kitty_image_storage_limit: 0,
                kitty_image_loading_limits: crate::kitty::graphics_image::Limits::DIRECT,
            },
        )
        .unwrap();
        s.test_write_string(b"hello https://example.com world")
            .unwrap();

        // Simulate clicking on "example" (x=14)
        let click_pin = s
            .pages
            .pin(point::Point::Active(point::Coordinate { x: 14, y: 0 }))
            .unwrap();

        // Get the line
        let line = s
            .select_line(SelectLine {
                pin: click_pin,
                whitespace: None,
                semantic_prompt_boundary: false,
            })
            .unwrap();
        let mut map: StringMap = StringMap {
            string: Vec::new(),
            map: Vec::new(),
        };
        let sel_str = s
            .selection_string(
                alloc,
                SelectionString {
                    sel: line,
                    trim: false,
                    map: Some(&mut map),
                },
            )
            .unwrap();
        drop(sel_str);

        // Search for URL match and verify click position is within URL
        let mut it = map.search_iterator(re);
        let mut found_url = false;
        loop {
            let mut match_ = match it.next().unwrap() {
                Some(m) => m,
                None => break,
            };

            let sel = match_.selection();
            if sel.contains(&s, click_pin) {
                found_url = true;
                // Verify URL bounds
                assert_eq!(
                    point::Point::Screen(point::Coordinate { x: 6, y: 0 }),
                    s.pages
                        .point_from_pin(point::Tag::Screen, sel.start())
                        .unwrap(),
                );
                assert_eq!(
                    point::Point::Screen(point::Coordinate { x: 24, y: 0 }),
                    s.pages
                        .point_from_pin(point::Tag::Screen, sel.end())
                        .unwrap(),
                );
                match_.deinit();
                break;
            }
            match_.deinit();
        }
        assert!(found_url);

        it.map.deinit(alloc);
        re.deinit();
        s.deinit();
    }
}
