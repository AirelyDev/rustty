//! rustty-vt search/terminal
//!
//! PORTS: ghostty src/terminal/search/terminal.zig
//!
//! PORT STATUS: complete (phase-6 W2; oracle `149c9f562` + `f0c918fc4`).
//! Faithful port of `TerminalSearch`, the whole-terminal search orchestration
//! that `149c9f562` extracted from the search thread (`Thread.zig`'s private
//! `Search` struct — previously ported as `thread::Search` — moved here and
//! became public so libghostty's C API can drive it without the xev-bound
//! thread), plus its 6 tests. `f0c918fc4` made `deinit` accept a null terminal
//! so a search and its terminal may be freed in any order.

// const std = @import("std");
// const testing = std.testing;
// const Allocator = std.mem.Allocator;
// const point = @import("../point.zig");
// const FlattenedHighlight = @import("../highlight.zig").Flattened;
// const ScreenSet = @import("../ScreenSet.zig");
// const Terminal = @import("../Terminal.zig");
//
// const ScreenSearch = @import("screen.zig").ScreenSearch;
// const ViewportSearch = @import("viewport.zig").ViewportSearch;
use super::screen::ScreenSearch;
use super::viewport::ViewportSearch;
use crate::alloc::Allocator;
use crate::highlight::Flattened as FlattenedHighlight;
use crate::pagelist::OutOfMemory;
use crate::point;
use crate::screen_set::Key;
use crate::screen_set::Key as ScreenSetKey;
use crate::terminal::Terminal;

// const log = std.log.scoped(.search_terminal);
//
// PORT NOTE: the reference `log.warn` calls are diagnostics on non-fatal
// paths; we mirror them with `eprintln!` behind the same message strings (the
// convention used across this crate, e.g. search/screen.rs, search/thread.rs).

/// Searches for a needle within an entire Terminal, orchestrating a
/// ScreenSearch per live screen plus a viewport search for the active
/// screen.
///
/// This reconciles per-screen searchers against the terminal's active
/// ScreenSet, retains results across primary/alt screen switches, and
/// recovers from resize/reflow, resets, scrollback pruning, etc.
///
/// The main ownership contract is that this is safe to call concurrently
/// with Terminal IO as long as you're not calling a function that takes
/// a terminal as an argument. Each function has more specific details about
/// its behavior.
// pub const TerminalSearch = struct {
//
// PORT NOTE (SB/lifetimes): this struct holds NO pointer to the terminal —
// every function that reads it takes `t: *Terminal` as an argument (ported as
// a raw `*mut Terminal`, never `&mut Terminal`, per the crate's raw-ptr
// discipline for Page/Row/Cell/Screen re-read seams). The raw aliases it DOES
// hold are all owned by its members and point into the terminal's HEAP
// allocations, never into the `Terminal` struct itself:
//   - `screens[k].screen: *mut Screen` — the ScreenSet's boxed Screen for key
//     `k`; invalidated when the ScreenSet destroys/replaces that Screen, which
//     is exactly what `screen_generations` + `screen_is_valid` detect;
//   - the ScreenSearch's tracked pins / `PageListSearch.list` — into that
//     Screen's PageList pool; die with the Screen (hence
//     `deinit_screen_invalid`);
//   - `viewport` (fingerprint of the active PageList) and `viewport_matches`
//     (`Flattened` chunks carrying `*mut Node` + serial) — page-node aliases
//     that are validity-checked by serial on use, never dereferenced blindly.
pub struct TerminalSearch {
    /// Allocator used for all search state.
    // alloc: Allocator,
    pub alloc: Allocator,

    /// Active viewport search for the active screen.
    // viewport: ViewportSearch,
    pub viewport: ViewportSearch,

    /// The searchers for all the screens.
    // screens: std.EnumMap(ScreenSet.Key, ScreenSearch),
    //
    // PORT NOTE: the reference `std.EnumMap(ScreenSet.Key, ScreenSearch)` is
    // ported to the per-key `Option<ScreenSearch>` array used elsewhere in this
    // crate for `EnumMap` (see `ScreenSet.all`), keyed by `Key as usize`.
    pub screens: ScreenSearchMap,

    /// ScreenSet generations captured when each searcher was initialized.
    /// Allocators may reuse a destroyed Screen address, so pointer equality
    /// alone cannot distinguish replacement screens from stale handles.
    // screen_generations: std.EnumMap(ScreenSet.Key, usize),
    //
    // PORT NOTE: `std.EnumMap(ScreenSet.Key, usize)` — an EnumMap entry can be
    // ABSENT, and `screenIsValid` distinguishes absent from present, so this is
    // a per-key `Option<usize>` array (NOT `[usize; COUNT]` like
    // `ScreenSet.generations`, which is always populated).
    pub screen_generations: [Option<usize>; Key::COUNT],

    /// The active screen key as of the last feed. All single-screen
    /// reads (total matches, selection, etc.) resolve against this.
    // active_key: ScreenSet.Key,
    pub active_key: ScreenSetKey,

    /// True when the cached viewport matches are stale and must be
    /// recollected from the viewport searcher on the next
    /// `viewportMatches` call.
    // stale_viewport_matches: bool,
    pub stale_viewport_matches: bool,

    /// Cached viewport matches. The viewport search's sliding window
    /// drains on read, so results are collected once per viewport
    /// change and cached here.
    // viewport_matches: std.ArrayList(FlattenedHighlight),
    pub viewport_matches: Vec<FlattenedHighlight>,
}

/// Overall status of the search. See `status`.
// pub const Status = enum {
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    /// `tick` can make progress without terminal access.
    // running,
    Running,

    /// Blocked until the next `feed`. This is also the initial
    /// state, since a search that has never been fed has never
    /// seen the terminal.
    // feed_required,
    FeedRequired,

    /// Caught up with the terminal state as of the last feed.
    // complete,
    Complete,
}

/// Viewport scroll behavior applied by `select` when a match
/// becomes selected.
// pub const SelectScroll = enum {
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectScroll {
    /// Scroll so the match is visible, only if it is not already.
    // if_needed,
    IfNeeded,

    /// Never scroll.
    // none,
    None,
}

/// Result of `TerminalSearch.tick`.
// pub const Tick = enum {
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tick {
    /// All searches are complete.
    // complete,
    Complete,

    /// Progress was made on at least one screen.
    // progress,
    Progress,

    /// All incomplete searches are blocked on feed.
    // blocked,
    Blocked,
}

/// `std.EnumMap(ScreenSet.Key, ScreenSearch)` ported as a per-key array.
pub struct ScreenSearchMap {
    pub entries: [Option<ScreenSearch>; Key::COUNT],
}

impl ScreenSearchMap {
    // .init(.{})
    pub fn init() -> ScreenSearchMap {
        ScreenSearchMap {
            entries: [None, None],
        }
    }

    // self.screens.getPtr(key)
    pub fn get_ptr(&mut self, key: ScreenSetKey) -> Option<&mut ScreenSearch> {
        self.entries[key as usize].as_mut()
    }

    // self.screens.get(key)
    pub fn get(&self, key: ScreenSetKey) -> Option<&ScreenSearch> {
        self.entries[key as usize].as_ref()
    }

    // self.screens.contains(key)
    pub fn contains(&self, key: ScreenSetKey) -> bool {
        self.entries[key as usize].is_some()
    }

    // self.screens.put(key, value)
    pub fn put(&mut self, key: ScreenSetKey, value: ScreenSearch) {
        self.entries[key as usize] = Some(value);
    }

    // self.screens.remove(key)
    pub fn remove(&mut self, key: ScreenSetKey) {
        self.entries[key as usize] = None;
    }

    /// `std.EnumMap` iteration order — key index to key.
    pub fn key_from_index(key_i: usize) -> ScreenSetKey {
        match key_i {
            0 => ScreenSetKey::Primary,
            _ => ScreenSetKey::Alternate,
        }
    }
}

impl TerminalSearch {
    /// Initialize a search for the given needle. The needle is copied.
    ///
    /// This doesn't read any terminal state. The first `feed` does.
    // pub fn init(
    //     alloc: Allocator,
    //     needle_unowned: []const u8,
    // ) Allocator.Error!TerminalSearch {
    pub fn init(alloc: Allocator, needle_unowned: &[u8]) -> Result<TerminalSearch, OutOfMemory> {
        // var vp: ViewportSearch = try .init(alloc, needle_unowned);
        // errdefer vp.deinit();
        let mut vp: ViewportSearch = ViewportSearch::init(alloc, needle_unowned)?;

        // We use dirty tracking for active area changes. Start with it
        // dirty so the first change is re-searched.
        // vp.active_dirty = true;
        vp.active_dirty = Some(true);

        // return .{
        //     .alloc = alloc,
        //     .viewport = vp,
        //     .screens = .init(.{}),
        //     .screen_generations = .init(.{}),
        //     .active_key = .primary,
        //     .stale_viewport_matches = true,
        //     .viewport_matches = .empty,
        // };
        Ok(TerminalSearch {
            alloc,
            viewport: vp,
            screens: ScreenSearchMap::init(),
            screen_generations: [None, None],
            active_key: ScreenSetKey::Primary,
            stale_viewport_matches: true,
            viewport_matches: Vec::new(),
        })
    }

    /// Release all state. The terminal must be the same one given to
    /// every other call, or null if the terminal has already been
    /// deinitialized. When the terminal is alive this releases tracked
    /// pins held within it. When it is null, those pins died with the
    /// terminal's page storage, so only search-owned memory is freed.
    // pub fn deinit(self: *TerminalSearch, t_: ?*Terminal) void {
    //
    // PORT NOTE: `t_` is a raw `?*Terminal` → `Option<*mut Terminal>`; the body
    // dereferences it to read its `ScreenSet`, and the caller guarantees
    // exclusive terminal access (the search thread holds the shared mutex;
    // the C API's caller contract), matching the reference.
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    pub fn deinit(&mut self, t_: Option<*mut Terminal>) {
        // self.clearViewportMatches();
        self.clear_viewport_matches();
        // self.viewport_matches.deinit(self.alloc);
        self.viewport_matches = Vec::new();
        // self.viewport.deinit();
        self.viewport.deinit();
        // var it = self.screens.iterator();
        // while (it.next()) |entry| {
        for key_i in 0..Key::COUNT {
            let entry_key = ScreenSearchMap::key_from_index(key_i);
            // The reference's `screenIsValid` takes `*const ScreenSearch` only
            // to read `search.screen`; passing that pointer directly keeps the
            // `&self` borrow disjoint from the `&mut` entry borrow below.
            let entry_screen = match self.screens.entries[key_i].as_ref() {
                Some(ss) => ss.screen,
                None => continue,
            };
            // const valid = if (t_) |t| self.screenIsValid(
            //     &t.screens,
            //     entry.key,
            //     entry.value,
            // ) else false;
            //
            // SB: raw `addr_of!` (reference `&t.screens`), NOT `&(*t).screens` —
            // the searchers hold live raw aliases into these screens' PageLists,
            // so no reference into the Terminal is formed here
            // (rust-mut-args-strongly-protected).
            let valid = match t_ {
                Some(t) => self.screen_is_valid(
                    unsafe { core::ptr::addr_of!((*t).screens) },
                    entry_key,
                    entry_screen,
                ),
                None => false,
            };
            let entry = self.screens.entries[key_i].as_mut().unwrap();
            // if (valid) {
            //     entry.value.deinit();
            // } else {
            //     entry.value.deinitScreenInvalid();
            // }
            if valid {
                entry.deinit();
            } else {
                entry.deinit_screen_invalid();
            }
        }
    }

    // fn screenIsValid(
    //     self: *const TerminalSearch,
    //     screens: *const ScreenSet,
    //     key: ScreenSet.Key,
    //     search: *const ScreenSearch,
    // ) bool {
    //
    // PORT NOTE: takes the searcher's `screen` pointer rather than the whole
    // `*const ScreenSearch` — that field is all the reference reads, and this
    // keeps the borrow disjoint from the caller's `&mut` on the same entry.
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    fn screen_is_valid(
        &self,
        screens: *const crate::screen_set::ScreenSet,
        key: ScreenSetKey,
        search_screen: *mut crate::screen::Screen,
    ) -> bool {
        // const generation = self.screen_generations.get(key) orelse return false;
        let generation = match self.screen_generations[key as usize] {
            Some(g) => g,
            None => return false,
        };
        // if (generation != screens.generation(key)) return false;
        if generation != unsafe { (*screens).generation(key) } {
            return false;
        }
        // const actual = screens.get(key) orelse return false;
        let actual = match unsafe { (*screens).get(key) } {
            Some(a) => a,
            None => return false,
        };
        // return actual == search.screen;
        core::ptr::eq(actual, search_screen)
    }

    /// The needle that this search is using, borrowed.
    // pub fn needle(self: *const TerminalSearch) []const u8 {
    pub fn needle(&self) -> &[u8] {
        // return self.viewport.needle();
        self.viewport.needle()
    }

    /// The searcher for the active screen as of the last feed. Null if
    /// the search has never been fed (or the screen failed to
    /// initialize).
    // pub fn activeScreenSearch(self: *TerminalSearch) ?*ScreenSearch {
    pub fn active_screen_search(&mut self) -> Option<&mut ScreenSearch> {
        // return self.screens.getPtr(self.active_key);
        self.screens.get_ptr(self.active_key)
    }

    /// Returns true if all searches on all screens are complete.
    // pub fn isComplete(self: *TerminalSearch) bool {
    pub fn is_complete(&self) -> bool {
        // var it = self.screens.iterator();
        // while (it.next()) |entry| {
        //     if (!entry.value.state.isComplete()) return false;
        // }
        for s in self.screens.entries.iter().flatten() {
            if !s.state.is_complete() {
                return false;
            }
        }

        // return true;
        true
    }

    /// The overall search status derived from the per-screen search
    /// states.
    ///
    /// Unlike `isComplete`, a search that has never been fed reports
    /// `feed_required` rather than `complete`. An empty screen map
    /// means the search has never seen the terminal, so reporting it
    /// as complete would be technically true but useless to a caller
    /// deciding what to do next.
    // pub fn status(self: *TerminalSearch) Status {
    pub fn status(&self) -> Status {
        // var saw_any = false;
        let mut saw_any = false;
        // var result: Status = .complete;
        let mut result: Status = Status::Complete;
        // var it = self.screens.iterator();
        // while (it.next()) |entry| {
        for entry in self.screens.entries.iter().flatten() {
            // saw_any = true;
            saw_any = true;
            // switch (entry.value.state) {
            match entry.state {
                // Progress is possible without a feed.
                // .active, .history => return .running,
                super::screen::State::Active | super::screen::State::History => {
                    return Status::Running
                }

                // Blocked until fed.
                // .history_feed => result = .feed_required,
                super::screen::State::HistoryFeed => result = Status::FeedRequired,

                // .complete => {},
                super::screen::State::Complete => {}
            }
        }

        // if (!saw_any) return .feed_required;
        if !saw_any {
            return Status::FeedRequired;
        }
        // return result;
        result
    }

    /// Tick the search forward as much as possible without reading
    /// any terminal state, so this is safe to call concurrently with
    /// terminal IO. Returns the overall tick progress.
    // pub fn tick(self: *TerminalSearch) Tick {
    pub fn tick(&mut self) -> Tick {
        // var result: Tick = .complete;
        let mut result: Tick = Tick::Complete;
        // var it = self.screens.iterator();
        // while (it.next()) |entry| {
        for key_i in 0..Key::COUNT {
            let entry_key = ScreenSearchMap::key_from_index(key_i);
            let entry = match self.screens.entries[key_i].as_mut() {
                Some(e) => e,
                None => continue,
            };
            // if (entry.value.tick()) {
            //     result = .progress;
            // } else |err| switch (err) {
            match entry.tick() {
                Ok(()) => {
                    result = Tick::Progress;
                }
                // Ignore... nothing we can do.
                // error.OutOfMemory => log.warn(
                //     "error ticking screen search key={} err={}",
                //     .{ entry.key, err },
                // ),
                Err(super::screen::TickError::OutOfMemory) => {
                    eprintln!(
                        "warning(search_terminal): error ticking screen search key={:?} err={:?}",
                        entry_key,
                        super::screen::TickError::OutOfMemory
                    );
                }

                // Ignore, good for us. State remains whatever it is.
                // error.SearchComplete => {},
                Err(super::screen::TickError::SearchComplete) => {}

                // Ignore, too, progressed
                // error.FeedRequired => switch (result) {
                Err(super::screen::TickError::FeedRequired) => match result {
                    // If we think we're complete, we're not because we're
                    // blocked now (nothing made progress).
                    // .complete => result = .blocked,
                    Tick::Complete => result = Tick::Blocked,

                    // If we made some progress, we remain in progress
                    // since blocked means no progress at all.
                    // .progress => {},
                    Tick::Progress => {}

                    // If we're blocked already then we remain blocked.
                    // .blocked => {},
                    Tick::Blocked => {}
                },
            }
        }

        // return result;
        result
    }

    /// Read the terminal to update any search state that requires it,
    /// such as reconciling screens, feeding more data to the
    /// searchers, and detecting viewport changes.
    ///
    /// This reads the terminal, so the caller must ensure the terminal
    /// isn't modified for the duration of this call (e.g. by holding a
    /// lock). Feeding is also the only way the search learns about
    /// terminal changes, so callers should feed periodically while a
    /// search is in use, even after it reports complete.
    ///
    /// `active_dirty` should be true when the active area may have
    /// changed since the last feed, which triggers a re-scan of the
    /// active area. Callers that know when the active area changes
    /// (e.g. Ghostty's renderer-maintained dirty flag) can pass false
    /// to skip the re-scan. Callers without that knowledge should
    /// always pass true. That is always correct, just slightly more
    /// work, and the active area search is cheap by design.
    // pub fn feed(
    //     self: *TerminalSearch,
    //     t: *Terminal,
    //     active_dirty: bool,
    // ) void {
    // PORT NOTE: `t` is a raw pointer (reference `*Terminal`); the body
    // dereferences it under the caller's exclusive-access guarantee, so the
    // lint is allowed (the convention used across this crate for
    // `*Terminal`/`*Screen` args).
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    pub fn feed(&mut self, t: *mut Terminal, active_dirty: bool) {
        // const alloc = self.alloc;
        let alloc = self.alloc;

        // Update our active screen
        // if (t.screens.active_key != self.active_key) {
        //     self.active_key = t.screens.active_key;
        // }
        if unsafe { (*t).screens.active_key } != self.active_key {
            self.active_key = unsafe { (*t).screens.active_key };
        }

        // Reconcile our screens with the terminal screens. Remove
        // searchers for screens that no longer exist and add searchers
        // for screens that do exist but we don't have yet.
        {
            // Remove screens we have that no longer exist or changed.
            // var it = self.screens.iterator();
            // while (it.next()) |entry| {
            for key_i in 0..Key::COUNT {
                let entry_key = ScreenSearchMap::key_from_index(key_i);
                let entry_screen = match self.screens.entries[key_i].as_ref() {
                    Some(ss) => ss.screen,
                    None => continue,
                };
                // const remove = !self.screenIsValid(
                //     &t.screens,
                //     entry.key,
                //     entry.value,
                // );
                // SB: raw `addr_of!`, NOT `&(*t).screens` — see `deinit`.
                let remove: bool = !self.screen_is_valid(
                    unsafe { core::ptr::addr_of!((*t).screens) },
                    entry_key,
                    entry_screen,
                );

                // if (remove) {
                if remove {
                    // entry.value.deinitScreenInvalid();
                    // _ = self.screens.remove(entry.key);
                    // _ = self.screen_generations.remove(entry.key);
                    //
                    // The screen this searcher was built on is gone or was
                    // replaced, so its tracked pins were freed with that
                    // screen's PageList pool and must not be untracked.
                    self.screens.entries[key_i]
                        .as_mut()
                        .unwrap()
                        .deinit_screen_invalid();
                    self.screens.remove(entry_key);
                    self.screen_generations[key_i] = None;
                }
            }
        }
        {
            // Add screens that exist but we don't have yet.
            // var it = t.screens.all.iterator();
            // while (it.next()) |entry| {
            for key_i in 0..Key::COUNT {
                let entry_key = ScreenSearchMap::key_from_index(key_i);
                let entry_value = match unsafe { (*t).screens.all[key_i] } {
                    Some(v) => v,
                    None => continue,
                };
                // if (self.screens.contains(entry.key)) continue;
                if self.screens.contains(entry_key) {
                    continue;
                }
                // const screen_search = ScreenSearch.init(
                //     alloc,
                //     entry.value.*,
                //     self.viewport.needle(),
                // ) catch |err| switch (err) {
                let needle_owned: Vec<u8> = self.viewport.needle().to_vec();
                match ScreenSearch::init(alloc, entry_value, &needle_owned) {
                    // self.screens.put(entry.key, screen_search);
                    // self.screen_generations.put(
                    //     entry.key,
                    //     t.screens.generation(entry.key),
                    // );
                    Ok(ss) => {
                        self.screens.put(entry_key, ss);
                        self.screen_generations[key_i] =
                            Some(unsafe { (*t).screens.generation(entry_key) });
                    }
                    // error.OutOfMemory => {
                    //     // OOM is probably going to sink the entire ship but
                    //     // we can just ignore it and wait on the next
                    //     // reconciliation to try again.
                    //     log.warn(
                    //         "error initializing screen search for key={} err={}",
                    //         .{ entry.key, err },
                    //     );
                    //     continue;
                    // },
                    Err(err) => {
                        eprintln!(
                            "warning(search_terminal): error initializing screen search for key={:?} err={:?}",
                            entry_key, err
                        );
                        continue;
                    }
                }
            }
        }

        // The caller told us the active area may have changed, so mark
        // our viewport searcher dirty (forcing a re-search) and reload
        // the active area for the active screen.
        // if (active_dirty) {
        if active_dirty {
            // Mark our viewport dirty so it researches the active
            // self.viewport.active_dirty = true;
            self.viewport.active_dirty = Some(true);

            // Reload our active area for our active screen
            // if (self.screens.getPtr(t.screens.active_key)) |screen_search| {
            let active_key = unsafe { (*t).screens.active_key };
            if let Some(screen_search) = self.screens.get_ptr(active_key) {
                // screen_search.reloadActive() catch |err| switch (err) {
                //     error.OutOfMemory => log.warn(
                //         "error reloading active area for screen key={} err={}",
                //         .{ t.screens.active_key, err },
                //     ),
                // };
                if let Err(err) = screen_search.reload_active() {
                    eprintln!(
                        "warning(search_terminal): error reloading active area for screen key={:?} err={:?}",
                        active_key, err
                    );
                }
            }
        }

        // Check our viewport for changes.
        // if (self.viewport.update(&t.screens.active.pages)) |updated| {
        //     if (updated) self.stale_viewport_matches = true;
        // } else |err| switch (err) {
        //     error.OutOfMemory => log.warn(
        //         "error updating viewport search err={}",
        //         .{err},
        //     ),
        // }
        let active_pages: *const crate::pagelist::PageList =
            unsafe { &(*(*t).screens.active).pages };
        match self.viewport.update(unsafe { &*active_pages }) {
            Ok(updated) => {
                if updated {
                    self.stale_viewport_matches = true;
                }
            }
            Err(err) => {
                eprintln!(
                    "warning(search_terminal): error updating viewport search err={:?}",
                    err
                );
            }
        }

        // Feed data
        // var it = self.screens.iterator();
        // while (it.next()) |entry| {
        for key_i in 0..Key::COUNT {
            let entry_key = ScreenSearchMap::key_from_index(key_i);
            let entry = match self.screens.entries[key_i].as_mut() {
                Some(e) => e,
                None => continue,
            };
            // if (entry.value.state.needsFeed()) {
            if entry.state.needs_feed() {
                // entry.value.feed() catch |err| switch (err) {
                //     error.OutOfMemory => log.warn(
                //         "error feeding screen search key={} err={}",
                //         .{ entry.key, err },
                //     ),
                // };
                if let Err(err) = entry.feed() {
                    eprintln!(
                        "warning(search_terminal): error feeding screen search key={:?} err={:?}",
                        entry_key, err
                    );
                }
            }
        }
    }

    /// Return the matches on the pages covering the viewport, as of
    /// the last feed. Note this can include matches slightly outside
    /// the visible viewport when they share a page with it.
    ///
    /// The results are cached, since the underlying viewport search
    /// drains as it is read. Matches are collected once after a feed
    /// notices a viewport change and the cached results are returned
    /// otherwise. The returned slice is owned by the search and valid
    /// until the next call to this function, the next feed, or deinit.
    ///
    /// This doesn't read any terminal state, so it is safe to call
    /// concurrently with terminal IO.
    // pub fn viewportMatches(
    //     self: *TerminalSearch,
    // ) Allocator.Error![]const FlattenedHighlight {
    pub fn viewport_matches(&mut self) -> Result<&[FlattenedHighlight], OutOfMemory> {
        // if (!self.stale_viewport_matches) return self.viewport_matches.items;
        if !self.stale_viewport_matches {
            return Ok(&self.viewport_matches);
        }

        // We always mark the cache fresh, even if collection fails
        // below: a failed collection isn't retried until the next feed
        // marks the cache stale again.
        // self.stale_viewport_matches = false;
        self.stale_viewport_matches = false;

        // self.clearViewportMatches();
        self.clear_viewport_matches();
        // errdefer {
        //     self.clearViewportMatches();
        //
        //     // Reset the viewport so we force an update (and therefore a
        //     // re-collection) on the next feed.
        //     self.viewport.reset();
        // }
        // while (self.viewport.next()) |hl| {
        while let Some(hl) = self.viewport.next() {
            // var hl_cloned = try hl.clone(self.alloc);
            // errdefer hl_cloned.deinit(self.alloc);
            // try self.viewport_matches.append(self.alloc, hl_cloned);
            //
            // PORT NOTE: the reference's `errdefer` cleanup (clear the cache and
            // `viewport.reset()`) fires when either the clone or the append
            // fails with OutOfMemory. `Flattened::clone` carries the fallible
            // signature, so the error arm is realized explicitly here;
            // `Vec::push` aborts on allocation failure rather than returning
            // an error, so only the clone can take it.
            let hl_cloned = match hl.clone() {
                Ok(c) => c,
                Err(err) => {
                    // -- errdefer --
                    self.clear_viewport_matches();

                    // Reset the viewport so we force an update (and therefore a
                    // re-collection) on the next feed.
                    self.viewport.reset();
                    return Err(err);
                }
            };
            self.viewport_matches.push(hl_cloned);
        }

        // return self.viewport_matches.items;
        Ok(&self.viewport_matches)
    }

    // fn clearViewportMatches(self: *TerminalSearch) void {
    fn clear_viewport_matches(&mut self) {
        // for (self.viewport_matches.items) |*hl| hl.deinit(self.alloc);
        for hl in self.viewport_matches.iter_mut() {
            hl.deinit();
        }
        // self.viewport_matches.clearRetainingCapacity();
        self.viewport_matches.clear();
    }

    /// Select the next or previous search result on the active screen,
    /// wrapping around at the ends, and optionally scrolling the
    /// viewport so the newly selected match is visible.
    ///
    /// This feeds first so the selection always works against current
    /// terminal state, making it safe to call at any time relative to
    /// feeds. Like `feed`, this reads (and possibly scrolls) the
    /// terminal, so the caller must ensure the terminal isn't
    /// otherwise being used for the duration of this call.
    ///
    /// Returns true if a match is selected after the operation, or
    /// false if there are no matches to select.
    // pub fn select(
    //     self: *TerminalSearch,
    //     t: *Terminal,
    //     to: ScreenSearch.Select,
    //     scroll: SelectScroll,
    // ) Allocator.Error!bool {
    // PORT NOTE: dereferences the raw `t`/`screen` pointers under the caller's
    // exclusive-access guarantee; lint allowed per the crate `*Terminal`/
    // `*Screen` convention. The `for i in 0..hl_chunks.len()` mirrors the
    // reference `for (0..hl_chunks.len) |i|` verbatim.
    #[allow(clippy::not_unsafe_ptr_arg_deref, clippy::needless_range_loop)]
    pub fn select(
        &mut self,
        t: *mut Terminal,
        to: super::screen::Select,
        scroll: SelectScroll,
    ) -> Result<bool, OutOfMemory> {
        // A screen can be removed or replaced between feeds. Reconcile
        // while holding the terminal lock before touching any
        // ScreenSearch pins.
        // self.feed(t, false);
        self.feed(t, false);
        // const screen_search = self.screens.getPtr(self.active_key) orelse
        //     return false;
        let active_key = self.active_key;
        let screen_search = match self.screens.get_ptr(active_key) {
            Some(ss) => ss,
            None => return Ok(false),
        };

        // Make the selection. Ignore the result because we don't
        // care if the selection didn't change.
        // _ = try screen_search.select(to);
        let _ = screen_search.select(to)?;

        // Grab our match if we have one. If we don't have a selection
        // then there was nothing to select.
        // const flattened = screen_search.selectedMatch() orelse return false;
        let flattened = match screen_search.selected_match() {
            Some(f) => f,
            None => return Ok(false),
        };

        // switch (scroll) {
        //     .none => return true,
        //     .if_needed => {},
        // }
        match scroll {
            SelectScroll::None => return Ok(true),
            SelectScroll::IfNeeded => {}
        }

        // Grab the current screen and see if this match is visible within
        // the viewport already. If it is, we do nothing.
        // const screen = t.screens.get(self.active_key) orelse return true;
        let screen: *mut crate::screen::Screen = match unsafe { (*t).screens.get(active_key) } {
            Some(s) => s,
            None => return Ok(true),
        };

        // Grab the viewport. Viewports and selections are usually small
        // so this check isn't very expensive, despite appearing O(N^2),
        // both Ns are usually equal to 1.
        // var it = screen.pages.pageIterator(
        //     .right_down,
        //     .{ .viewport = .{} },
        //     null,
        // );
        let mut it = unsafe {
            (*screen).pages.page_iterator(
                crate::pagelist::iter::Direction::RightDown,
                point::Point::Viewport(point::Coordinate { x: 0, y: 0 }),
                None,
            )
        };
        // const hl_chunks = flattened.chunks.slice();
        let hl_chunks = &flattened.chunks;
        // while (it.next()) |chunk| {
        while let Some(chunk) = it.next() {
            // for (0..hl_chunks.len) |i| {
            for i in 0..hl_chunks.len() {
                // const hl_chunk = hl_chunks.get(i);
                let hl_chunk = &hl_chunks[i];
                // if (chunk.overlaps(.{
                //     .node = hl_chunk.node,
                //     .start = hl_chunk.start,
                //     .end = hl_chunk.end,
                // })) return true;
                if chunk.overlaps(crate::pagelist::iter::Chunk {
                    node: hl_chunk.node,
                    start: hl_chunk.start,
                    end: hl_chunk.end,
                }) {
                    return Ok(true);
                }
            }
        }

        // screen.scroll(.{ .pin = flattened.startPin() });
        unsafe { (*screen).scroll(crate::screen::Scroll::Pin(flattened.start_pin())) };
        // return true;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::{DynamicPalette, DynamicRGB};
    use crate::kitty;
    use crate::modes as modespkg;
    use crate::point::Point;
    use crate::search::screen::Select;
    use crate::terminal::{Colors, Options as TerminalOptions, Terminal};

    // PORT NOTE: see search/screen.rs — supplies the Zig `Options` field defaults
    // the reference relies on implicitly for `Terminal.init`.
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
            kitty_image_storage_limit: 320 * 1000 * 1000,
            kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
        }
    }

    // PORT NOTE (SB): every test below reaches the terminal ONLY through a raw
    // `t_ptr` (`addr_of_mut!(t)`) once the search exists — the same discipline
    // as `thread.rs`'s `select_after_active_screen_removal`. The search's
    // `feed`/`select` read through that pointer's tag; a `&mut t` reborrow of
    // the local itself is a Unique retag that would pop it (Stacked Borrows
    // UB, memory: shaper-hook-sb-clean-interleave). The reference's single
    // long-lived `stream` is kept: `vt_stream` binds a raw `*mut Terminal`
    // (`Handler::init(self_ptr)`) derived from `t_ptr`, which survives the
    // search's reads through the parent pointer.

    // test "starts feed required and runs to complete" {
    #[test]
    fn starts_feed_required_and_runs_to_complete() {
        // const alloc = testing.allocator;
        let alloc = Allocator;
        // var t: Terminal = try .init(io, alloc, .{ .cols = 10, .rows = 2 });
        // defer t.deinit(alloc);
        let mut t: Terminal = Terminal::init(alloc, terminal_opts(10, 2, 10_000)).unwrap();
        let t_ptr: *mut Terminal = core::ptr::addr_of_mut!(t);

        // var stream = t.vtStream();
        // defer stream.deinit();
        // stream.nextSlice("Fizz\r\nBuzz\r\nFizz\r\nBang");
        let mut stream = unsafe { (*t_ptr).vt_stream() };
        stream.next_slice(b"Fizz\r\nBuzz\r\nFizz\r\nBang");

        // var search: TerminalSearch = try .init(alloc, "Fizz");
        // defer search.deinit(&t);
        let mut search: TerminalSearch = TerminalSearch::init(alloc, b"Fizz").unwrap();

        // A fresh search has never seen the terminal, so it must report
        // feed_required even though isComplete() is trivially true.
        // try testing.expect(search.isComplete());
        assert!(search.is_complete());
        // try testing.expectEqual(TerminalSearch.Status.feed_required, search.status());
        assert_eq!(Status::FeedRequired, search.status());

        // Pump until complete.
        // while (search.status() != .complete) {
        while search.status() != Status::Complete {
            // switch (search.status()) {
            //     .feed_required => search.feed(&t, true),
            //     .running => _ = search.tick(),
            //     .complete => unreachable,
            // }
            match search.status() {
                Status::FeedRequired => search.feed(t_ptr, true),
                Status::Running => {
                    let _ = search.tick();
                }
                Status::Complete => unreachable!(),
            }
        }

        // const screen_search = search.activeScreenSearch().?;
        let screen_search = search.active_screen_search().unwrap();
        // try testing.expectEqual(2, screen_search.matchesLen());
        assert_eq!(2, screen_search.matches_len());

        // defer search.deinit(&t); defer stream.deinit(); defer t.deinit(alloc);
        search.deinit(Some(t_ptr));
        stream.deinit();
        unsafe { (*t_ptr).deinit(alloc) };
    }

    // test "viewport matches are cached until the next feed" {
    #[test]
    fn viewport_matches_are_cached_until_the_next_feed() {
        // const alloc = testing.allocator;
        let alloc = Allocator;
        // var t: Terminal = try .init(io, alloc, .{ .cols = 10, .rows = 10 });
        // defer t.deinit(alloc);
        let mut t: Terminal = Terminal::init(alloc, terminal_opts(10, 10, 10_000)).unwrap();
        let t_ptr: *mut Terminal = core::ptr::addr_of_mut!(t);

        // var stream = t.vtStream();
        // defer stream.deinit();
        // stream.nextSlice("Fizz\r\nBuzz\r\nFizz\r\nBang");
        let mut stream = unsafe { (*t_ptr).vt_stream() };
        stream.next_slice(b"Fizz\r\nBuzz\r\nFizz\r\nBang");

        // var search: TerminalSearch = try .init(alloc, "Fizz");
        // defer search.deinit(&t);
        // search.feed(&t, true);
        let mut search: TerminalSearch = TerminalSearch::init(alloc, b"Fizz").unwrap();
        search.feed(t_ptr, true);

        // The sliding window drains on read, so a second read must return
        // the cached results rather than an empty list.
        // try testing.expectEqual(2, (try search.viewportMatches()).len);
        assert_eq!(2, search.viewport_matches().unwrap().len());
        // try testing.expectEqual(2, (try search.viewportMatches()).len);
        assert_eq!(2, search.viewport_matches().unwrap().len());

        // A feed that observes a change refreshes the cache.
        // stream.nextSlice("\r\nFizz");
        stream.next_slice(b"\r\nFizz");
        // search.feed(&t, true);
        search.feed(t_ptr, true);
        // try testing.expectEqual(3, (try search.viewportMatches()).len);
        assert_eq!(3, search.viewport_matches().unwrap().len());

        // defer search.deinit(&t); defer stream.deinit(); defer t.deinit(alloc);
        search.deinit(Some(t_ptr));
        stream.deinit();
        unsafe { (*t_ptr).deinit(alloc) };
    }

    /// PORTS: oracle 149c9f562 (moved from `Thread.zig`, where 73ac36fa5
    /// introduced it against the thread's private `Search`).
    // test "select after active screen removal" {
    #[test]
    fn select_after_active_screen_removal() {
        // const alloc = testing.allocator;
        let alloc = Allocator;
        // var t: Terminal = try .init(io, alloc, .{ .cols = 20, .rows = 2 });
        // defer t.deinit(alloc);
        let mut t: Terminal = Terminal::init(alloc, terminal_opts(20, 2, 10_000)).unwrap();
        let t_ptr: *mut Terminal = core::ptr::addr_of_mut!(t);

        // _ = try t.switchScreen(.alternate);
        unsafe { (*t_ptr).switch_screen(ScreenSetKey::Alternate) }.unwrap();

        // var search: TerminalSearch = try .init(alloc, "needle");
        // defer search.deinit(&t);
        // search.feed(&t, false);
        let mut search: TerminalSearch = TerminalSearch::init(alloc, b"needle").unwrap();
        search.feed(t_ptr, false);
        // try testing.expectEqual(ScreenSet.Key.alternate, search.active_key);
        assert_eq!(ScreenSetKey::Alternate, search.active_key);
        // try testing.expect(search.screens.contains(.alternate));
        assert!(search.screens.contains(ScreenSetKey::Alternate));

        // _ = try t.switchScreen(.primary);
        // t.screens.remove(alloc, .alternate);
        unsafe { (*t_ptr).switch_screen(ScreenSetKey::Primary) }.unwrap();
        unsafe { (*t_ptr).screens.remove(alloc, ScreenSetKey::Alternate) };

        // The select must reconcile against the live ScreenSet before
        // touching any pins from the removed screen.
        // _ = try search.select(&t, .next, .if_needed);
        let _ = search
            .select(t_ptr, Select::Next, SelectScroll::IfNeeded)
            .unwrap();
        // try testing.expectEqual(ScreenSet.Key.primary, search.active_key);
        assert_eq!(ScreenSetKey::Primary, search.active_key);
        // try testing.expect(!search.screens.contains(.alternate));
        assert!(!search.screens.contains(ScreenSetKey::Alternate));

        // defer search.deinit(&t); defer t.deinit(alloc);
        search.deinit(Some(t_ptr));
        unsafe { (*t_ptr).deinit(alloc) };
    }

    // test "select scrolls the viewport only when needed" {
    #[test]
    fn select_scrolls_the_viewport_only_when_needed() {
        // const alloc = testing.allocator;
        let alloc = Allocator;
        // var t: Terminal = try .init(io, alloc, .{
        //     .cols = 10,
        //     .rows = 2,
        //     .max_scrollback_bytes = std.math.maxInt(usize),
        // });
        // defer t.deinit(alloc);
        let mut t: Terminal = Terminal::init(alloc, terminal_opts(10, 2, usize::MAX)).unwrap();
        let t_ptr: *mut Terminal = core::ptr::addr_of_mut!(t);

        // var stream = t.vtStream();
        // defer stream.deinit();
        // stream.nextSlice("Fizz\r\n");
        // for (0..30) |_| stream.nextSlice("\r\n");
        // stream.nextSlice("Fizz");
        let mut stream = unsafe { (*t_ptr).vt_stream() };
        stream.next_slice(b"Fizz\r\n");
        for _ in 0..30 {
            stream.next_slice(b"\r\n");
        }
        stream.next_slice(b"Fizz");

        // var search: TerminalSearch = try .init(alloc, "Fizz");
        // defer search.deinit(&t);
        let mut search: TerminalSearch = TerminalSearch::init(alloc, b"Fizz").unwrap();
        // while (search.status() != .complete) {
        //     switch (search.status()) {
        //         .feed_required => search.feed(&t, true),
        //         .running => _ = search.tick(),
        //         .complete => unreachable,
        //     }
        // }
        while search.status() != Status::Complete {
            match search.status() {
                Status::FeedRequired => search.feed(t_ptr, true),
                Status::Running => {
                    let _ = search.tick();
                }
                Status::Complete => unreachable!(),
            }
        }

        // Whether the selected match is within the visible viewport rows.
        // pointFromPin only bounds-checks the top of the region, so rows
        // below the viewport must be rejected by the row count.
        // const Visible = struct {
        //     fn check(term: *Terminal, s: *TerminalSearch) bool {
        //         const pin = s.activeScreenSearch().?.selectedMatch().?.startPin();
        //         const pages = &term.screens.active.pages;
        //         const pt = pages.pointFromPin(.viewport, pin) orelse return false;
        //         return pt.viewport.y < pages.rows;
        //     }
        // };
        struct Visible;
        impl Visible {
            fn check(term: *mut Terminal, s: &mut TerminalSearch) -> bool {
                let pin = s
                    .active_screen_search()
                    .unwrap()
                    .selected_match()
                    .unwrap()
                    .start_pin();
                // SB: narrow raw reborrow of the active screen's `pages` (the
                // reference `&term.screens.active.pages`), read-only.
                let pages: *const crate::pagelist::PageList =
                    unsafe { &(*(*term).screens.active).pages };
                let pt = match unsafe { (*pages).point_from_pin(point::Tag::Viewport, pin) } {
                    Some(pt) => pt,
                    None => return false,
                };
                pt.coord().y < unsafe { (*pages).rows } as u32
            }
        }

        // First match is at the bottom which is already visible: no scroll.
        // try testing.expect(try search.select(&t, .next, .if_needed));
        assert!(search
            .select(t_ptr, Select::Next, SelectScroll::IfNeeded)
            .unwrap());
        // try testing.expectEqual(
        //     point.Point{ .active = .{ .x = 0, .y = 1 } },
        //     t.screens.active.pages.pointFromPin(
        //         .active,
        //         search.activeScreenSearch().?.selectedMatch().?.startPin(),
        //     ).?,
        // );
        assert_eq!(
            Point::Active(point::Coordinate { x: 0, y: 1 }),
            unsafe {
                (*(*t_ptr).screens.active).pages.point_from_pin(
                    point::Tag::Active,
                    search
                        .active_screen_search()
                        .unwrap()
                        .selected_match()
                        .unwrap()
                        .start_pin(),
                )
            }
            .unwrap()
        );
        // try testing.expect(Visible.check(&t, &search));
        assert!(Visible::check(t_ptr, &mut search));

        // Second match is in scrollback: selecting it must scroll the
        // viewport so it becomes visible.
        // try testing.expect(try search.select(&t, .next, .if_needed));
        assert!(search
            .select(t_ptr, Select::Next, SelectScroll::IfNeeded)
            .unwrap());
        // try testing.expect(Visible.check(&t, &search));
        assert!(Visible::check(t_ptr, &mut search));

        // With scrolling disabled the viewport must stay where it is even
        // though the next selection (wrap back to the bottom) is not
        // visible.
        // try testing.expect(try search.select(&t, .next, .none));
        assert!(search
            .select(t_ptr, Select::Next, SelectScroll::None)
            .unwrap());
        // try testing.expect(!Visible.check(&t, &search));
        assert!(!Visible::check(t_ptr, &mut search));

        // defer search.deinit(&t); defer stream.deinit(); defer t.deinit(alloc);
        search.deinit(Some(t_ptr));
        stream.deinit();
        unsafe { (*t_ptr).deinit(alloc) };
    }

    /// PORTS: oracle f0c918fc4
    // test "deinit after the terminal is gone" {
    #[test]
    fn deinit_after_the_terminal_is_gone() {
        // const alloc = testing.allocator;
        let alloc = Allocator;
        // var t: Terminal = try .init(io, alloc, .{
        //     .cols = 10,
        //     .rows = 2,
        //     .max_scrollback_bytes = std.math.maxInt(usize),
        // });
        let mut t: Terminal = Terminal::init(alloc, terminal_opts(10, 2, usize::MAX)).unwrap();
        let t_ptr: *mut Terminal = core::ptr::addr_of_mut!(t);

        // var stream = t.vtStream();
        // stream.nextSlice("Fizz\r\nBuzz\r\nFizz");
        let mut stream = unsafe { (*t_ptr).vt_stream() };
        stream.next_slice(b"Fizz\r\nBuzz\r\nFizz");

        // Run to complete and select a match so the search holds tracked
        // pins within the terminal's page storage.
        // var search: TerminalSearch = try .init(alloc, "Fizz");
        let mut search: TerminalSearch = TerminalSearch::init(alloc, b"Fizz").unwrap();
        // while (search.status() != .complete) {
        //     switch (search.status()) {
        //         .feed_required => search.feed(&t, true),
        //         .running => _ = search.tick(),
        //         .complete => unreachable,
        //     }
        // }
        while search.status() != Status::Complete {
            match search.status() {
                Status::FeedRequired => search.feed(t_ptr, true),
                Status::Running => {
                    let _ = search.tick();
                }
                Status::Complete => unreachable!(),
            }
        }
        // try testing.expect(try search.select(&t, .next, .none));
        assert!(search
            .select(t_ptr, Select::Next, SelectScroll::None)
            .unwrap());

        // Deinitialize the terminal first. The pins died with the page
        // storage, so deinit with a null terminal must free only
        // search-owned memory without touching the terminal.
        // stream.deinit();
        stream.deinit();
        // t.deinit(alloc);
        unsafe { (*t_ptr).deinit(alloc) };
        // search.deinit(null);
        search.deinit(None);
    }

    // test "no matches selects nothing" {
    #[test]
    fn no_matches_selects_nothing() {
        // const alloc = testing.allocator;
        let alloc = Allocator;
        // var t: Terminal = try .init(io, alloc, .{ .cols = 10, .rows = 2 });
        // defer t.deinit(alloc);
        let mut t: Terminal = Terminal::init(alloc, terminal_opts(10, 2, 10_000)).unwrap();
        let t_ptr: *mut Terminal = core::ptr::addr_of_mut!(t);

        // var search: TerminalSearch = try .init(alloc, "Fizz");
        // defer search.deinit(&t);
        // search.feed(&t, true);
        let mut search: TerminalSearch = TerminalSearch::init(alloc, b"Fizz").unwrap();
        search.feed(t_ptr, true);
        // try testing.expect(!try search.select(&t, .next, .if_needed));
        assert!(!search
            .select(t_ptr, Select::Next, SelectScroll::IfNeeded)
            .unwrap());
        // try testing.expect(!try search.select(&t, .prev, .if_needed));
        assert!(!search
            .select(t_ptr, Select::Prev, SelectScroll::IfNeeded)
            .unwrap());

        // defer search.deinit(&t); defer t.deinit(alloc);
        search.deinit(Some(t_ptr));
        unsafe { (*t_ptr).deinit(alloc) };
    }
}
