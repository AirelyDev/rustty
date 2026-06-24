//! rustty-vt string_map
//!
//! A string along with the mapping of each individual byte in the string
//! to the point in the screen.
//!
//! PORTS: ghostty src/terminal/StringMap.zig
//!
//! PORT STATUS: skeleton (vt-10) — `StringMap` always; the oniguruma search
//! surface (`SearchIterator`/`Match`/`oni::*`) is gated behind
//! `#[cfg(feature = "oniguruma")]` (off by default). Bodies `todo!()` land in
//! vt-11.
// PORT NOTE: this skeleton has `todo!()` method bodies (vt-11 lands the real
// bodies), so the parameters/imports they will consume are not yet read. The
// `unused_*` and `clippy::should_implement_trait` allows are skeleton-only and
// drop out once the bodies land.
#![allow(clippy::todo, dead_code, unused_variables, unused_imports)]
#![allow(clippy::should_implement_trait)]

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
        todo!()
    }

    /// Returns an iterator that yields the next match of the given regex.
    /// Requires Ghostty to be compiled with regex support.
    // PORT NOTE: the reference `searchIterator` is
    //   `if (build_options.oniguruma) searchIteratorOni else void`.
    // The oniguruma surface is gated behind `#[cfg(feature = "oniguruma")]`; the
    // `oni::*` binding crate is wired in vt-11 (off by default).
    #[cfg(feature = "oniguruma")]
    pub fn search_iterator(self, regex: oni::Regex) -> SearchIterator {
        todo!()
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
        todo!()
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
        todo!()
    }

    /// Returns the selection containing the full match.
    pub fn selection(self) -> crate::selection::Selection {
        todo!()
    }
}
