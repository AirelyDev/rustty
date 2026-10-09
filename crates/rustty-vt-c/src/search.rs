//! rustty-vt-c search
//!
//! PORTS: ghostty src/terminal/c/search.zig (NEW in the Phase-6 range:
//! `32601cd79` "terminal/c: add search wrapper implementing the whole-terminal
//! search API", `f9202919f` "libghostty: add ghostty_search_* terminal search
//! C API", `f0c918fc4` "terminal/c: allow freeing a search and its terminal in
//! any order", `76d9fcefe` "libghostty: set the search needle via
//! ghostty_search_set, drop GhosttySearchOptions" — END STATE only, so there
//! is no options struct here).
//!
//! PORT STATUS: done. Every C API function body, the private helpers and all
//! 17 `test "..."` blocks from the Phase-6 range are ported assertion-for-
//! assertion (record: `.port/phase-6/assessments/W3-search-capi.md`); Phase 7
//! added the guard and test from `b1c264163` (P7-W2 T1, N436).
//!
//! OWNERSHIP (the `f0c918fc4` contract, ported exactly): a `SearchWrapper`
//! BORROWS its terminal through a raw `terminal_c::Terminal` handle and is
//! REGISTERED in that terminal's `searches` set. Whichever handle is freed
//! first detaches the other: `terminal_c::free` nulls `terminal` on every
//! registered search, and `free` here unregisters itself and passes the still-
//! live terminal to `TerminalSearch::deinit` — or `None` when the terminal died
//! first, in which case the tracked pins died with it and only search-owned
//! memory is released. No reference counting is introduced: who frees what is
//! exactly the reference's (the terminal owns its storage, the search owns
//! its wrapper + `TerminalSearch`).
//!
//! SB/lifetime discipline: the wrapper is only ever reached through the raw
//! `Search` pointer (never a `&mut SearchWrapper` that would outlive a call —
//! the terminal's `searches` set writes `terminal = null` through its own copy
//! of the pointer), and the Zig terminal is handed to `TerminalSearch` as the
//! raw `*mut ZigTerminal` it expects (W2's raw-pointer orchestration type;
//! `rust-mut-args-strongly-protected`).

// const std = @import("std");
// const testing = std.testing;
// const lib = @import("../lib.zig");
// const CAllocator = lib.alloc.Allocator;
// const searchpkg = @import("../search.zig");
// const TerminalSearch = searchpkg.Terminal;
// const FlattenedHighlight = @import("../highlight.zig").Flattened;
// const selection_c = @import("selection.zig");
// const terminal_c = @import("terminal.zig");
// const Result = @import("result.zig").Result;
use crate::grid_ref::CGridRef;
use crate::result::Result;
use crate::selection::{CSelection, CSelectionBuffer};
use crate::terminal as terminal_c;
use allocator_api2::alloc::{Allocator as ApiAllocator, Layout};
use rustty_lib::allocator::Allocator as CAllocator;
use rustty_lib::String as LibString;
use rustty_vt::alloc::VtAllocator;
use rustty_vt::highlight::{Chunk, Flattened, Flattened as FlattenedHighlight};
use rustty_vt::search::terminal::{SelectScroll, TerminalSearch};
use rustty_vt::terminal::Terminal as ZigTerminal;
use std::ffi::c_void;
use std::mem::size_of;
use std::os::raw::c_int;

// const log = std.log.scoped(.search_c);
//
// PORT NOTE: rustty's scoped-log sink is `sys::log_fn` (see `selection.rs`);
// `LOG_SCOPE` is the `@tagName` of the reference scope.
pub(crate) const LOG_SCOPE: &str = "search_c";

/// C: GhosttySearch -> RusttySearch (R2)
// pub const Search = ?*SearchWrapper;
pub type Search = *mut SearchWrapper;

// pub const SearchWrapper = struct {
pub struct SearchWrapper {
    // alloc: std.mem.Allocator,
    //
    // PORT NOTE (F-008, parity tail S7-6): the C-resolved allocator
    // (`lib.alloc.default(alloc_)`) as the runtime handle. It owns the WRAPPER
    // storage (`alloc.create`/`alloc.destroy`) and, as in the reference, every
    // allocation of the `TerminalSearch` built on it (`TerminalSearch.init(
    // wrapper.alloc, ..)`). The search's `searches` registry entry and its
    // tracked pins are charged to the TERMINAL's allocator instead (N262). The
    // caller's `RusttyAllocator` must outlive the search (N260).
    pub alloc: VtAllocator,

    /// The terminal this search is bound to. This is borrowed, so the
    /// search never frees it. If the terminal is freed first, it sets
    /// this to null to detach the search: calls that need the terminal
    /// then fail cleanly and free releases only search-owned memory.
    // terminal: terminal_c.Terminal,
    pub terminal: terminal_c::Terminal,

    /// The search state. Null when no needle is set: the search
    /// starts idle and returns to idle when the needle is cleared.
    // search: ?TerminalSearch = null,
    pub search: Option<TerminalSearch>,

    /// The scroll policy applied by the select options. Persistent
    /// across selects, set via the select_scroll option.
    // select_scroll: TerminalSearch.SelectScroll = .if_needed,
    pub select_scroll: SelectScroll,
}

impl SearchWrapper {
    /// The Zig terminal this search is bound to, or null if the
    /// terminal was freed before the search.
    // fn zigTerminal(self: *const SearchWrapper) ?*terminal_c.ZigTerminal {
    //     const terminal_wrapper = self.terminal orelse return null;
    //     return terminal_wrapper.terminal;
    // }
    // PORT NOTE: raw-self helper (no `&SearchWrapper` is ever formed; see the
    // module SB note). Null stands in for the reference `?*`.
    /// # Safety
    /// `this` must point to a live wrapper.
    unsafe fn zig_terminal(this: *const SearchWrapper) -> *mut ZigTerminal {
        // const terminal_wrapper = self.terminal orelse return null;
        // SAFETY: `this` is a live wrapper (caller contract).
        let terminal_wrapper = unsafe { (*this).terminal };
        if terminal_wrapper.is_null() {
            return std::ptr::null_mut();
        }
        // return terminal_wrapper.terminal;
        // SAFETY: a non-null `terminal` is a live, registered-with terminal
        // wrapper (its `free` nulls this field before the storage goes away).
        unsafe { (*terminal_wrapper).terminal }
    }

    /// The status of the search. A search with no needle reports
    /// complete since there is nothing to look for.
    // fn status(self: *SearchWrapper) Status {
    //     const s = if (self.search) |*s| s else return .complete;
    //     return .fromZig(s.status());
    // }
    /// # Safety
    /// `this` must point to a live wrapper.
    unsafe fn status(this: *mut SearchWrapper) -> SearchStatus {
        // const s = if (self.search) |*s| s else return .complete;
        // SAFETY: `this` is a live wrapper (caller contract).
        let Some(s) = (unsafe { &(*this).search }).as_ref() else {
            return SearchStatus::Complete;
        };
        // return .fromZig(s.status());
        SearchStatus::from_zig(s.status())
    }
}

/// C: GhosttySearchStatus -> RusttySearchStatus (R2)
// pub const Status = enum(c_int) {
//     running = 0,
//     feed_required = 1,
//     complete = 2,
// };
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SearchStatus {
    Running = 0,
    FeedRequired = 1,
    Complete = 2,
}

const _: () = assert!(size_of::<SearchStatus>() == size_of::<c_int>());

impl SearchStatus {
    // fn fromZig(status: TerminalSearch.Status) Status {
    //     return switch (status) {
    //         .running => .running,
    //         .feed_required => .feed_required,
    //         .complete => .complete,
    //     };
    // }
    fn from_zig(status: rustty_vt::search::terminal::Status) -> SearchStatus {
        use rustty_vt::search::terminal::Status as S;
        match status {
            S::Running => SearchStatus::Running,
            S::FeedRequired => SearchStatus::FeedRequired,
            S::Complete => SearchStatus::Complete,
        }
    }
}

/// C: GhosttySearchScroll -> RusttySearchScroll (R2)
// pub const Scroll = enum(c_int) {
//     if_needed = 0,
//     none = 1,
// };
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SearchScroll {
    IfNeeded = 0,
    None = 1,
}

const _: () = assert!(size_of::<SearchScroll>() == size_of::<c_int>());

impl SearchScroll {
    /// Narrow an inbound C integer to a `SearchScroll`, mirroring
    /// `std.enums.fromInt`; `None` outside the declared range.
    pub fn from_c_int(v: c_int) -> Option<SearchScroll> {
        match v {
            0 => Some(SearchScroll::IfNeeded),
            1 => Some(SearchScroll::None),
            _ => None,
        }
    }

    // fn toZig(self: Scroll) TerminalSearch.SelectScroll {
    //     return switch (self) {
    //         .if_needed => .if_needed,
    //         .none => .none,
    //     };
    // }
    fn to_zig(self) -> SelectScroll {
        match self {
            SearchScroll::IfNeeded => SelectScroll::IfNeeded,
            SearchScroll::None => SelectScroll::None,
        }
    }

    // fn fromZig(scroll: TerminalSearch.SelectScroll) Scroll {
    //     return switch (scroll) {
    //         .if_needed => .if_needed,
    //         .none => .none,
    //     };
    // }
    fn from_zig(scroll: SelectScroll) -> SearchScroll {
        match scroll {
            SelectScroll::IfNeeded => SearchScroll::IfNeeded,
            SelectScroll::None => SearchScroll::None,
        }
    }
}

/// C: GhosttySearchData -> RusttySearchData (R2)
// pub const Data = enum(c_int) {
//     status = 0,
//     needle = 1,
//     total_matches = 2,
//     selected_index = 3,
//     selected_match = 4,
//     matches = 5,
//     viewport_matches = 6,
//     select_scroll = 7,
//
//     pub fn OutType(comptime self: Data) type {
//         return switch (self) {
//             .status => Status,
//             .needle => lib.String,
//             .total_matches => usize,
//             .selected_index => usize,
//             .selected_match => selection_c.CSelection,
//             .matches, .viewport_matches => selection_c.CSelectionBuffer,
//             .select_scroll => Scroll,
//         };
//     }
// };
//
// PORT NOTE: `OutType` is the comptime type table `getTyped` dispatches on;
// in Rust the per-arm `out as *mut <OutType>` casts in `get_typed` ARE that
// table (status -> SearchStatus, needle -> LibString, total_matches /
// selected_index -> usize, selected_match -> CSelection, matches /
// viewport_matches -> CSelectionBuffer, select_scroll -> SearchScroll).
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SearchData {
    Status = 0,
    Needle = 1,
    TotalMatches = 2,
    SelectedIndex = 3,
    SelectedMatch = 4,
    Matches = 5,
    ViewportMatches = 6,
    SelectScroll = 7,
}

// The C-ABI storage width the published header declares
// (`RUSTTY_SEARCH_DATA_MAX_VALUE = RUSTTY_ENUM_MAX_VALUE`), which is what makes
// reading a `get_multi` key array element as a raw `c_int` ABI-neutral.
const _: () = assert!(size_of::<SearchData>() == size_of::<c_int>());

impl SearchData {
    /// Narrow an inbound C integer to a `SearchData`, mirroring
    /// `std.enums.fromInt`; `None` outside the declared range.
    pub fn from_c_int(v: c_int) -> Option<SearchData> {
        // contiguous 0..=7
        if (0..=7).contains(&v) {
            // SAFETY: `v` is a valid `SearchData` discriminant (range-checked).
            Some(unsafe { std::mem::transmute::<c_int, SearchData>(v) })
        } else {
            None
        }
    }
}

/// C: GhosttySearchOption -> RusttySearchOption (R2)
// pub const Option = enum(c_int) {
//     needle = 0,
//     select_next = 1,
//     select_prev = 2,
//     select_scroll = 3,
//
//     pub fn Type(comptime self: Option) type {
//         return switch (self) {
//             .needle => lib.String,
//             // The value must be NULL. Reserved for future use.
//             .select_next, .select_prev => void,
//             .select_scroll => Scroll,
//         };
//     }
// };
//
// PORT NOTE: named `SearchOption` (the reference `Option` would shadow
// `core::option::Option`); `Type` is the comptime value-type table
// `setTyped` dispatches on — the per-arm `value as *const <Type>` casts in
// `set_typed` are that table (needle -> LibString, select_next/select_prev ->
// void [the value must be NULL; reserved for future use], select_scroll ->
// SearchScroll).
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SearchOption {
    Needle = 0,
    SelectNext = 1,
    SelectPrev = 2,
    SelectScroll = 3,
}

const _: () = assert!(size_of::<SearchOption>() == size_of::<c_int>());

impl SearchOption {
    /// Narrow an inbound C integer to a `SearchOption`, mirroring
    /// `std.enums.fromInt`; `None` outside the declared range.
    pub fn from_c_int(v: c_int) -> Option<SearchOption> {
        match v {
            0 => Some(SearchOption::Needle),
            1 => Some(SearchOption::SelectNext),
            2 => Some(SearchOption::SelectPrev),
            3 => Some(SearchOption::SelectScroll),
            _ => None,
        }
    }
}

// pub fn new(
//     alloc_: ?*const CAllocator,
//     out_search: ?*Search,
//     terminal: terminal_c.Terminal,
// ) callconv(lib.calling_conv) Result {
/// # Safety
/// `alloc_`, if non-null, must point to a valid `CAllocator`; `out_search`, if
/// non-null, must be a valid writable pointer to a `Search`; `terminal`, if
/// non-null, must be a live terminal handle.
pub unsafe fn new(
    alloc_: Option<&CAllocator>,
    out_search: *mut Search,
    terminal: terminal_c::Terminal,
) -> Result {
    // const out = out_search orelse return .invalid_value;
    if out_search.is_null() {
        return Result::InvalidValue;
    }
    // out.* = null;
    // SAFETY: `out_search` is a non-null valid writable `Search`.
    unsafe { *out_search = std::ptr::null_mut() };

    // const terminal_wrapper = terminal orelse return .invalid_value;
    if terminal.is_null() {
        return Result::InvalidValue;
    }
    let terminal_wrapper = terminal;
    // const alloc = lib.alloc.default(alloc_);
    // SAFETY: a non-null `alloc_` is a valid caller allocator that outlives the
    // search (the reference's contract, N260).
    let alloc: VtAllocator = unsafe { rustty_vt::alloc::default(alloc_) };
    // const wrapper = alloc.create(SearchWrapper) catch return .out_of_memory;
    // PORT NOTE (F-008): through the C-resolved allocator; a rejecting caller
    // allocator yields `.out_of_memory` like the reference.
    let wrapper: *mut SearchWrapper =
        match ApiAllocator::allocate(&alloc, Layout::new::<SearchWrapper>()) {
            Ok(ptr) => ptr.as_ptr() as *mut SearchWrapper,
            Err(_) => return Result::OutOfMemory,
        };
    // wrapper.* = .{
    //     .alloc = alloc,
    //     .terminal = terminal_wrapper,
    // };
    // SAFETY: `wrapper` is valid uninitialized `SearchWrapper` storage.
    unsafe {
        std::ptr::write(
            wrapper,
            SearchWrapper {
                alloc,
                terminal: terminal_wrapper,
                search: None,
                select_scroll: SelectScroll::IfNeeded,
            },
        )
    };

    // Store the search in the terminal so that when the terminal is
    // freed the search can be detached safely.
    // terminal_wrapper.searches.putNoClobber(
    //     terminal_wrapper.terminal.gpa(),
    //     wrapper,
    //     {},
    // ) catch {
    //     alloc.destroy(wrapper);
    //     return .out_of_memory;
    // };
    // PORT NOTE: `putNoClobber` asserts the key is absent (a fresh heap ptr is
    // always unique); `try_put_no_clobber` is its fallible spelling.
    //
    // PORT NOTE (parity S4-F3 / S176-F2): the reference's `catch` arm is the
    // map's OWN allocation failing, and `search.h:340-362` documents
    // `GHOSTTY_OUT_OF_MEMORY` for it. A bare `HashSet::insert` aborts on
    // allocation failure, which made that documented return unreachable AND
    // leaked the wrapper on the path the reference frees it. `try_reserve` is
    // the fallible half of the same growth, so the error path is real: it frees
    // the wrapper and returns `.out_of_memory` exactly as the reference does.
    // (This is a CHOICE the port had, not a Rust language limitation — which is
    // why the earlier "the port cannot produce this failure" reading of the
    // sibling row does not hold.)
    // The registry is bound to the terminal's allocator (`gpa()`, parity tail
    // S7-4), so this growth is charged to the terminal, as in the reference.
    // SAFETY: `searches` is a sibling field of the live terminal wrapper.
    let reserved = unsafe { (*terminal_wrapper).searches.try_put_no_clobber(wrapper, ()) };
    if reserved.is_err() {
        // alloc.destroy(wrapper);
        // PORT NOTE: nothing else owns anything yet (`search` is `None`), so
        // dropping the value in place and returning its storage through the
        // allocator it came from is the whole teardown.
        // SAFETY: `wrapper` was just allocated through `alloc` with this layout
        // and initialized above, and has not been handed out; it is dropped and
        // freed exactly once.
        unsafe {
            std::ptr::drop_in_place(wrapper);
            ApiAllocator::deallocate(
                &alloc,
                std::ptr::NonNull::new_unchecked(wrapper as *mut u8),
                Layout::new::<SearchWrapper>(),
            );
        }
        // return .out_of_memory;
        return Result::OutOfMemory;
    }

    // out.* = wrapper;
    // SAFETY: `out_search` is a non-null valid writable `Search`.
    unsafe { *out_search = wrapper };
    // return .success;
    Result::Success
}

// pub fn free(search_: Search) callconv(lib.calling_conv) void {
/// # Safety
/// `search_`, if non-null, must be a handle returned by `new` and not yet
/// freed.
pub unsafe fn free(search_: Search) {
    // const wrapper = search_ orelse return;
    if search_.is_null() {
        return;
    }
    // if (wrapper.terminal) |terminal_wrapper| {
    // SAFETY: `search_` is a live `new`-returned wrapper (caller contract);
    // raw-ptr reads of its fields.
    let terminal_wrapper = unsafe { (*search_).terminal };
    if !terminal_wrapper.is_null() {
        //     _ = terminal_wrapper.searches.swapRemove(wrapper);
        // SAFETY: a non-null `terminal` is a live terminal wrapper (it nulls
        // this field before it dies); `searches` is a sibling field.
        unsafe { (*terminal_wrapper).searches.swap_remove(&search_) };
        //     if (wrapper.search) |*s| s.deinit(terminal_wrapper.terminal);
        let t = unsafe { (*terminal_wrapper).terminal };
        if let Some(s) = unsafe { &mut (*search_).search }.as_mut() {
            s.deinit(Some(t));
        }
    } else {
        // } else {
        //     // The terminal was freed first. Tracked state died with it,
        //     // so only search-owned memory is freed.
        //     if (wrapper.search) |*s| s.deinit(null);
        // }
        // The terminal was freed first. Tracked state died with it,
        // so only search-owned memory is freed.
        if let Some(s) = unsafe { &mut (*search_).search }.as_mut() {
            s.deinit(None);
        }
    }
    // const alloc = wrapper.alloc;
    // alloc.destroy(wrapper);
    // PORT NOTE (F-008): the value is dropped in place (its `search` was
    // `deinit`ed above, so this releases nothing more) and the storage goes
    // back through the allocator that created it (the caller's free callback
    // fires).
    // SAFETY: `search_` was allocated by `new` through `alloc` with this
    // layout; it is dropped and freed exactly once.
    let alloc = unsafe { (*search_).alloc };
    unsafe {
        std::ptr::drop_in_place(search_);
        ApiAllocator::deallocate(
            &alloc,
            std::ptr::NonNull::new_unchecked(search_ as *mut u8),
            Layout::new::<SearchWrapper>(),
        );
    }
}

// pub fn tick(
//     search_: Search,
//     out_status: ?*Status,
// ) callconv(lib.calling_conv) Result {
/// # Safety
/// `search_`, if non-null, must be a live search handle; `out_status`, if
/// non-null, must be a valid writable pointer.
pub unsafe fn tick(search_: Search, out_status: *mut SearchStatus) -> Result {
    // const wrapper = search_ orelse return .invalid_value;
    if search_.is_null() {
        return Result::InvalidValue;
    }
    // History ticks read a pin tracked in the terminal's page list.
    // _ = wrapper.zigTerminal() orelse return .invalid_value;
    // SAFETY: `search_` is a live wrapper (caller contract).
    if unsafe { SearchWrapper::zig_terminal(search_) }.is_null() {
        return Result::InvalidValue;
    }
    // if (wrapper.search) |*s| _ = s.tick();
    // SAFETY: `search_` is a live wrapper (caller contract).
    if let Some(s) = unsafe { &mut (*search_).search }.as_mut() {
        let _ = s.tick();
    }
    // if (out_status) |out| out.* = wrapper.status();
    if !out_status.is_null() {
        // SAFETY: `out_status` is a non-null valid writable status.
        unsafe { *out_status = SearchWrapper::status(search_) };
    }
    // return .success;
    Result::Success
}

// pub fn feed(search_: Search) callconv(lib.calling_conv) Result {
/// # Safety
/// `search_`, if non-null, must be a live search handle, and the caller must
/// hold exclusive access to its terminal.
pub unsafe fn feed(search_: Search) -> Result {
    // const wrapper = search_ orelse return .invalid_value;
    if search_.is_null() {
        return Result::InvalidValue;
    }
    // const t = wrapper.zigTerminal() orelse return .invalid_value;
    // SAFETY: `search_` is a live wrapper (caller contract).
    let t = unsafe { SearchWrapper::zig_terminal(search_) };
    if t.is_null() {
        return Result::InvalidValue;
    }
    // const s = if (wrapper.search) |*s| s else return .success;
    let Some(s) = (unsafe { &mut (*search_).search }).as_mut() else {
        return Result::Success;
    };

    // The C API has no renderer cooperation to know whether the active
    // area changed, so it is always re-scanned. This is correct without
    // any dirty tracking and cheap because the active area search was
    // built for exactly this.
    // s.feed(t, true);
    s.feed(t, true);
    // PORT NOTE: reference defect (R13, UD305). `TerminalSearch.feed` logs and
    // swallows an allocation failure, and a feed that succeeds always moves
    // the search out of `feed_required` (a history screen either takes its
    // next page or completes; a screen search is created). So a search that is
    // still `feed_required` right after a feed failed to allocate, and the
    // documented protocol (feed while the status is feed_required) would
    // repeat the same failing feed forever with `GHOSTTY_SUCCESS`. The port
    // reports `GHOSTTY_OUT_OF_MEMORY` there, the error code allocator.h
    // promises for allocation failures; every other feed is unchanged.
    if s.status() == rustty_vt::search::terminal::Status::FeedRequired {
        return Result::OutOfMemory;
    }
    // return .success;
    Result::Success
}

// pub fn run(search_: Search) callconv(lib.calling_conv) Result {
/// # Safety
/// `search_`, if non-null, must be a live search handle, and the caller must
/// hold exclusive access to its terminal for the whole call.
pub unsafe fn run(search_: Search) -> Result {
    // const wrapper = search_ orelse return .invalid_value;
    if search_.is_null() {
        return Result::InvalidValue;
    }
    // const t = wrapper.zigTerminal() orelse return .invalid_value;
    // SAFETY: `search_` is a live wrapper (caller contract).
    let t = unsafe { SearchWrapper::zig_terminal(search_) };
    if t.is_null() {
        return Result::InvalidValue;
    }
    // const s = if (wrapper.search) |*s| s else return .success;
    let Some(s) = (unsafe { &mut (*search_).search }).as_mut() else {
        return Result::Success;
    };

    // Always start with a feed: complete only means caught up as of
    // the last feed, so run doubles as the "terminal changed, catch
    // up" convenience for one-shot embedders.
    // s.feed(t, true);
    //
    // PORT NOTE: reference defect (R13, UD305). The reference loops below until
    // the status is complete, and `feed` swallows an allocation failure: with
    // the search's allocator failing, the status stays `feed_required` after
    // every feed and `run` never returns. A successful feed always leaves
    // `feed_required` (see `feed` above), so the port returns
    // `GHOSTTY_OUT_OF_MEMORY` when a feed leaves the search there. `tick` needs
    // no such check: a failed tick still consumes the window it was reading,
    // so the loop makes progress.
    use rustty_vt::search::terminal::Status as S;
    s.feed(t, true);
    if s.status() == S::FeedRequired {
        return Result::OutOfMemory;
    }
    // while (true) {
    //     switch (s.status()) {
    //         .complete => return .success,
    //         .feed_required => s.feed(t, true),
    //         .running => _ = s.tick(),
    //     }
    // }
    loop {
        match s.status() {
            S::Complete => return Result::Success,
            S::FeedRequired => {
                s.feed(t, true);
                if s.status() == S::FeedRequired {
                    return Result::OutOfMemory;
                }
            }
            S::Running => {
                let _ = s.tick();
            }
        }
    }
}

// pub fn set(
//     search_: Search,
//     option: Option,
//     value: ?*const anyopaque,
// ) callconv(lib.calling_conv) Result {
//     if (comptime std.debug.runtime_safety) {
//         _ = std.enums.fromInt(Option, @intFromEnum(option)) orelse {
//             log.warn("search_set invalid option value={d}", .{@intFromEnum(option)});
//             return .invalid_value;
//         };
//     }
//
//     return switch (option) {
//         inline else => |comptime_option| setTyped(
//             search_,
//             comptime_option,
//             if (value) |ptr| @ptrCast(@alignCast(ptr)) else null,
//         ),
//     };
// }
//
// PORT NOTE (D11): the C entry point (`rustty_search_set` in lib.rs) carries
// the option as a raw `c_int` and reaches the `fromInt` guard + its `log.warn`
// through `set_checked` below; `set` itself keeps the typed `SearchOption`,
// which by construction cannot be out of range. The guard is unconditional
// here (the reference's `comptime std.debug.runtime_safety` is true in every
// mode this crate ships tests for, and materializing an out-of-range
// discriminant is UB in Rust in every build mode).
/// # Safety
/// `search_`, if non-null, must be a live search handle; `value`, if non-null,
/// must point to a value of the type required by `option`.
pub unsafe fn set(search_: Search, option: SearchOption, value: *const c_void) -> Result {
    // SAFETY: `value` validity/type is the caller's contract per `option`.
    unsafe { set_typed(search_, option, value) }
}

/// The raw-`c_int` half of `set`: the reference's `std.enums.fromInt(Option,
/// ...)` runtime-safety guard and its `log.warn`.
/// # Safety
/// As `set`.
pub unsafe fn set_checked(search_: Search, option: c_int, value: *const c_void) -> Result {
    let Some(opt) = SearchOption::from_c_int(option) else {
        // log.warn("search_set invalid option value={d}", .{@intFromEnum(option)});
        crate::sys::log_fn(
            crate::sys::StdLevel::Warn,
            LOG_SCOPE,
            format_args!("search_set invalid option value={option}"),
        );
        return Result::InvalidValue;
    };
    // SAFETY: `value` validity/type is the caller's contract per `option`.
    unsafe { set(search_, opt, value) }
}

// fn setTyped(
//     search_: Search,
//     comptime option: Option,
//     value: ?*const option.Type(),
// ) Result {
/// # Safety
/// `value`, if non-null, must point to a value of the type required by
/// `option`.
unsafe fn set_typed(search_: Search, option: SearchOption, value: *const c_void) -> Result {
    // const wrapper = search_ orelse return .invalid_value;
    if search_.is_null() {
        return Result::InvalidValue;
    }
    let wrapper = search_;
    // switch (option) {
    match option {
        // .needle => {
        SearchOption::Needle => {
            // The needle touches the terminal (replacing or clearing
            // an active search releases tracked pins through it), so
            // it requires the terminal to still be alive.
            // const t = wrapper.zigTerminal() orelse return .invalid_value;
            // SAFETY: `wrapper` is a live wrapper (checked non-null above).
            let t = unsafe { SearchWrapper::zig_terminal(wrapper) };
            if t.is_null() {
                return Result::InvalidValue;
            }

            // NULL and empty both clear the needle, returning the
            // search to idle. This matches the internal engine, where
            // an empty needle stops the search.
            // const v = value orelse return clearNeedle(wrapper, t);
            if value.is_null() {
                return unsafe { clear_needle(wrapper, t) };
            }
            // SAFETY: a non-null `value` points to a `LibString` (the
            // `.needle` arm of `Option.Type`, caller contract).
            let v: LibString = unsafe { *(value as *const LibString) };
            // if (v.len == 0) return clearNeedle(wrapper, t);
            if v.len == 0 {
                return unsafe { clear_needle(wrapper, t) };
            }
            // if (@intFromPtr(v.ptr) == 0) return .invalid_value;
            if v.ptr.is_null() {
                return Result::InvalidValue;
            }
            // const bytes = v.ptr[0..v.len];
            // SAFETY: `v` is a caller-provided (ptr, len) byte string.
            let bytes: &[u8] = unsafe { std::slice::from_raw_parts(v.ptr, v.len) };

            // Setting the current needle again keeps existing results,
            // using the same ASCII case-insensitive comparison as
            // matching. This is what a find bar wants on resubmit.
            // if (wrapper.search) |*s| {
            //     if (std.ascii.eqlIgnoreCase(s.needle(), bytes)) {
            //         return .success;
            //     }
            // }
            if let Some(s) = unsafe { &(*wrapper).search }.as_ref() {
                if s.needle().eq_ignore_ascii_case(bytes) {
                    return Result::Success;
                }
            }

            // Create the replacement before dropping the current
            // search so an allocation failure keeps existing state.
            // const replacement = TerminalSearch.init(
            //     wrapper.alloc,
            //     bytes,
            // ) catch return .out_of_memory;
            // SAFETY: `wrapper` is live (checked above).
            let replacement = match TerminalSearch::init(unsafe { (*wrapper).alloc }, bytes) {
                Ok(r) => r,
                Err(_) => return Result::OutOfMemory,
            };
            // if (wrapper.search) |*s| s.deinit(t);
            if let Some(s) = unsafe { &mut (*wrapper).search }.as_mut() {
                s.deinit(Some(t));
            }
            // wrapper.search = replacement;
            unsafe { (*wrapper).search = Some(replacement) };
        }

        // .select_next, .select_prev => {
        SearchOption::SelectNext | SearchOption::SelectPrev => {
            // The value is reserved for future use and must be NULL.
            // if (value != null) return .invalid_value;
            if !value.is_null() {
                return Result::InvalidValue;
            }

            // const t = wrapper.zigTerminal() orelse return .invalid_value;
            // SAFETY: `wrapper` is a live wrapper (checked non-null above).
            let t = unsafe { SearchWrapper::zig_terminal(wrapper) };
            if t.is_null() {
                return Result::InvalidValue;
            }
            // const s = if (wrapper.search) |*s| s else return .no_value;
            let select_scroll = unsafe { (*wrapper).select_scroll };
            let Some(s) = (unsafe { &mut (*wrapper).search }).as_mut() else {
                return Result::NoValue;
            };
            // const selected = s.select(
            //     t,
            //     switch (option) {
            //         .select_next => .next,
            //         .select_prev => .prev,
            //         else => comptime unreachable,
            //     },
            //     wrapper.select_scroll,
            // ) catch return .out_of_memory;
            let selected = match s.select(
                t,
                match option {
                    SearchOption::SelectNext => rustty_vt::search::screen::Select::Next,
                    SearchOption::SelectPrev => rustty_vt::search::screen::Select::Prev,
                    _ => unreachable!(),
                },
                select_scroll,
            ) {
                Ok(v) => v,
                Err(_) => return Result::OutOfMemory,
            };
            // if (!selected) return .no_value;
            if !selected {
                return Result::NoValue;
            }
        }

        // .select_scroll => {
        SearchOption::SelectScroll => {
            // const v = value orelse {
            //     wrapper.select_scroll = .if_needed;
            //     return .success;
            // };
            if value.is_null() {
                unsafe { (*wrapper).select_scroll = SelectScroll::IfNeeded };
                return Result::Success;
            }
            // const scroll = std.enums.fromInt(Scroll, @intFromEnum(v.*)) orelse
            //     return .invalid_value;
            // PORT NOTE (D11): read the C storage as the raw `c_int` it is and
            // narrow it — reading straight into a `SearchScroll` would
            // materialize an out-of-range discriminant (UB) before `fromInt`
            // could reject it.
            // SAFETY: a non-null `value` points to a `Scroll` (the
            // `.select_scroll` arm of `Option.Type`, caller contract).
            let raw: c_int = unsafe { *(value as *const c_int) };
            let Some(scroll) = SearchScroll::from_c_int(raw) else {
                return Result::InvalidValue;
            };
            // wrapper.select_scroll = scroll.toZig();
            unsafe { (*wrapper).select_scroll = scroll.to_zig() };
        }
    }

    // return .success;
    Result::Success
}

// fn clearNeedle(wrapper: *SearchWrapper, t: *terminal_c.ZigTerminal) Result {
//     if (wrapper.search) |*s| {
//         s.deinit(t);
//         wrapper.search = null;
//     }
//     return .success;
// }
/// # Safety
/// `wrapper` must be a live wrapper and `t` its live Zig terminal.
unsafe fn clear_needle(wrapper: *mut SearchWrapper, t: *mut ZigTerminal) -> Result {
    // if (wrapper.search) |*s| {
    // SAFETY: `wrapper` is live (caller contract).
    if let Some(s) = unsafe { &mut (*wrapper).search }.as_mut() {
        //     s.deinit(t);
        s.deinit(Some(t));
        //     wrapper.search = null;
        unsafe { (*wrapper).search = None };
    }
    // return .success;
    Result::Success
}

// pub fn get(
//     search_: Search,
//     data: Data,
//     value: ?*anyopaque,
// ) callconv(lib.calling_conv) Result {
//     if (comptime std.debug.runtime_safety) {
//         _ = std.enums.fromInt(Data, @intFromEnum(data)) orelse {
//             log.warn("search_get invalid data value={d}", .{@intFromEnum(data)});
//             return .invalid_value;
//         };
//     }
//
//     const out_ptr = value orelse return .invalid_value;
//     return switch (data) {
//         inline else => |comptime_data| getTyped(
//             search_,
//             comptime_data,
//             @ptrCast(@alignCast(out_ptr)),
//         ),
//     };
// }
//
// PORT NOTE (W66-GETMULTI-RAW-01 shape, as selection_gesture.rs): the
// `fromInt(Data, ...)` guard + its `log.warn` live in `get_checked` below, on
// the raw `c_int`, so that `get_multi` reaches them PER ARRAY ELEMENT —
// exactly as the reference's `get_multi` reaches the guard by calling `get`.
/// # Safety
/// `search_`, if non-null, must be a live search handle; `value` must point to
/// writable storage of the type required by `data`.
pub unsafe fn get(search_: Search, data: SearchData, value: *mut c_void) -> Result {
    // const out_ptr = value orelse return .invalid_value;
    if value.is_null() {
        return Result::InvalidValue;
    }
    // SAFETY: `value` validity/type is the caller's contract per `data`.
    unsafe { get_typed(search_, data, value) }
}

/// The raw-`c_int` half of `get`: the reference's `std.enums.fromInt(Data,
/// ...)` runtime-safety guard and its `log.warn`.
/// # Safety
/// As `get`.
pub unsafe fn get_checked(search_: Search, data: c_int, value: *mut c_void) -> Result {
    let Some(d) = SearchData::from_c_int(data) else {
        // log.warn("search_get invalid data value={d}", .{@intFromEnum(data)});
        crate::sys::log_fn(
            crate::sys::StdLevel::Warn,
            LOG_SCOPE,
            format_args!("search_get invalid data value={data}"),
        );
        return Result::InvalidValue;
    };
    // SAFETY: `value` validity/type is the caller's contract per `data`.
    unsafe { get(search_, d, value) }
}

// pub fn get_multi(
//     search_: Search,
//     count: usize,
//     keys: ?[*]const Data,
//     values: ?[*]?*anyopaque,
//     out_written: ?*usize,
// ) callconv(lib.calling_conv) Result {
/// # Safety
/// `keys`/`values` (if non-null) must have `count` elements; `out_written`, if
/// non-null, must be writable.
// PORT NOTE (W66-GETMULTI-RAW-01): `keys` is C memory typed `const
// RusttySearchData *` in the published header; each element is read as a raw
// `c_int` (ABI-identical — `SearchData` is `#[repr(C)]`, asserted int-sized
// above) and range-checked by `get_checked`, mirroring the reference's
// per-element `get` guard (first failing index reported through
// `out_written`).
pub unsafe fn get_multi(
    search_: Search,
    count: usize,
    keys: *const c_int,
    values: *mut *mut c_void,
    out_written: *mut usize,
) -> Result {
    // const k = keys orelse return .invalid_value;
    if keys.is_null() {
        return Result::InvalidValue;
    }
    // const v = values orelse return .invalid_value;
    if values.is_null() {
        return Result::InvalidValue;
    }

    // for (0..count) |i| {
    for i in 0..count {
        // SAFETY: caller guarantees `keys`/`values` have `count` elements.
        let key = unsafe { *keys.add(i) };
        let value = unsafe { *values.add(i) };
        //     const result = get(search_, k[i], v[i]);
        let result = unsafe { get_checked(search_, key, value) };
        //     if (result != .success) {
        //         if (out_written) |w| w.* = i;
        //         return result;
        //     }
        if result != Result::Success {
            if !out_written.is_null() {
                // SAFETY: caller-provided out pointer, checked non-null.
                unsafe { *out_written = i };
            }
            return result;
        }
    }
    // if (out_written) |w| w.* = count;
    if !out_written.is_null() {
        // SAFETY: caller-provided out pointer, checked non-null.
        unsafe { *out_written = count };
    }
    // return .success;
    Result::Success
}

// fn getTyped(
//     search_: Search,
//     comptime data: Data,
//     out: *data.OutType(),
// ) Result {
/// # Safety
/// `out` must point to writable storage of the type required by `data`.
unsafe fn get_typed(search_: Search, data: SearchData, out: *mut c_void) -> Result {
    // const wrapper = search_ orelse return .invalid_value;
    if search_.is_null() {
        return Result::InvalidValue;
    }
    let wrapper = search_;

    // switch (data) {
    match data {
        // .status => out.* = wrapper.status(),
        SearchData::Status => {
            // SAFETY: `out` is a writable `SearchStatus` (caller contract);
            // `wrapper` is live.
            unsafe { *(out as *mut SearchStatus) = SearchWrapper::status(wrapper) };
        }

        // .needle => {
        //     const s = if (wrapper.search) |*s| s else return .no_value;
        //     out.* = .init(s.needle());
        // },
        SearchData::Needle => {
            // SAFETY: `wrapper` is live.
            let Some(s) = (unsafe { &(*wrapper).search }).as_ref() else {
                return Result::NoValue;
            };
            // SAFETY: `out` is a writable `LibString` (caller contract).
            unsafe { *(out as *mut LibString) = LibString::init(s.needle()) };
        }

        // .total_matches => out.* = total: {
        //     const s = if (wrapper.search) |*s| s else break :total 0;
        //     const ss = s.activeScreenSearch() orelse break :total 0;
        //     break :total ss.matchesLen();
        // },
        SearchData::TotalMatches => {
            let total: usize = 'total: {
                // SAFETY: `wrapper` is live.
                let Some(s) = (unsafe { &mut (*wrapper).search }).as_mut() else {
                    break 'total 0;
                };
                let Some(ss) = s.active_screen_search() else {
                    break 'total 0;
                };
                ss.matches_len()
            };
            // SAFETY: `out` is a writable `usize` (caller contract).
            unsafe { *(out as *mut usize) = total };
        }

        // .selected_index => {
        //     const s = if (wrapper.search) |*s| s else return .no_value;
        //     const ss = s.activeScreenSearch() orelse return .no_value;
        //     const selected = ss.selected orelse return .no_value;
        //     out.* = selected.idx;
        // },
        SearchData::SelectedIndex => {
            // SAFETY: `wrapper` is live.
            let Some(s) = (unsafe { &mut (*wrapper).search }).as_mut() else {
                return Result::NoValue;
            };
            let Some(ss) = s.active_screen_search() else {
                return Result::NoValue;
            };
            let Some(selected) = ss.selected.as_ref() else {
                return Result::NoValue;
            };
            // SAFETY: `out` is a writable `usize` (caller contract).
            unsafe { *(out as *mut usize) = selected.idx };
        }

        // .selected_match => {
        //     const s = if (wrapper.search) |*s| s else return .no_value;
        //     const ss = s.activeScreenSearch() orelse return .no_value;
        //     const hl = ss.selectedMatch() orelse return .no_value;
        //     out.* = selectionFromHighlight(hl);
        // },
        SearchData::SelectedMatch => {
            // SAFETY: `wrapper` is live.
            let Some(s) = (unsafe { &mut (*wrapper).search }).as_mut() else {
                return Result::NoValue;
            };
            let Some(ss) = s.active_screen_search() else {
                return Result::NoValue;
            };
            let Some(hl) = ss.selected_match() else {
                return Result::NoValue;
            };
            // SAFETY: `out` is a writable `CSelection` (caller contract).
            unsafe { *(out as *mut CSelection) = selection_from_highlight(&hl) };
        }

        // .matches => {
        SearchData::Matches => {
            let out = out as *mut CSelectionBuffer;
            //     const ss = if (wrapper.search) |*s|
            //         s.activeScreenSearch()
            //     else
            //         null;
            // SAFETY: `wrapper` is live.
            let ss = match (unsafe { &mut (*wrapper).search }).as_mut() {
                Some(s) => s.active_screen_search(),
                None => None,
            };
            //     const total = if (ss) |v| v.matchesLen() else 0;
            let total = match ss.as_ref() {
                Some(v) => v.matches_len(),
                None => 0,
            };
            //     if (out.cap < total) {
            //         out.len = total;
            //         return .out_of_space;
            //     }
            // SAFETY: `out` is a writable `CSelectionBuffer` (caller contract).
            if unsafe { (*out).cap } < total {
                unsafe { (*out).len = total };
                return Result::OutOfSpace;
            }
            //     if (total > 0) {
            //         const dst = (out.ptr orelse return .invalid_value)[0..total];
            //         for (dst, 0..) |*d, i| {
            //             d.* = selectionFromHighlight(ss.?.matchAt(i).?);
            //         }
            //     }
            if total > 0 {
                let dst = unsafe { (*out).ptr };
                if dst.is_null() {
                    return Result::InvalidValue;
                }
                let ss = ss.expect("total > 0 implies an active screen search");
                for i in 0..total {
                    let hl = ss.match_at(i).expect("i < matchesLen");
                    // SAFETY: `dst` has `cap >= total` writable entries
                    // (caller contract for a non-null `ptr`).
                    unsafe { *dst.add(i) = selection_from_highlight(&hl) };
                }
            }
            //     out.len = total;
            unsafe { (*out).len = total };
        }

        // .viewport_matches => {
        SearchData::ViewportMatches => {
            let out = out as *mut CSelectionBuffer;
            //     const matches: []const FlattenedHighlight = if (wrapper.search) |*s|
            //         s.viewportMatches() catch return .out_of_memory
            //     else
            //         &.{};
            // SAFETY: `wrapper` is live.
            let matches: &[FlattenedHighlight] = match (unsafe { &mut (*wrapper).search }).as_mut()
            {
                Some(s) => match s.viewport_matches() {
                    Ok(m) => m,
                    Err(_) => return Result::OutOfMemory,
                },
                None => &[],
            };
            //     if (out.cap < matches.len) {
            //         out.len = matches.len;
            //         return .out_of_space;
            //     }
            // SAFETY: `out` is a writable `CSelectionBuffer` (caller contract).
            if unsafe { (*out).cap } < matches.len() {
                unsafe { (*out).len = matches.len() };
                return Result::OutOfSpace;
            }
            //     if (matches.len > 0) {
            //         const dst = (out.ptr orelse return .invalid_value)[0..matches.len];
            //         for (dst, matches) |*d, hl| d.* = selectionFromHighlight(hl);
            //     }
            if !matches.is_empty() {
                let dst = unsafe { (*out).ptr };
                if dst.is_null() {
                    return Result::InvalidValue;
                }
                for (i, hl) in matches.iter().enumerate() {
                    // SAFETY: `dst` has `cap >= matches.len()` writable entries
                    // (caller contract for a non-null `ptr`).
                    unsafe { *dst.add(i) = selection_from_highlight(hl) };
                }
            }
            //     out.len = matches.len;
            unsafe { (*out).len = matches.len() };
        }

        // .select_scroll => out.* = .fromZig(wrapper.select_scroll),
        SearchData::SelectScroll => {
            // SAFETY: `out` is a writable `SearchScroll` (caller contract).
            unsafe {
                *(out as *mut SearchScroll) = SearchScroll::from_zig((*wrapper).select_scroll)
            };
        }
    }

    // return .success;
    Result::Success
}

// fn selectionFromHighlight(hl: FlattenedHighlight) selection_c.CSelection {
//     const untracked = hl.untracked();
//     return .{
//         .start = .fromPin(untracked.start),
//         .end = .fromPin(untracked.end),
//         .rectangle = false,
//     };
// }
// PORT NOTE: `.size` rides `CSelection::default()` (the reference fills
// `.size = @sizeOf(CSelection)` via the field default). Takes `&Flattened`
// of either chunk storage: an owned result or a shallow copy
// (`FlattenedRef`), the reference's by-value `Flattened`.
fn selection_from_highlight<C: core::ops::Deref<Target = [Chunk]>>(
    hl: &Flattened<C>,
) -> CSelection {
    // const untracked = hl.untracked();
    let untracked = hl.untracked();
    CSelection {
        start: CGridRef::from_pin(untracked.start),
        end: CGridRef::from_pin(untracked.end),
        rectangle: false,
        ..CSelection::default()
    }
}

#[cfg(test)]
mod tests {
    //! PORT NOTE: ghostty `src/terminal/c/search.zig` `test {}` blocks, ported
    //! assertion-for-assertion. `&lib.alloc.test_allocator` becomes the C
    //! allocator from `with_test_allocator` (leaks caught by Miri per R6);
    //! `defer` frees run at the end of the closure in reverse order.

    use super::*;
    use crate::test_util::with_test_allocator;

    // fn testString(str: []const u8) lib.String {
    //     return .init(str);
    // }
    fn test_string(s: &[u8]) -> LibString {
        LibString::init(s)
    }

    // fn testNewSearch(terminal: terminal_c.Terminal, needle_str: []const u8) !Search {
    //     var search: Search = null;
    //     try testing.expectEqual(Result.success, new(
    //         &lib.alloc.test_allocator,
    //         &search,
    //         terminal,
    //     ));
    //     errdefer free(search);
    //     const needle_value = testString(needle_str);
    //     try testing.expectEqual(Result.success, set(search, .needle, &needle_value));
    //     return search;
    // }
    unsafe fn test_new_search(
        alloc: &CAllocator,
        terminal: terminal_c::Terminal,
        needle_str: &[u8],
    ) -> Search {
        let mut search: Search = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe {
            new(Some(alloc), &mut search, terminal)
        });
        let needle_value = test_string(needle_str);
        // errdefer free(search); — an `assert_eq!` failure aborts the test
        // (the leak is then irrelevant; Zig's errdefer only matters because
        // its testing allocator reports leaks as a second error).
        assert_eq!(Result::Success, unsafe {
            set(
                search,
                SearchOption::Needle,
                &needle_value as *const LibString as *const c_void,
            )
        });
        search
    }

    unsafe fn test_terminal(alloc: &CAllocator) -> terminal_c::Terminal {
        let mut terminal: terminal_c::Terminal = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe {
            terminal_c::new(alloc, &mut terminal, 10, 4)
        });
        terminal
    }

    unsafe fn vt_write(terminal: terminal_c::Terminal, s: &[u8]) {
        unsafe { terminal_c::vt_write(terminal, s.as_ptr(), s.len()) };
    }

    fn needle_bytes(s: &LibString) -> &[u8] {
        // SAFETY: a `.needle` read yields the search-owned needle bytes.
        unsafe { std::slice::from_raw_parts(s.ptr, s.len) }
    }

    // PORT NOTE: no reference test (the reference's arm is only reachable under
    // `testing.FailingAllocator`, which it does not use here). This is the
    // rustty proving test for parity S4-F3 / S176-F2: `search.h:340-362`
    // documents `GHOSTTY_OUT_OF_MEMORY` when `ghostty_search_new` cannot
    // register the search, and the reference's
    // `catch { alloc.destroy(wrapper); return .out_of_memory; }` both FREES the
    // wrapper and returns it. Both halves are asserted: the return value AND
    // that the search's allocator got its bytes back.
    //
    // Parity tail S7-6: the registry grows through the TERMINAL's allocator
    // (N262), so the failure is driven by a `FailingAllocator` under the
    // terminal (the port-only `REGISTER_TW` tripwire is retired), while the
    // wrapper lives on the search's own counting allocator.
    #[test]
    fn search_new_releases_the_wrapper_when_registry_growth_fails() {
        use rustty_vt::alloc::testing::{Config, CountingAllocator, FailingAllocator};

        let terminal_failing = FailingAllocator::over_heap(Config::default());
        let terminal_calloc = terminal_failing.allocator();
        let search_counting = CountingAllocator::over_heap();
        let search_calloc = search_counting.allocator();

        let terminal = unsafe { test_terminal(&terminal_calloc) };

        // Fail the terminal's next allocation: the registry's first growth.
        terminal_failing
            .fail_index
            .set(terminal_failing.alloc_index.get());
        let mut search: Search = std::ptr::null_mut();
        assert_eq!(Result::OutOfMemory, unsafe {
            new(Some(&search_calloc), &mut search, terminal)
        });
        assert!(search.is_null());
        assert!(terminal_failing.has_induced_failure.get());

        // The wrapper was allocated through the SEARCH allocator and handed
        // back to it (positively: one allocation and one free of the wrapper).
        assert_eq!(
            (1, 1),
            (
                search_counting.alloc_calls.get(),
                search_counting.free_calls.get()
            )
        );
        assert_eq!(0, search_counting.live_bytes.get());

        // The same call succeeds once the terminal allocator recovers, so the
        // assertion above is about the failure path and not about `new` being
        // broken.
        terminal_failing.fail_index.set(usize::MAX);
        let mut ok_search: Search = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe {
            new(Some(&search_calloc), &mut ok_search, terminal)
        });
        assert!(!ok_search.is_null());
        unsafe { free(ok_search) };

        unsafe { terminal_c::free(terminal) };
        assert_eq!(0, search_counting.live_bytes.get());
        assert_eq!(
            terminal_failing.allocated_bytes.get(),
            terminal_failing.freed_bytes.get()
        );
    }

    // test "search lifecycle and run to complete"
    #[test]
    fn search_lifecycle_and_run_to_complete() {
        with_test_allocator(|alloc| unsafe {
            let terminal = test_terminal(alloc);
            // defer terminal_c.free(terminal);

            // terminal_c.vt_write(terminal, "Fizz\r\nBuzz\r\nFizz\r\nBang", 22);
            vt_write(terminal, b"Fizz\r\nBuzz\r\nFizz\r\nBang");

            let search: Search = test_new_search(alloc, terminal, b"Fizz");
            // defer free(search);

            // Right after the needle is set the search must report
            // feed_required, not complete. The internal completion check is
            // trivially true before the search has seen the terminal.
            let mut status: SearchStatus = SearchStatus::Complete;
            assert_eq!(
                Result::Success,
                get(
                    search,
                    SearchData::Status,
                    &mut status as *mut SearchStatus as *mut c_void
                )
            );
            assert_eq!(SearchStatus::FeedRequired, status);

            assert_eq!(Result::Success, run(search));
            assert_eq!(
                Result::Success,
                get(
                    search,
                    SearchData::Status,
                    &mut status as *mut SearchStatus as *mut c_void
                )
            );
            assert_eq!(SearchStatus::Complete, status);

            let mut total: usize = 0;
            assert_eq!(
                Result::Success,
                get(
                    search,
                    SearchData::TotalMatches,
                    &mut total as *mut usize as *mut c_void
                )
            );
            assert_eq!(2usize, total);

            // The needle is borrowed and matches what we searched for.
            let mut needle: LibString = LibString::init(&[]);
            assert_eq!(
                Result::Success,
                get(
                    search,
                    SearchData::Needle,
                    &mut needle as *mut LibString as *mut c_void
                )
            );
            assert_eq!(b"Fizz", needle_bytes(&needle));

            free(search);
            terminal_c::free(terminal);
        });
    }

    // test "search tick reports feed required before first feed"
    #[test]
    fn search_tick_reports_feed_required_before_first_feed() {
        with_test_allocator(|alloc| unsafe {
            let terminal = test_terminal(alloc);
            // defer terminal_c.free(terminal);

            let search: Search = test_new_search(alloc, terminal, b"Fizz");
            // defer free(search);

            let mut status: SearchStatus = SearchStatus::Complete;
            assert_eq!(Result::Success, tick(search, &mut status));
            assert_eq!(SearchStatus::FeedRequired, status);

            // NULL status out is allowed.
            assert_eq!(Result::Success, tick(search, std::ptr::null_mut()));

            free(search);
            terminal_c::free(terminal);
        });
    }

    // test "search feed after write updates results"
    #[test]
    fn search_feed_after_write_updates_results() {
        with_test_allocator(|alloc| unsafe {
            let terminal = test_terminal(alloc);
            // defer terminal_c.free(terminal);

            // terminal_c.vt_write(terminal, "Fizz", 4);
            vt_write(terminal, b"Fizz");

            let search: Search = test_new_search(alloc, terminal, b"Fizz");
            // defer free(search);

            assert_eq!(Result::Success, run(search));
            let mut total: usize = 0;
            assert_eq!(
                Result::Success,
                get(
                    search,
                    SearchData::TotalMatches,
                    &mut total as *mut usize as *mut c_void
                )
            );
            assert_eq!(1usize, total);

            // Write another match: the caller-driven feed is the "terminal
            // changed" signal.
            // terminal_c.vt_write(terminal, "\r\nFizz", 6);
            vt_write(terminal, b"\r\nFizz");
            assert_eq!(Result::Success, run(search));
            assert_eq!(
                Result::Success,
                get(
                    search,
                    SearchData::TotalMatches,
                    &mut total as *mut usize as *mut c_void
                )
            );
            assert_eq!(2usize, total);

            free(search);
            terminal_c::free(terminal);
        });
    }

    // test "search alt screen flip and return retains results"
    #[test]
    fn search_alt_screen_flip_and_return_retains_results() {
        with_test_allocator(|alloc| unsafe {
            let terminal = test_terminal(alloc);
            // defer terminal_c.free(terminal);

            // terminal_c.vt_write(terminal, "Fizz\r\nFizz", 10);
            vt_write(terminal, b"Fizz\r\nFizz");

            let search: Search = test_new_search(alloc, terminal, b"Fizz");
            // defer free(search);

            assert_eq!(Result::Success, run(search));
            let mut total: usize = 0;
            let total_ptr = (&raw mut total).cast::<c_void>();
            assert_eq!(
                Result::Success,
                get(search, SearchData::TotalMatches, total_ptr)
            );
            assert_eq!(2usize, total);

            // Flip to the alternate screen: reads now reflect that screen.
            // terminal_c.vt_write(terminal, "\x1b[?1049h", 8);
            vt_write(terminal, b"\x1b[?1049h");
            assert_eq!(Result::Success, run(search));
            assert_eq!(
                Result::Success,
                get(search, SearchData::TotalMatches, total_ptr)
            );
            assert_eq!(0usize, total);

            // terminal_c.vt_write(terminal, "Fizz", 4);
            vt_write(terminal, b"Fizz");
            assert_eq!(Result::Success, run(search));
            assert_eq!(
                Result::Success,
                get(search, SearchData::TotalMatches, total_ptr)
            );
            assert_eq!(1usize, total);

            // Return to the primary screen: results are retained and restored.
            // terminal_c.vt_write(terminal, "\x1b[?1049l", 8);
            vt_write(terminal, b"\x1b[?1049l");
            assert_eq!(Result::Success, run(search));
            assert_eq!(
                Result::Success,
                get(search, SearchData::TotalMatches, total_ptr)
            );
            assert_eq!(2usize, total);

            free(search);
            terminal_c::free(terminal);
        });
    }

    // test "search select wraps in both directions"
    #[test]
    fn search_select_wraps_in_both_directions() {
        with_test_allocator(|alloc| unsafe {
            let terminal = test_terminal(alloc);
            // defer terminal_c.free(terminal);

            // terminal_c.vt_write(terminal, "Fizz\r\nBuzz\r\nFizz", 16);
            vt_write(terminal, b"Fizz\r\nBuzz\r\nFizz");

            let search: Search = test_new_search(alloc, terminal, b"Fizz");
            // defer free(search);
            assert_eq!(Result::Success, run(search));

            // No selection yet.
            let mut idx: usize = 999;
            let idx_ptr = (&raw mut idx).cast::<c_void>();
            assert_eq!(
                Result::NoValue,
                get(search, SearchData::SelectedIndex, idx_ptr)
            );

            // Next: newest match first (index 0), then older (1), then wrap
            // back to 0.
            let null = std::ptr::null::<c_void>();
            assert_eq!(Result::Success, set(search, SearchOption::SelectNext, null));
            assert_eq!(
                Result::Success,
                get(search, SearchData::SelectedIndex, idx_ptr)
            );
            assert_eq!(0usize, idx);

            // var match: selection_c.CSelection = undefined;
            let mut match_: CSelection = CSelection::default();
            assert_eq!(
                Result::Success,
                get(
                    search,
                    SearchData::SelectedMatch,
                    &mut match_ as *mut CSelection as *mut c_void
                )
            );
            assert!(match_.start.to_pin().is_some());

            assert_eq!(Result::Success, set(search, SearchOption::SelectNext, null));
            assert_eq!(
                Result::Success,
                get(search, SearchData::SelectedIndex, idx_ptr)
            );
            assert_eq!(1usize, idx);

            assert_eq!(Result::Success, set(search, SearchOption::SelectNext, null));
            assert_eq!(
                Result::Success,
                get(search, SearchData::SelectedIndex, idx_ptr)
            );
            assert_eq!(0usize, idx);

            // Prev: wrap backward to the oldest match.
            assert_eq!(Result::Success, set(search, SearchOption::SelectPrev, null));
            assert_eq!(
                Result::Success,
                get(search, SearchData::SelectedIndex, idx_ptr)
            );
            assert_eq!(1usize, idx);

            free(search);
            terminal_c::free(terminal);
        });
    }

    // test "search select with no matches returns no value"
    #[test]
    fn search_select_with_no_matches_returns_no_value() {
        with_test_allocator(|alloc| unsafe {
            let terminal = test_terminal(alloc);
            // defer terminal_c.free(terminal);

            let search: Search = test_new_search(alloc, terminal, b"Fizz");
            // defer free(search);

            let null = std::ptr::null::<c_void>();
            assert_eq!(Result::NoValue, set(search, SearchOption::SelectNext, null));
            assert_eq!(Result::NoValue, set(search, SearchOption::SelectPrev, null));

            free(search);
            terminal_c::free(terminal);
        });
    }

    // test "search select rejects a non-null reserved value"
    #[test]
    fn search_select_rejects_a_non_null_reserved_value() {
        with_test_allocator(|alloc| unsafe {
            let terminal = test_terminal(alloc);
            // defer terminal_c.free(terminal);

            let search: Search = test_new_search(alloc, terminal, b"Fizz");
            // defer free(search);

            // const bogus: c_int = 0;
            let bogus: c_int = 0;
            let bogus_ptr = &bogus as *const c_int as *const c_void;
            assert_eq!(
                Result::InvalidValue,
                set(search, SearchOption::SelectNext, bogus_ptr)
            );
            assert_eq!(
                Result::InvalidValue,
                set(search, SearchOption::SelectPrev, bogus_ptr)
            );

            free(search);
            terminal_c::free(terminal);
        });
    }

    // test "search select scroll option"
    #[test]
    fn search_select_scroll_option() {
        with_test_allocator(|alloc| unsafe {
            let terminal = test_terminal(alloc);
            // defer terminal_c.free(terminal);

            let search: Search = test_new_search(alloc, terminal, b"Fizz");
            // defer free(search);

            let mut scroll: SearchScroll = SearchScroll::None;
            let scroll_ptr = (&raw mut scroll).cast::<c_void>();
            assert_eq!(
                Result::Success,
                get(search, SearchData::SelectScroll, scroll_ptr)
            );
            assert_eq!(SearchScroll::IfNeeded, scroll);

            // const none: Scroll = .none;
            let none: SearchScroll = SearchScroll::None;
            assert_eq!(
                Result::Success,
                set(
                    search,
                    SearchOption::SelectScroll,
                    &none as *const SearchScroll as *const c_void
                )
            );
            assert_eq!(
                Result::Success,
                get(search, SearchData::SelectScroll, scroll_ptr)
            );
            assert_eq!(SearchScroll::None, scroll);

            // NULL resets to the default.
            assert_eq!(
                Result::Success,
                set(search, SearchOption::SelectScroll, std::ptr::null())
            );
            assert_eq!(
                Result::Success,
                get(search, SearchData::SelectScroll, scroll_ptr)
            );
            assert_eq!(SearchScroll::IfNeeded, scroll);

            // Invalid enum values are rejected.
            // const bogus: c_int = 42;
            let bogus: c_int = 42;
            assert_eq!(
                Result::InvalidValue,
                set(
                    search,
                    SearchOption::SelectScroll,
                    &bogus as *const c_int as *const c_void
                )
            );

            free(search);
            terminal_c::free(terminal);
        });
    }

    // test "search selection dropped when reset prunes results"
    #[test]
    fn search_selection_dropped_when_reset_prunes_results() {
        with_test_allocator(|alloc| unsafe {
            let terminal = test_terminal(alloc);
            // defer terminal_c.free(terminal);

            // terminal_c.vt_write(terminal, "Fizz\r\nBuzz", 10);
            vt_write(terminal, b"Fizz\r\nBuzz");

            let search: Search = test_new_search(alloc, terminal, b"Fizz");
            // defer free(search);
            assert_eq!(Result::Success, run(search));
            assert_eq!(
                Result::Success,
                set(search, SearchOption::SelectNext, std::ptr::null())
            );

            // A selection survives unrelated writes...
            // terminal_c.vt_write(terminal, "\r\nBang", 6);
            vt_write(terminal, b"\r\nBang");
            assert_eq!(Result::Success, run(search));
            // var match: selection_c.CSelection = undefined;
            let mut match_: CSelection = CSelection::default();
            let match_ptr = (&raw mut match_).cast::<c_void>();
            assert_eq!(
                Result::Success,
                get(search, SearchData::SelectedMatch, match_ptr)
            );

            // ...but a full reset prunes every result, dropping the selection.
            // terminal_c.vt_write(terminal, "\x1bc", 2);
            vt_write(terminal, b"\x1bc");
            assert_eq!(Result::Success, run(search));
            let mut total: usize = 999;
            assert_eq!(
                Result::Success,
                get(
                    search,
                    SearchData::TotalMatches,
                    &mut total as *mut usize as *mut c_void
                )
            );
            assert_eq!(0usize, total);
            assert_eq!(
                Result::NoValue,
                get(search, SearchData::SelectedMatch, match_ptr)
            );

            free(search);
            terminal_c::free(terminal);
        });
    }

    // test "search matches buffer semantics"
    #[test]
    fn search_matches_buffer_semantics() {
        with_test_allocator(|alloc| unsafe {
            let terminal = test_terminal(alloc);
            // defer terminal_c.free(terminal);

            // terminal_c.vt_write(terminal, "Fizz\r\nBuzz\r\nFizz", 16);
            vt_write(terminal, b"Fizz\r\nBuzz\r\nFizz");

            let search: Search = test_new_search(alloc, terminal, b"Fizz");
            // defer free(search);
            assert_eq!(Result::Success, run(search));

            // NULL ptr with cap 0 queries the required capacity.
            // var buf: selection_c.CSelectionBuffer = .{};
            // PORT NOTE (SB): the pointer is re-taken per call, as the
            // reference's `&buf` is — `buf` is reassigned by value between
            // calls, which would invalidate a hoisted raw tag (Miri-proven).
            let mut buf: CSelectionBuffer = CSelectionBuffer::default();
            assert_eq!(
                Result::OutOfSpace,
                get(
                    search,
                    SearchData::Matches,
                    &mut buf as *mut CSelectionBuffer as *mut c_void
                )
            );
            assert_eq!(2usize, buf.len);

            // Too-small buffers report the required capacity.
            // var one: [1]selection_c.CSelection = undefined;
            let mut one: [CSelection; 1] = [CSelection::default(); 1];
            buf = CSelectionBuffer {
                ptr: one.as_mut_ptr(),
                cap: one.len(),
                ..CSelectionBuffer::default()
            };
            assert_eq!(
                Result::OutOfSpace,
                get(
                    search,
                    SearchData::Matches,
                    &mut buf as *mut CSelectionBuffer as *mut c_void
                )
            );
            assert_eq!(2usize, buf.len);

            // A large-enough buffer is filled newest to oldest.
            // var storage: [4]selection_c.CSelection = undefined;
            let mut storage: [CSelection; 4] = [CSelection::default(); 4];
            buf = CSelectionBuffer {
                ptr: storage.as_mut_ptr(),
                cap: storage.len(),
                ..CSelectionBuffer::default()
            };
            assert_eq!(
                Result::Success,
                get(
                    search,
                    SearchData::Matches,
                    &mut buf as *mut CSelectionBuffer as *mut c_void
                )
            );
            assert_eq!(2usize, buf.len);
            // const first = storage[0].start.toPin().?;
            let first = storage[0].start.to_pin().unwrap();
            // const second = storage[1].start.toPin().?;
            let second = storage[1].start.to_pin().unwrap();
            assert!(first.y != second.y || first.node != second.node);

            // Viewport matches use the same conventions and are cached across
            // reads.
            buf = CSelectionBuffer::default();
            assert_eq!(
                Result::OutOfSpace,
                get(
                    search,
                    SearchData::ViewportMatches,
                    &mut buf as *mut CSelectionBuffer as *mut c_void
                )
            );
            assert_eq!(2usize, buf.len);
            buf = CSelectionBuffer {
                ptr: storage.as_mut_ptr(),
                cap: storage.len(),
                ..CSelectionBuffer::default()
            };
            assert_eq!(
                Result::Success,
                get(
                    search,
                    SearchData::ViewportMatches,
                    &mut buf as *mut CSelectionBuffer as *mut c_void
                )
            );
            assert_eq!(2usize, buf.len);
            buf = CSelectionBuffer {
                ptr: storage.as_mut_ptr(),
                cap: storage.len(),
                ..CSelectionBuffer::default()
            };
            assert_eq!(
                Result::Success,
                get(
                    search,
                    SearchData::ViewportMatches,
                    &mut buf as *mut CSelectionBuffer as *mut c_void
                )
            );
            assert_eq!(2usize, buf.len);

            free(search);
            terminal_c::free(terminal);
        });
    }

    // test "search get_multi returns first failing index"
    #[test]
    fn search_get_multi_returns_first_failing_index() {
        with_test_allocator(|alloc| unsafe {
            let terminal = test_terminal(alloc);
            // defer terminal_c.free(terminal);

            // terminal_c.vt_write(terminal, "Fizz", 4);
            vt_write(terminal, b"Fizz");

            let search: Search = test_new_search(alloc, terminal, b"Fizz");
            // defer free(search);
            assert_eq!(Result::Success, run(search));

            // No selection: selected_index fails at index 1.
            // const keys = [_]Data{ .total_matches, .selected_index, .status };
            let keys = [
                SearchData::TotalMatches,
                SearchData::SelectedIndex,
                SearchData::Status,
            ];
            let mut total: usize = 0;
            let mut idx: usize = 0;
            let mut status: SearchStatus = SearchStatus::Running;
            // var values = [_]?*anyopaque{ &total, &idx, &status };
            // N505 (P7-W15): `&raw mut` out-pointers, no reborrow tag, so the
            // test's direct reads of `total`/`idx`/`status` between calls do
            // not freeze the pointers `get_multi` writes through (Tree Borrows).
            let mut values: [*mut c_void; 3] = [
                (&raw mut total).cast::<c_void>(),
                (&raw mut idx).cast::<c_void>(),
                (&raw mut status).cast::<c_void>(),
            ];
            let mut written: usize = 999;
            assert_eq!(
                Result::NoValue,
                get_multi(
                    search,
                    keys.len(),
                    keys.as_ptr() as *const c_int,
                    values.as_mut_ptr(),
                    &mut written,
                )
            );
            assert_eq!(1usize, written);
            assert_eq!(1usize, total);

            // After selecting, the whole batch succeeds.
            assert_eq!(
                Result::Success,
                set(search, SearchOption::SelectNext, std::ptr::null())
            );
            assert_eq!(
                Result::Success,
                get_multi(
                    search,
                    keys.len(),
                    keys.as_ptr() as *const c_int,
                    values.as_mut_ptr(),
                    &mut written,
                )
            );
            assert_eq!(keys.len(), written);
            assert_eq!(0usize, idx);

            free(search);
            terminal_c::free(terminal);
        });
    }

    // test "search new validates options"
    #[test]
    fn search_new_validates_options() {
        with_test_allocator(|alloc| unsafe {
            let terminal = test_terminal(alloc);
            // defer terminal_c.free(terminal);

            let mut search: Search = std::ptr::null_mut();

            // NULL out and NULL terminal are invalid.
            assert_eq!(
                Result::InvalidValue,
                new(Some(alloc), std::ptr::null_mut(), terminal)
            );
            assert_eq!(
                Result::InvalidValue,
                new(Some(alloc), &mut search, std::ptr::null_mut())
            );
            assert!(search.is_null());

            terminal_c::free(terminal);
        });
    }

    // test "search without a needle is idle"
    #[test]
    fn search_without_a_needle_is_idle() {
        with_test_allocator(|alloc| unsafe {
            let terminal = test_terminal(alloc);
            // defer terminal_c.free(terminal);

            // terminal_c.vt_write(terminal, "Fizz", 4);
            vt_write(terminal, b"Fizz");

            let mut search: Search = std::ptr::null_mut();
            assert_eq!(Result::Success, new(Some(alloc), &mut search, terminal));
            // defer free(search);

            // With nothing to look for, the search is complete and empty, and
            // driving it is a harmless no-op.
            let mut status: SearchStatus = SearchStatus::Running;
            assert_eq!(
                Result::Success,
                get(
                    search,
                    SearchData::Status,
                    &mut status as *mut SearchStatus as *mut c_void
                )
            );
            assert_eq!(SearchStatus::Complete, status);
            assert_eq!(Result::Success, feed(search));
            assert_eq!(Result::Success, run(search));
            assert_eq!(Result::Success, tick(search, &mut status));
            assert_eq!(SearchStatus::Complete, status);

            // var needle_out: lib.String = undefined;
            let mut needle_out: LibString = LibString::init(&[]);
            assert_eq!(
                Result::NoValue,
                get(
                    search,
                    SearchData::Needle,
                    &mut needle_out as *mut LibString as *mut c_void
                )
            );
            let mut total: usize = 999;
            assert_eq!(
                Result::Success,
                get(
                    search,
                    SearchData::TotalMatches,
                    &mut total as *mut usize as *mut c_void
                )
            );
            assert_eq!(0usize, total);
            assert_eq!(
                Result::NoValue,
                set(search, SearchOption::SelectNext, std::ptr::null())
            );

            // var buf: selection_c.CSelectionBuffer = .{};
            // PORT NOTE (SB): the pointer is re-taken per call, as the
            // reference's `&buf` is — `buf` is reassigned by value between
            // calls, which would invalidate a hoisted raw tag (Miri-proven).
            let mut buf: CSelectionBuffer = CSelectionBuffer::default();
            assert_eq!(
                Result::Success,
                get(
                    search,
                    SearchData::Matches,
                    &mut buf as *mut CSelectionBuffer as *mut c_void
                )
            );
            assert_eq!(0usize, buf.len);
            buf = CSelectionBuffer::default();
            assert_eq!(
                Result::Success,
                get(
                    search,
                    SearchData::ViewportMatches,
                    &mut buf as *mut CSelectionBuffer as *mut c_void
                )
            );
            assert_eq!(0usize, buf.len);

            // Clearing an already-clear needle is fine.
            assert_eq!(
                Result::Success,
                set(search, SearchOption::Needle, std::ptr::null())
            );

            free(search);
            terminal_c::free(terminal);
        });
    }

    // test "search needle change and clear"
    #[test]
    fn search_needle_change_and_clear() {
        with_test_allocator(|alloc| unsafe {
            let terminal = test_terminal(alloc);
            // defer terminal_c.free(terminal);

            // terminal_c.vt_write(terminal, "Fizz\r\nBuzz", 10);
            vt_write(terminal, b"Fizz\r\nBuzz");

            let search: Search = test_new_search(alloc, terminal, b"Fizz");
            // defer free(search);
            assert_eq!(Result::Success, run(search));
            assert_eq!(
                Result::Success,
                set(search, SearchOption::SelectNext, std::ptr::null())
            );

            let mut total: usize = 0;
            let total_ptr = (&raw mut total).cast::<c_void>();
            assert_eq!(
                Result::Success,
                get(search, SearchData::TotalMatches, total_ptr)
            );
            assert_eq!(1usize, total);

            // Setting the same needle again (any ASCII case) keeps existing
            // results and the selection.
            // const same = testString("fIZZ");
            let same = test_string(b"fIZZ");
            assert_eq!(
                Result::Success,
                set(
                    search,
                    SearchOption::Needle,
                    &same as *const LibString as *const c_void
                )
            );
            // var needle_out: lib.String = undefined;
            let mut needle_out: LibString = LibString::init(&[]);
            let needle_ptr = (&raw mut needle_out).cast::<c_void>();
            assert_eq!(Result::Success, get(search, SearchData::Needle, needle_ptr));
            assert_eq!(b"Fizz", needle_bytes(&needle_out));
            let mut idx: usize = 999;
            let idx_ptr = (&raw mut idx).cast::<c_void>();
            assert_eq!(
                Result::Success,
                get(search, SearchData::SelectedIndex, idx_ptr)
            );
            assert_eq!(0usize, idx);

            // Changing the needle restarts the search from scratch.
            // const changed = testString("Buzz");
            let changed = test_string(b"Buzz");
            assert_eq!(
                Result::Success,
                set(
                    search,
                    SearchOption::Needle,
                    &changed as *const LibString as *const c_void
                )
            );
            let mut status: SearchStatus = SearchStatus::Complete;
            let status_ptr = (&raw mut status).cast::<c_void>();
            assert_eq!(Result::Success, get(search, SearchData::Status, status_ptr));
            assert_eq!(SearchStatus::FeedRequired, status);
            assert_eq!(
                Result::NoValue,
                get(search, SearchData::SelectedIndex, idx_ptr)
            );
            assert_eq!(Result::Success, run(search));
            assert_eq!(
                Result::Success,
                get(search, SearchData::TotalMatches, total_ptr)
            );
            assert_eq!(1usize, total);
            assert_eq!(Result::Success, get(search, SearchData::Needle, needle_ptr));
            assert_eq!(b"Buzz", needle_bytes(&needle_out));

            // An empty needle clears the search, same as NULL.
            // const empty = testString("");
            let empty = test_string(b"");
            assert_eq!(
                Result::Success,
                set(
                    search,
                    SearchOption::Needle,
                    &empty as *const LibString as *const c_void
                )
            );
            assert_eq!(Result::NoValue, get(search, SearchData::Needle, needle_ptr));
            assert_eq!(Result::Success, get(search, SearchData::Status, status_ptr));
            assert_eq!(SearchStatus::Complete, status);
            assert_eq!(
                Result::Success,
                get(search, SearchData::TotalMatches, total_ptr)
            );
            assert_eq!(0usize, total);

            free(search);
            terminal_c::free(terminal);
        });
    }

    // test "search free after terminal free"
    #[test]
    fn search_free_after_terminal_free() {
        with_test_allocator(|alloc| unsafe {
            let terminal = test_terminal(alloc);

            // terminal_c.vt_write(terminal, "Fizz\r\nFizz", 10);
            vt_write(terminal, b"Fizz\r\nFizz");

            let search: Search = test_new_search(alloc, terminal, b"Fizz");

            // Run and select so the search holds tracked pins within the
            // terminal's page storage.
            assert_eq!(Result::Success, run(search));
            assert_eq!(
                Result::Success,
                set(search, SearchOption::SelectNext, std::ptr::null())
            );

            // Free the terminal first. The search detaches: calls that need
            // the terminal fail cleanly instead of touching freed memory.
            terminal_c::free(terminal);
            assert_eq!(Result::InvalidValue, feed(search));
            assert_eq!(Result::InvalidValue, run(search));
            // try testing.expectEqual(Result.invalid_value, tick(search, null));
            assert_eq!(Result::InvalidValue, tick(search, std::ptr::null_mut()));
            assert_eq!(
                Result::InvalidValue,
                set(search, SearchOption::SelectNext, std::ptr::null())
            );
            // const needle_value = testString("Buzz");
            let needle_value = test_string(b"Buzz");
            assert_eq!(
                Result::InvalidValue,
                set(
                    search,
                    SearchOption::Needle,
                    &needle_value as *const LibString as *const c_void
                )
            );

            // Reads that only touch search-owned state still answer.
            // var needle_out: lib.String = undefined;
            let mut needle_out: LibString = LibString::init(&[]);
            assert_eq!(
                Result::Success,
                get(
                    search,
                    SearchData::Needle,
                    &mut needle_out as *mut LibString as *mut c_void
                )
            );
            assert_eq!(b"Fizz", needle_bytes(&needle_out));
            // const scroll: Scroll = .none;
            let scroll: SearchScroll = SearchScroll::None;
            assert_eq!(
                Result::Success,
                set(
                    search,
                    SearchOption::SelectScroll,
                    &scroll as *const SearchScroll as *const c_void
                )
            );

            // The search can still be freed, releasing only its own memory.
            free(search);
        });
    }

    // test "search tick after terminal free with history"
    #[test]
    fn search_tick_after_terminal_free_with_history() {
        with_test_allocator(|alloc| unsafe {
            // var terminal: terminal_c.Terminal = null;
            // try testing.expectEqual(Result.success, terminal_c.new(
            //     &lib.alloc.test_allocator,
            //     &terminal,
            //     10,
            //     3,
            // ));
            let mut terminal: terminal_c::Terminal = std::ptr::null_mut();
            assert_eq!(
                Result::Success,
                terminal_c::new(alloc, &mut terminal, 10, 3)
            );

            // Push matches into scrollback so the search has history.
            // terminal_c.vt_write(terminal, "Fizz\r\nFizz\r\nFizz\r\nFizz\r\n", 24);
            vt_write(terminal, b"Fizz\r\nFizz\r\nFizz\r\nFizz\r\n");

            // const search: Search = try testNewSearch(terminal, "Fizz");
            let search: Search = test_new_search(alloc, terminal, b"Fizz");
            // defer free(search);

            assert_eq!(Result::Success, feed(search));
            // var status: Status = .complete;
            let mut status: SearchStatus = SearchStatus::Complete;
            assert_eq!(
                Result::Success,
                get(
                    search,
                    SearchData::Status,
                    &mut status as *mut SearchStatus as *mut c_void
                )
            );
            assert_eq!(SearchStatus::Running, status);

            // The history search's tracked pin is freed with the terminal.
            terminal_c::free(terminal);
            assert_eq!(Result::InvalidValue, tick(search, &mut status));

            free(search);
        });
    }

    // test "search freed before terminal detaches from the registry"
    #[test]
    fn search_freed_before_terminal_detaches_from_the_registry() {
        with_test_allocator(|alloc| unsafe {
            let terminal = test_terminal(alloc);
            // defer terminal_c.free(terminal);

            // Create two searches and free one while the terminal is alive.
            // The freed search must be unregistered so the later terminal free
            // only detaches the survivor.
            let a: Search = test_new_search(alloc, terminal, b"Fizz");
            let b: Search = test_new_search(alloc, terminal, b"Fizz");
            // defer free(b);

            assert_eq!(Result::Success, run(a));
            free(a);
            assert_eq!(Result::Success, run(b));

            free(b);
            terminal_c::free(terminal);
        });
    }

    // Parity tail S7-4b (routing evidence, no reference counterpart): the
    // terminal's `searches` registry grows through the TERMINAL's allocator
    // (`wrapper.searches.putNoClobber(t.gpa(), ..)`, bound in `wrap`), not the
    // search's: a search created over another allocator registers itself in a
    // map bound to the terminal's, whose growth is visible there, and freeing
    // the search and then the terminal returns every byte.
    #[test]
    fn search_registry_routes_through_the_terminals_allocator() {
        use rustty_vt::alloc::testing::CountingAllocator;
        let counting = CountingAllocator::over_heap();
        let calloc = counting.allocator();
        with_test_allocator(|search_alloc| unsafe {
            let terminal = test_terminal(&calloc);
            let live = counting.live_bytes.get();
            let before = counting.alloc_calls.get();
            let search: Search = test_new_search(search_alloc, terminal, b"Fizz");
            assert_eq!(1, (*terminal).searches.count());
            assert_eq!(
                rustty_vt::alloc::default(Some(&calloc)),
                *(*terminal).searches.allocator(),
                "the registry is bound to the terminal's allocator"
            );
            assert!(
                counting.alloc_calls.get() > before && counting.live_bytes.get() > live,
                "the registry's growth reaches the terminal's allocator"
            );
            free(search);
            assert_eq!(0, (*terminal).searches.count());
            terminal_c::free(terminal);
        });
        assert_eq!(0, counting.live_bytes.get(), "every byte is returned");
    }

    // test "search free null"
    #[test]
    fn search_free_null() {
        unsafe { free(std::ptr::null_mut()) };
    }

    // ---------------------------------------------------------------------
    // Parity tail S7-6: allocator routing (S176-F1, S4-F2 search half), the
    // terminal-charged split (N262), the C-API failure sweep, and UD305.
    // Rust-only tests: `c/search.zig` has no allocator test.
    // ---------------------------------------------------------------------

    use rustty_vt::alloc::testing::{Config, CountingAllocator, FailingAllocator};

    /// Pins tracked by the terminal's active screen.
    unsafe fn tracked_pins(terminal: terminal_c::Terminal) -> usize {
        unsafe {
            (*(*(*terminal).terminal).screens.active)
                .pages
                .tracked_pins
                .count()
        }
    }

    /// A match on each of `pages_wanted` pages of history. With two, a feed has
    /// a page to append (the history searcher walks backwards one page per
    /// feed). The sweeps use one page under Miri (history is still searched:
    /// the rows that scrolled out of the active area).
    unsafe fn write_corpus_pages(terminal: terminal_c::Terminal, pages_wanted: usize) {
        let pages = || unsafe {
            (*(*(*terminal).terminal).screens.active)
                .pages
                .total_pages()
        };
        for page in 1..=pages_wanted {
            unsafe { vt_write(terminal, b"Fizz\r\nBuzz\r\n") };
            let mut lines = 0usize;
            while pages() < page || lines < 6 {
                unsafe { vt_write(terminal, b"\r\n") };
                lines += 1;
            }
        }
        unsafe { vt_write(terminal, b"Fizz\r\n\r\n\r\n\r\n\r\nFizz.") };
    }

    /// The corpus: two pages natively, one under Miri (the small corpus).
    unsafe fn write_corpus(terminal: terminal_c::Terminal) {
        unsafe { write_corpus_pages(terminal, if cfg!(miri) { 1 } else { 2 }) };
    }

    unsafe fn set_needle(search: Search, needle: &[u8]) -> Result {
        let v = test_string(needle);
        unsafe {
            set(
                search,
                SearchOption::Needle,
                &v as *const LibString as *const c_void,
            )
        }
    }

    unsafe fn search_status(search: Search) -> SearchStatus {
        let mut status = SearchStatus::Complete;
        assert_eq!(Result::Success, unsafe {
            get(
                search,
                SearchData::Status,
                &mut status as *mut SearchStatus as *mut c_void,
            )
        });
        status
    }

    /// The bytes the search holds from its allocator: the wrapper and every
    /// container of its `TerminalSearch`.
    unsafe fn search_owned_bytes(search: Search) -> usize {
        size_of::<SearchWrapper>()
            + unsafe { (*search).search.as_ref() }.map_or(0, |s| s.owned_bytes())
    }

    // The wrapper and every allocation of the `TerminalSearch` are on the
    // allocator given to `ghostty_search_new` (the counting allocator's live
    // bytes equal what the search holds, at every step); the registry entry
    // and the tracked pins are on the terminal's (N262).
    #[test]
    #[cfg_attr(
        miri,
        ignore = "MIRI EXCEPTION (rustty-vt-c, parity tail C-1): intractable under Miri, two full pages of scrollback written through the terminal; measured past a 20-minute bound; runs natively and under ASan (see MIRI_EXCEPTIONS.md)"
    )]
    fn search_internals_route_through_the_callers_allocator() {
        let term = CountingAllocator::over_heap();
        let tc = term.allocator();
        let srch = CountingAllocator::over_heap();
        let sc = srch.allocator();
        unsafe {
            let terminal = test_terminal(&tc);
            write_corpus_pages(terminal, 2);
            let pins_before = tracked_pins(terminal);
            let registry_before = (*terminal).searches.count();

            let mut search: Search = std::ptr::null_mut();
            assert_eq!(Result::Success, new(Some(&sc), &mut search, terminal));
            // `alloc.create(SearchWrapper)`: the one allocation of `new`.
            assert_eq!(1, srch.alloc_calls.get());
            assert_eq!(
                Some((
                    size_of::<SearchWrapper>(),
                    std::mem::align_of::<SearchWrapper>().trailing_zeros() as u8
                )),
                srch.last_alloc.get()
            );
            // The registry entry is the terminal's.
            assert_eq!(registry_before + 1, (*terminal).searches.count());
            assert_eq!(
                rustty_vt::alloc::default(Some(&tc)),
                *(*terminal).searches.allocator()
            );

            // `TerminalSearch.init(wrapper.alloc, needle)`: the needle dupe and
            // the overlap buffer.
            assert_eq!(Result::Success, set_needle(search, b"Fizz"));
            assert_eq!(3, srch.alloc_calls.get());
            assert_eq!(search_owned_bytes(search), srch.live_bytes.get());

            assert_eq!(Result::Success, run(search));
            assert_eq!(SearchStatus::Complete, search_status(search));
            assert_eq!(search_owned_bytes(search), srch.live_bytes.get());
            // The history searcher's two pins are the terminal's.
            assert_eq!(pins_before + 2, tracked_pins(terminal));

            let mut buf = [CSelection::default(); 16];
            let mut out = CSelectionBuffer {
                ptr: buf.as_mut_ptr(),
                cap: buf.len(),
                len: 0,
            };
            assert_eq!(
                Result::Success,
                get(
                    search,
                    SearchData::ViewportMatches,
                    &mut out as *mut CSelectionBuffer as *mut c_void,
                )
            );
            assert!(out.len >= 1);
            assert_eq!(search_owned_bytes(search), srch.live_bytes.get());
            // `.matches` copies shallow results: no allocation.
            let calls = srch.alloc_calls.get();
            out.len = 0;
            assert_eq!(
                Result::Success,
                get(
                    search,
                    SearchData::Matches,
                    &mut out as *mut CSelectionBuffer as *mut c_void,
                )
            );
            assert!(out.len >= 3);
            assert_eq!(calls, srch.alloc_calls.get());

            assert_eq!(
                Result::Success,
                set(search, SearchOption::SelectNext, std::ptr::null())
            );
            // The selected match's two pins are the terminal's too.
            assert_eq!(pins_before + 4, tracked_pins(terminal));
            assert_eq!(search_owned_bytes(search), srch.live_bytes.get());

            // A new needle builds the replacement on the search's allocator
            // and releases the old search (and its pins).
            assert_eq!(Result::Success, set_needle(search, b"Buzz"));
            assert_eq!(pins_before, tracked_pins(terminal));
            assert_eq!(search_owned_bytes(search), srch.live_bytes.get());

            free(search);
            assert_eq!(0, srch.live_bytes.get());
            assert_eq!(pins_before, tracked_pins(terminal));
            assert_eq!(registry_before, (*terminal).searches.count());

            terminal_c::free(terminal);
            assert_eq!(0, term.live_bytes.get());
        }
    }

    // PORT NOTE: reference defect (R13, UD305). With the search's allocator
    // failing, a feed that needs to append a history page fails and is
    // swallowed by `TerminalSearch.feed`; the status stays `feed_required`, so
    // the reference's `run` loops forever and `feed` keeps answering SUCCESS.
    // The port answers `GHOSTTY_OUT_OF_MEMORY` from both, and `run` returns.
    // Negative controls (task S7-6): with `feed`'s check removed this test
    // fails at "a failed feed reports OUT_OF_MEMORY"; with `run`'s loop check
    // removed it never returns (killed by a timeout).
    #[test]
    #[cfg_attr(
        miri,
        ignore = "MIRI EXCEPTION (rustty-vt-c, parity tail C-1): intractable under Miri, two full pages of scrollback written through the terminal; measured past a 20-minute bound; runs natively and under ASan (see MIRI_EXCEPTIONS.md)"
    )]
    fn search_feed_and_run_report_out_of_memory_instead_of_spinning() {
        // N625: this test reaches `sys::log_fn`; hold the sys guard so its
        // warnings cannot land in another test's log sink.
        let _guard = crate::sys::tests::reset_global();
        let failing = FailingAllocator::over_heap(Config::default());
        let fc = failing.allocator();
        with_test_allocator(|talloc| unsafe {
            let terminal = test_terminal(talloc);
            write_corpus_pages(terminal, 2);
            let search: Search = test_new_search(&fc, terminal, b"Fizz");

            // Search everything the first feed loaded, until the history
            // searcher needs its next page.
            assert_eq!(Result::Success, feed(search));
            let mut rounds = 0;
            while search_status(search) == SearchStatus::Running {
                assert_eq!(Result::Success, tick(search, std::ptr::null_mut()));
                rounds += 1;
                assert!(rounds < 10_000);
            }
            assert_eq!(SearchStatus::FeedRequired, search_status(search));

            // Every allocation fails from here on.
            failing.fail_index.set(failing.alloc_index.get());
            assert_eq!(
                Result::OutOfMemory,
                feed(search),
                "a failed feed reports OUT_OF_MEMORY"
            );
            assert!(failing.has_induced_failure.get());
            assert_eq!(SearchStatus::FeedRequired, search_status(search));
            assert_eq!(Result::OutOfMemory, run(search), "run returns");

            // The allocator recovers: the same search completes.
            failing.fail_index.set(usize::MAX);
            assert_eq!(Result::Success, run(search));
            assert_eq!(SearchStatus::Complete, search_status(search));

            free(search);
            terminal_c::free(terminal);
        });
        assert_eq!(failing.allocated_bytes.get(), failing.freed_bytes.get());
    }

    /// One pass of the C API over `terminal` with a search on `calloc`.
    /// Returns `(new_failed, out_of_memory_results)`; every result is checked
    /// to be SUCCESS, OUT_OF_MEMORY, or (for a call that needs a search or a
    /// match that a failed call did not produce) NO_VALUE.
    unsafe fn sweep_once(terminal: terminal_c::Terminal, calloc: &CAllocator) -> (bool, usize) {
        let mut ooms = 0usize;
        let mut check = |r: Result, what: &str, no_value_ok: bool| {
            assert!(
                r == Result::Success
                    || r == Result::OutOfMemory
                    || (no_value_ok && r == Result::NoValue),
                "{what}: unexpected {r:?}"
            );
            if r == Result::OutOfMemory {
                ooms += 1;
            }
        };
        let mut search: Search = std::ptr::null_mut();
        let r = unsafe { new(Some(calloc), &mut search, terminal) };
        if r == Result::OutOfMemory {
            assert!(search.is_null());
            return (true, 1);
        }
        assert_eq!(Result::Success, r);
        unsafe {
            check(set_needle(search, b"Fizz"), "needle", false);
            check(run(search), "run", false);
            let mut total = 0usize;
            check(
                get(
                    search,
                    SearchData::TotalMatches,
                    &mut total as *mut usize as *mut c_void,
                ),
                "total",
                false,
            );
            let mut buf = [CSelection::default(); 16];
            let mut out = CSelectionBuffer {
                ptr: buf.as_mut_ptr(),
                cap: buf.len(),
                len: 0,
            };
            check(
                get(
                    search,
                    SearchData::ViewportMatches,
                    &mut out as *mut CSelectionBuffer as *mut c_void,
                ),
                "viewport matches",
                false,
            );
            check(
                get(
                    search,
                    SearchData::Matches,
                    &mut out as *mut CSelectionBuffer as *mut c_void,
                ),
                "matches",
                false,
            );
            for opt in [
                SearchOption::SelectNext,
                SearchOption::SelectNext,
                SearchOption::SelectPrev,
            ] {
                check(set(search, opt, std::ptr::null()), "select", true);
            }
            let mut sel = CSelection::default();
            check(
                get(
                    search,
                    SearchData::SelectedMatch,
                    &mut sel as *mut CSelection as *mut c_void,
                ),
                "selected match",
                true,
            );
            check(feed(search), "feed", false);
            check(tick(search, std::ptr::null_mut()), "tick", false);
            // A replacement search (and the old one's release).
            check(set_needle(search, b"Buzz"), "needle change", false);
            check(run(search), "run again", false);
            free(search);
        }
        (false, ooms)
    }

    // The C-API allocation-failure sweep (the S-7 design's
    // `checkAllAllocationFailures` for the search): for every allocation
    // index n of the search's allocator, the n-th allocation fails (sticky).
    // Nothing aborts or panics, every result is SUCCESS or OUT_OF_MEMORY (or
    // NO_VALUE where a failed call left nothing to select), every byte and
    // allocation taken from the search's allocator is returned, and the
    // terminal's registry and pins are back where they started. A second loop
    // refuses the r-th remap instead; its fallback allocates, so nothing fails.
    #[test]
    fn search_allocation_failure_sweep_through_the_c_api() {
        // N625: this test reaches `sys::log_fn`; hold the sys guard so its
        // warnings cannot land in another test's log sink.
        let _guard = crate::sys::tests::reset_global();
        with_test_allocator(|talloc| unsafe {
            let terminal = test_terminal(talloc);
            write_corpus(terminal);
            let pins_before = tracked_pins(terminal);
            let registry_before = (*terminal).searches.count();
            let check = |failing: &FailingAllocator, what: &str| {
                assert_eq!(
                    failing.allocated_bytes.get(),
                    failing.freed_bytes.get(),
                    "{what}: every byte taken from the search's allocator is returned"
                );
                assert_eq!(
                    failing.allocations.get(),
                    failing.deallocations.get(),
                    "{what}: every allocation is freed"
                );
                assert_eq!(pins_before, tracked_pins(terminal), "{what}: pins");
                assert_eq!(
                    registry_before,
                    (*terminal).searches.count(),
                    "{what}: registry"
                );
            };

            let clean = FailingAllocator::over_heap(Config::default());
            let cc = clean.allocator();
            assert_eq!((false, 0), sweep_once(terminal, &cc));
            check(&clean, "clean run");
            let remaps = clean.resize_index.get();
            assert!(remaps > 0, "the corpus must grow a list by remap");

            let mut n = 0usize;
            let mut new_failures = 0usize;
            let mut call_failures = 0usize;
            loop {
                let failing = FailingAllocator::over_heap(Config {
                    fail_index: n,
                    resize_fail_index: usize::MAX,
                });
                let fc = failing.allocator();
                let (new_failed, ooms) = sweep_once(terminal, &fc);
                check(&failing, &format!("alloc n={n}"));
                if !failing.has_induced_failure.get() {
                    assert!(!new_failed && ooms == 0, "n={n}: no induced failure");
                    break;
                }
                if new_failed {
                    new_failures += 1;
                } else {
                    call_failures += ooms;
                }
                n += 1;
            }
            assert!(n > 10, "the sweep covers the search's allocations ({n})");
            assert_eq!(1, new_failures, "only index 0 (the wrapper) fails `new`");
            assert!(call_failures > 0);

            for r in 0..=remaps {
                let failing = FailingAllocator::over_heap(Config {
                    fail_index: usize::MAX,
                    resize_fail_index: r,
                });
                let fc = failing.allocator();
                assert_eq!(
                    (false, 0),
                    sweep_once(terminal, &fc),
                    "remap r={r}: a refused remap falls back to alloc"
                );
                check(&failing, &format!("remap r={r}"));
            }

            terminal_c::free(terminal);
        });
    }

    // The terminal-charged half (N262): one allocator shared by the terminal
    // and the search (what an embedder passing one allocator gets), failing
    // at the k-th allocation after the corpus, so the registry growth and the
    // search's tracked pins (terminal-charged) fail as well as the search's
    // own state. The same checks hold, and the terminal frees every byte.
    #[test]
    fn search_shared_allocator_failure_sweep_through_the_c_api() {
        // N625: this test reaches `sys::log_fn`; hold the sys guard so its
        // warnings cannot land in another test's log sink.
        let _guard = crate::sys::tests::reset_global();
        let failing = FailingAllocator::over_heap(Config::default());
        let fc = failing.allocator();
        unsafe {
            let terminal = test_terminal(&fc);
            write_corpus(terminal);
            let pins_before = tracked_pins(terminal);
            let registry_before = (*terminal).searches.count();
            let mut k = 0usize;
            let mut induced = 0usize;
            loop {
                failing.has_induced_failure.set(false);
                failing.fail_index.set(failing.alloc_index.get() + k);
                let _ = sweep_once(terminal, &fc);
                failing.fail_index.set(usize::MAX);
                assert_eq!(pins_before, tracked_pins(terminal), "k={k}: pins");
                assert_eq!(
                    registry_before,
                    (*terminal).searches.count(),
                    "k={k}: registry"
                );
                if !failing.has_induced_failure.get() {
                    break;
                }
                induced += 1;
                k += 1;
            }
            assert!(induced > 10, "the sweep covers the allocations ({induced})");
            terminal_c::free(terminal);
        }
        assert_eq!(failing.allocated_bytes.get(), failing.freed_bytes.get());
        assert_eq!(failing.allocations.get(), failing.deallocations.get());
    }
}
