//! rustty-vt search/screen
//!
//! PORTS: ghostty src/terminal/search/screen.zig
//!
//! PORT STATUS: complete (vt-15). Faithful port of `ScreenSearch`, the
//! high-level searcher that caches results, drives the active/history state
//! machine, and keeps a selected match accurate as the `Screen` changes.

// const std = @import("std");
// const assert = @import("../../quirks.zig").inlineAssert;
// const testing = std.testing;
// const tripwire = @import("../../tripwire.zig");
// const Allocator = std.mem.Allocator;
// const point = @import("../point.zig");
// const highlight = @import("../highlight.zig");
// const size = @import("../size.zig");
// const FlattenedHighlight = highlight.Flattened;
// const TrackedHighlight = highlight.Tracked;
// const PageList = @import("../PageList.zig");
// const Pin = PageList.Pin;
// const Screen = @import("../Screen.zig");
// const Terminal = @import("../Terminal.zig");
// const ActiveSearch = @import("active.zig").ActiveSearch;
// const PageListSearch = @import("pagelist.zig").PageListSearch;
// const SlidingWindow = @import("sliding_window.zig").SlidingWindow;
use super::active::ActiveSearch;
use super::pagelist::PageListSearch;
use super::sliding_window::{Direction, SlidingWindow};
use crate::alloc::{AVec, Allocator};
use crate::highlight::Flattened as FlattenedHighlight;
use crate::highlight::FlattenedRef;
use crate::highlight::Tracked as TrackedHighlight;
use crate::highlight::FLATTENED_INIT_CAPACITY;
use crate::pagelist::{Node, OutOfMemory, PageList, Pin};
use crate::point;
use crate::screen::Screen;
use crate::size;

// const log = std.log.scoped(.search_screen);
//
// PORT NOTE: the reference `log.info(...)` calls in `reloadActive` are reached
// only on an internal error path (a failed `select` retry); they go through the
// `log` facade with this scope as the target (`log::info!(target: LOG_SCOPE,
// ...)`), behind the same message string (N14).
const LOG_SCOPE: &str = "search_screen";

// const reloadActive_tw = tripwire.module(enum {
//     history_append_new,
//     history_append_existing,
// }, ScreenSearch.reloadActive);
//
// PORT NOTE: see PageList's tripwire PORT NOTE. The Zig `tripwire.module(P, E)`
// is type-static state; the Rust `rustty_util::tripwire::Module<P, E>` is an
// instance. We hold it in a `#[cfg(test)] thread_local!` and route `check`
// through the small helper below (matching the PageList pattern). The two
// reference tests that arm this module (`errorAlways`/`errorAfter`) access it
// through the same thread-local handle.
#[cfg(test)]
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum ReloadActiveFp {
    HistoryAppendNew,
    HistoryAppendExisting,
}

#[cfg(test)]
thread_local! {
    static RELOAD_ACTIVE_TW: core::cell::RefCell<
        rustty_util::tripwire::Module<ReloadActiveFp, OutOfMemory>,
    > = core::cell::RefCell::new(rustty_util::tripwire::module());
}

// `try tw.check(point)` for the `reloadActive` tripwire module. Non-test builds
// have no failpoints (the call sites are `#[cfg(test)]`-gated), matching the Zig
// `enabled = builtin.is_test` gate.
#[cfg(test)]
#[inline]
fn reload_active_tw_check(point: ReloadActiveFp) -> Result<(), OutOfMemory> {
    RELOAD_ACTIVE_TW.with(|tw| tw.borrow_mut().check(point))
}

/// Searches for a needle within a Screen, handling active area updates,
/// pages being pruned from the screen (e.g. scrollback limits), and more.
///
/// Unlike our lower-level searchers (like ActiveSearch and PageListSearch),
/// this will cache and store all search results so the caller can re-access
/// them as needed. This structure does this because it is intended to help
/// the caller handle the case where the Screen is changing while the user
/// is searching.
///
/// An inactive screen can continue to be searched in the background, and when
/// screen state changes, the renderer/caller can access the existing search
/// results without needing to re-search everything. This prevents a particularly
/// nasty UX where going to alt screen (e.g. neovim) and then back would
/// restart the full scrollback search.
//
// PORT NOTE: `screen` is a `*mut Screen` (reference `*Screen`). The searcher
// both reads it (`screen.pages`) and mutates it (`trackPin`/`untrackPin` via
// tracked highlights); a raw pointer mirrors that shared mutable aliasing.
pub struct ScreenSearch {
    /// The screen being searched.
    pub screen: *mut Screen,

    /// The active area search state
    pub active: ActiveSearch,

    /// The history (scrollback) search state. May be null if there is
    /// no history yet.
    pub history: Option<HistorySearch>,

    /// Current state of the search, a state machine.
    pub state: State,

    /// The currently selected match, if any. As the screen contents
    /// change or get pruned, the screen search will do its best to keep
    /// this accurate.
    // selected: ?SelectedMatch = null,
    pub selected: Option<SelectedMatch>,

    /// The results found so far. These are stored separately because history
    /// is mostly immutable once found, while active area results may
    /// change. This lets us easily reset the active area results for a
    /// re-search scenario.
    //
    // PORT NOTE (parity tail S7-6): `std.ArrayList(FlattenedHighlight)` grown
    // through `self.allocator()` (the search's allocator), bound to it at
    // `init`; each element's chunks were cloned through it too.
    pub history_results: AVec<FlattenedHighlight>,
    pub active_results: AVec<FlattenedHighlight>,

    /// The dimensions of the screen. When this changes we need to
    /// restart the whole search, currently.
    pub rows: size::CellCountInt,
    pub cols: size::CellCountInt,
}

pub struct SelectedMatch {
    /// Index from the end of the match list (0 = most recent match)
    pub idx: usize,

    /// Tracked highlight so we can detect movement.
    pub highlight: TrackedHighlight,
}

impl SelectedMatch {
    // pub fn deinit(self: *SelectedMatch, screen: *Screen) void {
    // SB: `screen` is `*mut Screen` (reference `*Screen`), NOT `&mut Screen` — a
    // whole-`Screen` `&mut` reborrow would invalidate the searcher's live raw
    // aliases (rust-mut-args-strongly-protected). Passes the raw ptr straight
    // through to `TrackedHighlight::deinit`, which reborrows `.pages` narrowly.
    #[allow(clippy::not_unsafe_ptr_arg_deref)] // reference takes `screen: *Screen` (raw)
    pub fn deinit(self, screen: *mut Screen) {
        // self.highlight.deinit(screen);
        self.highlight.deinit(screen);
    }
}

/// History search state.
pub struct HistorySearch {
    /// The actual searcher state.
    pub searcher: PageListSearch,

    /// The pin for the first node that this searcher is searching from.
    /// We use this when the active area changes to find the diff between
    /// the top of the new active area and the previous start point
    /// to determine if we need to search more history.
    pub start_pin: *mut Pin,
}

impl HistorySearch {
    // pub fn deinit(self: *HistorySearch, screen: *Screen) void {
    // SB: `screen` is `*mut Screen` (reference `*Screen`), NOT `&mut Screen`;
    // `.pages` is reborrowed narrowly for the single `untrack_pin` call so no
    // whole-`Screen` retag invalidates the searcher's live raw aliases.
    #[allow(clippy::not_unsafe_ptr_arg_deref)] // reference takes `screen: *Screen` (raw)
    pub fn deinit(&mut self, screen: *mut Screen) {
        // self.searcher.deinit();
        self.searcher.deinit();
        // screen.pages.untrackPin(self.start_pin);
        unsafe { (*screen).pages.untrack_pin(self.start_pin) };
    }
}

/// Search state machine
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum State {
    /// Currently searching the active area
    Active,

    /// Currently searching the history area
    History,

    /// History search is waiting for more data to be fed before
    /// it can progress.
    HistoryFeed,

    /// Search is complete given the current terminal state.
    Complete,
}

impl State {
    // pub fn isComplete(self: State) bool {
    pub fn is_complete(self) -> bool {
        // return switch (self) {
        //     .complete => true,
        //     else => false,
        // };
        matches!(self, State::Complete)
    }

    // pub fn needsFeed(self: State) bool {
    pub fn needs_feed(self) -> bool {
        // return switch (self) {
        //     .history_feed => true,
        //
        //     // Not obvious but complete search states will prune
        //     // stale history results on feed, and pick up pages
        //     // prepended below the searched frontier since the last
        //     // feed (incremental snapshot history restore).
        //     .complete => true,
        //
        //     else => false,
        // };
        match self {
            State::HistoryFeed => true,

            // Not obvious but complete search states will prune
            // stale history results on feed, and pick up pages
            // prepended below the searched frontier since the last
            // feed (incremental snapshot history restore).
            State::Complete => true,

            _ => false,
        }
    }
}

/// The errors `tick` can produce. (Zig: `TickError`.)
// pub const TickError = Allocator.Error || error{
//     FeedRequired,
//     SearchComplete,
// };
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TickError {
    OutOfMemory,
    FeedRequired,
    SearchComplete,
}

impl From<OutOfMemory> for TickError {
    fn from(_: OutOfMemory) -> Self {
        TickError::OutOfMemory
    }
}

/// The to-direction for `select`.
pub enum Select {
    /// Next selection, in reverse order (newest to oldest),
    /// non-wrapping.
    Next,

    /// Prev selection, in forward order (oldest to newest),
    /// non-wrapping.
    Prev,
}

impl ScreenSearch {
    // Initialize a screen search for the given screen and needle.
    // pub fn init(
    //     alloc: Allocator,
    //     screen: *Screen,
    //     needle_unowned: []const u8,
    // ) Allocator.Error!ScreenSearch {
    // PORT NOTE: `screen` is a raw pointer (reference takes `*Screen`); the body
    // dereferences it to read `pages.rows`/`pages.cols`, so the clippy lint is
    // allowed here, matching the convention in `pagelist/pin.rs` etc.
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    pub fn init(
        alloc: Allocator,
        screen: *mut Screen,
        needle_unowned: &[u8],
    ) -> Result<ScreenSearch, OutOfMemory> {
        // var result: ScreenSearch = .{
        //     .screen = screen,
        //     .rows = screen.pages.rows,
        //     .cols = screen.pages.cols,
        //     .active = try .init(alloc, needle_unowned),
        //     .history = null,
        //     .state = .active,
        //     .active_results = .empty,
        //     .history_results = .empty,
        // };
        // errdefer result.deinit();
        let mut result: ScreenSearch = ScreenSearch {
            screen,
            rows: unsafe { (*screen).pages.rows },
            cols: unsafe { (*screen).pages.cols },
            active: ActiveSearch::init(alloc, needle_unowned)?,
            history: None,
            state: State::Active,
            active_results: AVec::new_in(alloc),
            history_results: AVec::new_in(alloc),
            selected: None,
        };

        // Update our initial active area state
        // try result.reloadActive();
        if let Err(e) = result.reload_active() {
            result.deinit();
            return Err(e);
        }

        // return result;
        Ok(result)
    }

    // pub fn deinit(self: *ScreenSearch) void {
    pub fn deinit(&mut self) {
        // self.deinitInternal(true);
        self.deinit_internal(true);
    }

    /// Release owned search state after the underlying Screen has already
    /// been destroyed. Tracked pins belonged to that Screen's PageList pool
    /// and were freed with it, so only independently owned memory is freed.
    ///
    /// PORTS: oracle@main search/screen.zig `deinitScreenInvalid` (73ac36fa5).
    // pub fn deinitScreenInvalid(self: *ScreenSearch) void {
    pub fn deinit_screen_invalid(&mut self) {
        // self.deinitInternal(false);
        self.deinit_internal(false);
    }

    // fn deinitInternal(self: *ScreenSearch, screen_valid: bool) void {
    fn deinit_internal(&mut self, screen_valid: bool) {
        // const alloc = self.allocator();
        let alloc = self.allocator();
        // self.active.deinit();
        self.active.deinit();
        // if (self.history) |*h| {
        //     if (screen_valid) {
        //         h.deinit(self.screen);
        //     } else {
        //         h.searcher.deinitListInvalid();
        //     }
        // }
        if let Some(h) = self.history.as_mut() {
            if screen_valid {
                h.deinit(self.screen);
            } else {
                h.searcher.deinit_list_invalid();
            }
        }
        // if (screen_valid) {
        //     if (self.selected) |*m| m.deinit(self.screen);
        // }
        if screen_valid {
            if let Some(m) = self.selected.take() {
                m.deinit(self.screen);
            }
        }
        // for (self.active_results.items) |*hl| hl.deinit(alloc);
        // self.active_results.deinit(alloc);
        for hl in self.active_results.iter_mut() {
            hl.deinit(alloc);
        }
        self.active_results = AVec::new_in(alloc);
        // for (self.history_results.items) |*hl| hl.deinit(alloc);
        // self.history_results.deinit(alloc);
        for hl in self.history_results.iter_mut() {
            hl.deinit(alloc);
        }
        self.history_results = AVec::new_in(alloc);
    }

    /// Rust-only test accounting (parity tail S7-6): the active and history
    /// windows, both result lists and every stored result's chunks. The
    /// history start pin and the selected match's pins are the TERMINAL's
    /// (N262) and are not counted.
    #[cfg(any(test, feature = "test-allocators"))]
    pub fn owned_bytes(&self) -> usize {
        use super::cap_bytes;
        let results: usize = self
            .active_results
            .iter()
            .chain(self.history_results.iter())
            .map(|hl| cap_bytes(&hl.chunks))
            .sum();
        self.active.owned_bytes()
            + self
                .history
                .as_ref()
                .map_or(0, |h| h.searcher.owned_bytes())
            + cap_bytes(&self.active_results)
            + cap_bytes(&self.history_results)
            + results
    }

    // fn allocator(self: *ScreenSearch) Allocator {
    fn allocator(&self) -> Allocator {
        // return self.active.window.alloc;
        self.active.window.alloc
    }

    /// The needle that this search is using.
    // pub fn needle(self: *const ScreenSearch) []const u8 {
    pub fn needle(&self) -> &[u8] {
        // assert(self.active.window.direction == .forward);
        assert!(self.active.window.direction == Direction::Forward);
        // return self.active.window.needle;
        &self.active.window.needle
    }

    /// Returns the total number of matches found so far.
    // pub fn matchesLen(self: *const ScreenSearch) usize {
    pub fn matches_len(&self) -> usize {
        // return self.active_results.items.len + self.history_results.items.len;
        self.active_results.len() + self.history_results.len()
    }

    /// Reinitialize cached search state after a screen resize. Reflow can
    /// replace every PageList node, so even flattened highlights that weren't
    /// selected contain invalid node pointers once either dimension changes.
    ///
    /// Returns true when the reset occurred. The replacement search has
    /// already loaded its active area through `init` in that case.
    //
    // PORT NOTE (oracle 627518447, `P3-W1 T5`): extracted VERBATIM from the
    // inline body that used to live at the top of `feed`. The needle borrow of
    // `self` ends when `init` returns (the new search owns its own copy), before
    // `self.deinit()` / `*self = new`, so no port-only copy of the needle is
    // made (parity tail S7-6). `self.screen` is a raw `*mut Screen` alias
    // throughout (memory: rust-mut-args-strongly-protected) and is never
    // re-borrowed as `&mut Screen` here.
    // fn resetIfDimensionsChanged(self: *ScreenSearch) Allocator.Error!bool {
    fn reset_if_dimensions_changed(&mut self) -> Result<bool, OutOfMemory> {
        // if (self.screen.pages.rows == self.rows and
        //     self.screen.pages.cols == self.cols)
        // {
        //     return false;
        // }
        if unsafe { (*self.screen).pages.rows } == self.rows
            && unsafe { (*self.screen).pages.cols } == self.cols
        {
            return Ok(false);
        }

        // const new: ScreenSearch = try .init(
        //     self.allocator(),
        //     self.screen,
        //     self.needle(),
        // );
        let new: ScreenSearch = ScreenSearch::init(self.allocator(), self.screen, self.needle())?;

        self.deinit();
        *self = new;

        // New result should have matching dimensions
        assert!(unsafe { (*self.screen).pages.rows } == self.rows);
        assert!(unsafe { (*self.screen).pages.cols } == self.cols);
        Ok(true)
    }

    /// Returns all matches as an owned slice (caller must free).
    /// The matches are ordered from most recent to oldest (e.g. bottom
    /// of the screen to top of the screen).
    // pub fn matches(
    //     self: *ScreenSearch,
    //     alloc: Allocator,
    // ) Allocator.Error![]FlattenedHighlight {
    //
    // PORT NOTE (parity tail S7-6): the reference's elements are shallow copies
    // that alias this search's stored chunks; here they are `FlattenedRef`s
    // borrowing them, so the one allocation (the slice, through `alloc`) is
    // the reference's and the result cannot outlive the search's results.
    pub fn matches(&self, alloc: Allocator) -> Result<AVec<FlattenedRef<'_>>, OutOfMemory> {
        // const active_results = self.active_results.items;
        // const history_results = self.history_results.items;
        // const results = try alloc.alloc(
        //     FlattenedHighlight,
        //     active_results.len + history_results.len,
        // );
        // errdefer alloc.free(results);
        let active_results = &self.active_results;
        let history_results = &self.history_results;
        let mut results: AVec<FlattenedRef<'_>> = AVec::new_in(alloc);
        crate::zig_list::ensure_total_capacity_precise(
            &mut results,
            active_results.len() + history_results.len(),
        )?;

        // Active does a forward search, so we add the active results then
        // reverse them. There are usually not many active results so this
        // is fast enough compared to adding them in reverse order.
        // assert(self.active.window.direction == .forward);
        assert!(self.active.window.direction == Direction::Forward);
        // @memcpy(results[0..active_results.len], active_results);
        // std.mem.reverse(FlattenedHighlight, results[0..active_results.len]);
        for hl in active_results.iter() {
            results.push(hl.as_ref());
        }
        results.reverse();

        // History does a backward search, so we can just append them
        // after.
        // @memcpy(results[active_results.len..], history_results);
        for hl in history_results.iter() {
            results.push(hl.as_ref());
        }

        // return results;
        Ok(results)
    }

    /// Search the full screen state. This will block until the search
    /// is complete. For performance, it is recommended to use `tick` and
    /// `feed` to incrementally make progress on the search instead.
    // pub fn searchAll(self: *ScreenSearch) Allocator.Error!void {
    pub fn search_all(&mut self) -> Result<(), OutOfMemory> {
        // while (true) {
        loop {
            // self.tick() catch |err| switch (err) {
            //     error.OutOfMemory => return error.OutOfMemory,
            //     error.FeedRequired => try self.feed(),
            //     error.SearchComplete => return,
            // };
            match self.tick() {
                Ok(()) => {}
                Err(TickError::OutOfMemory) => return Err(OutOfMemory),
                Err(TickError::FeedRequired) => self.feed()?,
                Err(TickError::SearchComplete) => return Ok(()),
            }
        }
    }

    /// Make incremental progress on the search without accessing any
    /// screen state (so no lock is required).
    ///
    /// This will return error.FeedRequired if the search cannot make progress
    /// without being fed more data. In this case, the caller should call
    /// the `feed` function to provide more data to the searcher.
    ///
    /// This will return error.SearchComplete if the search is fully complete.
    /// This is to signal to the caller that it can move to a more efficient
    /// sleep/wait state until there is more work to do (e.g. new data to feed).
    // pub fn tick(self: *ScreenSearch) TickError!void {
    pub fn tick(&mut self) -> Result<(), TickError> {
        // switch (self.state) {
        //     .active => try self.tickActive(),
        //     .history => try self.tickHistory(),
        //     .history_feed => return error.FeedRequired,
        //     .complete => return error.SearchComplete,
        // }
        match self.state {
            State::Active => {
                self.tick_active()?;
                Ok(())
            }
            State::History => {
                self.tick_history()?;
                Ok(())
            }
            State::HistoryFeed => Err(TickError::FeedRequired),
            State::Complete => Err(TickError::SearchComplete),
        }
    }

    /// Feed more data to the searcher so it can continue searching. This
    /// accesses the screen state, so the caller must hold the necessary locks.
    ///
    /// Feed on a complete screen search will perform some cleanup of
    /// potentially stale history results (pruned) and reclaim some memory.
    ///
    /// If pages were prepended to the PageList since the search completed
    /// (incremental snapshot history restore), the feed picks them up and
    /// the search resumes, since "complete" only ever means caught up with
    /// the PageList as of the last feed.
    // pub fn feed(self: *ScreenSearch) Allocator.Error!void {
    pub fn feed(&mut self) -> Result<(), OutOfMemory> {
        // Resize/reflow invalidates every flattened result, not just search
        // state that needs another history feed.
        // _ = try self.resetIfDimensionsChanged();
        let _ = self.reset_if_dimensions_changed()?;

        // const history: *PageListSearch = if (self.history) |*h| &h.searcher else {
        //     // No history to feed, search is complete.
        //     self.state = .complete;
        //     return;
        // };
        if self.history.is_none() {
            // No history to feed, search is complete.
            self.state = State::Complete;
            return Ok(());
        }

        // Future: we may want to feed multiple pages at once here to
        // lower the frequency of lock acquisitions.
        // if (!try history.feed()) {
        let fed = self.history.as_mut().unwrap().searcher.feed()?;
        if !fed {
            // No more data to feed, search is complete.
            // self.state = .complete;
            self.state = State::Complete;

            // We use this opportunity to also clean up older history
            // results that may be gone due to scrollback pruning, though.
            // self.pruneHistory();
            self.prune_history();

            // return;
            return Ok(());
        }

        // Depending on our state handle where feed goes
        // switch (self.state) {
        match self.state {
            // If we're searching active or history, then feeding doesn't
            // change the state.
            // .active, .history => {},
            State::Active | State::History => {}

            // Feed goes back to searching history.
            // .history_feed => self.state = .history,
            State::HistoryFeed => self.state = State::History,

            // A complete search had exhausted the PageList as of its last
            // feed, but pages can still be prepended below that afterward
            // (incremental snapshot history restore). The history searcher's
            // tracked pin keeps its node identity through a prepend, so it
            // just discovered those older pages and loaded them. Resume
            // searching history.
            // .complete => self.state = .history,
            State::Complete => self.state = State::History,
        }

        Ok(())
    }

    // fn pruneHistory(self: *ScreenSearch) void {
    // fn historyResultIsValid(
    //     self: *const ScreenSearch,
    //     hl: *const FlattenedHighlight,
    // ) bool {
    //
    // INSPECT, NOT RESTORE: `node_is_valid` only walks the live node list and
    // compares each node's `serial` stamp (a pagelist Node field, never in the
    // page mapping), so validating cached history must not decompress anything.
    fn history_result_is_valid(&self, hl: &FlattenedHighlight) -> bool {
        // const chunks = hl.chunks.slice();
        // const nodes = chunks.items(.node);
        // const serials = chunks.items(.serial);
        // for (nodes, serials) |node, serial| {
        //     if (!self.screen.pages.nodeIsValid(node, serial)) return false;
        // }
        // return true;
        hl.chunks
            .iter()
            .all(|c| unsafe { (*self.screen).pages.node_is_valid(c.node, c.serial) })
    }

    /// Clear a selected history match if its flattened page coordinates are
    /// stale. reloadActive only inspects the selected history result, so this
    /// avoids validating every cached result on each active-area refresh.
    // fn validateSelectedHistory(self: *ScreenSearch) void {
    fn validate_selected_history(&mut self) {
        // const m = if (self.selected) |*m| m else return;
        let m = match self.selected.as_ref() {
            Some(m) => m,
            None => return,
        };
        // const active_len = self.active_results.items.len;
        let active_len = self.active_results.len();
        // if (m.idx < active_len) return;
        if m.idx < active_len {
            return;
        }

        // const history_idx = m.idx - active_len;
        let history_idx = m.idx - active_len;
        // if (history_idx < self.history_results.items.len and
        //     self.historyResultIsValid(&self.history_results.items[history_idx]))
        // {
        //     return;
        // }
        if history_idx < self.history_results.len()
            && self.history_result_is_valid(&self.history_results[history_idx])
        {
            return;
        }

        // m.deinit(self.screen);
        // self.selected = null;
        self.selected.take().unwrap().deinit(self.screen);
    }

    // PERF NOTE (WS7 Phase C review, M3): `select` calls this on every
    // navigation, and `node_is_valid` (called per surviving chunk below, via
    // `history_result_is_valid`/the epoch check) walks the whole live page
    // list to confirm a node pointer is still on it. Worst case this makes
    // `prune_history` O(results * chunks_per_result * live_pages) per
    // `select` — real cost on a very deep scrollback with many cached
    // results. This is ORACLE-IDENTICAL (`PageList.zig`'s `nodeIsValid` walks
    // the same way) — parity, not a rustty-introduced regression — so it is
    // documented here rather than changed. See also `.port/phase-2/PERF_LOG.md`
    // (WS7 Phase E perf section).
    fn prune_history(&mut self) {
        // Go through our history results in order (newest to oldest) to find
        // any result that contains an invalid serial.
        // var i: usize = 0;
        // while (i < self.history_results.items.len) {
        let mut i: usize = 0;
        while i < self.history_results.len() {
            // const hl = &self.history_results.items[i];
            let hl = &self.history_results[i];
            // const serials = hl.chunks.items(.serial);
            // const first_serial = serials[0];
            let first_serial = hl.chunks[0].serial;
            // if (first_serial < self.screen.pages.page_serial_epoch) {
            if first_serial < unsafe { (*self.screen).pages.page_serial_epoch } {
                // Only a whole-list reset advances the epoch. Results are
                // newest to oldest, so this result and the entire remaining
                // suffix predate that reset. Drop them without scanning the
                // live page list for every captured chunk.
                // const alloc = self.allocator();
                // if (self.selected) |*m| {
                //     const first_pruned = self.active_results.items.len + i;
                //     if (m.idx >= first_pruned) {
                //         m.deinit(self.screen);
                //         self.selected = null;
                //     }
                // }
                //
                // A tracked selection can survive a partial history erase (its
                // pin moves coherently to the next page) while the result it
                // names is about to be pruned. Drop it (oracle d239f9805).
                //
                // PORT NOTE: after f1a5fab45 (C3) only `reset` advances the
                // epoch, so in the FINAL upstream state this branch is reached
                // only after a whole-list reset. The guard is kept verbatim: a
                // selection naming a pre-reset history result must still be
                // dropped.
                //
                // RECORDED, NOT FIXED (WS7 Phase C review, M4): no rustty test
                // currently drives `select` with a tracked selection that
                // survives a `reset` and then names an about-to-be-pruned
                // history result, so this `if let Some(m) = ...` sub-branch is
                // not exercised by the ported test suite today. It remains
                // reachable in principle at runtime (a live selection across a
                // `Terminal::reset`/full-clear followed by `select`), and it is
                // present verbatim in `oracle@main`, so it is kept rather than
                // removed as dead code.
                let alloc = self.allocator();
                if let Some(m) = self.selected.as_ref() {
                    let first_pruned = self.active_results.len() + i;
                    if m.idx >= first_pruned {
                        self.selected.take().unwrap().deinit(self.screen);
                    }
                }

                // for (self.history_results.items[i..]) |*prune_hl| prune_hl.deinit(alloc);
                // self.history_results.shrinkAndFree(alloc, i);
                // return;
                for prune_hl in self.history_results[i..].iter_mut() {
                    prune_hl.deinit(alloc);
                }
                crate::zig_list::shrink_and_free(&mut self.history_results, i);
                return;
            }

            // Ordinary pruning, layout changes, and replacements do not
            // advance the epoch. Validate their pointer-plus-generation pairs
            // against the live list.
            //
            // if (self.historyResultIsValid(hl)) { i += 1; continue; }
            if self.history_result_is_valid(hl) {
                i += 1;
                continue;
            }

            // Only this result is known to be invalid. Older results may live
            // on unrelated pages and remain usable, so remove it individually.
            // const result_idx = self.active_results.items.len + i;
            let result_idx = self.active_results.len() + i;
            // if (self.selected) |*m| { ... }
            if let Some(m) = self.selected.as_mut() {
                if m.idx == result_idx {
                    self.selected.take().unwrap().deinit(self.screen);
                } else if m.idx > result_idx {
                    m.idx -= 1;
                }
            }

            // var removed = self.history_results.orderedRemove(i);
            // removed.deinit(self.allocator());
            let mut removed = self.history_results.remove(i);
            removed.deinit(self.allocator());
        }
    }

    // fn tickActive(self: *ScreenSearch) Allocator.Error!void {
    fn tick_active(&mut self) -> Result<(), OutOfMemory> {
        // For the active area, we consume the entire search in one go
        // because the active area is generally small.
        // const alloc = self.allocator();
        // while (self.active.next()) |hl| {
        let alloc = self.allocator();
        while let Some(hl) = self.active.next() {
            // If this fails, then we miss a result since `active.next()`
            // moves forward and prunes data. In the future, we may want
            // to have some more robust error handling but the only
            // scenario this would fail is OOM and we're probably in
            // deeper trouble at that point anyways.
            // var hl_cloned = try hl.clone(alloc);
            // errdefer hl_cloned.deinit(alloc);
            // try self.active_results.append(alloc, hl_cloned);
            //
            // PORT NOTE: the errdefer is `hl_cloned`'s drop when the append
            // fails (it frees through `alloc`).
            let hl_cloned = hl.clone(alloc)?;
            crate::zig_list::append(&mut self.active_results, hl_cloned, FLATTENED_INIT_CAPACITY)?;
        }

        // We've consumed the entire active area, move to history.
        // self.state = .history;
        self.state = State::History;
        Ok(())
    }

    // fn tickHistory(self: *ScreenSearch) Allocator.Error!void {
    fn tick_history(&mut self) -> Result<(), OutOfMemory> {
        // const history: *HistorySearch = if (self.history) |*h| h else {
        //     // No history to search, we're done.
        //     self.state = .complete;
        //     return;
        // };
        if self.history.is_none() {
            // No history to search, we're done.
            self.state = State::Complete;
            return Ok(());
        }
        // Try to consume all the loaded matches in one go, because
        // the search is generally fast for loaded data.
        // const alloc = self.allocator();
        // while (history.searcher.next()) |hl| {
        let alloc = self.allocator();
        let history = self.history.as_mut().unwrap();
        while let Some(hl) = history.searcher.next() {
            // Ignore selections that are found within the starting
            // node since those are covered by the active area search.
            // if (hl.chunks.items(.node)[0] == history.start_pin.node) continue;
            if core::ptr::eq(hl.chunks[0].node, unsafe { (*history.start_pin).node }) {
                continue;
            }

            // Same note as tickActive for error handling.
            // var hl_cloned = try hl.clone(alloc);
            // errdefer hl_cloned.deinit(alloc);
            // try self.history_results.append(alloc, hl_cloned);
            let hl_cloned = hl.clone(alloc)?;
            crate::zig_list::append(
                &mut self.history_results,
                hl_cloned,
                FLATTENED_INIT_CAPACITY,
            )?;

            // Since history only appends to our results in reverse order,
            // we don't need to update any selected match state. The index
            // and prior results are unaffected.
        }

        // We need to be fed more data.
        // self.state = .history_feed;
        self.state = State::HistoryFeed;
        Ok(())
    }

    /// Reload the active area because it has changed.
    ///
    /// Since it is very fast, this will also do the full active area
    /// search again, too. This avoids any complexity around the search
    /// state machine.
    ///
    /// The caller must hold the necessary locks to access the screen state.
    // pub fn reloadActive(self: *ScreenSearch) Allocator.Error!void {
    pub fn reload_active(&mut self) -> Result<(), OutOfMemory> {
        // This check must precede all inspection of cached highlights and
        // searchers: column reflow may have freed every node they reference.
        // if (try self.resetIfDimensionsChanged()) return;
        if self.reset_if_dimensions_changed()? {
            return Ok(());
        }

        // reloadActive only inspects the selected history result, so validate
        // that result without scanning every cached match on this hot path.
        // self.validateSelectedHistory();
        self.validate_selected_history();

        // const tw = reloadActive_tw;

        // If our selection pin became garbage it means we scrolled off
        // the end. Clear our selection and on exit of this function,
        // try to select the last match.
        // const select_prev: bool = select_prev: { ... };
        let select_prev: bool = 'select_prev: {
            // const m = if (self.selected) |*m| m else break :select_prev false;
            let m = match self.selected.as_ref() {
                Some(m) => m,
                None => break 'select_prev false,
            };
            // if (!m.highlight.start.garbage and
            //     !m.highlight.end.garbage) break :select_prev false;
            if unsafe { !(*m.highlight.start).garbage && !(*m.highlight.end).garbage } {
                break 'select_prev false;
            }

            // m.deinit(self.screen);
            // self.selected = null;
            // break :select_prev true;
            self.selected.take().unwrap().deinit(self.screen);
            true
        };
        // defer if (select_prev) { ... };
        //
        // PORT NOTE: the `defer` runs at every return. We wrap the body in a
        // closure-free helper by running the deferred `select(.prev)` at the
        // single `Ok(())` exit and on the error path of the function. Because
        // `reloadActive` returns `!void` and the only fallible operations are
        // after this point, we collect the function result and run the deferred
        // step exactly once before returning, mirroring `defer`'s semantics.
        let result = self.reload_active_inner();
        if select_prev {
            // _ = self.select(.prev) catch |err| {
            //     log.info("reload failed to reset search selection err={}", .{err});
            // };
            if let Err(err) = self.select(Select::Prev) {
                log::info!(
                    target: LOG_SCOPE,
                    "reload failed to reset search selection err={:?}",
                    err,
                );
            }
        }
        result
    }

    // The body of `reloadActive` after the `select_prev`/`defer` setup. Split out
    // so the `defer if (select_prev)` in `reload_active` runs at every exit.
    //
    // PORT NOTE: several fallible steps use an immediately-invoked closure to
    // scope `?`-propagation so we can run the reference's `errdefer`/`defer`
    // cleanup (free the partially-built list, `window.deinit()`) on the error
    // path before returning. `redundant_closure_call` is allowed for that
    // deliberate error-scoping pattern.
    #[allow(clippy::redundant_closure_call)]
    fn reload_active_inner(&mut self) -> Result<(), OutOfMemory> {
        // const alloc = self.allocator();
        let alloc = self.allocator();
        // const list: *PageList = &self.screen.pages;
        // SB: raw `addr_of_mut!` (reference `*PageList`), NOT `&mut (*self.screen).pages`
        // — a `&mut` reborrow here would be invalidated by the later `&mut`-free
        // deinit/track calls that reborrow `.pages` narrowly; the raw alias survives.
        let list: *mut PageList = unsafe { core::ptr::addr_of_mut!((*self.screen).pages) };

        // if (try self.active.update(list)) |history_node| history: {
        let update_result = self.active.update(unsafe { &*list })?;
        if let Some(history_node) = update_result {
            'history: {
                // We need to account for any active area growth that would
                // cause new pages to move into our history. If there are new
                // pages then we need to re-search the pages and add it to
                // our history results.

                // If our screen has no scrollback then we have no history.
                // if (self.screen.no_scrollback) {
                //     assert(self.history == null);
                //     break :history;
                // }
                if unsafe { (*self.screen).no_scrollback } {
                    assert!(self.history.is_none());
                    break 'history;
                }

                // const history_: ?*HistorySearch = if (self.history) |*h| state: { ... } else null;
                let history_exists: bool = if self.history.is_some() {
                    // If our start pin became garbage, it means we pruned all
                    // the way up through it, so we have no history anymore.
                    // Reset our history state.
                    // if (h.start_pin.garbage) { ... break :state null; }
                    let start_garbage =
                        unsafe { (*self.history.as_ref().unwrap().start_pin).garbage };
                    if start_garbage {
                        self.history.as_mut().unwrap().deinit(self.screen);
                        self.history = None;
                        for hl in self.history_results.iter_mut() {
                            hl.deinit(alloc);
                        }
                        self.history_results.clear();
                        false
                    } else {
                        // break :state h;
                        true
                    }
                } else {
                    false
                };

                // const history = history_ orelse { ... break :history; };
                if !history_exists {
                    // No history search yet, but we now have history. So let's
                    // initialize.

                    // var search: PageListSearch = try .init(
                    //     alloc,
                    //     self.needle(),
                    //     list,
                    //     history_node,
                    // );
                    // errdefer search.deinit();
                    let mut search: PageListSearch =
                        PageListSearch::init(alloc, self.needle(), list, history_node)?;

                    // const pin = try list.trackPin(.{ .node = history_node });
                    // errdefer list.untrackPin(pin);
                    let pin = match unsafe { (*list).track_pin(node_pin(history_node)) } {
                        Ok(p) => p,
                        Err(e) => {
                            search.deinit();
                            return Err(e);
                        }
                    };

                    // self.history = .{
                    //     .searcher = search,
                    //     .start_pin = pin,
                    // };
                    self.history = Some(HistorySearch {
                        searcher: search,
                        start_pin: pin,
                    });

                    // We don't need to update any history since we had no history
                    // before, so we can break out of the whole conditional.
                    // break :history;
                    break 'history;
                }

                // if (history.start_pin.node == history_node) {
                //     // No change in the starting node, we're done.
                //     break :history;
                // }
                if core::ptr::eq(
                    unsafe { (*self.history.as_ref().unwrap().start_pin).node },
                    history_node,
                ) {
                    // No change in the starting node, we're done.
                    break 'history;
                }

                // Do a forward search from our prior node to this one. We
                // collect all the results into a new list. We ASSUME that
                // reloadActive is being called frequently enough that there isn't
                // a massive amount of history to search here.
                // var window: SlidingWindow = try .init(alloc, .forward, self.needle());
                // defer window.deinit();
                let mut window: SlidingWindow =
                    SlidingWindow::init(alloc, Direction::Forward, self.needle())?;
                // while (true) {
                //     _ = try window.append(history.start_pin.node);
                //     if (history.start_pin.node == history_node) break;
                //     const next = history.start_pin.node.next orelse break;
                //     history.start_pin.node = next;
                // }
                // assert(history.start_pin.node == history_node);
                let window_result: Result<(), OutOfMemory> = (|| {
                    loop {
                        let start_node =
                            unsafe { (*self.history.as_ref().unwrap().start_pin).node };
                        let _ = window.append(start_node)?;
                        if core::ptr::eq(start_node, history_node) {
                            break;
                        }
                        let next = unsafe { (*start_node).next };
                        if next.is_null() {
                            break;
                        }
                        unsafe { (*self.history.as_ref().unwrap().start_pin).node = next };
                    }
                    Ok(())
                })();
                if let Err(e) = window_result {
                    window.deinit();
                    return Err(e);
                }
                assert!(core::ptr::eq(
                    unsafe { (*self.history.as_ref().unwrap().start_pin).node },
                    history_node
                ));

                // var results: std.ArrayList(FlattenedHighlight) = try .initCapacity(
                //     alloc,
                //     self.history_results.items.len,
                // );
                // errdefer { for (results.items) |*hl| hl.deinit(alloc); results.deinit(alloc); }
                let mut results: AVec<FlattenedHighlight> = AVec::new_in(alloc);
                if let Err(e) = crate::zig_list::ensure_total_capacity_precise(
                    &mut results,
                    self.history_results.len(),
                ) {
                    window.deinit();
                    return Err(e.into());
                }

                // while (window.next()) |hl| {
                //     if (hl.chunks.items(.node)[0] == history_node) continue;
                //     var hl_cloned = try hl.clone(alloc);
                //     errdefer hl_cloned.deinit(alloc);
                //     try tw.check(.history_append_new);
                //     try results.append(alloc, hl_cloned);
                // }
                let loop_result: Result<(), OutOfMemory> = (|| {
                    while let Some(hl) = window.next() {
                        if core::ptr::eq(hl.chunks[0].node, history_node) {
                            continue;
                        }
                        let hl_cloned = hl.clone(alloc)?;
                        #[cfg(test)]
                        reload_active_tw_check(ReloadActiveFp::HistoryAppendNew)?;
                        crate::zig_list::append(&mut results, hl_cloned, FLATTENED_INIT_CAPACITY)?;
                    }
                    Ok(())
                })();
                if let Err(e) = loop_result {
                    for hl in results.iter_mut() {
                        hl.deinit(alloc);
                    }
                    window.deinit();
                    return Err(e);
                }

                // If we have no matches then there is nothing to change
                // in our history (fast path)
                // if (results.items.len == 0) break :history;
                if results.is_empty() {
                    window.deinit();
                    break 'history;
                }

                // The number added to our history. Needed for updating
                // our selection if we have one.
                // const added_len = results.items.len;
                let added_len = results.len();

                // Matches! Reverse our list then append all the remaining
                // history items that didn't start on our original node.
                // std.mem.reverse(FlattenedHighlight, results.items);
                results.reverse();
                // try tw.check(.history_append_existing);
                let append_result: Result<(), OutOfMemory> = (|| {
                    #[cfg(test)]
                    reload_active_tw_check(ReloadActiveFp::HistoryAppendExisting)?;
                    // try results.appendSlice(alloc, self.history_results.items);
                    //
                    // PORT NOTE: the reference `appendSlice` shallow-copies the
                    // (non-owning) `Flattened` values; ownership then transfers
                    // from `self.history_results` (which is replaced wholesale by
                    // `results` below). We mirror by growing `results` as
                    // `appendSlice` does (`ensureUnusedCapacity`) and then moving
                    // each owned `Flattened` out of `self.history_results`.
                    crate::zig_list::ensure_unused_capacity(
                        &mut results,
                        self.history_results.len(),
                        FLATTENED_INIT_CAPACITY,
                    )?;
                    for hl in self.history_results.drain(..) {
                        results.push(hl);
                    }
                    Ok(())
                })();
                if let Err(e) = append_result {
                    for hl in results.iter_mut() {
                        hl.deinit(alloc);
                    }
                    window.deinit();
                    return Err(e);
                }
                // self.history_results.deinit(alloc);
                // self.history_results = results;
                //
                // PORT NOTE: `self.history_results` was drained above (all owned
                // values moved into `results`); assigning replaces the now-empty
                // list (freeing its storage through `alloc`, the deinit) with the
                // rebuilt one.
                self.history_results = results;

                window.deinit();

                // If our prior selection was in the history area, update
                // the offset.
                // if (self.selected) |*m| selected: {
                if let Some(m) = self.selected.as_mut() {
                    'selected: {
                        // const active_len = self.active_results.items.len;
                        // if (m.idx < active_len) break :selected;
                        let active_len = self.active_results.len();
                        if m.idx < active_len {
                            break 'selected;
                        }
                        // m.idx += added_len;
                        m.idx += added_len;

                        // Moving the idx should not change our targeted result
                        // since the history is immutable.
                        // if (comptime std.debug.runtime_safety) {
                        //     const hl = self.history_results.items[m.idx - active_len];
                        //     assert(m.highlight.start.eql(hl.startPin()));
                        // }
                        if cfg!(debug_assertions) {
                            let hl = &self.history_results[m.idx - active_len];
                            assert!(unsafe { (*m.highlight.start).eql(hl.start_pin()) });
                        }
                    }
                }
            }
        } else {
            // No history node means we have no history
            // if (self.history) |*h| { ... }
            if self.history.is_some() {
                self.history.as_mut().unwrap().deinit(self.screen);
                self.history = None;
                for hl in self.history_results.iter_mut() {
                    hl.deinit(alloc);
                }
                self.history_results.clear();
            }

            // If we have a selection in the history area, we need to
            // move it to the end of the active area.
            // if (self.selected) |*m| selected: {
            'selected: {
                if self.selected.is_some() {
                    // const active_len = self.active_results.items.len;
                    // if (m.idx < active_len) break :selected;
                    let active_len = self.active_results.len();
                    if self.selected.as_ref().unwrap().idx < active_len {
                        break 'selected;
                    }
                    // m.deinit(self.screen);
                    // self.selected = null;
                    self.selected.take().unwrap().deinit(self.screen);
                    // _ = self.select(.prev) catch |err| { ... };
                    if let Err(err) = self.select(Select::Prev) {
                        log::info!(
                            target: LOG_SCOPE,
                            "reload failed to reset search selection err={:?}",
                            err,
                        );
                    }
                }
            }
        }

        // Figure out if we need to fixup our selection later because
        // it was in the active area.
        // const old_active_len = self.active_results.items.len;
        let old_active_len = self.active_results.len();
        // const old_selection_idx: ?usize = if (self.selected) |m| m.idx else null;
        let old_selection_idx: Option<usize> = self.selected.as_ref().map(|m| m.idx);
        // errdefer if (old_selection_idx != null and old_selection_idx.? < old_active_len) { ... };
        //
        // PORT NOTE: this `errdefer` fires only if a fallible step below fails.
        // The only fallible call after this point is `self.tickActive()` (the
        // active re-search). We wrap that call and run the errdefer cleanup on
        // its error path before propagating, mirroring `errdefer`.
        let errdefer_fixup = |this: &mut ScreenSearch| {
            if old_selection_idx.is_some() && old_selection_idx.unwrap() < old_active_len {
                // This is the error scenario. If something fails below,
                // our active area is probably gone, so we just go back
                // to the first result because our selection can't be trusted.
                if this.selected.is_some() {
                    this.selected.take().unwrap().deinit(this.screen);
                    if let Err(err) = this.select(Select::Next) {
                        log::info!(
                            target: LOG_SCOPE,
                            "reload failed to reset search selection err={:?}",
                            err,
                        );
                    }
                }
            }
        };

        // Reset our active search results and search again.
        // for (self.active_results.items) |*hl| hl.deinit(alloc);
        // self.active_results.clearRetainingCapacity();
        for hl in self.active_results.iter_mut() {
            hl.deinit(alloc);
        }
        self.active_results.clear();
        // switch (self.state) {
        let tick_result: Result<(), OutOfMemory> = match self.state {
            // If we're in the active state we run a normal tick so
            // we can move into a better state.
            // .active => try self.tickActive(),
            State::Active => self.tick_active(),

            // Otherwise, just tick it and move back to whatever state
            // we were in.
            // else => { const old_state = self.state; defer self.state = old_state; try self.tickActive(); },
            _ => {
                let old_state = self.state;
                let r = self.tick_active();
                self.state = old_state;
                r
            }
        };
        if let Err(e) = tick_result {
            errdefer_fixup(self);
            return Err(e);
        }

        // If we have no scrollback, we need to prune any active results
        // that aren't in the actual active area. We only do this for the
        // no scrollback scenario because with scrollback we actually
        // rely on our active search searching by page to find history
        // items as well. This is all related to the fact that PageList
        // scrollback limits are discrete by page size except we special
        // case zero.
        // if (self.screen.no_scrollback and self.active_results.items.len > 0) active_prune: {
        if unsafe { (*self.screen).no_scrollback } && !self.active_results.is_empty() {
            'active_prune: {
                // const items = self.active_results.items;
                // const tl = self.screen.pages.getTopLeft(.active);
                let tl = unsafe { (*self.screen).pages.get_top_left(point::Tag::Active) };
                // for (0.., items) |i, *hl| {
                let mut prune_to: Option<usize> = None;
                let mut none_in_active = true;
                for i in 0..self.active_results.len() {
                    // if (!tl.before(hl.endPin())) {
                    //     // Deinit because its going to be pruned no matter
                    //     // what at some point for not being in the active area.
                    //     hl.deinit(alloc);
                    //     continue;
                    // }
                    let end_pin = self.active_results[i].end_pin();
                    if !tl.before(end_pin) {
                        self.active_results[i].deinit(alloc);
                        continue;
                    }

                    // In the active area! Since our results are sorted
                    // that means everything after this is also in the active
                    // area, so we prune up to this i.
                    // if (i > 0) self.active_results.replaceRangeAssumeCapacity(0, i, &.{});
                    // break :active_prune;
                    prune_to = Some(i);
                    none_in_active = false;
                    break;
                }

                if let Some(i) = prune_to {
                    if i > 0 {
                        // replaceRangeAssumeCapacity(0, i, &.{}) removes the first i.
                        self.active_results.drain(0..i);
                    }
                    break 'active_prune;
                }

                // None are in the active area...
                // self.active_results.clearRetainingCapacity();
                if none_in_active {
                    self.active_results.clear();
                }
            }
        }

        // Now we have to fixup our selection if we had one.
        // fixup: {
        'fixup: {
            // const old_idx = old_selection_idx orelse break :fixup;
            let old_idx = match old_selection_idx {
                Some(v) => v,
                None => break 'fixup,
            };
            // const m = if (self.selected) |*m| m else break :fixup;
            if self.selected.is_none() {
                break 'fixup;
            }

            // If our old selection wasn't in the active area, then we
            // need to fix up our offsets.
            // if (old_idx >= old_active_len) {
            //     m.idx -= old_active_len;
            //     m.idx += self.active_results.items.len;
            //     break :fixup;
            // }
            if old_idx >= old_active_len {
                let m = self.selected.as_mut().unwrap();
                m.idx -= old_active_len;
                m.idx += self.active_results.len();
                break 'fixup;
            }

            // We search for the matching highlight in the new active results.
            // for (0.., self.active_results.items) |i, hl| {
            //     const untracked = hl.untracked();
            //     if (m.highlight.start.eql(untracked.start) and
            //         m.highlight.end.eql(untracked.end))
            //     {
            //         m.idx = self.active_results.items.len - 1 - i;
            //         break :fixup;
            //     }
            // }
            let active_len = self.active_results.len();
            let mut found = false;
            for i in 0..active_len {
                let untracked = self.active_results[i].untracked();
                let m = self.selected.as_ref().unwrap();
                if unsafe {
                    (*m.highlight.start).eql(untracked.start)
                        && (*m.highlight.end).eql(untracked.end)
                } {
                    // Found it! Update our index.
                    self.selected.as_mut().unwrap().idx = active_len - 1 - i;
                    found = true;
                    break;
                }
            }
            if found {
                break 'fixup;
            }

            // No match, just go back to the first match.
            // m.deinit(self.screen);
            // self.selected = null;
            self.selected.take().unwrap().deinit(self.screen);
            // _ = self.select(.next) catch |err| { ... };
            if let Err(err) = self.select(Select::Next) {
                log::info!(
                    target: LOG_SCOPE,
                    "reload failed to reset search selection err={:?}",
                    err,
                );
            }
        }

        Ok(())
    }

    /// Return the match at the given index in newest-to-oldest order
    /// (0 = most recent match). Returns null if the index is out of
    /// range.
    ///
    /// This does not require read/write access to the underlying screen.
    // pub fn matchAt(self: *const ScreenSearch, idx: usize) ?FlattenedHighlight {
    //
    // PORT NOTE (oracle `149c9f562`, parity tail S7-6): the reference returns a
    // (shallow, aliasing) `FlattenedHighlight` by value; this returns the same
    // shallow copy as a `FlattenedRef` borrowing the stored result (no
    // allocation). Returns `None` on the out-of-range path exactly as the
    // reference returns `null`.
    pub fn match_at(&self, idx: usize) -> Option<FlattenedRef<'_>> {
        // const active_len = self.active_results.items.len;
        let active_len = self.active_results.len();
        // if (idx < active_len) {
        //     return self.active_results.items[active_len - 1 - idx];
        // }
        if idx < active_len {
            return Some(self.active_results[active_len - 1 - idx].as_ref());
        }

        // const history_len = self.history_results.items.len;
        let history_len = self.history_results.len();
        // if (idx < active_len + history_len) {
        //     return self.history_results.items[idx - active_len];
        // }
        if idx < active_len + history_len {
            return Some(self.history_results[idx - active_len].as_ref());
        }

        // return null;
        None
    }

    /// Return the selected match.
    ///
    /// This does not require read/write access to the underlying screen.
    // pub fn selectedMatch(self: *const ScreenSearch) ?FlattenedHighlight {
    pub fn selected_match(&self) -> Option<FlattenedRef<'_>> {
        // const sel = self.selected orelse return null;
        let sel = self.selected.as_ref()?;
        // return self.matchAt(sel.idx);
        self.match_at(sel.idx)
    }

    /// Select the next or previous search result. This requires read/write
    /// access to the underlying screen, since we utilize tracked pins to
    /// ensure our selection sticks with contents changing.
    // pub fn select(self: *ScreenSearch, to: Select) Allocator.Error!bool {
    pub fn select(&mut self, to: Select) -> Result<bool, OutOfMemory> {
        // A resize can replace any page node, including nodes retained by
        // flattened history results. Reset before reloadActive or tracking a
        // selected result so no pre-resize pointer is ever dereferenced.
        //
        // PORT NOTE (`P3-W1 T5`): this call must NOT sit inside any borrow of
        // `self.history_results` / `self.active_results` / `self.selected` —
        // after a reset those are new allocations. It is the first statement of
        // the function precisely so that no such borrow can be live.
        // _ = try self.resetIfDimensionsChanged();
        let _ = self.reset_if_dimensions_changed()?;

        // Reload validates the selected history result before inspecting its
        // cached coordinates. Prune the remaining results afterward so every
        // candidate is valid before selection tracks it.
        // try self.reloadActive();
        self.reload_active()?;
        // self.pruneHistory();
        self.prune_history();

        // return switch (to) {
        //     .next => try self.selectNext(),
        //     .prev => try self.selectPrev(),
        // };
        match to {
            Select::Next => self.select_next(),
            Select::Prev => self.select_prev(),
        }
    }

    // fn selectNext(self: *ScreenSearch) Allocator.Error!bool {
    fn select_next(&mut self) -> Result<bool, OutOfMemory> {
        // Get our previous match so we can change it. If we have no
        // prior match, we have the easy task of getting the first.
        // var prev = if (self.selected) |*m| m else { ... };
        if self.selected.is_none() {
            // Get our highlight
            // const hl: FlattenedHighlight = hl: { ... };
            let hl: FlattenedRef<'_> = {
                if !self.active_results.is_empty() {
                    // Active is in forward order
                    let len = self.active_results.len();
                    self.active_results[len - 1].as_ref()
                } else if !self.history_results.is_empty() {
                    // History is in reverse order
                    self.history_results[0].as_ref()
                } else {
                    // No matches at all. Can't select anything.
                    return Ok(false);
                }
            };

            // Pin it so we can track any movement
            // const tracked = try hl.untracked().track(self.screen);
            // errdefer tracked.deinit(self.screen);
            let tracked = hl.untracked().track(self.screen)?;

            // Our selection is index zero since we just started and
            // we store our selection.
            // self.selected = .{ .idx = 0, .highlight = tracked };
            // return true;
            self.selected = Some(SelectedMatch {
                idx: 0,
                highlight: tracked,
            });
            return Ok(true);
        }

        // const active_len = self.active_results.items.len;
        // const history_len = self.history_results.items.len;
        let active_len = self.active_results.len();
        let history_len = self.history_results.len();
        // const next_idx = if (prev.idx + 1 >= active_len + history_len) 0 else prev.idx + 1;
        let prev_idx = self.selected.as_ref().unwrap().idx;
        let next_idx = if prev_idx + 1 >= active_len + history_len {
            0
        } else {
            prev_idx + 1
        };
        // const hl: FlattenedHighlight = if (next_idx < active_len)
        //     self.active_results.items[active_len - 1 - next_idx]
        // else
        //     self.history_results.items[next_idx - active_len];
        let hl: FlattenedRef<'_> = if next_idx < active_len {
            self.active_results[active_len - 1 - next_idx].as_ref()
        } else {
            self.history_results[next_idx - active_len].as_ref()
        };

        // Pin it so we can track any movement
        // const tracked = try hl.untracked().track(self.screen);
        // errdefer tracked.deinit(self.screen);
        let tracked = hl.untracked().track(self.screen)?;

        // Free our previous match and setup our new selection
        // prev.deinit(self.screen);
        // self.selected = .{ .idx = next_idx, .highlight = tracked };
        self.selected.take().unwrap().deinit(self.screen);
        self.selected = Some(SelectedMatch {
            idx: next_idx,
            highlight: tracked,
        });

        // return true;
        Ok(true)
    }

    // fn selectPrev(self: *ScreenSearch) Allocator.Error!bool {
    fn select_prev(&mut self) -> Result<bool, OutOfMemory> {
        // Get our previous match so we can change it. If we have no
        // prior match, we have the easy task of getting the last.
        // var prev = if (self.selected) |*m| m else { ... };
        if self.selected.is_none() {
            // Get our highlight (oldest match)
            // const hl: FlattenedHighlight = hl: { ... };
            let hl: FlattenedRef<'_> = {
                if !self.history_results.is_empty() {
                    // History is in reverse order, so last item is oldest
                    let len = self.history_results.len();
                    self.history_results[len - 1].as_ref()
                } else if !self.active_results.is_empty() {
                    // Active is in forward order, so first item is oldest
                    self.active_results[0].as_ref()
                } else {
                    // No matches at all. Can't select anything.
                    return Ok(false);
                }
            };

            // Pin it so we can track any movement
            // const tracked = try hl.untracked().track(self.screen);
            // errdefer tracked.deinit(self.screen);
            let tracked = hl.untracked().track(self.screen)?;

            // Our selection is the last index since we just started
            // and we store our selection.
            // const active_len = self.active_results.items.len;
            // const history_len = self.history_results.items.len;
            // self.selected = .{ .idx = active_len + history_len - 1, .highlight = tracked };
            // return true;
            let active_len = self.active_results.len();
            let history_len = self.history_results.len();
            self.selected = Some(SelectedMatch {
                idx: active_len + history_len - 1,
                highlight: tracked,
            });
            return Ok(true);
        }

        // const active_len = self.active_results.items.len;
        // const history_len = self.history_results.items.len;
        let active_len = self.active_results.len();
        let history_len = self.history_results.len();
        // const next_idx = if (prev.idx != 0) prev.idx - 1 else active_len + history_len - 1;
        let prev_idx = self.selected.as_ref().unwrap().idx;
        let next_idx = if prev_idx != 0 {
            prev_idx - 1
        } else {
            active_len + history_len - 1
        };

        // const hl: FlattenedHighlight = if (next_idx < active_len)
        //     self.active_results.items[active_len - 1 - next_idx]
        // else
        //     self.history_results.items[next_idx - active_len];
        let hl: FlattenedRef<'_> = if next_idx < active_len {
            self.active_results[active_len - 1 - next_idx].as_ref()
        } else {
            self.history_results[next_idx - active_len].as_ref()
        };

        // Pin it so we can track any movement
        // const tracked = try hl.untracked().track(self.screen);
        // errdefer tracked.deinit(self.screen);
        let tracked = hl.untracked().track(self.screen)?;

        // Free our previous match and setup our new selection
        // prev.deinit(self.screen);
        // self.selected = .{ .idx = next_idx, .highlight = tracked };
        self.selected.take().unwrap().deinit(self.screen);
        self.selected = Some(SelectedMatch {
            idx: next_idx,
            highlight: tracked,
        });

        // return true;
        Ok(true)
    }
}

// PORT NOTE: the reference builds `.{ .node = history_node }` — a `Pin`
// struct literal that relies on Zig field defaults (`y = 0`, `x = 0`,
// `garbage = false`). The Rust `Pin` has no defaults, so this helper supplies
// them, mirroring `track_pin(.{ .node = history_node })`.
fn node_pin(node: *mut Node) -> Pin {
    Pin {
        node,
        y: 0,
        x: 0,
        garbage: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::{DynamicPalette, DynamicRGB};
    use crate::csi::EraseDisplay;
    use crate::kitty;
    use crate::modes as modespkg;
    use crate::point::Point;
    use crate::screen_set::Key;
    use crate::terminal::{Colors, Options as TerminalOptions, Terminal};
    use rustty_util::tripwire::ResetMode;

    // PORT NOTE: see active.rs — supplies the Zig `Options` field defaults the
    // reference relies on implicitly.
    fn terminal_opts(cols: u16, rows: u16, max_scrollback_bytes: usize) -> TerminalOptions {
        TerminalOptions {
            cols,
            rows,
            max_scrollback_bytes: Some(max_scrollback_bytes),
            max_scrollback_lines: None,
            colors: Colors {
                background: DynamicRGB::UNSET,
                foreground: DynamicRGB::UNSET,
                cursor: DynamicRGB::UNSET,
                palette: DynamicPalette::default_palette(),
            },
            default_modes: modespkg::ModePacked::default(),
            default_cursor_style: crate::screen::CursorStyle::Block,
            default_cursor_blink: Some(false),
            default_xt_checksum: Default::default(),
            kitty_image_storage_limit: 320 * 1000 * 1000,
            kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
        }
    }

    // test "simple search"
    #[test]
    fn simple_search() {
        let alloc = Allocator;
        // var t: Terminal = try .init(alloc, .{ .cols = 10, .rows = 2 });
        let mut t: Terminal = Terminal::init(alloc, terminal_opts(10, 2, 10_000)).unwrap();
        let active_ptr: *mut Screen = t.screens.active;

        let mut s = t.vt_stream();
        s.next_slice(b"Fizz\r\nBuzz\r\nFizz\r\nBang");

        // var search: ScreenSearch = try .init(alloc, t.screens.active, "Fizz");
        let mut search: ScreenSearch = ScreenSearch::init(alloc, active_ptr, b"Fizz").unwrap();
        // try search.searchAll();
        search.search_all().unwrap();
        // try testing.expectEqual(2, search.active_results.items.len);
        assert_eq!(2, search.active_results.len());
        // We don't test history results since there is overlap

        // Get all matches
        let matches = search.matches(alloc).unwrap();
        // try testing.expectEqual(2, matches.len);
        assert_eq!(2, matches.len());

        // SB: the active screen is shared with the searcher (raw `self.screen`); read it
        // via the raw `active_ptr` per-use, never a `&mut Screen` held across `search.*`
        // calls that mutate it (rust-mut-args-strongly-protected).
        {
            let sel = matches[0].untracked();
            assert_eq!(
                Some(Point::Screen(point::Coordinate { x: 0, y: 2 })),
                unsafe {
                    (*active_ptr)
                        .pages
                        .point_from_pin(point::Tag::Screen, sel.start)
                }
            );
            assert_eq!(
                Some(Point::Screen(point::Coordinate { x: 3, y: 2 })),
                unsafe {
                    (*active_ptr)
                        .pages
                        .point_from_pin(point::Tag::Screen, sel.end)
                }
            );
        }
        {
            let sel = matches[1].untracked();
            assert_eq!(
                Some(Point::Screen(point::Coordinate { x: 0, y: 0 })),
                unsafe {
                    (*active_ptr)
                        .pages
                        .point_from_pin(point::Tag::Screen, sel.start)
                }
            );
            assert_eq!(
                Some(Point::Screen(point::Coordinate { x: 3, y: 0 })),
                unsafe {
                    (*active_ptr)
                        .pages
                        .point_from_pin(point::Tag::Screen, sel.end)
                }
            );
        }

        drop(matches);
        search.deinit();
        s.deinit();
        t.deinit(alloc);
    }

    // test "simple search with history"
    #[cfg_attr(
        all(miri, debug_assertions),
        ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): `Page::assert_integrity` is O(page) and runs on nearly every PageList mutation under `debug_assertions`, which puts this test out of Miri's budget in the debug-assertions-ON G6 lane. It RUNS there — UB-clean and leak-clean — in the G6c lane (`check_leaks.sh --debug-assertions-off`), measured per test in single-worker isolation; see the WS19 T6 per-test table. Do NOT widen this to a plain `miri` ignore: that would silently drop the test from BOTH lanes."
    )]
    #[test]
    fn simple_search_with_history() {
        let alloc = Allocator;
        // var t: Terminal = try .init(alloc, .{ .cols = 10, .rows = 2, .max_scrollback_bytes = std.math.maxInt(usize) });
        let mut t: Terminal = Terminal::init(alloc, terminal_opts(10, 2, usize::MAX)).unwrap();
        let active_ptr: *mut Screen = t.screens.active;
        // const list: *PageList = &t.screens.active.pages;

        let mut s = t.vt_stream();

        s.next_slice(b"Fizz\r\n");
        // while (list.totalPages() < 3) s.nextSlice("\r\n");
        while unsafe { (*active_ptr).pages.total_pages() } < 3 {
            s.next_slice(b"\r\n");
        }
        // for (0..list.rows) |_| s.nextSlice("\r\n");
        let rows = unsafe { (*active_ptr).pages.rows };
        for _ in 0..rows {
            s.next_slice(b"\r\n");
        }
        s.next_slice(b"hello.");

        let mut search: ScreenSearch = ScreenSearch::init(alloc, active_ptr, b"Fizz").unwrap();
        search.search_all().unwrap();
        // try testing.expectEqual(0, search.active_results.items.len);
        assert_eq!(0, search.active_results.len());

        // Get all matches
        let matches = search.matches(alloc).unwrap();
        assert_eq!(1, matches.len());

        // SB: the active screen is shared with the searcher (raw `self.screen`); read it
        // via the raw `active_ptr` per-use, never a `&mut Screen` held across `search.*`
        // calls that mutate it (rust-mut-args-strongly-protected).
        {
            let sel = matches[0].untracked();
            assert_eq!(
                Some(Point::Screen(point::Coordinate { x: 0, y: 0 })),
                unsafe {
                    (*active_ptr)
                        .pages
                        .point_from_pin(point::Tag::Screen, sel.start)
                }
            );
            assert_eq!(
                Some(Point::Screen(point::Coordinate { x: 3, y: 0 })),
                unsafe {
                    (*active_ptr)
                        .pages
                        .point_from_pin(point::Tag::Screen, sel.end)
                }
            );
        }

        drop(matches);
        search.deinit();
        s.deinit();
        t.deinit(alloc);
    }

    // test "reload active with history change"
    #[cfg_attr(
        all(miri, debug_assertions),
        ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): `Page::assert_integrity` is O(page) and runs on nearly every PageList mutation under `debug_assertions`, which puts this test out of Miri's budget in the debug-assertions-ON G6 lane. It RUNS there — UB-clean and leak-clean — in the G6c lane (`check_leaks.sh --debug-assertions-off`), measured per test in single-worker isolation; see the WS19 T6 per-test table. Do NOT widen this to a plain `miri` ignore: that would silently drop the test from BOTH lanes."
    )]
    #[test]
    fn reload_active_with_history_change() {
        let alloc = Allocator;
        let mut t: Terminal = Terminal::init(alloc, terminal_opts(10, 2, usize::MAX)).unwrap();
        let active_ptr: *mut Screen = t.screens.active;

        let mut s = t.vt_stream();
        s.next_slice(b"Fizz\r\n");

        // Start up our search which will populate our initial active area.
        let mut search: ScreenSearch = ScreenSearch::init(alloc, active_ptr, b"Fizz").unwrap();
        search.search_all().unwrap();
        {
            let matches = search.matches(alloc).unwrap();
            assert_eq!(1, matches.len());
        }

        // Grow into two pages so our history pin will move.
        // while (list.totalPages() < 2) s.nextSlice("\r\n");
        while unsafe { (*active_ptr).pages.total_pages() } < 2 {
            s.next_slice(b"\r\n");
        }
        // for (0..list.rows) |_| s.nextSlice("\r\n");
        let rows = unsafe { (*active_ptr).pages.rows };
        for _ in 0..rows {
            s.next_slice(b"\r\n");
        }
        s.next_slice(b"2Fizz");

        // Active area changed so reload
        search.reload_active().unwrap();
        search.search_all().unwrap();

        // SB: the active screen is shared with the searcher (raw `self.screen`); read it
        // via the raw `active_ptr` per-use, never a `&mut Screen` held across `search.*`
        // calls that mutate it (rust-mut-args-strongly-protected).
        // Get all matches
        {
            let matches = search.matches(alloc).unwrap();
            assert_eq!(2, matches.len());
            {
                let sel = matches[1].untracked();
                assert_eq!(
                    Some(Point::Screen(point::Coordinate { x: 0, y: 0 })),
                    unsafe {
                        (*active_ptr)
                            .pages
                            .point_from_pin(point::Tag::Screen, sel.start)
                    }
                );
                assert_eq!(
                    Some(Point::Screen(point::Coordinate { x: 3, y: 0 })),
                    unsafe {
                        (*active_ptr)
                            .pages
                            .point_from_pin(point::Tag::Screen, sel.end)
                    }
                );
            }
            {
                let sel = matches[0].untracked();
                assert_eq!(
                    Some(Point::Active(point::Coordinate { x: 1, y: 1 })),
                    unsafe {
                        (*active_ptr)
                            .pages
                            .point_from_pin(point::Tag::Active, sel.start)
                    }
                );
                assert_eq!(
                    Some(Point::Active(point::Coordinate { x: 4, y: 1 })),
                    unsafe {
                        (*active_ptr)
                            .pages
                            .point_from_pin(point::Tag::Active, sel.end)
                    }
                );
            }
        }

        // Reset the screen which will make our pin garbage.
        // t.fullReset();
        //
        // PORT NOTE: the reference keeps `s` (the Stream) alive across
        // `t.fullReset()`; Rust's borrow checker forbids touching `t` while the
        // `Stream<'_>` borrow is live. We drop the stream (its parser is in
        // ground state after the printable `2Fizz`, so re-init is behavior
        // identical) and re-acquire it after the reset.
        s.deinit();
        drop(s);
        t.full_reset();
        let mut s = t.vt_stream();
        s.next_slice(b"WeFizzing");
        search.reload_active().unwrap();
        search.search_all().unwrap();

        // SB: the active screen is shared with the searcher (raw `self.screen`); read it
        // via the raw `active_ptr` per-use, never a `&mut Screen` held across `search.*`
        // calls that mutate it (rust-mut-args-strongly-protected).
        {
            let matches = search.matches(alloc).unwrap();
            assert_eq!(1, matches.len());
            {
                let sel = matches[0].untracked();
                assert_eq!(
                    Some(Point::Active(point::Coordinate { x: 2, y: 0 })),
                    unsafe {
                        (*active_ptr)
                            .pages
                            .point_from_pin(point::Tag::Active, sel.start)
                    }
                );
                assert_eq!(
                    Some(Point::Active(point::Coordinate { x: 5, y: 0 })),
                    unsafe {
                        (*active_ptr)
                            .pages
                            .point_from_pin(point::Tag::Active, sel.end)
                    }
                );
            }
        }

        search.deinit();
        s.deinit();
        t.deinit(alloc);
    }

    // test "active change contents"
    #[test]
    fn active_change_contents() {
        let alloc = Allocator;
        let mut t: Terminal = Terminal::init(alloc, terminal_opts(10, 5, 10_000)).unwrap();
        let active_ptr: *mut Screen = t.screens.active;

        let mut s = t.vt_stream();
        s.next_slice(b"Fuzz\r\nBuzz\r\nFizz\r\nBang");

        let mut search: ScreenSearch = ScreenSearch::init(alloc, active_ptr, b"Fizz").unwrap();
        search.search_all().unwrap();
        assert_eq!(1, search.active_results.len());

        // Erase the screen, move our cursor to the top, and change contents.
        s.next_slice(b"\x1b[2J\x1b[H"); // Clear screen and move home
        s.next_slice(b"Bang\r\nFizz\r\nHello!");

        search.reload_active().unwrap();
        search.search_all().unwrap();
        assert_eq!(1, search.active_results.len());

        // Get all matches
        let matches = search.matches(alloc).unwrap();
        assert_eq!(1, matches.len());

        // SB: the active screen is shared with the searcher (raw `self.screen`); read it
        // via the raw `active_ptr` per-use, never a `&mut Screen` held across `search.*`
        // calls that mutate it (rust-mut-args-strongly-protected).
        {
            let sel = matches[0].untracked();
            assert_eq!(
                Some(Point::Screen(point::Coordinate { x: 0, y: 1 })),
                unsafe {
                    (*active_ptr)
                        .pages
                        .point_from_pin(point::Tag::Screen, sel.start)
                }
            );
            assert_eq!(
                Some(Point::Screen(point::Coordinate { x: 3, y: 1 })),
                unsafe {
                    (*active_ptr)
                        .pages
                        .point_from_pin(point::Tag::Screen, sel.end)
                }
            );
        }

        drop(matches);
        search.deinit();
        s.deinit();
        t.deinit(alloc);
    }

    // test "select after resize resets stale flattened results"
    #[test]
    fn select_after_resize_resets_stale_flattened_results() {
        let alloc = Allocator;
        let mut t: Terminal = Terminal::init(alloc, terminal_opts(10, 3, usize::MAX)).unwrap();
        let active_ptr: *mut Screen = t.screens.active;

        {
            let mut stream = t.vt_stream();
            stream.next_slice(b"Fizz\r\nBuzz");
            stream.deinit();
        }

        let mut search: ScreenSearch = ScreenSearch::init(alloc, active_ptr, b"Fizz").unwrap();
        search.search_all().unwrap();
        assert!(search.select(Select::Next).unwrap());
        assert!(search.selected_match().is_some());
        assert_eq!(10 as size::CellCountInt, search.cols);

        // Column reflow replaces the nodes referenced by the cached flattened
        // active result. select() calls reload_active(), so dimension
        // invalidation must run before either path dereferences those old nodes.
        //
        // SB: the searcher holds a raw `*mut Screen`; drive the resize through
        // the SAME raw pointer rather than a `&mut Screen`
        // (rust-mut-args-strongly-protected).
        unsafe {
            (*active_ptr)
                .pages
                .resize(crate::pagelist::resize::Resize {
                    cols: Some(5),
                    ..Default::default()
                })
                .unwrap()
        };
        assert_eq!(10 as size::CellCountInt, search.cols);

        assert!(search.select(Select::Next).unwrap());
        assert_eq!(5 as size::CellCountInt, search.cols);

        let selected = search.selected_match().unwrap().untracked();
        assert_eq!(5, unsafe {
            crate::pagelist::Node::cols(selected.start.node)
        });
        assert_eq!(5, unsafe { crate::pagelist::Node::cols(selected.end.node) });

        search.deinit();
        t.deinit(alloc);
    }

    /// `P3-W1 T5` step 3. The oracle's own `select after resize ...` test cannot
    /// tell the two new call sites apart: `select` delegates to `reload_active`,
    /// and `reload_active`'s reset RETURNS EARLY, so removing either site alone
    /// still resets (proven by mutation — see the commit body). This test gives
    /// the `reload_active` site its own reproducer by calling it DIRECTLY after a
    /// column resize, with no `select` in the path.
    #[test]
    fn reload_active_after_resize_resets_stale_flattened_results() {
        let alloc = Allocator;
        let mut t: Terminal = Terminal::init(alloc, terminal_opts(10, 3, usize::MAX)).unwrap();
        let active_ptr: *mut Screen = t.screens.active;

        {
            let mut stream = t.vt_stream();
            stream.next_slice(b"Fizz\r\nBuzz");
            stream.deinit();
        }

        let mut search: ScreenSearch = ScreenSearch::init(alloc, active_ptr, b"Fizz").unwrap();
        search.search_all().unwrap();
        assert!(search.select(Select::Next).unwrap());
        assert_eq!(10 as size::CellCountInt, search.cols);

        unsafe {
            (*active_ptr)
                .pages
                .resize(crate::pagelist::resize::Resize {
                    cols: Some(5),
                    ..Default::default()
                })
                .unwrap()
        };
        assert_eq!(10 as size::CellCountInt, search.cols);

        // No select() in the path: reload_active alone must invalidate.
        search.reload_active().unwrap();
        assert_eq!(5 as size::CellCountInt, search.cols);
        assert_eq!(3 as size::CellCountInt, search.rows);

        search.deinit();
        t.deinit(alloc);
    }

    // test "select next"
    #[test]
    fn select_next() {
        let alloc = Allocator;
        let mut t: Terminal = Terminal::init(alloc, terminal_opts(10, 2, 10_000)).unwrap();
        let active_ptr: *mut Screen = t.screens.active;

        let mut s = t.vt_stream();
        s.next_slice(b"Fizz\r\nBuzz\r\nFizz\r\nBang");

        let mut search: ScreenSearch = ScreenSearch::init(alloc, active_ptr, b"Fizz").unwrap();

        // Initially no selection
        assert!(search.selected_match().is_none());

        // Select our next match (first)
        search.search_all().unwrap();
        let _ = search.select(Select::Next).unwrap();
        // SB: the active screen is shared with the searcher (raw `self.screen`); read it
        // via the raw `active_ptr` per-use, never a `&mut Screen` held across `search.*`
        // calls that mutate it (rust-mut-args-strongly-protected).
        {
            let sel = search.selected_match().unwrap().untracked();
            assert_eq!(
                Some(Point::Screen(point::Coordinate { x: 0, y: 2 })),
                unsafe {
                    (*active_ptr)
                        .pages
                        .point_from_pin(point::Tag::Screen, sel.start)
                }
            );
            assert_eq!(
                Some(Point::Screen(point::Coordinate { x: 3, y: 2 })),
                unsafe {
                    (*active_ptr)
                        .pages
                        .point_from_pin(point::Tag::Screen, sel.end)
                }
            );
        }

        // Next match
        let _ = search.select(Select::Next).unwrap();
        {
            let sel = search.selected_match().unwrap().untracked();
            assert_eq!(
                Some(Point::Screen(point::Coordinate { x: 0, y: 0 })),
                unsafe {
                    (*active_ptr)
                        .pages
                        .point_from_pin(point::Tag::Screen, sel.start)
                }
            );
            assert_eq!(
                Some(Point::Screen(point::Coordinate { x: 3, y: 0 })),
                unsafe {
                    (*active_ptr)
                        .pages
                        .point_from_pin(point::Tag::Screen, sel.end)
                }
            );
        }

        // Next match (wrap)
        let _ = search.select(Select::Next).unwrap();
        {
            let sel = search.selected_match().unwrap().untracked();
            assert_eq!(
                Some(Point::Screen(point::Coordinate { x: 0, y: 2 })),
                unsafe {
                    (*active_ptr)
                        .pages
                        .point_from_pin(point::Tag::Screen, sel.start)
                }
            );
            assert_eq!(
                Some(Point::Screen(point::Coordinate { x: 3, y: 2 })),
                unsafe {
                    (*active_ptr)
                        .pages
                        .point_from_pin(point::Tag::Screen, sel.end)
                }
            );
        }

        search.deinit();
        s.deinit();
        t.deinit(alloc);
    }

    // test "select in active changes contents completely"
    #[test]
    fn select_in_active_changes_contents_completely() {
        let alloc = Allocator;
        let mut t: Terminal = Terminal::init(alloc, terminal_opts(10, 5, 10_000)).unwrap();
        let active_ptr: *mut Screen = t.screens.active;

        let mut s = t.vt_stream();
        s.next_slice(b"Fizz\r\nBuzz\r\nFizz\r\nBang");

        let mut search: ScreenSearch = ScreenSearch::init(alloc, active_ptr, b"Fizz").unwrap();
        search.search_all().unwrap();
        let _ = search.select(Select::Next).unwrap();
        let _ = search.select(Select::Next).unwrap();
        // SB: the active screen is shared with the searcher (raw `self.screen`); read it
        // via the raw `active_ptr` per-use, never a `&mut Screen` held across `search.*`
        // calls that mutate it (rust-mut-args-strongly-protected).
        {
            // Initial selection is the first fizz
            let sel = search.selected_match().unwrap().untracked();
            assert_eq!(
                Some(Point::Screen(point::Coordinate { x: 0, y: 0 })),
                unsafe {
                    (*active_ptr)
                        .pages
                        .point_from_pin(point::Tag::Screen, sel.start)
                }
            );
            assert_eq!(
                Some(Point::Screen(point::Coordinate { x: 3, y: 0 })),
                unsafe {
                    (*active_ptr)
                        .pages
                        .point_from_pin(point::Tag::Screen, sel.end)
                }
            );
        }

        // Erase the screen, move our cursor to the top, and change contents.
        s.next_slice(b"\x1b[2J\x1b[H"); // Clear screen and move home
        s.next_slice(b"Fuzz\r\nFizz\r\nHello!");

        search.reload_active().unwrap();
        {
            // Our selection should move to the first
            let sel = search.selected_match().unwrap().untracked();
            assert_eq!(
                Some(Point::Screen(point::Coordinate { x: 0, y: 1 })),
                unsafe {
                    (*active_ptr)
                        .pages
                        .point_from_pin(point::Tag::Screen, sel.start)
                }
            );
            assert_eq!(
                Some(Point::Screen(point::Coordinate { x: 3, y: 1 })),
                unsafe {
                    (*active_ptr)
                        .pages
                        .point_from_pin(point::Tag::Screen, sel.end)
                }
            );
        }

        // Erase the screen, redraw with same contents.
        s.next_slice(b"\x1b[2J\x1b[H"); // Clear screen and move home
        s.next_slice(b"Fuzz\r\nFizz\r\nFizz");

        search.reload_active().unwrap();
        {
            // Our selection should not move to the first
            let sel = search.selected_match().unwrap().untracked();
            assert_eq!(
                Some(Point::Screen(point::Coordinate { x: 0, y: 1 })),
                unsafe {
                    (*active_ptr)
                        .pages
                        .point_from_pin(point::Tag::Screen, sel.start)
                }
            );
            assert_eq!(
                Some(Point::Screen(point::Coordinate { x: 3, y: 1 })),
                unsafe {
                    (*active_ptr)
                        .pages
                        .point_from_pin(point::Tag::Screen, sel.end)
                }
            );
        }

        search.deinit();
        s.deinit();
        t.deinit(alloc);
    }

    // test "select into history"
    #[cfg_attr(
        all(miri, debug_assertions),
        ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): `Page::assert_integrity` is O(page) and runs on nearly every PageList mutation under `debug_assertions`, which puts this test out of Miri's budget in the debug-assertions-ON G6 lane. It RUNS there — UB-clean and leak-clean — in the G6c lane (`check_leaks.sh --debug-assertions-off`), measured per test in single-worker isolation; see the WS19 T6 per-test table. Do NOT widen this to a plain `miri` ignore: that would silently drop the test from BOTH lanes."
    )]
    #[test]
    fn select_into_history() {
        let alloc = Allocator;
        let mut t: Terminal = Terminal::init(alloc, terminal_opts(10, 2, usize::MAX)).unwrap();
        let active_ptr: *mut Screen = t.screens.active;

        let mut s = t.vt_stream();

        s.next_slice(b"Fizz\r\n");
        while unsafe { (*active_ptr).pages.total_pages() } < 3 {
            s.next_slice(b"\r\n");
        }
        let rows = unsafe { (*active_ptr).pages.rows };
        for _ in 0..rows {
            s.next_slice(b"\r\n");
        }
        s.next_slice(b"hello.");

        let mut search: ScreenSearch = ScreenSearch::init(alloc, active_ptr, b"Fizz").unwrap();
        search.search_all().unwrap();

        // SB: the active screen is shared with the searcher (raw `self.screen`); read it
        // via the raw `active_ptr` per-use, never a `&mut Screen` held across `search.*`
        // calls that mutate it (rust-mut-args-strongly-protected).
        // Get all matches
        let _ = search.select(Select::Next).unwrap();
        {
            let sel = search.selected_match().unwrap().untracked();
            assert_eq!(
                Some(Point::Screen(point::Coordinate { x: 0, y: 0 })),
                unsafe {
                    (*active_ptr)
                        .pages
                        .point_from_pin(point::Tag::Screen, sel.start)
                }
            );
            assert_eq!(
                Some(Point::Screen(point::Coordinate { x: 3, y: 0 })),
                unsafe {
                    (*active_ptr)
                        .pages
                        .point_from_pin(point::Tag::Screen, sel.end)
                }
            );
        }

        // Erase the screen, redraw with same contents.
        s.next_slice(b"\x1b[2J\x1b[H"); // Clear screen and move home
        s.next_slice(b"yo yo");

        search.reload_active().unwrap();
        {
            // Our selection should not move since the history is still active.
            let sel = search.selected_match().unwrap().untracked();
            assert_eq!(
                Some(Point::Screen(point::Coordinate { x: 0, y: 0 })),
                unsafe {
                    (*active_ptr)
                        .pages
                        .point_from_pin(point::Tag::Screen, sel.start)
                }
            );
            assert_eq!(
                Some(Point::Screen(point::Coordinate { x: 3, y: 0 })),
                unsafe {
                    (*active_ptr)
                        .pages
                        .point_from_pin(point::Tag::Screen, sel.end)
                }
            );
        }

        // Create some new history by adding more lines.
        s.next_slice(b"\r\nfizz\r\nfizz\r\nfizz"); // Clear screen and move home
        search.reload_active().unwrap();
        {
            // Our selection should not move since the history is still not
            // pruned.
            let sel = search.selected_match().unwrap().untracked();
            assert_eq!(
                Some(Point::Screen(point::Coordinate { x: 0, y: 0 })),
                unsafe {
                    (*active_ptr)
                        .pages
                        .point_from_pin(point::Tag::Screen, sel.start)
                }
            );
            assert_eq!(
                Some(Point::Screen(point::Coordinate { x: 3, y: 0 })),
                unsafe {
                    (*active_ptr)
                        .pages
                        .point_from_pin(point::Tag::Screen, sel.end)
                }
            );
        }

        search.deinit();
        s.deinit();
        t.deinit(alloc);
    }

    // test "select prev"
    #[test]
    fn select_prev() {
        let alloc = Allocator;
        let mut t: Terminal = Terminal::init(alloc, terminal_opts(10, 2, 10_000)).unwrap();
        let active_ptr: *mut Screen = t.screens.active;

        let mut s = t.vt_stream();
        s.next_slice(b"Fizz\r\nBuzz\r\nFizz\r\nBang");

        let mut search: ScreenSearch = ScreenSearch::init(alloc, active_ptr, b"Fizz").unwrap();

        // Initially no selection
        assert!(search.selected_match().is_none());

        // Select prev (oldest first)
        search.search_all().unwrap();
        let _ = search.select(Select::Prev).unwrap();
        // SB: the active screen is shared with the searcher (raw `self.screen`); read it
        // via the raw `active_ptr` per-use, never a `&mut Screen` held across `search.*`
        // calls that mutate it (rust-mut-args-strongly-protected).
        {
            let sel = search.selected_match().unwrap().untracked();
            assert_eq!(
                Some(Point::Screen(point::Coordinate { x: 0, y: 0 })),
                unsafe {
                    (*active_ptr)
                        .pages
                        .point_from_pin(point::Tag::Screen, sel.start)
                }
            );
            assert_eq!(
                Some(Point::Screen(point::Coordinate { x: 3, y: 0 })),
                unsafe {
                    (*active_ptr)
                        .pages
                        .point_from_pin(point::Tag::Screen, sel.end)
                }
            );
        }

        // Prev match (towards newest)
        let _ = search.select(Select::Prev).unwrap();
        {
            let sel = search.selected_match().unwrap().untracked();
            assert_eq!(
                Some(Point::Screen(point::Coordinate { x: 0, y: 2 })),
                unsafe {
                    (*active_ptr)
                        .pages
                        .point_from_pin(point::Tag::Screen, sel.start)
                }
            );
            assert_eq!(
                Some(Point::Screen(point::Coordinate { x: 3, y: 2 })),
                unsafe {
                    (*active_ptr)
                        .pages
                        .point_from_pin(point::Tag::Screen, sel.end)
                }
            );
        }

        // Prev match (wrap)
        let _ = search.select(Select::Prev).unwrap();
        {
            let sel = search.selected_match().unwrap().untracked();
            assert_eq!(
                Some(Point::Screen(point::Coordinate { x: 0, y: 0 })),
                unsafe {
                    (*active_ptr)
                        .pages
                        .point_from_pin(point::Tag::Screen, sel.start)
                }
            );
            assert_eq!(
                Some(Point::Screen(point::Coordinate { x: 3, y: 0 })),
                unsafe {
                    (*active_ptr)
                        .pages
                        .point_from_pin(point::Tag::Screen, sel.end)
                }
            );
        }

        search.deinit();
        s.deinit();
        t.deinit(alloc);
    }

    // test "select prev then next"
    #[test]
    fn select_prev_then_next() {
        let alloc = Allocator;
        let mut t: Terminal = Terminal::init(alloc, terminal_opts(10, 2, 10_000)).unwrap();
        let active_ptr: *mut Screen = t.screens.active;

        let mut s = t.vt_stream();
        s.next_slice(b"Fizz\r\nBuzz\r\nFizz\r\nBang");

        let mut search: ScreenSearch = ScreenSearch::init(alloc, active_ptr, b"Fizz").unwrap();
        search.search_all().unwrap();

        // SB: the active screen is shared with the searcher (raw `self.screen`); read it
        // via the raw `active_ptr` per-use, never a `&mut Screen` held across `search.*`
        // calls that mutate it (rust-mut-args-strongly-protected).
        // Select next (newest first)
        let _ = search.select(Select::Next).unwrap();
        {
            let sel = search.selected_match().unwrap().untracked();
            assert_eq!(
                Some(Point::Screen(point::Coordinate { x: 0, y: 2 })),
                unsafe {
                    (*active_ptr)
                        .pages
                        .point_from_pin(point::Tag::Screen, sel.start)
                }
            );
        }

        // Select next (older)
        let _ = search.select(Select::Next).unwrap();
        {
            let sel = search.selected_match().unwrap().untracked();
            assert_eq!(
                Some(Point::Screen(point::Coordinate { x: 0, y: 0 })),
                unsafe {
                    (*active_ptr)
                        .pages
                        .point_from_pin(point::Tag::Screen, sel.start)
                }
            );
        }

        // Select prev (back to newer)
        let _ = search.select(Select::Prev).unwrap();
        {
            let sel = search.selected_match().unwrap().untracked();
            assert_eq!(
                Some(Point::Screen(point::Coordinate { x: 0, y: 2 })),
                unsafe {
                    (*active_ptr)
                        .pages
                        .point_from_pin(point::Tag::Screen, sel.start)
                }
            );
        }

        search.deinit();
        s.deinit();
        t.deinit(alloc);
    }

    // test "select prev with history"
    #[cfg_attr(
        all(miri, debug_assertions),
        ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): `Page::assert_integrity` is O(page) and runs on nearly every PageList mutation under `debug_assertions`, which puts this test out of Miri's budget in the debug-assertions-ON G6 lane. It RUNS there — UB-clean and leak-clean — in the G6c lane (`check_leaks.sh --debug-assertions-off`), measured per test in single-worker isolation; see the WS19 T6 per-test table. Do NOT widen this to a plain `miri` ignore: that would silently drop the test from BOTH lanes."
    )]
    #[test]
    fn select_prev_with_history() {
        let alloc = Allocator;
        let mut t: Terminal = Terminal::init(alloc, terminal_opts(10, 2, usize::MAX)).unwrap();
        let active_ptr: *mut Screen = t.screens.active;

        let mut s = t.vt_stream();

        s.next_slice(b"Fizz\r\n");
        while unsafe { (*active_ptr).pages.total_pages() } < 3 {
            s.next_slice(b"\r\n");
        }
        let rows = unsafe { (*active_ptr).pages.rows };
        for _ in 0..rows {
            s.next_slice(b"\r\n");
        }
        s.next_slice(b"Fizz.");

        let mut search: ScreenSearch = ScreenSearch::init(alloc, active_ptr, b"Fizz").unwrap();
        search.search_all().unwrap();

        // SB: the active screen is shared with the searcher (raw `self.screen`); read it
        // via the raw `active_ptr` per-use, never a `&mut Screen` held across `search.*`
        // calls that mutate it (rust-mut-args-strongly-protected).
        // Select prev (oldest first, should be in history)
        let _ = search.select(Select::Prev).unwrap();
        {
            let sel = search.selected_match().unwrap().untracked();
            assert_eq!(
                Some(Point::Screen(point::Coordinate { x: 0, y: 0 })),
                unsafe {
                    (*active_ptr)
                        .pages
                        .point_from_pin(point::Tag::Screen, sel.start)
                }
            );
            assert_eq!(
                Some(Point::Screen(point::Coordinate { x: 3, y: 0 })),
                unsafe {
                    (*active_ptr)
                        .pages
                        .point_from_pin(point::Tag::Screen, sel.end)
                }
            );
        }

        // Select prev (towards newer, should move to active area)
        let _ = search.select(Select::Prev).unwrap();
        {
            let sel = search.selected_match().unwrap().untracked();
            assert_eq!(
                Some(Point::Active(point::Coordinate { x: 0, y: 1 })),
                unsafe {
                    (*active_ptr)
                        .pages
                        .point_from_pin(point::Tag::Active, sel.start)
                }
            );
            assert_eq!(
                Some(Point::Active(point::Coordinate { x: 3, y: 1 })),
                unsafe {
                    (*active_ptr)
                        .pages
                        .point_from_pin(point::Tag::Active, sel.end)
                }
            );
        }

        search.deinit();
        s.deinit();
        t.deinit(alloc);
    }

    // test "select prev wraps when all matches are in history"
    #[cfg_attr(
        all(miri, debug_assertions),
        ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): `Page::assert_integrity` is O(page) and runs on nearly every PageList mutation under `debug_assertions`, which puts this test out of Miri's budget in the debug-assertions-ON G6 lane. It RUNS there — UB-clean and leak-clean — in the G6c lane (`check_leaks.sh --debug-assertions-off`), measured per test in single-worker isolation; see the WS19 T6 per-test table. Do NOT widen this to a plain `miri` ignore: that would silently drop the test from BOTH lanes."
    )]
    #[test]
    fn select_prev_wraps_when_all_matches_are_in_history() {
        // Regression test: when every match is in scrollback (the active area
        // has none, so active_len == 0), selecting prev from index 0 must wrap
        // to the last result without underflowing `active_len - 1`.
        let alloc = Allocator;
        let mut t: Terminal = Terminal::init(alloc, terminal_opts(10, 2, usize::MAX)).unwrap();
        let active_ptr: *mut Screen = t.screens.active;

        let mut s = t.vt_stream();

        // Put the only match in scrollback, then scroll the active area to all
        // blank lines so it contains no match (active_len == 0, history_len == 1).
        s.next_slice(b"Fizz\r\n");
        while unsafe { (*active_ptr).pages.total_pages() } < 3 {
            s.next_slice(b"\r\n");
        }
        let rows = unsafe { (*active_ptr).pages.rows };
        for _ in 0..rows {
            s.next_slice(b"\r\n");
        }

        let mut search: ScreenSearch = ScreenSearch::init(alloc, active_ptr, b"Fizz").unwrap();
        search.search_all().unwrap();
        assert_eq!(0, search.active_results.len());

        // Select the first match (idx 0), then wrap backwards. This must not
        // panic and must keep a valid selection.
        let _ = search.select(Select::Next).unwrap();
        let _ = search.select(Select::Prev).unwrap();
        assert!(search.selected_match().is_some());

        search.deinit();
        s.deinit();
        t.deinit(alloc);
    }

    // test "select after all matches disappear drops the selection"
    #[test]
    fn select_after_all_matches_disappear_drops_the_selection() {
        // The wrap arithmetic in selectPrev (active_len + history_len - 1) would
        // underflow if a selection were ever live while both result lists are
        // empty. This guards the invariant that makes that unreachable: when a
        // reload/prune empties the results, the selection is dropped, so the next
        // select() hits the "no matches" guard instead of the wrap arithmetic.
        let alloc = Allocator;
        let mut t: Terminal = Terminal::init(alloc, terminal_opts(10, 2, 10_000)).unwrap();
        let active_ptr: *mut Screen = t.screens.active;

        let mut s = t.vt_stream();
        s.next_slice(b"Fizz");

        let mut search: ScreenSearch = ScreenSearch::init(alloc, active_ptr, b"Fizz").unwrap();
        search.search_all().unwrap();
        assert_eq!(1, search.active_results.len());

        // Take a selection, then overwrite the only match so a reload finds none
        // (active and history both empty).
        let _ = search.select(Select::Next).unwrap();
        assert!(search.selected_match().is_some());
        s.next_slice(b"\x1b[1;1H    ");

        // Must not underflow; the selection is dropped and nothing is selected.
        let _ = search.select(Select::Prev).unwrap();
        assert!(search.selected_match().is_none());
        assert_eq!(0, search.active_results.len());
        assert_eq!(0, search.history_results.len());

        search.deinit();
        s.deinit();
        t.deinit(alloc);
    }

    // test "screen search no scrollback has no history"
    #[test]
    fn screen_search_no_scrollback_has_no_history() {
        let alloc = Allocator;
        let mut t: Terminal = Terminal::init(alloc, terminal_opts(10, 2, 0)).unwrap();

        // Alt screen has no scrollback
        // _ = try t.switchScreen(.alternate);
        let _ = t.switch_screen(Key::Alternate).unwrap();
        let active_ptr: *mut Screen = t.screens.active;

        let mut s = t.vt_stream();

        // This will probably stop working at some point and we'll have
        // no way to test it using public APIs, but at the time of writing
        // this test, CSI 22 J (scroll complete) pushes into scrollback
        // with alt screen.
        s.next_slice(b"Fizz\r\n");
        s.next_slice(b"\x1b[22J");
        s.next_slice(b"hello.");

        let mut search: ScreenSearch = ScreenSearch::init(alloc, active_ptr, b"Fizz").unwrap();
        search.search_all().unwrap();
        assert_eq!(0, search.active_results.len());

        // Get all matches
        let matches = search.matches(alloc).unwrap();
        assert_eq!(0, matches.len());

        drop(matches);
        search.deinit();
        s.deinit();
        t.deinit(alloc);
    }

    // test "reloadActive partial history cleanup on appendSlice error"
    #[cfg_attr(
        miri,
        ignore = "MIRI EXCEPTION (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): out of budget in the debug-assertions-ON G6 lane, and NOT rescuable by the G6c debug-assertions-off lane — this test drives a `debug_assertions`-only mechanism and is already `cfg_attr(not(debug_assertions), ignore)`, so turning the assertions off makes it inert rather than fast. Covered natively + by ASan."
    )]
    #[test]
    // WS18 release lane (CORRECTED — fix round 1 of parity S166-F2): the sentence
    // that used to stand here, "`tripwire::Module::enabled()` is
    // `cfg!(debug_assertions)`", is FALSE and had this test `#[ignore]`d in
    // release for a reason that did not exist. `enabled()` is
    // `cfg!(any(test, debug_assertions, feature = "tripwire-test"))`, and
    // rustty-vt turns `rustty-util/tripwire-test` on as a dev-dependency feature
    // precisely so this crate's fail points trip in the RELEASE test lane too —
    // which is what Zig's `builtin.is_test` does at every optimize mode. Verified:
    // this test RUNS and PASSES under `cargo test -p rustty-vt --release`.
    fn reload_active_partial_history_cleanup_on_append_slice_error() {
        // This test verifies that when reloadActive fails at appendSlice (after
        // the loop), all FlattenedHighlight items are properly cleaned up.
        let alloc = Allocator;
        let mut t: Terminal = Terminal::init(alloc, terminal_opts(10, 2, usize::MAX)).unwrap();
        let active_ptr: *mut Screen = t.screens.active;

        let mut s = t.vt_stream();

        // Write multiple "Fizz" matches that will end up in history.
        // We need enough content to push "Fizz" entries into scrollback.
        s.next_slice(b"Fizz\r\nFizz\r\n");
        while unsafe { (*active_ptr).pages.total_pages() } < 3 {
            s.next_slice(b"\r\n");
        }
        let rows = unsafe { (*active_ptr).pages.rows };
        for _ in 0..rows {
            s.next_slice(b"\r\n");
        }
        s.next_slice(b"Fizz.");

        // Complete initial search
        let mut search: ScreenSearch = ScreenSearch::init(alloc, active_ptr, b"Fizz").unwrap();
        search.search_all().unwrap();

        // Now trigger reloadActive by adding more content that changes the
        // active/history boundary. First add more "Fizz" entries to history.
        s.next_slice(b"\r\nFizz\r\nFizz\r\n");
        while unsafe { (*active_ptr).pages.total_pages() } < 4 {
            s.next_slice(b"\r\n");
        }
        let rows = unsafe { (*active_ptr).pages.rows };
        for _ in 0..rows {
            s.next_slice(b"\r\n");
        }

        // Arm the tripwire to fail at appendSlice (after the loop completes).
        // At this point, there are FlattenedHighlight items in the results list
        // that need cleanup.
        // const tw = reloadActive_tw;
        // defer tw.end(.reset) catch unreachable;
        // tw.errorAlways(.history_append_existing, error.OutOfMemory);
        RELOAD_ACTIVE_TW.with(|tw| {
            tw.borrow_mut()
                .errorAlways(ReloadActiveFp::HistoryAppendExisting, OutOfMemory)
        });

        // reloadActive is called by select(), which should trigger the error path.
        // If the bug exists, testing.allocator will report a memory leak
        // because FlattenedHighlight items weren't cleaned up.
        // try testing.expectError(error.OutOfMemory, search.select(.next));
        assert_eq!(Err(OutOfMemory), search.select(Select::Next));

        RELOAD_ACTIVE_TW.with(|tw| tw.borrow_mut().end(ResetMode::Reset).unwrap());

        search.deinit();
        s.deinit();
        t.deinit(alloc);
    }

    // test "reloadActive partial history cleanup on loop append error"
    #[cfg_attr(
        miri,
        ignore = "MIRI EXCEPTION (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): out of budget in the debug-assertions-ON G6 lane, and NOT rescuable by the G6c debug-assertions-off lane — this test drives a `debug_assertions`-only mechanism and is already `cfg_attr(not(debug_assertions), ignore)`, so turning the assertions off makes it inert rather than fast. Covered natively + by ASan."
    )]
    #[test]
    // WS18 release lane (CORRECTED — fix round 1 of parity S166-F2): the sentence
    // that used to stand here, "`tripwire::Module::enabled()` is
    // `cfg!(debug_assertions)`", is FALSE and had this test `#[ignore]`d in
    // release for a reason that did not exist. `enabled()` is
    // `cfg!(any(test, debug_assertions, feature = "tripwire-test"))`, and
    // rustty-vt turns `rustty-util/tripwire-test` on as a dev-dependency feature
    // precisely so this crate's fail points trip in the RELEASE test lane too —
    // which is what Zig's `builtin.is_test` does at every optimize mode. Verified:
    // this test RUNS and PASSES under `cargo test -p rustty-vt --release`.
    fn reload_active_partial_history_cleanup_on_loop_append_error() {
        // This test verifies that when reloadActive fails inside the loop
        // (after some items have been appended), all FlattenedHighlight items
        // are properly cleaned up.
        let alloc = Allocator;
        let mut t: Terminal = Terminal::init(alloc, terminal_opts(10, 2, usize::MAX)).unwrap();
        let active_ptr: *mut Screen = t.screens.active;

        let mut s = t.vt_stream();

        // Write multiple "Fizz" matches that will end up in history.
        // We need enough content to push "Fizz" entries into scrollback.
        s.next_slice(b"Fizz\r\nFizz\r\n");
        while unsafe { (*active_ptr).pages.total_pages() } < 3 {
            s.next_slice(b"\r\n");
        }
        let rows = unsafe { (*active_ptr).pages.rows };
        for _ in 0..rows {
            s.next_slice(b"\r\n");
        }
        s.next_slice(b"Fizz.");

        // Complete initial search
        let mut search: ScreenSearch = ScreenSearch::init(alloc, active_ptr, b"Fizz").unwrap();
        search.search_all().unwrap();

        // Now trigger reloadActive by adding more content that changes the
        // active/history boundary. First add more "Fizz" entries to history.
        s.next_slice(b"\r\nFizz\r\nFizz\r\n");
        while unsafe { (*active_ptr).pages.total_pages() } < 4 {
            s.next_slice(b"\r\n");
        }
        let rows = unsafe { (*active_ptr).pages.rows };
        for _ in 0..rows {
            s.next_slice(b"\r\n");
        }

        // Arm the tripwire to fail after the first loop append succeeds.
        // This leaves at least one FlattenedHighlight in the results list
        // that needs cleanup.
        // const tw = reloadActive_tw;
        // defer tw.end(.reset) catch unreachable;
        // tw.errorAfter(.history_append_new, error.OutOfMemory, 1);
        RELOAD_ACTIVE_TW.with(|tw| {
            tw.borrow_mut()
                .errorAfter(ReloadActiveFp::HistoryAppendNew, OutOfMemory, 1)
        });

        // reloadActive is called by select(), which should trigger the error path.
        assert_eq!(Err(OutOfMemory), search.select(Select::Next));

        RELOAD_ACTIVE_TW.with(|tw| tw.borrow_mut().end(ResetMode::Reset).unwrap());

        search.deinit();
        s.deinit();
        t.deinit(alloc);
    }

    /// PORTS: oracle d239f9805
    /// `test "select after partial history erase drops a pruned selection"`
    #[cfg_attr(
        all(miri, debug_assertions),
        ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): `Page::assert_integrity` is O(page) and runs on nearly every PageList mutation under `debug_assertions`, which puts this test out of Miri's budget in the debug-assertions-ON G6 lane. It RUNS there — UB-clean and leak-clean — in the G6c lane (`check_leaks.sh --debug-assertions-off`), measured per test in single-worker isolation; see the WS19 T6 per-test table. Do NOT widen this to a plain `miri` ignore: that would silently drop the test from BOTH lanes."
    )]
    #[test]
    fn select_after_partial_history_erase_drops_a_pruned_selection() {
        let alloc = Allocator;
        // PORT NOTE: 80 cols instead of the reference's 10. Nothing in this
        // test depends on the column count, and 10 cols yields ~35k-row pages
        // whose row-by-row fill costs ~30s in a debug Rust build.
        let mut t: Terminal = Terminal::init(alloc, terminal_opts(80, 2, usize::MAX)).unwrap();
        let active_ptr: *mut Screen = t.screens.active;
        // PORT NOTE (WS19 T6) — RE-DERIVE, NEVER CACHE. `Terminal::active_mut()`
        // (`&mut *self.screens.active`) is a Unique retag over the whole `Screen`
        // allocation, so it pops any raw pointer derived from that allocation
        // earlier; reading through a cached one afterwards is Stacked-Borrows UB
        // (memory: rust-mut-args-strongly-protected). This test used to cache it
        // across its `vt_stream()`/search writes. The violation was real and
        // deterministic, and had never been observed because a blanket
        // "multi-page PageList intractable" annotation kept the test out of Miri
        // entirely — found the moment the G6c (`-C debug-assertions=off`) lane
        // made it runnable. Test-code only: no production path caches this.
        let list = || -> *mut PageList { unsafe { &raw mut (*active_ptr).pages } };

        let mut stream = t.vt_stream();
        stream.next_slice(b"error\r\n");
        let first = unsafe { (*list()).pages.first };
        while unsafe { (*list()).total_pages() } < 3 {
            stream.next_slice(b"\r\n");
        }
        let first_rows = unsafe { crate::pagelist::Node::rows(first) };

        let mut search: ScreenSearch = ScreenSearch::init(alloc, active_ptr, b"error").unwrap();
        search.search_all().unwrap();
        assert_eq!(search.active_results.len(), 0);
        assert_eq!(search.history_results.len(), 1);
        assert!(search.select(Select::Next).unwrap());

        unsafe {
            (*list()).erase_history(Some(point::Point::History(point::Coordinate {
                x: 0,
                y: (first_rows - 1) as u32,
            })))
        };
        assert!(!unsafe { (*search.selected.as_ref().unwrap().highlight.start).garbage });

        assert!(!search.select(Select::Next).unwrap());
        assert!(search.selected.is_none());

        search.deinit();
        stream.deinit();
        t.deinit(alloc);
    }

    /// PORTS: oracle 86e1f7b8b
    /// `test "select after history compaction ignores replaced results"`
    #[cfg_attr(
        all(miri, debug_assertions),
        ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): `Page::assert_integrity` is O(page) and runs on nearly every PageList mutation under `debug_assertions`, which puts this test out of Miri's budget in the debug-assertions-ON G6 lane. It RUNS there — UB-clean and leak-clean — in the G6c lane (`check_leaks.sh --debug-assertions-off`), measured per test in single-worker isolation; see the WS19 T6 per-test table. Do NOT widen this to a plain `miri` ignore: that would silently drop the test from BOTH lanes."
    )]
    #[test]
    fn select_after_history_compaction_ignores_replaced_results() {
        let alloc = Allocator;
        // PORT NOTE: 80 cols instead of the reference's 10. Nothing in this
        // test depends on the column count, and 10 cols yields ~35k-row pages
        // whose row-by-row fill costs ~30s in a debug Rust build.
        let mut t: Terminal = Terminal::init(alloc, terminal_opts(80, 2, usize::MAX)).unwrap();
        let active_ptr: *mut Screen = t.screens.active;
        // PORT NOTE (WS19 T6) — RE-DERIVE, NEVER CACHE. `Terminal::active_mut()`
        // (`&mut *self.screens.active`) is a Unique retag over the whole `Screen`
        // allocation, so it pops any raw pointer derived from that allocation
        // earlier; reading through a cached one afterwards is Stacked-Borrows UB
        // (memory: rust-mut-args-strongly-protected). This test used to cache it
        // across its `vt_stream()`/search writes. The violation was real and
        // deterministic, and had never been observed because a blanket
        // "multi-page PageList intractable" annotation kept the test out of Miri
        // entirely — found the moment the G6c (`-C debug-assertions=off`) lane
        // made it runnable. Test-code only: no production path caches this.
        let list = || -> *mut PageList { unsafe { &raw mut (*active_ptr).pages } };

        let mut stream = t.vt_stream();
        stream.next_slice(b"error\r\n");
        let first = unsafe { (*list()).pages.first };
        while unsafe { (*list()).total_pages() } < 3 {
            stream.next_slice(b"\r\n");
        }
        unsafe {
            (*list())
                .split(Pin {
                    node: first,
                    y: crate::pagelist::Node::rows(first) / 2,
                    x: 0,
                    garbage: false,
                })
                .unwrap()
        };

        let mut search: ScreenSearch = ScreenSearch::init(alloc, active_ptr, b"error").unwrap();
        search.search_all().unwrap();
        assert_eq!(search.history_results.len(), 1);

        let replacement = unsafe { (*list()).compact(first) }.unwrap().unwrap();
        assert!(!core::ptr::eq(replacement, first));

        assert!(!search.select(Select::Next).unwrap());
        assert!(search.selected.is_none());

        search.deinit();
        stream.deinit();
        t.deinit(alloc);
    }

    /// PORTS: oracle 89eca063e
    /// `test "reload defers pruning unselected history results"`
    #[cfg_attr(
        all(miri, debug_assertions),
        ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): `Page::assert_integrity` is O(page) and runs on nearly every PageList mutation under `debug_assertions`, which puts this test out of Miri's budget in the debug-assertions-ON G6 lane. It RUNS there — UB-clean and leak-clean — in the G6c lane (`check_leaks.sh --debug-assertions-off`), measured per test in single-worker isolation; see the WS19 T6 per-test table. Do NOT widen this to a plain `miri` ignore: that would silently drop the test from BOTH lanes."
    )]
    #[test]
    fn reload_defers_pruning_unselected_history_results() {
        let alloc = Allocator;
        // PORT NOTE: 80 cols instead of the reference's 10. Nothing in this
        // test depends on the column count, and 10 cols yields ~35k-row pages
        // whose row-by-row fill costs ~30s in a debug Rust build.
        let mut t: Terminal = Terminal::init(alloc, terminal_opts(80, 2, usize::MAX)).unwrap();
        let active_ptr: *mut Screen = t.screens.active;
        // PORT NOTE (WS19 T6) — RE-DERIVE, NEVER CACHE. `Terminal::active_mut()`
        // (`&mut *self.screens.active`) is a Unique retag over the whole `Screen`
        // allocation, so it pops any raw pointer derived from that allocation
        // earlier; reading through a cached one afterwards is Stacked-Borrows UB
        // (memory: rust-mut-args-strongly-protected). This test used to cache it
        // across its `vt_stream()`/search writes. The violation was real and
        // deterministic, and had never been observed because a blanket
        // "multi-page PageList intractable" annotation kept the test out of Miri
        // entirely — found the moment the G6c (`-C debug-assertions=off`) lane
        // made it runnable. Test-code only: no production path caches this.
        let list = || -> *mut PageList { unsafe { &raw mut (*active_ptr).pages } };

        let mut stream = t.vt_stream();
        // const first = list.pages.first.?;
        // while (first.rows() < first.capacity().rows) stream.nextSlice("\r\n");
        // stream.nextSlice("error");
        // for (0..list.rows + 1) |_| stream.nextSlice("\r\n");
        let first = unsafe { (*list()).pages.first };
        while unsafe { crate::pagelist::Node::rows(first) }
            < unsafe { crate::pagelist::Node::capacity(first) }.rows
        {
            stream.next_slice(b"\r\n");
        }
        stream.next_slice(b"error");
        for _ in 0..unsafe { (*list()).rows } + 1 {
            stream.next_slice(b"\r\n");
        }

        let mut search: ScreenSearch = ScreenSearch::init(alloc, active_ptr, b"error").unwrap();
        search.search_all().unwrap();
        // try testing.expectEqual(1, search.history_results.items.len);
        assert_eq!(search.history_results.len(), 1);

        // list.eraseHistory(.{ .history = .{ .y = 0 } });
        unsafe {
            (*list()).erase_history(Some(point::Point::History(point::Coordinate {
                x: 0,
                y: 0,
            })))
        };

        // Routine active refreshes don't inspect unselected history results.
        // try search.reloadActive();
        // try testing.expectEqual(1, search.history_results.items.len);
        search.reload_active().unwrap();
        assert_eq!(search.history_results.len(), 1);

        // Selection prunes every candidate before attempting to track it.
        // try testing.expect(!try search.select(.next));
        // try testing.expectEqual(0, search.history_results.items.len);
        assert!(!search.select(Select::Next).unwrap());
        assert_eq!(search.history_results.len(), 0);

        search.deinit();
        stream.deinit();
        t.deinit(alloc);
    }

    /// PORTS: oracle a89c6133c
    /// `test "reload after partial history page erase drops shifted selection first"`
    #[cfg_attr(
        all(miri, debug_assertions),
        ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): `Page::assert_integrity` is O(page) and runs on nearly every PageList mutation under `debug_assertions`, which puts this test out of Miri's budget in the debug-assertions-ON G6 lane. It RUNS there — UB-clean and leak-clean — in the G6c lane (`check_leaks.sh --debug-assertions-off`), measured per test in single-worker isolation; see the WS19 T6 per-test table. Do NOT widen this to a plain `miri` ignore: that would silently drop the test from BOTH lanes."
    )]
    #[test]
    fn reload_after_partial_history_page_erase_drops_shifted_selection_first() {
        let alloc = Allocator;
        // PORT NOTE: 80 cols instead of the reference's 10. Nothing in this
        // test depends on the column count, and 10 cols yields ~35k-row pages
        // whose row-by-row fill costs ~30s in a debug Rust build.
        let mut t: Terminal = Terminal::init(alloc, terminal_opts(80, 2, usize::MAX)).unwrap();
        let active_ptr: *mut Screen = t.screens.active;
        // PORT NOTE (WS19 T6) — RE-DERIVE, NEVER CACHE. `Terminal::active_mut()`
        // (`&mut *self.screens.active`) is a Unique retag over the whole `Screen`
        // allocation, so it pops any raw pointer derived from that allocation
        // earlier; reading through a cached one afterwards is Stacked-Borrows UB
        // (memory: rust-mut-args-strongly-protected). This test used to cache it
        // across its `vt_stream()`/search writes. The violation was real and
        // deterministic, and had never been observed because a blanket
        // "multi-page PageList intractable" annotation kept the test out of Miri
        // entirely — found the moment the G6c (`-C debug-assertions=off`) lane
        // made it runnable. Test-code only: no production path caches this.
        let list = || -> *mut PageList { unsafe { &raw mut (*active_ptr).pages } };

        let mut stream = t.vt_stream();
        let first = unsafe { (*list()).pages.first };
        while unsafe { crate::pagelist::Node::rows(first) }
            < unsafe { crate::pagelist::Node::capacity(first) }.rows
        {
            stream.next_slice(b"\r\n");
        }
        stream.next_slice(b"error");
        for _ in 0..unsafe { (*list()).rows } + 1 {
            stream.next_slice(b"\r\n");
        }

        let mut search: ScreenSearch = ScreenSearch::init(alloc, active_ptr, b"error").unwrap();
        search.search_all().unwrap();
        // try testing.expect(try search.select(.next));
        // try testing.expect(search.selected != null);
        assert!(search.select(Select::Next).unwrap());
        assert!(search.selected.is_some());

        // list.eraseHistory(.{ .history = .{ .y = 0 } });
        unsafe {
            (*list()).erase_history(Some(point::Point::History(point::Coordinate {
                x: 0,
                y: 0,
            })))
        };

        // Advance the active boundary into another node and add matches so
        // reloadActive updates the history result offsets. It must discard the
        // stale selected result before comparing any of its captured coordinates.
        // const active_node = list.getTopLeft(.active).node;
        // while (list.getTopLeft(.active).node == active_node) {
        //     stream.nextSlice("error\r\n");
        // }
        let active_node = unsafe { (*list()).get_top_left(point::Tag::Active) }.node;
        while core::ptr::eq(
            unsafe { (*list()).get_top_left(point::Tag::Active) }.node,
            active_node,
        ) {
            stream.next_slice(b"error\r\n");
        }

        // try search.reloadActive();
        // try testing.expect(search.selected == null);
        search.reload_active().unwrap();
        assert!(search.selected.is_none());

        search.deinit();
        stream.deinit();
        t.deinit(alloc);
    }

    /// PORTS: oracle a804e3ac7
    /// `test "select after partial history page erase ignores shifted results"`
    #[cfg_attr(
        all(miri, debug_assertions),
        ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): `Page::assert_integrity` is O(page) and runs on nearly every PageList mutation under `debug_assertions`, which puts this test out of Miri's budget in the debug-assertions-ON G6 lane. It RUNS there — UB-clean and leak-clean — in the G6c lane (`check_leaks.sh --debug-assertions-off`), measured per test in single-worker isolation; see the WS19 T6 per-test table. Do NOT widen this to a plain `miri` ignore: that would silently drop the test from BOTH lanes."
    )]
    #[test]
    fn select_after_partial_history_page_erase_ignores_shifted_results() {
        let alloc = Allocator;
        // PORT NOTE: 80 cols instead of the reference's 10. Nothing in this
        // test depends on the column count, and 10 cols yields ~35k-row pages
        // whose row-by-row fill costs ~30s in a debug Rust build.
        let mut t: Terminal = Terminal::init(alloc, terminal_opts(80, 2, usize::MAX)).unwrap();
        let active_ptr: *mut Screen = t.screens.active;
        // PORT NOTE (WS19 T6) — RE-DERIVE, NEVER CACHE. `Terminal::active_mut()`
        // (`&mut *self.screens.active`) is a Unique retag over the whole `Screen`
        // allocation, so it pops any raw pointer derived from that allocation
        // earlier; reading through a cached one afterwards is Stacked-Borrows UB
        // (memory: rust-mut-args-strongly-protected). This test used to cache it
        // across its `vt_stream()`/search writes. The violation was real and
        // deterministic, and had never been observed because a blanket
        // "multi-page PageList intractable" annotation kept the test out of Miri
        // entirely — found the moment the G6c (`-C debug-assertions=off`) lane
        // made it runnable. Test-code only: no production path caches this.
        let list = || -> *mut PageList { unsafe { &raw mut (*active_ptr).pages } };

        let mut stream = t.vt_stream();
        let first = unsafe { (*list()).pages.first };
        while unsafe { crate::pagelist::Node::rows(first) }
            < unsafe { crate::pagelist::Node::capacity(first) }.rows
        {
            stream.next_slice(b"\r\n");
        }
        stream.next_slice(b"error");
        for _ in 0..unsafe { (*list()).rows } + 1 {
            stream.next_slice(b"\r\n");
        }
        assert!(!core::ptr::eq(first, unsafe { (*list()).pages.last }));

        let mut search: ScreenSearch = ScreenSearch::init(alloc, active_ptr, b"error").unwrap();
        search.search_all().unwrap();
        assert_eq!(search.active_results.len(), 0);
        assert_eq!(search.history_results.len(), 1);

        let old_rows = unsafe { crate::pagelist::Node::rows(first) };
        unsafe {
            (*list()).erase_history(Some(point::Point::History(point::Coordinate {
                x: 0,
                y: 0,
            })))
        };
        assert_eq!(old_rows - 1, unsafe { crate::pagelist::Node::rows(first) });

        assert!(!search.select(Select::Next).unwrap());
        assert!(search.selected.is_none());

        search.deinit();
        stream.deinit();
        t.deinit(alloc);
    }

    /// PORTS: oracle 91a63e528
    /// `test "select after history page split ignores moved results"`
    #[cfg_attr(
        all(miri, debug_assertions),
        ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): `Page::assert_integrity` is O(page) and runs on nearly every PageList mutation under `debug_assertions`, which puts this test out of Miri's budget in the debug-assertions-ON G6 lane. It RUNS there — UB-clean and leak-clean — in the G6c lane (`check_leaks.sh --debug-assertions-off`), measured per test in single-worker isolation; see the WS19 T6 per-test table. Do NOT widen this to a plain `miri` ignore: that would silently drop the test from BOTH lanes."
    )]
    #[test]
    fn select_after_history_page_split_ignores_moved_results() {
        let alloc = Allocator;
        // PORT NOTE: 80 cols instead of the reference's 10. Nothing in this
        // test depends on the column count, and 10 cols yields ~35k-row pages
        // whose row-by-row fill costs ~30s in a debug Rust build.
        let mut t: Terminal = Terminal::init(alloc, terminal_opts(80, 2, usize::MAX)).unwrap();
        let active_ptr: *mut Screen = t.screens.active;
        // PORT NOTE (WS19 T6) — RE-DERIVE, NEVER CACHE. `Terminal::active_mut()`
        // (`&mut *self.screens.active`) is a Unique retag over the whole `Screen`
        // allocation, so it pops any raw pointer derived from that allocation
        // earlier; reading through a cached one afterwards is Stacked-Borrows UB
        // (memory: rust-mut-args-strongly-protected). This test used to cache it
        // across its `vt_stream()`/search writes. The violation was real and
        // deterministic, and had never been observed because a blanket
        // "multi-page PageList intractable" annotation kept the test out of Miri
        // entirely — found the moment the G6c (`-C debug-assertions=off`) lane
        // made it runnable. Test-code only: no production path caches this.
        let list = || -> *mut PageList { unsafe { &raw mut (*active_ptr).pages } };

        let mut stream = t.vt_stream();
        let first = unsafe { (*list()).pages.first };
        while unsafe { crate::pagelist::Node::rows(first) }
            < unsafe { crate::pagelist::Node::capacity(first) }.rows
        {
            stream.next_slice(b"\r\n");
        }
        stream.next_slice(b"error");
        for _ in 0..unsafe { (*list()).rows } + 1 {
            stream.next_slice(b"\r\n");
        }

        let mut search: ScreenSearch = ScreenSearch::init(alloc, active_ptr, b"error").unwrap();
        search.search_all().unwrap();
        assert_eq!(search.history_results.len(), 1);

        unsafe {
            (*list())
                .split(Pin {
                    node: first,
                    y: crate::pagelist::Node::rows(first) / 2,
                    x: 0,
                    garbage: false,
                })
                .unwrap()
        };

        assert!(!search.select(Select::Next).unwrap());
        assert!(search.selected.is_none());

        search.deinit();
        stream.deinit();
        t.deinit(alloc);
    }

    // test "select after clearing scrollback"
    #[cfg_attr(
        all(miri, debug_assertions),
        ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): `Page::assert_integrity` is O(page) and runs on nearly every PageList mutation under `debug_assertions`, which puts this test out of Miri's budget in the debug-assertions-ON G6 lane. It RUNS there — UB-clean and leak-clean — in the G6c lane (`check_leaks.sh --debug-assertions-off`), measured per test in single-worker isolation; see the WS19 T6 per-test table. Do NOT widen this to a plain `miri` ignore: that would silently drop the test from BOTH lanes."
    )]
    #[test]
    fn select_after_clearing_scrollback() {
        // Regression test for: https://github.com/ghostty-org/ghostty/issues/11957
        // After clearing scrollback (CSI 3J), selecting next/prev should not crash.
        let alloc = Allocator;
        let mut t: Terminal = Terminal::init(alloc, terminal_opts(10, 2, usize::MAX)).unwrap();
        let active_ptr: *mut Screen = t.screens.active;

        let mut s = t.vt_stream();

        // Write enough content to push matches into scrollback history.
        s.next_slice(b"error\r\n");
        while unsafe { (*active_ptr).pages.total_pages() } < 3 {
            s.next_slice(b"error\r\n");
        }
        let rows = unsafe { (*active_ptr).pages.rows };
        for _ in 0..rows {
            s.next_slice(b"\r\n");
        }
        s.next_slice(b"error.");

        // Start search and find all matches.
        let mut search: ScreenSearch = ScreenSearch::init(alloc, active_ptr, b"error").unwrap();
        search.search_all().unwrap();

        // Should have matches in both history and active areas.
        assert!(!search.history_results.is_empty());
        assert!(!search.active_results.is_empty());

        // Select a match first (so we have a selection).
        let _ = search.select(Select::Next).unwrap();
        assert!(search.selected.is_some());

        // Clear scrollback (equivalent to CSI 3J / Cmd+K erasing scrollback).
        // t.eraseDisplay(.scrollback, false);
        s.deinit();
        t.erase_display(EraseDisplay::Scrollback, false);

        // Selecting next/prev after clearing scrollback should not crash.
        // Before the fix, this would hit an assertion in trackPin because
        // the FlattenedHighlight contained dangling node pointers.
        let _ = search.select(Select::Next).unwrap();
        let _ = search.select(Select::Prev).unwrap();

        search.deinit();
        t.deinit(alloc);
    }

    // Parity tail S7-6 (Rust-only): `matches(alloc)` is `alloc.alloc(
    // FlattenedHighlight, n)` plus shallow copies: ONE exact allocation through
    // the allocator it is given (the caller's, not the search's), whose
    // elements alias the stored results' chunks.
    #[test]
    fn screen_search_matches_is_one_allocation_through_the_given_allocator() {
        use crate::alloc::testing::{with_handle, CountingAllocator};

        let alloc = Allocator;
        let mut t: Terminal = Terminal::init(alloc, terminal_opts(10, 2, 10_000)).unwrap();
        let active_ptr: *mut Screen = t.screens.active;
        let mut s = t.vt_stream();
        s.next_slice(b"Fizz\r\nBuzz\r\nFizz\r\nBang");

        let mut search: ScreenSearch = ScreenSearch::init(alloc, active_ptr, b"Fizz").unwrap();
        search.search_all().unwrap();
        assert_eq!(2, search.matches_len());

        let counting = CountingAllocator::over_heap();
        let c = counting.allocator();
        with_handle(&c, |calloc| {
            let matches = search.matches(calloc).unwrap();
            assert_eq!(2, matches.len());
            assert_eq!(1, counting.alloc_calls.get());
            assert_eq!(
                2 * core::mem::size_of::<FlattenedRef<'_>>(),
                counting.live_bytes.get()
            );
            // Newest first: the last active result, then the first.
            assert!(core::ptr::eq(
                matches[0].chunks.as_ptr(),
                search.active_results[1].chunks.as_ptr()
            ));
            assert!(core::ptr::eq(
                matches[1].chunks.as_ptr(),
                search.active_results[0].chunks.as_ptr()
            ));
            drop(matches);
            assert_eq!(0, counting.live_bytes.get());
        });

        search.deinit();
        s.deinit();
        t.deinit(alloc);
    }

    /// PORTS: oracle 06178eeaa (N435)
    /// `test "feed after complete discovers prepended history pages"`
    #[cfg_attr(
        all(miri, debug_assertions),
        ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): `Page::assert_integrity` is O(page) and runs on nearly every PageList mutation under `debug_assertions`, which puts this test out of Miri's budget in the debug-assertions-ON G6 lane. It RUNS there — UB-clean and leak-clean — in the G6c lane (`check_leaks.sh --debug-assertions-off`), measured per test in single-worker isolation; see the WS19 T6 per-test table. Do NOT widen this to a plain `miri` ignore: that would silently drop the test from BOTH lanes."
    )]
    #[test]
    fn feed_after_complete_discovers_prepended_history_pages() {
        use crate::page::{Capacity, Cell};
        use crate::pagelist::builder::Location;
        use crate::pagelist::Node;
        use crate::screen::SelectionString;
        use crate::selection::Selection;

        let alloc = Allocator;
        // var t: Terminal = try .init(io, alloc, .{
        //     .cols = 10,
        //     .rows = 2,
        //     .max_scrollback_bytes = std.math.maxInt(usize),
        // });
        let mut t: Terminal = Terminal::init(alloc, terminal_opts(10, 2, usize::MAX)).unwrap();
        let active_ptr: *mut Screen = t.screens.active;
        // const list: *PageList = &t.screens.active.pages;
        //
        // SB: `list()` re-derives the pointer via `&raw mut` (no retag) each
        // time, rather than caching one `*mut PageList` across the
        // interleaved `s.next_slice`/`selection_string`/`search.*` calls
        // below that reborrow the whole `Screen` — the same idiom already
        // used elsewhere in this file (e.g. `erase_history` after `feed`
        // tests) and required here so a literal `&mut (*active_ptr).pages`
        // never invalidates `search`'s own internal PageList pointer
        // (rust-mut-args-strongly-protected).
        let list = || -> *mut PageList { unsafe { &raw mut (*active_ptr).pages } };

        let mut s = t.vt_stream();

        // The screen begins with the tail of the needle so a restored page which
        // soft-wraps into it can complete a match across the restore boundary.
        // s.nextSlice("st\r\nTest\r\n");
        s.next_slice(b"st\r\nTest\r\n");
        // while (list.totalPages() < 2) s.nextSlice("\r\n");
        while unsafe { (*list()).total_pages() } < 2 {
            s.next_slice(b"\r\n");
        }
        // for (0..list.rows) |_| s.nextSlice("\r\n");
        let rows = unsafe { (*list()).rows };
        for _ in 0..rows {
            s.next_slice(b"\r\n");
        }
        // s.nextSlice("Test");
        s.next_slice(b"Test");
        // const old_first = list.pages.first.?;
        let old_first = unsafe { (*list()).pages.first };

        // Search to completion, then select the oldest match so we can verify
        // that restored (older) results never move an existing selection.
        // var search: ScreenSearch = try .init(alloc, t.screens.active, "Test");
        let mut search: ScreenSearch = ScreenSearch::init(alloc, active_ptr, b"Test").unwrap();
        // try search.searchAll();
        search.search_all().unwrap();
        // try testing.expect(search.state.isComplete());
        assert!(search.state.is_complete());
        // try testing.expectEqual(1, search.active_results.items.len);
        assert_eq!(1, search.active_results.len());
        // try testing.expectEqual(1, search.history_results.items.len);
        assert_eq!(1, search.history_results.len());
        // try testing.expect(try search.select(.prev));
        assert!(search.select(Select::Prev).unwrap());
        // const selected_idx = search.selected.?.idx;
        let selected_idx = search.selected.as_ref().unwrap().idx;
        // try testing.expectEqual(1, selected_idx);
        assert_eq!(1, selected_idx);
        // const selected_before = search.selectedMatch().?.untracked();
        let selected_before = search.selected_match().unwrap().untracked();
        // try testing.expectEqual(old_first, selected_before.start.node);
        assert!(core::ptr::eq(selected_before.start.node, old_first));

        // Restore the newest history page: a full match on its first row and a
        // soft-wrapped last row ending in "Te" which joins the screen's "st".
        {
            // var allocation = try list.allocatePage(.{ .cols = 10, .rows = 2 });
            let mut allocation = unsafe {
                PageList::allocate_page(
                    list(),
                    Capacity {
                        cols: 10,
                        rows: 2,
                        ..Capacity::default()
                    },
                )
            }
            .unwrap();
            // defer allocation.deinit();
            // const page = allocation.page();
            let page = allocation.page();
            // page.size.rows = 2;
            unsafe {
                (*page).size.rows = 2;
            }
            // for ("Test", 0..) |c, x| page.getRowAndCell(x, 0).cell.* = .init(c);
            for (x, c) in b"Test".iter().enumerate() {
                unsafe {
                    *(*page).get_row_and_cell(x, 0).cell = Cell::init(*c as u32);
                }
            }
            // for ("xxxxxxxxTe", 0..) |c, x| page.getRowAndCell(x, 1).cell.* = .init(c);
            for (x, c) in b"xxxxxxxxTe".iter().enumerate() {
                unsafe {
                    *(*page).get_row_and_cell(x, 1).cell = Cell::init(*c as u32);
                }
            }
            // page.getRow(1).wrap = true;
            unsafe {
                (*(*page).get_row(1)).set_wrap(true);
            }
            // old_first.page().getRow(0).wrap_continuation = true;
            unsafe {
                (*(*Node::page(old_first)).get_row(0)).set_wrap_continuation(true);
            }
            // try allocation.finalize(.prepend);
            allocation.finalize(Location::Prepend).unwrap();
            allocation.deinit();
        }
        // const newest = list.pages.first.?;
        let newest = unsafe { (*list()).pages.first };

        // The completed search is fed like any periodic refresh. It must pick
        // the page up and go back to searching rather than staying complete.
        // try search.feed();
        search.feed().unwrap();
        // try testing.expect(!search.state.isComplete());
        assert!(!search.state.is_complete());
        // try search.searchAll();
        search.search_all().unwrap();
        // try testing.expectEqual(1, search.active_results.items.len);
        assert_eq!(1, search.active_results.len());
        // try testing.expectEqual(3, search.history_results.items.len);
        assert_eq!(3, search.history_results.len());

        // New results are older than everything cached, so they append in
        // newest-to-oldest order: the boundary match first, then the row above.
        {
            // const boundary = search.history_results.items[1].untracked();
            let boundary = search.history_results[1].untracked();
            // try testing.expectEqual(newest, boundary.start.node);
            assert!(core::ptr::eq(boundary.start.node, newest));
            // try testing.expectEqual(old_first, boundary.end.node);
            assert!(core::ptr::eq(boundary.end.node, old_first));
            // try testing.expectEqual(@as(size.CellCountInt, 1), boundary.start.y);
            assert_eq!(1, boundary.start.y);
            // try testing.expectEqual(@as(size.CellCountInt, 8), boundary.start.x);
            assert_eq!(8, boundary.start.x);
            // try testing.expectEqual(@as(size.CellCountInt, 0), boundary.end.y);
            assert_eq!(0, boundary.end.y);
            // try testing.expectEqual(@as(size.CellCountInt, 1), boundary.end.x);
            assert_eq!(1, boundary.end.x);
            // const str = try t.screens.active.selectionString(alloc, .{
            //     .sel = .init(boundary.start, boundary.end, false),
            // });
            let str = unsafe {
                (*active_ptr).selection_string(
                    alloc,
                    SelectionString {
                        sel: Selection::init(boundary.start, boundary.end, false),
                        trim: true,
                    },
                )
            }
            .unwrap();
            // defer alloc.free(str);
            // try testing.expectEqualStrings("Test", str);
            assert_eq!(b"Test".as_slice(), str.as_slice());
        }
        {
            // const oldest = search.history_results.items[2].untracked();
            let oldest = search.history_results[2].untracked();
            // try testing.expectEqual(newest, oldest.start.node);
            assert!(core::ptr::eq(oldest.start.node, newest));
            // try testing.expectEqual(@as(size.CellCountInt, 0), oldest.start.y);
            assert_eq!(0, oldest.start.y);
            // try testing.expectEqual(@as(size.CellCountInt, 0), oldest.start.x);
            assert_eq!(0, oldest.start.x);
        }

        // The selection kept both its index and its target.
        // try testing.expectEqual(selected_idx, search.selected.?.idx);
        assert_eq!(selected_idx, search.selected.as_ref().unwrap().idx);
        {
            // const selected_after = search.selectedMatch().?.untracked();
            let selected_after = search.selected_match().unwrap().untracked();
            // try testing.expect(selected_before.start.eql(selected_after.start));
            assert!(selected_before.start.eql(selected_after.start));
            // try testing.expect(selected_before.end.eql(selected_after.end));
            assert!(selected_before.end.eql(selected_after.end));
        }

        // Restore an even older page. Each incremental step is discovered from
        // the frontier left behind by the previous one.
        {
            // var allocation = try list.allocatePage(.{ .cols = 10, .rows = 2 });
            let mut allocation = unsafe {
                PageList::allocate_page(
                    list(),
                    Capacity {
                        cols: 10,
                        rows: 2,
                        ..Capacity::default()
                    },
                )
            }
            .unwrap();
            // defer allocation.deinit();
            // const page = allocation.page();
            let page = allocation.page();
            // page.size.rows = 2;
            unsafe {
                (*page).size.rows = 2;
            }
            // for ("Test", 0..) |c, x| page.getRowAndCell(x, 0).cell.* = .init(c);
            for (x, c) in b"Test".iter().enumerate() {
                unsafe {
                    *(*page).get_row_and_cell(x, 0).cell = Cell::init(*c as u32);
                }
            }
            // try allocation.finalize(.prepend);
            allocation.finalize(Location::Prepend).unwrap();
            allocation.deinit();
        }
        // const oldest_node = list.pages.first.?;
        let oldest_node = unsafe { (*list()).pages.first };
        // try search.feed();
        search.feed().unwrap();
        // try search.searchAll();
        search.search_all().unwrap();
        // try testing.expectEqual(4, search.history_results.items.len);
        assert_eq!(4, search.history_results.len());
        // try testing.expectEqual(
        //     oldest_node,
        //     search.history_results.items[3].untracked().start.node,
        // );
        assert!(core::ptr::eq(
            search.history_results[3].untracked().start.node,
            oldest_node
        ));
        // try testing.expectEqual(selected_idx, search.selected.?.idx);
        assert_eq!(selected_idx, search.selected.as_ref().unwrap().idx);
        {
            // const selected_after = search.selectedMatch().?.untracked();
            let selected_after = search.selected_match().unwrap().untracked();
            // try testing.expect(selected_before.start.eql(selected_after.start));
            assert!(selected_before.start.eql(selected_after.start));
            // try testing.expect(selected_before.end.eql(selected_after.end));
            assert!(selected_before.end.eql(selected_after.end));
        }

        // With nothing new, a feed on the complete search stays complete.
        // try search.feed();
        search.feed().unwrap();
        // try testing.expect(search.state.isComplete());
        assert!(search.state.is_complete());
        // try testing.expectEqual(5, search.matchesLen());
        assert_eq!(5, search.matches_len());

        search.deinit();
        s.deinit();
        t.deinit(alloc);
    }
}
