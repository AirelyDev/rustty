//! rustty-vt-c terminal
//!
//! PORTS: ghostty src/terminal/c/terminal.zig
//!
//! PORT STATUS: IN PROGRESS (mutual-atom Stage B v4 — D3 SEAM landed). The D3
//! trampoline seam + `ZigTerminal` lifecycle machinery are filled: `zig_terminal`,
//! `new`/`new_`/`free` (Box-based lifecycle), `vt_write`, `resize`, `reset`,
//! `scroll_viewport` (the `ScrollViewport` `[2]u64` union
//! dispatch), `get`/`get_multi`/`get_typed` (the non-deferred-type arms), and the
//! `Effects` trampolines (each recovers the owning `TerminalWrapper` from the
//! stream handler via the exposed-provenance `container_of` and calls the C
//! fn-ptr). The get/set + grid_ref sub-batches have LANDED (VALIDATION_REPORT
//! M3): `set`/`setTyped`, every `get_typed` arm, and
//! `grid_ref`/`grid_ref_track`/`point_from_grid_ref` are live — zero
//! `todo!()` remains in this crate. The formerly irreducible mutual cluster
//! (terminal<->grid_ref<->grid_ref_tracked<->selection) is fully resolved.
//! P4-W5 Bundle M (cfc19e805) REMOVED `mode_get`/`mode_set` in favor of the
//! `.mode`/`.mode_default` option arms and the `.mode` data arm.

// const std = @import("std");
// const testing = std.testing;
// const build_options = @import("terminal_options");
// const lib = @import("../lib.zig");
// const CAllocator = lib.alloc.Allocator;
// pub const ZigTerminal = @import("../Terminal.zig");
// const Stream = @import("../stream_terminal.zig").Stream;
// const Screen = @import("../Screen.zig");
// const ScreenSet = @import("../ScreenSet.zig");
// const PageList = @import("../PageList.zig");
// const apc = @import("../apc.zig");
// const kitty = @import("../kitty/key.zig");
// const kitty_gfx_c = @import("kitty_graphics.zig");
// const modes = @import("../modes.zig");
// const point = @import("../point.zig");
// const size = @import("../size.zig");
// const device_attributes = @import("../device_attributes.zig");
// const device_status = @import("../device_status.zig");
// const size_report = @import("../size_report.zig");
// const cell_c = @import("cell.zig");
// const row_c = @import("row.zig");
// const grid_ref_c = @import("grid_ref.zig");
// const grid_ref_tracked_c = @import("grid_ref_tracked.zig");
// const selection_c = @import("selection.zig");
// const style_c = @import("style.zig");
// const color = @import("../color.zig");
// const Result = @import("result.zig").Result;
// const Handler = @import("../stream_terminal.zig").Handler;
use crate::grid_ref::CGridRef;
use crate::grid_ref_tracked::{CTrackedGridRef, TrackedGridRef};
use crate::kitty_graphics::KittyGraphics as KittyGraphicsHandle;
use crate::point::PointC;
use crate::result::Result;
use crate::search::SearchWrapper;
use crate::writer::{CountingWriter, FixedWriter};
use rustty_lib::allocator::Allocator as CAllocator;
use rustty_lib::allocator::{ZigAllocator, ZigSlice};
use rustty_lib::String as LibString;
use rustty_vt::clipboard;
use rustty_vt::device_attributes;
use rustty_vt::device_status;
use rustty_vt::kitty::graphics_image::{DirBorrow, TemporaryFileLimit};
use rustty_vt::modes;
use rustty_vt::pagelist::grow::ScrollbarC;
use rustty_vt::point::{Coordinate, Tag};
use rustty_vt::screen_set::Key as ScreenSetKey;
use rustty_vt::size;
use rustty_vt::size_report;
use rustty_vt::stream_terminal::{Handler, Stream};
use rustty_vt::terminal::{Colors, Terminal as ZigTerminal};
use rustty_vt::terminal::{Options as ZigTerminalOptions, ScrollViewport as ZigScrollViewport};
use std::ffi::c_void;
use std::mem::{align_of, offset_of, size_of};
use std::os::raw::c_int;

mod data;
mod effects;
mod grid_ref;
mod options;
mod write;

pub use self::data::*;
pub use self::effects::*;
pub use self::grid_ref::*;
pub use self::options::*;
pub use self::write::*;

// const log = std.log.scoped(.terminal_c);
//
// PORT NOTE (W35-B03/B06/B08): rustty's scoped-log sink is `sys::log_fn` (the
// port of `terminal/c/sys.zig`'s `logFn`), which is silent until a C caller
// installs a log callback via `rustty_sys_set(RUSTTY_SYS_OPT_LOG, ...)`.
// `LOG_SCOPE` is the `@tagName` of the reference's scope, which `log_fn`
// forwards to the callback as the scope text.
pub(crate) const LOG_SCOPE: &str = "terminal_c";

// PORT NOTE (F-008, supersedes the D3-era `GLOBAL_ZIG_ALLOCATOR_FOR_OSC`
// static): the reference threads the C allocator into the OSC parser via
// `Stream.initAlloc(alloc, ...)`, so dynamic OSC payloads (e.g. OSC 52
// clipboard) grow through the CALLER's allocator. CORRECTION (parity tail
// S7-7): the OSC parser used to want a borrowed `ZigAllocatorRef` over the
// wrapper's boxed allocator; `stream::Options.allocator` is the runtime handle
// now (`rustty_vt::alloc::default(alloc_)`), as the reference passes `alloc`.

/// PORT NOTE (oracle `e8525c0fd`/`960c2cca5`, amended `105d0a545`/P7-W2 T3):
/// the reference's `wrapper.tmp_dir_path` was originally a
/// `[std.fs.max_path_bytes]u8` inline buffer; `105d0a545` turned it into an
/// allocator-owned `?[]u8`, allocated only once the embedder sets the option
/// (`TerminalWrapper::tmp_dir_path`, this file). Every screen's
/// `Limits.temporary_file` (`TemporaryFileLimit::Enabled.directory`, a raw
/// `*const [u8]`, `rustty_vt::kitty::graphics_image`) BORROWS that one wrapper
/// copy — DECISIONS B6, faithful. The length bound the old fixed buffer
/// imposed is still an observable part of the C contract
/// (`RUSTTY_OUT_OF_MEMORY` for an over-long directory), so the constant
/// survives regardless of storage shape. Values match `std.fs.max_path_bytes`:
/// `PATH_MAX` on POSIX (1024 on Darwin/BSD, 4096 on Linux), `PATH_MAX_WIDE * 3
/// + 1` on Windows, and 4096 on WASI/freestanding. R3: a plain constant, no
/// `libc`.
const MAX_PATH_BYTES: usize = if cfg!(any(
    target_os = "macos",
    target_os = "ios",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd",
    target_os = "dragonfly"
)) {
    1024
} else if cfg!(windows) {
    32767 * 3 + 1
} else {
    4096
};

/// C terminals do not retain replay bytes unless the embedding application
/// opts in through `RUSTTY_TERMINAL_OPT_CONTINUATION_MAX_BYTES`.
// pub const default_continuation_max_bytes: usize = 0;
pub const DEFAULT_CONTINUATION_MAX_BYTES: usize = 0;

// /// Owns the `std.Io` implementation retained by every C terminal.
// ///
// /// Snapshot decoding creates this before the native terminal exists and
// /// transfers it into the final C wrapper after READY.
// ///
// /// This is TinyIo on every target: it is stateless and supports exactly
// /// the operations the terminal needs at a fraction of the code size of
// /// `std.Io.Threaded` (see lib/TinyIo.zig), on POSIX and Windows alike.
// /// On the remaining targets (e.g. freestanding wasm) TinyIo degrades to
// /// `std.Io.failing`, which is correct: they have no filesystem.
// pub const Io = struct {
//     impl: lib.TinyIo,
//
//     pub const init: Io = .{ .impl = .init };
//
//     /// Return the value passed to native terminal construction and decoding.
//     pub fn io(self: Io) std.Io {
//         return self.impl.io();
//     }
// };
//
// PORT NOTE (Bundle S, extends the `io_impl` note below): the reference `Io`
// owns the `std.Io` implementation retained by every C terminal (TinyIo on
// every target, failing on freestanding) so snapshot
// decoding can create it before the native terminal exists and transfer it
// into the final C wrapper (`fromDecoded` retains it in the returned wrapper). rustty
// has no `std.Io` seam anywhere — the snapshot codec's Rust signatures dropped
// the `io` parameter entirely (see `rustty_vt::snapshot::codec`'s module PORT
// NOTE) — so the type, its `init`/`io`, and every transfer edge are
// structurally N/A. Decision recorded on the TinyIo row of
// `.port/phase-4/NON_PORTS.md`.

/// Wrapper around ZigTerminal that tracks additional state for C API usage,
/// such as the persistent VT stream needed to handle escape sequences split
/// across multiple vt_write calls.
// PORT NOTE (internal, NOT C-ABI): `terminal: *ZigTerminal` is the raw owning
// back-pointer to the heap `ZigTerminal` (allocated by `new_` through the
// C-resolved allocator via `create_raw`, F-008). `stream:
// Stream<'static>` is the persistent VT stream whose handler aliases that
// terminal; the self-referential aliasing is resolved by storing the terminal as
// a raw pointer (the handler holds `*mut Terminal`) and exposing the wrapper's
// provenance so the always-installed effect trampolines can recover it (see
// `wrapper_from_handler`). `tracked_grid_refs` is the set of live
// `*TrackedGridRef` so `free` can detach them safely (Zig
// `AutoArrayHashMapUnmanaged(*TrackedGridRef, void)`); `searches` (`f0c918fc4`)
// is the same registry for live `*SearchWrapper`s, so a search and its
// terminal can be freed in either order (`search.rs` module note).
pub struct TerminalWrapper {
    pub terminal: *mut ZigTerminal,

    // PORT NOTE (F-008, internal): the C-resolved allocator (`lib.alloc
    // .default(alloc_)`) that owns the wrapper-level lifecycle: the terminal
    // and wrapper STORAGE (`alloc.create`/`alloc.destroy`) and the OSC stream
    // allocator (`.initAlloc(alloc, ...)`). The reference recovers it in
    // `free` via `t.gpa()`; rustty's `Terminal::gpa()` is the (Global)
    // internals allocator, so the resolved allocator is stored here instead.
    // A raw pointer (from `Box::into_raw`) rather than a `Box` field: the
    // sibling `stream` holds a `ZigAllocatorRef` aliasing the pointee, and a
    // `Box` field would retag on the wrapper move in `free` and invalidate
    // that reference under Stacked Borrows. `free` reclaims it LAST.
    pub alloc: *mut dyn rustty_lib::allocator::ZigAllocator,

    // PORT NOTE (parity S179-F3, internal): a copy of the caller's C allocator
    // (`alloc_`, null for the default) for objects that can OUTLIVE this
    // wrapper and must be freed through the caller's allocator, as the
    // reference's `t.gpa()` is: a `TrackedGridRef` (`grid_ref_track`). Such an
    // object keeps its own copy, as `OscParserWrapper` does, since `alloc`
    // above is gone once the terminal is freed.
    pub c_alloc: Option<CAllocator>,

    // /// C construction has no I/O argument, so the wrapper retains the owner
    // /// created by `new` or transferred from snapshot decoding until `free`.
    // /// Freestanding owners contain no native allocation and expose failing I/O.
    // io: Io,
    //
    // PORT NOTE (`e8525c0fd`/`da04b65d4`, pin field `io: Io`): N/A. Zig 0.16
    // threads an explicit `std.Io` through every blocking call, so the C API —
    // which has no place to take one — must own the `Io` owner quoted above.
    // Rust's I/O is ambient, so there is nothing to store, allocate in `new`,
    // transfer from snapshot decoding, or `deinit` in `free`.
    //
    // /// Allocator-owned copy of the temporary directory path for some
    // /// operations (e.g. kitty graphics). This is only allocated once the
    // /// embedder sets the option.
    // tmp_dir_path: ?[]u8 = null,
    //
    /// PORT NOTE (oracle `105d0a545`, P7-W2 T3, DECISIONS B6): the wrapper's
    /// OWN copy of the kitty temp-dir path, allocated through the CALLER's C
    /// allocator (F-008: `alloc.dupe` in the reference is `wrapper.terminal
    /// .gpa()`, i.e. the SAME allocator as `wrapper.alloc` here) only when
    /// `kitty_image_medium_temp_file` is set — never unconditionally, unlike
    /// the pre-`105d0a545` fixed buffer this replaced. Every screen's
    /// `Limits.temporary_file` BORROWS this buffer as a raw `*const [u8]`
    /// (`TemporaryFileLimit::Enabled`, `rustty_vt::kitty::graphics_image`) —
    /// never an independent copy. `ZigSlice` (not `Vec<u8>`) because the
    /// backing bytes must round-trip through `wrapper.alloc`'s raw
    /// alloc/free, exactly like every other F-008 allocation in this file —
    /// a plain `Vec<u8>` would silently use the RUST GLOBAL allocator
    /// instead, breaking a caller-supplied allocator's accounting.
    ///
    /// SAFETY (Stacked Borrows discipline,
    /// `.port/audits/GHOSTTY_LIFETIMES.tsv`): every write site below updates
    /// ALL screens to a NEW value (a fresh copy, or `Disabled`) BEFORE the
    /// old value here is freed/replaced, so no screen ever reads through a
    /// dangling pointer — covering path clear, path set twice, and (via the
    /// explicit `free_bytes` call in `free` below, since `ZigSlice` has no
    /// `Drop` of its own) terminal free.
    pub tmp_dir_path: Option<ZigSlice>,
    //
    /// The terminfo name reported for XTGETTCAP "TN". The stream handler holds
    /// a slice into this.
    // terminfo_name_buf: [Handler.max_terminfo_name_bytes]u8,
    //
    // SB (SOUNDNESS): the handler's `terminfo_name: Option<&[u8]>` slice is a
    // self-referential borrow of this buffer. It is created from a raw place
    // projection off the wrapper base pointer (`set_typed(.terminfo_name)`),
    // and every write into the buffer also goes through that base — never
    // through a wrapper-level `&mut` that would invalidate the stored slice
    // (the rust-mut-args-strongly-protected discipline). The wrapper is
    // heap-stable for its whole life, so the slice address never moves.
    pub terminfo_name_buf: [u8; rustty_vt::stream_terminal::MAX_TERMINFO_NAME_BYTES],
    pub stream: Stream<'static>,
    pub effects: Effects,
    // tracked_grid_refs: std.AutoArrayHashMapUnmanaged(*TrackedGridRef, void) = .{},
    // PORT NOTE (parity tail S7-4): grown through the terminal's allocator
    // (`t.gpa()`, the caller's), which the map is bound to in `wrap`.
    pub tracked_grid_refs: WrapperSet<*mut TrackedGridRef>,
    // searches: std.AutoArrayHashMapUnmanaged(*search_c.SearchWrapper, void) = .{},
    // (the terminal's allocator too: the search's registration is charged to
    // the terminal, N262.)
    pub searches: WrapperSet<*mut SearchWrapper>,

    /// PORT NOTE (N452, P7-W2 T9; LIFETIMES.tsv `TerminalWrapper
    /// live_handler`): no reference field. The handler pointer of the
    /// innermost effect trampoline that is running a C callback, or null.
    ///
    /// A C callback may re-enter the C API on this terminal (the reference
    /// forbids only `vt_write`/`vt_write_until_ground` there). While it runs,
    /// the stream's handler dispatch holds a protected `&mut Handler` (the
    /// `stream::Handler` trait methods take `&mut self`), so reaching the
    /// handler's bytes through this wrapper would be Stacked Borrows UB even
    /// though the reference's plain `*TerminalWrapper` access is sound. Each
    /// trampoline therefore derives ONE raw pointer from its own `&mut
    /// Handler` argument — a child of every live handler borrow — and
    /// publishes it here for exactly the duration of the callback
    /// ([`LiveHandler`]); [`handler_ptr`] hands it to every C API that touches
    /// handler bytes. Outside callbacks it is null and those APIs reach the
    /// handler by place.
    pub live_handler: *mut Handler<'static>,
}

/// The handler a C API call must reach (N452; see
/// `TerminalWrapper::live_handler`): the running trampoline's handler
/// pointer inside an effect callback, else the wrapper's own
/// `stream.handler` by place. Neither forms a reference.
///
/// # Safety
/// `wrapper` must be a live terminal wrapper.
#[inline]
pub(crate) unsafe fn handler_ptr(wrapper: *mut TerminalWrapper) -> *mut Handler<'static> {
    // SAFETY: `wrapper` is live (caller contract); a read of a sibling of
    // `stream`, then a place projection.
    unsafe {
        let live = (*wrapper).live_handler;
        if live.is_null() {
            std::ptr::addr_of_mut!((*wrapper).stream.handler)
        } else {
            live
        }
    }
}

/// Publishes a trampoline's handler pointer in `live_handler` for the
/// duration of one C callback, restoring the previous value on drop so
/// nested effects (a callback's `resize` firing `write_pty`) nest (N452).
pub(crate) struct LiveHandler {
    wrapper: *mut TerminalWrapper,
    prev: *mut Handler<'static>,
}

impl LiveHandler {
    /// `handler` must be the raw pointer the trampoline derived from its own
    /// handler argument (or, for C `reset`, the by-place handler pointer),
    /// and `wrapper` the wrapper recovered from it.
    #[inline]
    pub(crate) fn publish(wrapper: *mut TerminalWrapper, handler: *mut Handler<'static>) -> Self {
        // SAFETY: `wrapper` is the live owning wrapper; `live_handler` is a
        // sibling of `stream`, written by place.
        let prev =
            unsafe { std::ptr::replace(std::ptr::addr_of_mut!((*wrapper).live_handler), handler) };
        LiveHandler { wrapper, prev }
    }
}

impl Drop for LiveHandler {
    #[inline]
    fn drop(&mut self) {
        // SAFETY: as in `publish`; the wrapper outlives the callback.
        unsafe { *std::ptr::addr_of_mut!((*self.wrapper).live_handler) = self.prev };
    }
}

/// `std.AutoArrayHashMapUnmanaged(K, void)` over the terminal's allocator
/// handle: the wrapper's registries of live tracked refs and searches.
pub type WrapperSet<K> = rustty_vt::alloc::AArrayHashMap<K, ()>;

/// C: GhosttyTerminal -> RusttyTerminal (R2)
// pub const Terminal = ?*TerminalWrapper;
// PORT NOTE: nullable opaque handle; raw `*mut TerminalWrapper` (null == none).
pub type Terminal = *mut TerminalWrapper;

/// C: GhosttyTerminalCompressionMode -> RusttyTerminalCompressionMode (R2)
// pub const CompressionMode = ZigTerminal.CompressionMode;
// PORT NOTE: the reference nests these on `Terminal`; rustty hoists them to
// `rustty_vt::terminal` module scope (Rust has no namespaced type declarations
// inside an `impl`) with a `Terminal` prefix, so the alias re-exports the
// hoisted name under the reference's unprefixed C-API name.
pub type CompressionMode = rustty_vt::terminal::TerminalCompressionMode;

/// C: GhosttyTerminalCompressionResult -> RusttyTerminalCompressionResult (R2)
// pub const CompressionResult = ZigTerminal.CompressionResult;
pub type CompressionResult = rustty_vt::terminal::TerminalCompressionResult;

// BUILD-failure gate (D2 pattern): the declaration order of both enums is part
// of the librustty-vt C ABI, and `compress` narrows the incoming C `c_int` to
// `CompressionMode` with an explicit `match` on these literal discriminants.
// Pin them (and the `c_int` ABI width the headers declare) so any drift fails
// the build rather than silently changing the ABI or the accepted `mode` range.
const _: () = assert!(CompressionMode::Incremental as c_int == 0);
const _: () = assert!(CompressionMode::Full as c_int == 1);
const _: () = assert!(CompressionResult::Unsupported as c_int == 0);
const _: () = assert!(CompressionResult::Pending as c_int == 1);
const _: () = assert!(CompressionResult::Complete as c_int == 2);
const _: () = assert!(size_of::<CompressionMode>() == size_of::<c_int>());
const _: () = assert!(size_of::<CompressionResult>() == size_of::<c_int>());

// pub fn zigTerminal(terminal_: Terminal) ?*ZigTerminal {
//     return (terminal_ orelse return null).terminal;
// }
/// # Safety
/// `terminal_`, if non-null, must be a live terminal handle.
pub unsafe fn zig_terminal(terminal_: Terminal) -> *mut ZigTerminal {
    if terminal_.is_null() {
        return std::ptr::null_mut();
    }
    // SAFETY: `terminal_` is a live `new`-returned wrapper (caller contract).
    unsafe { (*terminal_).terminal }
}

// PORT NOTE (oracle 03d5fa268): `GhosttyTerminalOptions` (rustty:
// `RusttyTerminalOptions`) was REMOVED from the ABI entirely. `terminal_new`
// now takes `cols`/`rows` directly and the scrollback limits moved to
// `terminal_set` options 27/28. The `types.rs` struct table asserts the name is
// GONE from the emitted JSON (`!root.contains("RusttyTerminalOptions")`), so a
// re-introduction cannot pass silently.

// const NewError = error{ InvalidValue, OutOfMemory };

// PORT NOTE (D3, internal): error analog of the reference `NewError`. The
// reference `new` calls `new_` and maps the error to a `Result`; we inline that
// mapping here (Rust has no `errdefer`, so the cleanup is hand-unwound).
enum NewError {
    InvalidValue,
    OutOfMemory,
}

// pub fn new(
//     alloc_: ?*const CAllocator,
//     result: *Terminal,
//     cols: size.CellCountInt,
//     rows: size.CellCountInt,
// ) ... {
//     result.* = new_(alloc_, cols, rows) catch |err| {
//         result.* = null;
//         return switch (err) {
//             error.InvalidValue => .invalid_value,
//             error.OutOfMemory => .out_of_memory,
//         };
//     };
//     return .success;
// }
/// # Safety
/// `alloc_`, if non-null, must point to a valid `CAllocator`; `result` must be a
/// valid writable pointer to a `Terminal`.
pub unsafe fn new(
    alloc_: *const CAllocator,
    result: *mut Terminal,
    cols: size::CellCountInt,
    rows: size::CellCountInt,
) -> Result {
    // SAFETY: `alloc_` validity and `result` writability are caller contracts.
    let alloc_ref = unsafe { alloc_.as_ref() };
    match new_(alloc_ref, cols, rows) {
        Ok(wrapper) => {
            unsafe { *result = wrapper };
            Result::Success
        }
        Err(err) => {
            unsafe { *result = std::ptr::null_mut() };
            match err {
                NewError::InvalidValue => Result::InvalidValue,
                NewError::OutOfMemory => Result::OutOfMemory,
            }
        }
    }
}

// fn new_(
//     alloc_: ?*const CAllocator,
//     cols: size.CellCountInt,
//     rows: size.CellCountInt,
// ) NewError!*TerminalWrapper {
//     if (cols == 0 or rows == 0) return error.InvalidValue;
//     const alloc = lib.alloc.default(alloc_);
//     const t = alloc.create(ZigTerminal) catch return error.OutOfMemory;
//     errdefer alloc.destroy(t);
//     const wrapper = alloc.create(TerminalWrapper) catch return error.OutOfMemory;
//     errdefer alloc.destroy(wrapper);
//     t.* = try .init(alloc, .{ .cols = cols, .rows = rows });
//     errdefer t.deinit(alloc);
//     t.flags.shell_redraws_prompt = .false;
//     var handler: Stream.Handler = t.vtHandler();
//     handler.effects = .{ .write_pty = &Effects.writePtyTrampoline, ... };
//     wrapper.* = .{ .terminal = t, .stream = .initAlloc(alloc, handler) };
//     return wrapper;
// }
//
// PORT NOTE (F-008, supersedes the D3-lifecycle substitution): the terminal
// and wrapper STORAGE now flow through the C-resolved allocator exactly like
// the reference (`alloc.create(...) catch return error.OutOfMemory`, with the
// `errdefer alloc.destroy(...)`s as explicit hand unwinds), and the OSC stream
// is backed by the same allocator (`.initAlloc(alloc, handler)`). A rejecting
// caller allocator therefore yields a recoverable `RUSTTY_OUT_OF_MEMORY`
// instead of silently succeeding on the Rust global heap.
//
// PORT NOTE (parity tail S7-3): `t.init(alloc, ...)` now receives the same
// allocator as the runtime handle (`rustty_vt::alloc::default(alloc_)`), so the
// terminal's internals that rustty-vt routes (the PageList node/pin pools and
// tracked-pin sets, compressed pages, the screens themselves, the cursor
// hyperlink, tabstops, pwd/title and the glyph glossary map) grow through the
// caller's allocator, as the reference's do. The rest of the internals (kitty
// graphics, APC/DCS, the stream, clipboard, the wrapper maps) are S7-4's.
fn new_(
    alloc_: Option<&CAllocator>,
    cols: size::CellCountInt,
    rows: size::CellCountInt,
) -> std::result::Result<Terminal, NewError> {
    // if (cols == 0 or rows == 0) return error.InvalidValue;
    if cols == 0 || rows == 0 {
        return Err(NewError::InvalidValue);
    }

    // const alloc = lib.alloc.default(alloc_);
    // PORT NOTE (F-008): held as a raw heap pointer so the stream's
    // `ZigAllocatorRef` can alias it for the wrapper's lifetime (see the
    // `TerminalWrapper.alloc` field note); `free` reclaims it last.
    let alloc_holder: *mut dyn rustty_lib::allocator::ZigAllocator =
        Box::into_raw(rustty_lib::allocator::default(alloc_));
    // SAFETY: `alloc_holder` is a live heap allocation until `free` (or the
    // unwind arms below) reclaims it.
    let alloc: &'static dyn rustty_lib::allocator::ZigAllocator = unsafe { &*alloc_holder };
    // `lib.alloc.default(alloc_)` as the runtime handle (a non-NULL `alloc_`
    // outlives the terminal: the caller's contract, N260).
    // SAFETY: that contract.
    let gpa = unsafe { rustty_vt::alloc::default(alloc_) };

    // const t = alloc.create(ZigTerminal) catch return error.OutOfMemory;
    let terminal_ptr: *mut ZigTerminal = match rustty_lib::allocator::create_raw(alloc) {
        Ok(ptr) => ptr,
        Err(_) => {
            // SAFETY: reclaim the holder created above; nothing else refers to it.
            drop(unsafe { Box::from_raw(alloc_holder) });
            return Err(NewError::OutOfMemory);
        }
    };

    // t.* = try .init(alloc, .{ ... });
    // errdefer t.deinit(alloc);
    let mut terminal = match ZigTerminal::init(
        gpa,
        ZigTerminalOptions {
            cols,
            rows,
            // PORT NOTE: the reference relies on `Terminal.Options`'s own
            // `max_scrollback_bytes = 10_000` / `max_scrollback_lines = null`
            // defaults here; rustty-vt has no `Default` for `Options`, so they
            // are written out (see the note on the remaining fields below).
            max_scrollback_bytes: Some(10_000),
            max_scrollback_lines: None,
            // PORT NOTE: the reference relies on the `Options` field defaults for
            // the remaining fields. rustty-vt has no `Default` for `Options`, so
            // they are written out. This crate is the reference's `.lib`
            // artifact (libghostty-vt), so the artifact-selected kitty image
            // storage default is the `.lib` one (10MB).
            // CORRECTION (parity S141-F1 / S179-F1): this note used to say "we
            // replicate the (ghostty-artifact) defaults that rustty-vt's source
            // of truth produces", and the limit was the application's
            // `320 * 1000 * 1000`.
            colors: Colors::default(),
            default_modes: rustty_vt::modes::ModePacked::default(),
            default_cursor_style: rustty_vt::screen::CursorStyle::Block,
            default_cursor_blink: Some(false),
            kitty_image_storage_limit: ZigTerminalOptions::kitty_image_storage_limit_default(
                rustty_vt::build_options::Artifact::Lib,
            ),
            kitty_image_loading_limits: rustty_vt::kitty::graphics_image::Limits::DIRECT,
        },
    ) {
        Ok(t) => t,
        Err(_oom) => {
            // errdefer alloc.destroy(t);
            // SAFETY: `terminal_ptr` is still uninitialized storage from
            // `create_raw`.
            unsafe { rustty_lib::allocator::destroy_raw(alloc, terminal_ptr) };
            drop(unsafe { Box::from_raw(alloc_holder) });
            return Err(NewError::OutOfMemory);
        }
    };

    // libghostty-vt embedders don't necessarily install Ghostty's shell
    // integration, so don't assume OSC 133 prompts can be redrawn on resize.
    // Shells can still opt in with OSC 133;A;redraw=1.
    // t.flags.shell_redraws_prompt = .false;
    terminal.flags.shell_redraws_prompt = rustty_vt::osc::semantic_prompt::Redraw::False;

    // t.* = ...; — move the terminal value into its caller-allocated storage
    // so the wrapper's stream handler can alias a stable address.
    // SAFETY: `terminal_ptr` is valid uninitialized `ZigTerminal` storage.
    unsafe { std::ptr::write(terminal_ptr, terminal) };

    // return try wrap(alloc, t, io, default_continuation_max_bytes);
    // (reference errdefers on wrap failure: t.deinit + destroy(t) + io.deinit —
    // the explicit unwind below; `io` is N/A, see the `Io` PORT NOTE.)
    match unsafe {
        wrap(
            alloc_holder,
            alloc_.copied(),
            gpa,
            terminal_ptr,
            DEFAULT_CONTINUATION_MAX_BYTES,
        )
    } {
        Ok(wrapper) => Ok(wrapper),
        Err(_oom) => {
            // SAFETY: `terminal_ptr` holds the initialized terminal written
            // above; deinit + drop it, then return its storage; holder last.
            unsafe {
                (*terminal_ptr).deinit(gpa);
                std::ptr::drop_in_place(terminal_ptr);
                rustty_lib::allocator::destroy_raw(alloc, terminal_ptr);
            }
            drop(unsafe { Box::from_raw(alloc_holder) });
            Err(NewError::OutOfMemory)
        }
    }
}

/// `error{OutOfMemory}` for `wrap`.
struct WrapOutOfMemory;

/// Attach the persistent C stream and I/O owner to a heap-stable terminal.
/// The caller retains ownership of both inputs if wrapper allocation fails.
// fn wrap(
//     alloc: std.mem.Allocator,
//     t: *ZigTerminal,
//     io: Io,
//     continuation_max_bytes: usize,
// ) error{OutOfMemory}!Terminal {
//
// PORT NOTE: the `io: Io` owner parameter is N/A (see the `Io` PORT NOTE);
// rustty's analog of the reference `alloc` parameter is the F-008
// `alloc_holder` (see `TerminalWrapper.alloc`), whose ownership likewise stays
// with the caller on failure. `capture_alloc` is the same allocator as the
// runtime handle (`rustty_vt::alloc::default(alloc_)`): the reference's
// `alloc` for the stream (`Stream.init(.{ .allocator = alloc, .. })`: its OSC
// parser and continuation tracker) and the wrapper maps (parity tail S7-1,
// S7-4, S7-7).
//
// # Safety
// `alloc_holder` must be a live `Box::into_raw` holder; `terminal_ptr` must
// point to an initialized, heap-stable `ZigTerminal` allocated through it.
unsafe fn wrap(
    alloc_holder: *mut dyn rustty_lib::allocator::ZigAllocator,
    c_alloc: Option<CAllocator>,
    capture_alloc: rustty_vt::alloc::Allocator,
    terminal_ptr: *mut ZigTerminal,
    continuation_max_bytes: usize,
) -> std::result::Result<Terminal, WrapOutOfMemory> {
    // SAFETY: `alloc_holder` is live until `free` reclaims it (caller
    // contract).
    let alloc: &'static dyn rustty_lib::allocator::ZigAllocator = unsafe { &*alloc_holder };

    // const wrapper = alloc.create(TerminalWrapper) catch return error.OutOfMemory;
    let wrapper_ptr: *mut TerminalWrapper = match rustty_lib::allocator::create_raw(alloc) {
        Ok(ptr) => ptr,
        Err(_) => return Err(WrapOutOfMemory),
    };

    // Trampolines are always installed so setting C callbacks later takes
    // effect immediately.
    // var handler: Stream.Handler = t.vtHandler();
    // SB (SOUNDNESS): build the Handler DIRECTLY from the heap box's base raw
    // pointer. Going through `(*terminal_ptr).vt_handler()` would autoref a
    // temporary `&mut ZigTerminal` and the handler would store a child of that
    // temporary's tag — the FIRST later `&mut`-method call on the terminal
    // from any export (e.g. `resize`'s `(*t).gpa()`) would pop it and make
    // every subsequent `vt_write` Stacked-Borrows UB (Miri-caught via the
    // kitty `image_get_multi_success` KAT). With the base tag, transient
    // `&mut` children created by exports die at the next handler use instead
    // of killing it — the rust-mut-args-strongly-protected rule; the
    // reference's `t.vtHandler()` is a plain unprotected Zig pointer copy.
    let mut handler: Handler<'static> = Handler::init(terminal_ptr);

    // handler.effects = .{ .write_pty = &Effects.writePtyTrampoline, ... };
    handler.effects = rustty_vt::stream_terminal::Effects {
        write_pty: Some(Effects::write_pty_trampoline),
        bell: Some(Effects::bell_trampoline),
        color_scheme: Some(Effects::color_scheme_trampoline),
        desktop_notification: Some(Effects::desktop_notification_trampoline),
        // .drag_and_drop = null,
        drag_and_drop: None,
        device_attributes: Some(Effects::device_attributes_trampoline),
        enquiry: Some(Effects::enquiry_trampoline),
        xtversion: Some(Effects::xtversion_trampoline),
        title_changed: Some(Effects::title_changed_trampoline),
        pwd_changed: Some(Effects::pwd_changed_trampoline),
        progress_report: Some(Effects::progress_report_trampoline),
        size: Some(Effects::size_trampoline),
        render_hold: Some(Effects::render_hold_trampoline),
        // PORT NOTE (P8-W5 T2): the vt `semantic_prompt`, `reset` and
        // `program_status` effects exist; their C trampolines and options
        // (42, 43, 46) land with P8-W5 T3, so nothing is installed yet.
        semantic_prompt: None,
        reset: None,
        program_status: None,

        // Installed dynamically when the callback is set; see Effects.
        clipboard_write: None,
        clipboard_read: None,
    };

    // wrapper.* = .{
    //     .terminal = t,
    //     .io = io,
    //     .tmp_dir_path = undefined,
    //     .terminfo_name_buf = undefined,
    //     .stream = Stream.init(.{
    //         .allocator = alloc,
    //         .handler = handler,
    //         .continuation_max_bytes = continuation_max_bytes,
    //     }),
    // };
    // PORT NOTE (F-008): the C-resolved allocator now backs the OSC parser,
    // exactly like the reference's `Stream.init(.{ .allocator = alloc, ... })` —
    // dynamic OSC growth (e.g. OSC 52 clipboard) goes through the CALLER's
    // allocator. (68beeeb3f: `initAlloc` became the `Options`-taking `init`.)
    // SAFETY: `wrapper_ptr` is valid uninitialized `TerminalWrapper` storage.
    unsafe {
        std::ptr::write(
            wrapper_ptr,
            TerminalWrapper {
                terminal: terminal_ptr,
                alloc: alloc_holder,
                c_alloc,
                // .terminfo_name_buf = undefined,
                // (a zeroed seed — never observed before `set(.terminfo_name)`
                // overwrites the prefix it then publishes)
                terminfo_name_buf: [0; rustty_vt::stream_terminal::MAX_TERMINFO_NAME_BYTES],
                // (parity tail S7-7: `Options.allocator` is the runtime
                // handle, so the OSC parser and the continuation tracker both
                // grow through the caller's allocator, as in the reference.)
                stream: Stream::init(rustty_vt::stream::Options {
                    allocator: Some(capture_alloc),
                    handler,
                    continuation_max_bytes: Some(continuation_max_bytes),
                    osc_unknown_max_bytes: 0,
                }),
                effects: Effects::default(),
                // .tmp_dir_path = undefined -> None: the reference leaves it
                // `undefined` because `105d0a545` made it `?[]u8 = null` with
                // a field default (the literal above never overrides
                // defaults); the first `set(.kitty_image_medium_temp_file)`
                // populates it (or it stays `None` for the terminal's whole
                // life if the embedder never sets the option).
                tmp_dir_path: None,
                tracked_grid_refs: WrapperSet::new_in(capture_alloc),
                searches: WrapperSet::new_in(capture_alloc),
                live_handler: std::ptr::null_mut(),
            },
        )
    };

    // return wrapper;
    // PROVENANCE: expose the whole wrapper allocation so the effect trampolines
    // can recover it (siblings of the stream handler) via
    // `with_exposed_provenance_mut` — see `wrapper_from_handler`.
    let _ = (wrapper_ptr as *const u8).expose_provenance();
    Ok(wrapper_ptr)
}

// pub const RestoreContinuationError = error{
//     OutOfMemory,
//     ContinuationDisabled,
//     ContinuationUnavailable,
//     InvalidContinuation,
// };
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RestoreContinuationError {
    OutOfMemory,
    ContinuationDisabled,
    ContinuationUnavailable,
    InvalidContinuation,
}

/// Replay a decoded continuation exactly once into a newly created C terminal
/// and verify that the persistent stream exports the identical canonical
/// bytes. The terminal remains valid on error and may be freed normally.
///
/// # Safety
/// `terminal_`, if non-null, must be a live terminal handle.
pub unsafe fn restore_continuation(
    terminal_: Terminal,
    continuation: &[u8],
) -> std::result::Result<(), RestoreContinuationError> {
    // const wrapper = terminal_ orelse return error.InvalidContinuation;
    if terminal_.is_null() {
        return Err(RestoreContinuationError::InvalidContinuation);
    }
    // if (continuation.len > 0) wrapper.stream.nextSlice(continuation);
    if !continuation.is_empty() {
        // SAFETY: `terminal_` is a live wrapper (caller contract); the
        // stream's handler aliases this wrapper's terminal.
        unsafe {
            Stream::next_slice_raw(std::ptr::addr_of_mut!((*terminal_).stream), continuation)
        };
    }

    // var exported: std.Io.Writer.Allocating = .init(wrapper.terminal.gpa());
    // defer exported.deinit();
    // PORT NOTE (parity tail S7-8): the writer grows through the terminal's
    // allocator, and a failed growth is the `WriteFailed => OutOfMemory` arm.
    // SAFETY: `terminal_` is a live wrapper (caller contract); raw reads of
    // the active screen's allocator (`Terminal.gpa()`), no `&mut` retag of
    // the terminal the stream handler aliases.
    let gpa = unsafe { (*(*(*terminal_).terminal).screens.active).alloc };
    let mut exported = rustty_vt::alloc::AllocatingWriter::init(gpa);
    // wrapper.stream.writeContinuation(&exported.writer) catch |err| switch (err) { ... };
    // SAFETY: `terminal_` is a live wrapper (caller contract).
    match unsafe {
        Stream::write_continuation_raw(std::ptr::addr_of!((*terminal_).stream), &mut exported)
    } {
        Ok(()) => {}
        Err(rustty_vt::stream::WriteContinuationError::WriteFailed(_)) => {
            return Err(RestoreContinuationError::OutOfMemory)
        }
        Err(rustty_vt::stream::WriteContinuationError::ContinuationDisabled) => {
            return Err(RestoreContinuationError::ContinuationDisabled)
        }
        Err(rustty_vt::stream::WriteContinuationError::ContinuationUnavailable) => {
            return Err(RestoreContinuationError::ContinuationUnavailable)
        }
    }
    // if (!std.mem.eql(u8, continuation, exported.written())) {
    //     return error.InvalidContinuation;
    // }
    if continuation != exported.written() {
        return Err(RestoreContinuationError::InvalidContinuation);
    }
    Ok(())
}

// pub const FromDecodedError = error{
//     OutOfMemory,
//     InvalidContinuation,
// };
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FromDecodedError {
    OutOfMemory,
    InvalidContinuation,
}

/// Transfer a core snapshot result into a caller-owned C terminal.
///
/// `io` is the owner the decoded terminal was built with and is retained
/// by the returned wrapper. The decoded terminal is transferred only
/// after its final heap address has been allocated; its
/// continuation remains in `decoded` and is replayed before returning.
/// `continuation_max_bytes` selects the returned terminal's tracking policy:
/// zero uses a temporary exact-size tracker and restores the ordinary C
/// default before returning, while a nonzero value leaves tracking enabled
/// with that limit.
// pub fn fromDecoded(
//     alloc: std.mem.Allocator,
//     io: Io,
//     decoded: *snapshot_core.Decoded,
//     continuation_max_bytes: usize,
// ) FromDecodedError!Terminal {
//
// PORT NOTE: the `io: Io` owner parameter (and every `io.deinit(alloc)` unwind
// arm) is N/A — see the `Io` PORT NOTE. The reference `alloc` is the decoder's
// resolved allocator; rustty passes the decoder's stored C-allocator COPY and
// builds a fresh F-008 holder here, so the produced terminal owns its own
// holder exactly like a `new`-created one (`free` reclaims it). The decoded
// terminal was built over the same allocator as the runtime handle (parity
// tail S7-4: the snapshot decoder passes `lib.alloc.default(alloc_)` to
// `ready`), so its internals, like a `new`-created terminal's, grow through
// the caller's allocator.
///
/// # Safety
/// `alloc_`, if provided, must reference a valid `CAllocator`.
pub unsafe fn from_decoded(
    alloc_: Option<&CAllocator>,
    decoded: &mut rustty_vt::snapshot::Decoded,
    continuation_max_bytes: usize,
) -> std::result::Result<Terminal, FromDecodedError> {
    let alloc_holder: *mut dyn rustty_lib::allocator::ZigAllocator =
        Box::into_raw(rustty_lib::allocator::default(alloc_));
    // SAFETY: live until `free` (or the unwind arms below) reclaims it.
    let alloc: &'static dyn rustty_lib::allocator::ZigAllocator = unsafe { &*alloc_holder };

    // const native = alloc.create(ZigTerminal) catch { io.deinit(alloc); return error.OutOfMemory; };
    let terminal_ptr: *mut ZigTerminal = match rustty_lib::allocator::create_raw(alloc) {
        Ok(ptr) => ptr,
        Err(_) => {
            drop(unsafe { Box::from_raw(alloc_holder) });
            return Err(FromDecodedError::OutOfMemory);
        }
    };
    // native.* = decoded.toOwned();
    // SAFETY: `terminal_ptr` is valid uninitialized `ZigTerminal` storage.
    unsafe { std::ptr::write(terminal_ptr, decoded.to_owned()) };
    // The decoded terminal's allocator is the one the wrapper stores below.
    // SAFETY: `terminal_ptr` holds the transferred terminal; `alloc_` is the
    // caller's allocator (the function's contract).
    unsafe {
        (*terminal_ptr)
            .gpa()
            .debug_assert_same(rustty_vt::alloc::default(alloc_))
    };

    // const continuation = switch (decoded.continuation) {
    //     .ground => "",
    //     .bytes => |bytes| bytes,
    // };
    let continuation: &[u8] = match &decoded.continuation {
        rustty_vt::snapshot::Continuation::Ground => b"",
        rustty_vt::snapshot::Continuation::Bytes(bytes) => bytes,
    };
    assert!(continuation_max_bytes == 0 || continuation.len() <= continuation_max_bytes);

    // Without opt-in retention, non-ground state needs tracking only long
    // enough to verify that replay reconstructed the exact canonical
    // continuation. With retention, even ground state needs a tracker so its
    // continuation can be exported successfully as an empty slice.
    let tracker_max_bytes = if continuation_max_bytes > 0 {
        continuation_max_bytes
    } else {
        continuation.len()
    };
    // const terminal = wrap(alloc, native, io, tracker_max_bytes) catch |err| { ... };
    let terminal = match unsafe {
        wrap(
            alloc_holder,
            alloc_.copied(),
            // `lib.alloc.default(alloc_)` as the runtime handle (a non-NULL
            // `alloc_` outlives the terminal: the caller's contract).
            rustty_vt::alloc::default(alloc_),
            terminal_ptr,
            tracker_max_bytes,
        )
    } {
        Ok(terminal) => terminal,
        Err(_oom) => {
            // native.deinit(alloc); alloc.destroy(native); io.deinit(alloc);
            // PORT NOTE: `deinit` takes the allocator the decoded terminal was
            // built with (the snapshot decoder's, the caller's), which the
            // terminal records as `gpa()`.
            // SAFETY: `terminal_ptr` holds the transferred terminal.
            unsafe {
                let gpa = (*terminal_ptr).gpa();
                (*terminal_ptr).deinit(gpa);
                std::ptr::drop_in_place(terminal_ptr);
                rustty_lib::allocator::destroy_raw(alloc, terminal_ptr);
            }
            drop(unsafe { Box::from_raw(alloc_holder) });
            return Err(FromDecodedError::OutOfMemory);
        }
    };
    // errdefer free(terminal); — the explicit unwind in the Err arm below.

    if !continuation.is_empty() {
        // SAFETY: `terminal` is the live wrapper created above.
        if let Err(err) = unsafe { restore_continuation(terminal, continuation) } {
            unsafe { free(terminal) };
            return Err(match err {
                // The decoded bytes fit this fresh tracker's cap, so losing them
                // while replaying can only be an allocation failure.
                RestoreContinuationError::OutOfMemory
                | RestoreContinuationError::ContinuationUnavailable => {
                    FromDecodedError::OutOfMemory
                }
                RestoreContinuationError::ContinuationDisabled
                | RestoreContinuationError::InvalidContinuation => {
                    FromDecodedError::InvalidContinuation
                }
            });
        }
    }

    // Unless the decoder opted into retention, restore the same disabled
    // tracking default used by newly created C terminals. Disabling tracking
    // does not alter the parser or UTF-8 state reconstructed above.
    if continuation_max_bytes == 0 {
        // SAFETY: `terminal` is the live wrapper created above.
        unsafe { set_continuation_max_bytes(terminal, DEFAULT_CONTINUATION_MAX_BYTES) };
    }
    Ok(terminal)
}

/// C: GhosttyDeviceAttributes -> RusttyDeviceAttributes (R2)
// pub const DeviceAttributes = Effects.CDeviceAttributes;
pub type DeviceAttributes = CDeviceAttributes;

/// C: GhosttyTerminalScrollViewport -> RusttyTerminalScrollViewport (R2)
// pub const ScrollViewport = ZigTerminal.ScrollViewport.C;
//
// PORT NOTE: `ZigTerminal.ScrollViewport` is `union(Tag)` with the C-ABI
// `lib.TaggedUnion(target, @This(), [2]u64)` projection deferred here. We author
// it by hand following the `TaggedUnion` expansion (see point.rs for the same
// pattern): a `#[repr(C)]` struct `{ tag: ScrollViewportTag, value:
// ScrollViewportCValue }` where `ScrollViewportCValue` is an `extern union` with
// one member per tag (`top`/`bottom` carry no payload -> unit `()` is not a
// union member, so they ride the padding; `delta: isize`) plus the PINNED
// `_padding: [u64; 2]`. The `[2]u64` padding is verbatim from the reference
// comment ("largest variant is isize (8 bytes on 64-bit); use [2]u64 (16 bytes)
// for future expansion") and must NEVER change (ABI stability).
pub type ScrollViewport = ScrollViewportC;

/// C: RusttyTerminalScrollViewportTag (R2) — checked by the A5 enum-discriminant arm.
/// The C ABI compatible tag for `ScrollViewport`.
// PORT NOTE: `Tag = lib.Enum(lib.target, &.{ "top", "bottom", "delta", "row" })`;
// for the `.c` target a fieldless `enum(c_int)` in key order. `Row` is appended
// (oracle 3a2e28329) — the union already reserves `[2]u64` so this is ABI stable.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrollViewportTag {
    Top = 0,
    Bottom = 1,
    Delta = 2,
    Row = 3,
}

/// The C ABI compatible union value for `ScrollViewport`.
// PORT NOTE: `lib.TaggedUnion` gives one union field per tag. `top`/`bottom`
// have `void` payload (no addressable member in C); only `delta: isize` carries
// a value. The `_padding` member pins the union to `[2]u64`.
#[repr(C)]
#[derive(Clone, Copy)]
pub union ScrollViewportCValue {
    pub delta: isize,
    /// Absolute row offset for the `Row` tag (oracle 3a2e28329).
    pub row: usize,
    /// Padding to the PINNED ABI size. `[2]u64` (16 bytes) per the reference.
    pub _padding: [u64; 2],
}

/// PORT NOTE (`9673a22b0` ABI manifest): `ScrollViewport`'s `lib.TaggedUnion`
/// options carry NO `field_renames` (Terminal.zig: `.{ .padding = [2]u64 }`),
/// so `cFieldRename` yields null for the payload-less `top`/`bottom` tags and
/// the tag name for `delta`/`row`. Consumed by `types.rs` for the
/// `RusttyTerminalScrollViewport` arms and the `RusttyTerminalScrollViewportValue`
/// union fields.
pub const SCROLL_VIEWPORT_C_FIELD_ARMS: &[(&str, Option<&str>)] = &[
    ("top", None),
    ("bottom", None),
    ("delta", Some("delta")),
    ("row", Some("row")),
];

/// `cFieldRename` arms for the `UnknownSequence` tagged value field (the
/// `types.zig` `.generated` union arms; consumed by the ABI manifest rows).
pub const UNKNOWN_SEQUENCE_C_FIELD_ARMS: &[(&str, Option<&str>)] = &[("apc", Some("apc"))];

/// C: GhosttyTerminalScrollViewport -> RusttyTerminalScrollViewport (R2) — the hand-authored tagged union.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ScrollViewportC {
    /// D11 (P3-W6 T2): held as the C integer, NOT as `ScrollViewportTag`.
    /// `rustty_terminal_scroll_viewport` takes this struct BY VALUE, so a
    /// `ScrollViewportTag`-typed field is materialized by the callee prologue
    /// and an undeclared value is instant UB there, before the `match` below can
    /// run. `include/rustty/vt/terminal.h` itself declares
    /// `RUSTTY_TERMINAL_SCROLL_VIEWPORT_MAX_VALUE`, so a conforming C caller can
    /// write one. This is the IN-RANGE member of the D11 population: oracle
    /// `3a2e28329` added `Row = 3` to this very enum.
    pub tag: c_int,
    pub value: ScrollViewportCValue,
}

// BUILD-failure gate (D2): pin the `[2]u64` padding size, the natural alignment,
// and the `c_int`-width tag. Drift here fails the build.
const _: () = assert!(size_of::<ScrollViewportCValue>() == size_of::<[u64; 2]>());
const _: () = assert!(align_of::<ScrollViewportCValue>() == 8);
// KEEP (P3-W6 T2 Step 1): this assert is what proves the `tag: c_int` field
// leaves the published ABI provably unchanged.
const _: () = assert!(size_of::<ScrollViewportTag>() == size_of::<c_int>());

impl ScrollViewportTag {
    /// Narrow an inbound C integer to a `ScrollViewportTag`, mirroring
    /// `std.meta.intToEnum`; `None` outside the declared range.
    pub fn from_c_int(v: c_int) -> Option<ScrollViewportTag> {
        match v {
            0 => Some(ScrollViewportTag::Top),
            1 => Some(ScrollViewportTag::Bottom),
            2 => Some(ScrollViewportTag::Delta),
            3 => Some(ScrollViewportTag::Row),
            _ => None,
        }
    }
}

// pub fn scroll_viewport(terminal_: Terminal, behavior: ScrollViewport) ... {
//     const t: *ZigTerminal = (terminal_ orelse return).terminal;
//     t.scrollViewport(switch (behavior.tag) {
//         .top => .top,
//         .bottom => .bottom,
//         .delta => .{ .delta = behavior.value.delta },
//         .row => .{ .row = behavior.value.row },
//     });
// }
/// # Safety
/// `terminal_`, if non-null, must be a live terminal handle.
pub unsafe fn scroll_viewport(terminal_: Terminal, behavior: ScrollViewport) {
    if terminal_.is_null() {
        return;
    }
    // SAFETY: `terminal_` is a live wrapper; `terminal` is its owned heap
    // terminal.
    let t: *mut ZigTerminal = unsafe { (*terminal_).terminal };
    // D11 (P3-W6 T2): the narrowing point for `behavior.tag`. Exactly the shape
    // `compress` uses for its `mode_` (`:960`): an explicit `match` on the
    // incoming C integer, the sound analog of `std.meta.intToEnum`.
    //
    // PORT NOTE: reference defect (R13), UPSTREAM-DEFECTS.md UD113. The
    // reference switches directly on the typed tag (`c/terminal.zig:1508`), so
    // an undeclared tag from the C caller panics "switch on corrupt value" in
    // Debug/ReleaseSafe and takes an arbitrary arm in ReleaseFast (measured,
    // Zig 0.16.0). The tag is input from outside the library, so it is not an
    // invariant (R7) and must not trap here either: this port used to carry a
    // `debug_assert!` mirroring the trap, which aborted across `extern "C"` in
    // debug and `release-safe` builds (task R13-D, inventory P4). Every build
    // now takes the defined early return — the same path as a null terminal
    // handle above, since the function returns nothing to report an error in.
    let Some(zig_behavior) = ScrollViewportTag::from_c_int(behavior.tag) else {
        return;
    };
    let zig_behavior = match zig_behavior {
        ScrollViewportTag::Top => ZigScrollViewport::Top,
        ScrollViewportTag::Bottom => ZigScrollViewport::Bottom,
        // SAFETY: the `delta` tag selects the `delta` union member (C ABI
        // contract mirrored from the reference `behavior.value.delta`).
        ScrollViewportTag::Delta => ZigScrollViewport::Delta(unsafe { behavior.value.delta }),
        // SAFETY: the `row` tag selects the `row` union member.
        ScrollViewportTag::Row => ZigScrollViewport::Row(unsafe { behavior.value.row }),
    };
    unsafe { (*t).scroll_viewport(zig_behavior) };
}

// pub fn resize(terminal_, cols, rows, cell_width_px, cell_height_px) ... {
//     const wrapper = terminal_ orelse return .invalid_value;
//     wrapper.stream.handler.resize(.{
//         .cols = cols,
//         .rows = rows,
//         .cell_size_px = .{
//             .width = cell_width_px,
//             .height = cell_height_px,
//         },
//     }) catch |err| return switch (err) {
//         error.InvalidValue => .invalid_value,
//         error.OutOfMemory => .out_of_memory,
//     };
//     return .success;
// }
/// # Safety
/// `terminal_`, if non-null, must be a live terminal handle.
pub unsafe fn resize(
    terminal_: Terminal,
    cols: size::CellCountInt,
    rows: size::CellCountInt,
    cell_width_px: u32,
    cell_height_px: u32,
) -> Result {
    // const wrapper = terminal_ orelse return .invalid_value;
    if terminal_.is_null() {
        return Result::InvalidValue;
    }

    // wrapper.stream.handler.resize(.{ ... }) catch |err| return switch (err) {
    //     error.InvalidValue => .invalid_value,
    //     error.OutOfMemory => .out_of_memory,
    // };
    //
    // PORT NOTE (oracle 89b103dd5, area C): the whole inline body that used to
    // live here — the zero-dimension check, the saturating `width_px`/
    // `height_px` multiply, the `modes.set(.synchronized_output, false)`, and
    // the in-band mode-2048 `size_report::encode` — is now owned by
    // `Terminal::resize` + `stream_terminal::Handler::resize`. Do NOT re-add any
    // of it: the geometry must be applied even when the grid is unchanged, and
    // the synchronized-output clear must happen exactly once (guarded by
    // `rustty_vt::modes::sync_output_clear_count`).
    // SAFETY: `terminal_` is a live wrapper; the handler aliases its terminal.
    let r = unsafe {
        (*handler_ptr(terminal_)).resize(rustty_vt::terminal::Resize {
            cols,
            rows,
            cell_size_px: Some(rustty_vt::terminal::ResizeCellSize {
                width: cell_width_px,
                height: cell_height_px,
            }),
        })
    };
    if let Err(err) = r {
        return match err {
            rustty_vt::terminal::ResizeError::InvalidValue => Result::InvalidValue,
            rustty_vt::terminal::ResizeError::OutOfMemory => Result::OutOfMemory,
        };
    }

    // return .success;
    Result::Success
}

// pub fn reset(terminal_: Terminal) ... {
//     const wrapper = terminal_ orelse return;
//     const t: *ZigTerminal = wrapper.terminal;
//
//     // A reset always turns off synchronized output, ending its hold.
//     const sync = t.modes.get(.synchronized_output);
//     t.fullReset();
//     if (sync) Effects.renderHoldTrampoline(&wrapper.stream.handler, false);
// }
/// # Safety
/// `terminal_`, if non-null, must be a live terminal handle.
pub unsafe fn reset(terminal_: Terminal) {
    // const wrapper = terminal_ orelse return;
    if terminal_.is_null() {
        return;
    }
    // SAFETY: `terminal_` is a live wrapper.
    let t: *mut ZigTerminal = unsafe { (*terminal_).terminal };

    // A reset always turns off synchronized output, ending its hold.
    // SAFETY: short-lived place reads/calls through the raw terminal
    // pointer; no terminal borrow outlives its statement.
    let sync = unsafe { (*t).modes.get(rustty_vt::modes::Mode::SynchronizedOutput) };
    unsafe { (*t).full_reset() };
    if sync {
        // SAFETY (SB): `handler_ptr` reaches the handler by place (or, when
        // this `reset` itself runs inside an effect callback, through the
        // running trampoline's `live_handler`, N452), and the raw trampoline
        // never dereferences it, so no `&mut Handler` exists while the
        // callback re-enters the terminal.
        let handler = unsafe { handler_ptr(terminal_) };
        Effects::render_hold_trampoline_raw(handler, false);
    }
}

// (cfc19e805: `mode_get`/`mode_set` were REMOVED — replaced by the
// `.mode`/`.mode_default` option arms in `set_typed` and the `.mode`
// `TerminalData` arm in `get_typed`.)

// pub fn free(terminal_: Terminal) ... {
//     const wrapper = terminal_ orelse return;
//     const t = wrapper.terminal;
//     const alloc = t.gpa();
//     for (wrapper.tracked_grid_refs.keys()) |ref| ref.terminal = null;
//     wrapper.tracked_grid_refs.deinit(alloc);
//     for (wrapper.searches.keys()) |search| search.terminal = null;
//     wrapper.searches.deinit(alloc);
//     wrapper.stream.deinit();
//     t.deinit(alloc);
//     if (wrapper.tmp_dir_path) |path| alloc.free(path);
//     alloc.destroy(t);
//     alloc.destroy(wrapper);
// }
//
// PORT NOTE (oracle `105d0a545`, P7-W2 T3): the reference's explicit
// `if (wrapper.tmp_dir_path) |path| alloc.free(path);` is ported as the same
// explicit `free_bytes` call, below, through the SAME caller allocator
// (`alloc_holder`) it was allocated from — `TerminalWrapper::tmp_dir_path` is
// `Option<ZigSlice>`, which (unlike `Option<Vec<u8>>`) has no `Drop` of its
// own, matching the F-008 rule that this buffer's lifecycle is entirely
// manual, routed through the caller's allocator like every other byte this
// file hands the embedder. It runs AFTER every screen was torn down by
// `t.deinit(alloc)` above, so no screen's borrowed `*const [u8]` is read
// afterward.
//
// PORT NOTE (F-008, supersedes the D3-lifecycle substitution): the reference
// frees `t` and `wrapper` via the C allocator (`alloc.destroy`), recovered as
// `t.gpa()`. rustty allocated both through the C-resolved allocator stored on
// the wrapper (see `new_`), so the storage is returned through it — the caller
// allocator's free callback fires. rustty's `t.gpa()` is the Terminal
// INTERNALS allocator (`Global`), used only for `t.deinit`. The terminal is
// freed AFTER the stream (the stream's handler aliases it), matching the
// reference order (`t.deinit` + `destroy(t)` after `stream.deinit`); the
// resolved-allocator holder itself is reclaimed LAST (everything above frees
// through it).
/// # Safety
/// `terminal_`, if non-null, must be a live terminal handle previously returned
/// by `new` and not yet freed.
pub unsafe fn free(terminal_: Terminal) {
    // const wrapper = terminal_ orelse return;
    if terminal_.is_null() {
        return;
    }

    // const t = wrapper.terminal;
    // SAFETY: `terminal_` is a `new`-returned wrapper pointer, not yet freed
    // (caller contract); raw-ptr reads of its sibling fields.
    let t: *mut ZigTerminal = unsafe { (*terminal_).terminal };
    // const alloc = t.gpa();
    // PORT NOTE (parity tail S7-3): this is the caller's allocator, the one
    // `new` passed to `Terminal.init` (or the Rust heap for NULL); the
    // terminal and wrapper storage are freed through the wrapper's `alloc`
    // holder, the same allocator.
    // SAFETY: `t` is the wrapper's live, owned terminal.
    let alloc = unsafe { (*t).gpa() };
    let alloc_holder: *mut dyn rustty_lib::allocator::ZigAllocator = unsafe { (*terminal_).alloc };

    // for (wrapper.tracked_grid_refs.keys()) |ref| ref.terminal = null;
    // SAFETY: each tracked-ref pointer was inserted by `grid_ref_track` and
    // is still live (it back-references this wrapper); detach it so a later
    // tracked-ref free does not dereference the freed terminal.
    for &ref_ in unsafe { (*terminal_).tracked_grid_refs.keys() } {
        unsafe { (*ref_).terminal = std::ptr::null_mut() };
    }
    // wrapper.tracked_grid_refs.deinit(alloc);
    // (the map frees now, through the allocator it was bound to in `wrap`.)
    unsafe { (*terminal_).tracked_grid_refs.clear_and_free() };

    // for (wrapper.searches.keys()) |search| search.terminal = null;
    // PORT NOTE (`f0c918fc4`): detach every registered search so a later
    // `search::free` (or any search call that needs the terminal) sees a null
    // terminal instead of this freed storage — the "free in any order"
    // contract. Written through the registry's own copy of the raw pointer
    // (no `&mut SearchWrapper` exists anywhere; `search.rs` SB note).
    // SAFETY: each search pointer was inserted by `search::new` and is still
    // live (a freed search unregisters itself first).
    for &search in unsafe { (*terminal_).searches.keys() } {
        unsafe { (*search).terminal = std::ptr::null_mut() };
    }
    // wrapper.searches.deinit(alloc);
    // (the map frees now, through the allocator it was bound to in `wrap`.)
    unsafe { (*terminal_).searches.clear_and_free() };

    // wrapper.stream.deinit();
    unsafe { (*terminal_).stream.deinit() };

    // if (builtin.os.tag != .freestanding) {
    //     // Deinit is always safe to call, even for single-threaded instances
    //     wrapper.io_impl.deinit();
    //     alloc.destroy(wrapper.io_impl);
    // }
    //   N/A: rustty has no `io_impl` to tear down (see `TerminalWrapper`).

    // t.deinit(alloc);
    unsafe { (*t).deinit(alloc) };

    // if (wrapper.tmp_dir_path) |path| alloc.free(path);
    //
    // PORT NOTE (oracle `105d0a545`, P7-W2 T3): `ZigSlice` has no `Drop` of
    // its own (see `TerminalWrapper::tmp_dir_path`'s PORT NOTE) — the
    // reference's explicit free is ported as an explicit free here too,
    // through the SAME caller allocator (`alloc_holder`) it was allocated
    // from. This runs AFTER every screen was torn down by `t.deinit` above,
    // so no screen's borrowed `*const [u8]` is read afterward.
    // SAFETY: `terminal_` is still live, initialized storage; taking the
    // field leaves `None` behind, so the read further below sees nothing to
    // double-free.
    if let Some(old) = unsafe { (*terminal_).tmp_dir_path.take() } {
        unsafe { (&*alloc_holder).free_bytes(old) };
    }

    // alloc.destroy(t); — drop the terminal value in place (Rust must run its
    // Drop; Zig has no destructors), then return its storage through the
    // C-resolved allocator, AFTER the stream's handler (which aliased it) is
    // torn down.
    // SAFETY: `t` was created by `new_` via `create_raw` and is uniquely owned.
    unsafe { std::ptr::drop_in_place(t) };
    unsafe { rustty_lib::allocator::destroy_raw(&*alloc_holder, t) };

    // alloc.destroy(wrapper); — move the wrapper value out of the caller
    // storage (its `alloc` field is a raw pointer, so no double reclaim),
    // return the storage through the C-resolved allocator, then drop the
    // value (stream buffers/tracked set) while the allocator is still alive.
    //
    // PORT NOTE (N451, P7-W2 T9): the handler's `terminfo_name` borrows this
    // wrapper's own `terminfo_name_buf`. The moved-out value outlives the
    // storage it points into (it is dropped after `destroy_raw`), and a
    // dangling `&[u8]` inside a moved value is UB in Rust (Miri: "constructing
    // invalid value ... dangling reference"), though the reference's Zig slice
    // may dangle. Clear the borrow by place first; nothing reads it again.
    // SAFETY: `terminal_` is still live, initialized storage.
    unsafe { (*terminal_).stream.handler.terminfo_name = None };
    // SAFETY: `terminal_` was created by `new_` via `create_raw`; after this
    // read the storage is treated as uninitialized and freed exactly once.
    let wrapper_val: TerminalWrapper = unsafe { std::ptr::read(terminal_) };
    unsafe { rustty_lib::allocator::destroy_raw(&*alloc_holder, terminal_) };
    drop(wrapper_val);

    // Reclaim the resolved-allocator holder LAST (see `TerminalWrapper.alloc`).
    // SAFETY: `alloc_holder` came from `Box::into_raw` in `new_`; nothing
    // refers to it anymore.
    drop(unsafe { Box::from_raw(alloc_holder) });
}

// PORT NOTE: `TerminalData` aliases `color.PaletteC` / `color.RGB.C` /
// `style_c.Style` / `selection_c.CSelection` / `KittyGraphics` in its
// `OutType`, and `Option` aliases the same in `InType`. Those projections are
// resolved by the fill sub-batch's `setTyped`/`getTyped` bodies (and
// `color::PaletteC` itself is a DEFERRED vt-c projection per the recon); they
// are intentionally NOT imported in this types+signatures skeleton.

#[cfg(test)]
mod tests;
