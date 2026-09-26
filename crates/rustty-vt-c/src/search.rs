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
//! 17 `test "..."` blocks are ported assertion-for-assertion (record:
//! `.port/phase-6/assessments/W3-search-capi.md`).
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
use rustty_lib::allocator::Allocator as CAllocator;
use rustty_lib::String as LibString;
use rustty_vt::highlight::Flattened as FlattenedHighlight;
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
    // PORT NOTE (F-008): the C-resolved allocator (`lib.alloc.default(alloc_)`)
    // that owns the WRAPPER storage (`alloc.create`/`alloc.destroy`). The
    // `TerminalSearch` itself is built on `rustty_vt`'s internals allocator
    // (`allocator_api2::Global`, the type its `init` takes — see render.rs /
    // selection_gesture.rs for the same seam); the reference passes this
    // allocator to `TerminalSearch.init` as well.
    pub alloc: Box<dyn rustty_lib::allocator::ZigAllocator>,

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
    let alloc = rustty_lib::allocator::default(alloc_);
    // const wrapper = alloc.create(SearchWrapper) catch return .out_of_memory;
    // PORT NOTE (F-008): through the C-resolved allocator; a rejecting caller
    // allocator yields `.out_of_memory` like the reference.
    let wrapper: *mut SearchWrapper = match rustty_lib::allocator::create_raw(&*alloc) {
        Ok(ptr) => ptr,
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
    // SAFETY: `searches` is a sibling field of the live terminal wrapper.
    let wrapper_ptr = wrapper;
    #[cfg(test)]
    let reserved = register_tw_check().and_then(|()| unsafe {
        (*terminal_wrapper)
            .searches
            .try_put_no_clobber(wrapper, ())
            .map_err(|_| RegistryFull)
    });
    #[cfg(not(test))]
    let reserved = unsafe { (*terminal_wrapper).searches.try_put_no_clobber(wrapper, ()) };
    if reserved.is_err() {
        // alloc.destroy(wrapper);
        // PORT NOTE: the `free` pattern — `alloc` was moved into the wrapper by
        // the `ptr::write` above, so the value is read back out before the
        // storage is returned through it, then dropped. Nothing else owns
        // anything yet (`search` is `None`), so this is the whole teardown.
        // SAFETY: `wrapper` was just created by `create_raw` with this resolved
        // allocator and has not been handed out; after the read the storage is
        // treated as uninitialized and freed exactly once.
        let wrapper = unsafe { std::ptr::read(wrapper) };
        unsafe { rustty_lib::allocator::destroy_raw(&*wrapper.alloc, wrapper_ptr) };
        drop(wrapper);
        // return .out_of_memory;
        return Result::OutOfMemory;
    }

    // out.* = wrapper;
    // SAFETY: `out_search` is a non-null valid writable `Search`.
    unsafe { *out_search = wrapper };
    // return .success;
    Result::Success
}

// PORT NOTE: tripwire fail-point standing in for the reference tests'
// `std.testing.FailingAllocator` at the ONE allocation `new` performs after the
// wrapper exists: the terminal's search registry growing
// (`terminal_wrapper.searches.putNoClobber(gpa, wrapper, {})`). It is the only
// way to exercise the reference's `catch { alloc.destroy(wrapper); return
// .out_of_memory; }` arm, which `search.h:340-362` documents as
// `GHOSTTY_OUT_OF_MEMORY` (parity S4-F3 / S176-F2). The crate's established
// seam; see `rustty-vt`'s `stream_continuation.rs` `TRACKER_TW` PORT NOTE.
#[cfg(test)]
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) enum RegisterFp {
    Reserve,
}

/// The tripwire's error value (Zig `Allocator.Error`).
#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RegistryFull;

#[cfg(test)]
thread_local! {
    pub(crate) static REGISTER_TW: core::cell::RefCell<
        rustty_util::tripwire::Module<RegisterFp, RegistryFull>,
    > = core::cell::RefCell::new(rustty_util::tripwire::module());
}

#[cfg(test)]
#[inline]
fn register_tw_check() -> core::result::Result<(), RegistryFull> {
    REGISTER_TW.with(|tw| tw.borrow_mut().check(RegisterFp::Reserve))
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
    // PORT NOTE (F-008): move the wrapper value out of the caller-allocator
    // storage, return the STORAGE through the resolved allocator (the caller
    // free callback fires), then drop the value (its `search` was `deinit`ed
    // above; dropping it releases only the Vec/array storage it still owns —
    // the reference's `alloc.destroy` reclaims the same bytes) and the `alloc`
    // box itself last.
    // SAFETY: `search_` was created by `new` via `create_raw` with this same
    // resolved allocator; after the read the storage is treated as
    // uninitialized and freed exactly once.
    let wrapper = unsafe { std::ptr::read(search_) };
    unsafe { rustty_lib::allocator::destroy_raw(&*wrapper.alloc, search_) };
    drop(wrapper);
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
    s.feed(t, true);
    // while (true) {
    //     switch (s.status()) {
    //         .complete => return .success,
    //         .feed_required => s.feed(t, true),
    //         .running => _ = s.tick(),
    //     }
    // }
    loop {
        use rustty_vt::search::terminal::Status as S;
        match s.status() {
            S::Complete => return Result::Success,
            S::FeedRequired => s.feed(t, true),
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
            let replacement = match TerminalSearch::init(rustty_vt::alloc::Global, bytes) {
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
// because the port's `Flattened` owns its chunk `Vec` (not `Copy`).
fn selection_from_highlight(hl: &FlattenedHighlight) -> CSelection {
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
    // wrapper and returns it. The port used an infallible `HashSet::insert`, so
    // the documented return was unreachable and the wrapper would have leaked on
    // that path. Both halves are asserted: the return value AND that the C
    // allocator got its bytes back.
    #[test]
    fn search_new_releases_the_wrapper_when_registry_growth_fails() {
        use rustty_util::tripwire::ResetMode;

        let stats = crate::test_util::CountingCAllocator::new();
        let calloc = stats.c_allocator();

        let terminal = unsafe { test_terminal(&calloc) };
        let live_before = stats.live_bytes.get();
        let frees_before = stats.free_calls.get();

        REGISTER_TW.with(|tw| {
            tw.borrow_mut()
                .errorAlways(RegisterFp::Reserve, RegistryFull)
        });

        let mut search: Search = std::ptr::null_mut();
        assert_eq!(Result::OutOfMemory, unsafe {
            new(Some(&calloc), &mut search, terminal)
        });
        assert!(search.is_null());

        REGISTER_TW
            .with(|tw| tw.borrow_mut().end(ResetMode::Reset))
            .unwrap();

        // The wrapper was allocated through the CALLER allocator and handed
        // back to it: no bytes outstanding beyond what the terminal holds, and
        // at least one free happened (positively proving the destroy arm ran,
        // rather than only that nothing leaked).
        assert!(stats.free_calls.get() > frees_before);
        assert_eq!(live_before, stats.live_bytes.get());

        // The same call succeeds with the fail point disarmed, so the assertion
        // above is about the failure path and not about `new` being broken.
        let mut ok_search: Search = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe {
            new(Some(&calloc), &mut ok_search, terminal)
        });
        assert!(!ok_search.is_null());
        unsafe { free(ok_search) };

        unsafe { terminal_c::free(terminal) };
        assert_eq!(0, stats.live_bytes.get());
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
            let total_ptr = &mut total as *mut usize as *mut c_void;
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
            let idx_ptr = &mut idx as *mut usize as *mut c_void;
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
            let scroll_ptr = &mut scroll as *mut SearchScroll as *mut c_void;
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
            let match_ptr = &mut match_ as *mut CSelection as *mut c_void;
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
            let mut values: [*mut c_void; 3] = [
                &mut total as *mut usize as *mut c_void,
                &mut idx as *mut usize as *mut c_void,
                &mut status as *mut SearchStatus as *mut c_void,
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
            let total_ptr = &mut total as *mut usize as *mut c_void;
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
            let needle_ptr = &mut needle_out as *mut LibString as *mut c_void;
            assert_eq!(Result::Success, get(search, SearchData::Needle, needle_ptr));
            assert_eq!(b"Fizz", needle_bytes(&needle_out));
            let mut idx: usize = 999;
            let idx_ptr = &mut idx as *mut usize as *mut c_void;
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
            let status_ptr = &mut status as *mut SearchStatus as *mut c_void;
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

    // test "search free null"
    #[test]
    fn search_free_null() {
        unsafe { free(std::ptr::null_mut()) };
    }
}
