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
// pub const Io = struct {
//     impl: Impl,
//
//     /// Platform-specific storage backing the public `std.Io` value.
//     ///
//     /// Where supported (POSIX) we use TinyIo, which is stateless and
//     /// supports exactly the operations the terminal needs at a fraction
//     /// of the code size (see lib/TinyIo.zig). On Windows we use
//     /// `std.Io.Threaded` since TinyIo doesn't implement the NT
//     /// operations. On the remaining targets (e.g. freestanding wasm)
//     /// TinyIo degrades to `std.Io.failing`, which is correct: they have
//     /// no filesystem.
//     const Impl = if (builtin.os.tag == .windows)
//         *std.Io.Threaded
//     else
//         lib.TinyIo;
//
//     /// Allocation failures possible while constructing an I/O owner.
//     pub const Error = error{OutOfMemory};
//
//     /// Allocate the native I/O implementation when the platform requires it.
//     pub fn init(alloc: std.mem.Allocator) Error!Io { ... }
//
//     /// Return the value passed to native terminal construction and decoding.
//     pub fn io(self: Io) std.Io { ... }
//
//     /// Release an I/O implementation that has not already been transferred.
//     pub fn deinit(self: Io, alloc: std.mem.Allocator) void {
//         // Note: this must not name `std.Io.Threaded` in the condition
//         // because resolving that type trips its container-level comptime
//         // checks on targets it doesn't support (e.g. wasm32-freestanding).
//         if (comptime Impl != lib.TinyIo) { ... }
//     }
// };
//
// PORT NOTE (Bundle S, extends the `io_impl` note below): the reference `Io`
// owns the `std.Io` implementation retained by every C terminal (TinyIo on
// POSIX, `std.Io.Threaded` on Windows, failing on freestanding) so snapshot
// decoding can create it before the native terminal exists and transfer it
// into the final C wrapper (`fromDecoded` consumes it on every path). rustty
// has no `std.Io` seam anywhere — the snapshot codec's Rust signatures dropped
// the `io` parameter entirely (see `rustty_vt::snapshot::codec`'s module PORT
// NOTE) — so the type, its `init`/`io`/`deinit`, and every transfer edge are
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
}

/// `std.AutoArrayHashMapUnmanaged(K, void)` over the terminal's allocator
/// handle: the wrapper's registries of live tracked refs and searches.
pub type WrapperSet<K> = rustty_vt::alloc::AArrayHashMap<K, ()>;

/// C callback state for terminal effects. Most trampolines are always
/// installed on the stream handler; they check these fields and no-op when
/// the corresponding callback is null. The unknown-sequence and
/// clipboard trampolines are installed dynamically to preserve their
/// null fast paths (for clipboard_write, a null Zig-level effect makes
/// Kitty clipboard writes fail up front instead of spooling a
/// transaction that can never commit; for clipboard_read it keeps
/// reads denied).
// PORT NOTE (internal, NOT C-ABI): mirrors the reference `Effects` struct field
// for field. The fn-ptr typedefs and `CDeviceAttributes` ARE part of the C ABI
// (named below). `da_features_buf` is the scratch buffer the device-attributes
// trampoline fills (`[64]device_attributes.Primary.Feature`).
pub struct Effects {
    pub userdata: *mut c_void,
    pub write_pty: Option<WritePtyFn>,
    pub bell: Option<BellFn>,
    pub color_scheme: Option<ColorSchemeFn>,
    pub device_attributes_cb: Option<DeviceAttributesFn>,
    pub enquiry: Option<EnquiryFn>,
    pub xtversion: Option<XtversionFn>,
    pub title_changed: Option<TitleChangedFn>,
    pub pwd_changed: Option<PwdChangedFn>,
    pub size_cb: Option<SizeFn>,
    pub clipboard_write: Option<ClipboardWriteFn>,
    pub clipboard_read: Option<ClipboardReadFn>,
    pub desktop_notification: Option<DesktopNotificationFn>,
    pub progress_report: Option<ProgressReportFn>,
    pub unknown_sequence: Option<UnknownSequenceFn>,

    /// Scratch buffer for DA1 feature codes. The device attributes
    /// trampoline converts C feature codes into this buffer and returns
    /// a slice pointing into it. Storing it here ensures the slice
    /// remains valid after the trampoline returns, since the caller
    /// (`reportDeviceAttributes`) reads it before any re-entrant call.
    pub da_features_buf: [rustty_vt::device_attributes::Feature; 64],
}

// PORT NOTE: the reference `Effects` defaults every callback to `null`, the
// `userdata` to `null`, and `da_features_buf` to `undefined` (it is always
// overwritten before being read). `TerminalWrapper { effects: Effects = .{} }`
// relies on those defaults; this is the parity analog (`undefined` -> an
// arbitrary `Feature` placeholder, never observed).
impl Default for Effects {
    fn default() -> Effects {
        Effects {
            userdata: std::ptr::null_mut(),
            write_pty: None,
            bell: None,
            color_scheme: None,
            device_attributes_cb: None,
            enquiry: None,
            xtversion: None,
            title_changed: None,
            pwd_changed: None,
            size_cb: None,
            clipboard_write: None,
            clipboard_read: None,
            desktop_notification: None,
            progress_report: None,
            unknown_sequence: None,
            da_features_buf: [rustty_vt::device_attributes::Feature::AnsiColor; 64],
        }
    }
}

/// C function pointer type for the write_pty callback.
// pub const WritePtyFn = *const fn (Terminal, ?*anyopaque, [*]const u8, usize) callconv(lib.calling_conv) void;
pub type WritePtyFn = extern "C" fn(Terminal, *mut c_void, *const u8, usize);

/// C function pointer type for the bell callback.
// pub const BellFn = *const fn (Terminal, ?*anyopaque) callconv(lib.calling_conv) void;
pub type BellFn = extern "C" fn(Terminal, *mut c_void);

/// C function pointer type for the color_scheme callback.
/// Returns true and fills out_scheme if a color scheme is available,
/// or returns false to silently ignore the query.
// pub const ColorSchemeFn = *const fn (Terminal, ?*anyopaque, *device_status.ColorScheme) callconv(lib.calling_conv) bool;
pub type ColorSchemeFn =
    extern "C" fn(Terminal, *mut c_void, *mut device_status::ColorScheme) -> bool;

/// C function pointer type for the enquiry callback.
/// Returns the response bytes. The memory must remain valid
/// until the callback returns.
// pub const EnquiryFn = *const fn (Terminal, ?*anyopaque) callconv(lib.calling_conv) lib.String;
pub type EnquiryFn = extern "C" fn(Terminal, *mut c_void) -> LibString;

/// C function pointer type for the xtversion callback.
/// Returns the version string (e.g. "ghostty 1.2.3"). The memory
/// must remain valid until the callback returns. An empty string
/// (len=0) causes the default "libghostty" to be reported.
// pub const XtversionFn = *const fn (Terminal, ?*anyopaque) callconv(lib.calling_conv) lib.String;
pub type XtversionFn = extern "C" fn(Terminal, *mut c_void) -> LibString;

/// C function pointer type for the title_changed callback.
// pub const TitleChangedFn = *const fn (Terminal, ?*anyopaque) callconv(lib.calling_conv) void;
pub type TitleChangedFn = extern "C" fn(Terminal, *mut c_void);

/// C function pointer type for the pwd_changed callback.
// pub const PwdChangedFn = *const fn (Terminal, ?*anyopaque) callconv(lib.calling_conv) void;
pub type PwdChangedFn = extern "C" fn(Terminal, *mut c_void);

/// C function pointer type for the size callback. Used by XTWINOPS queries
/// and VT-driven mode 2048 enable reports. Returns true and fills out_size
/// if size is available, or false to suppress the XTWINOPS response or
/// mode 2048 report.
// pub const SizeFn = *const fn (Terminal, ?*anyopaque, *size_report.Size) callconv(lib.calling_conv) bool;
pub type SizeFn = extern "C" fn(Terminal, *mut c_void, *mut size_report::Size) -> bool;

/// C function pointer type for the device_attributes callback.
/// Returns true and fills out_attrs if attributes are available,
/// or returns false to silently ignore the query.
// pub const DeviceAttributesFn = *const fn (Terminal, ?*anyopaque, *CDeviceAttributes) callconv(lib.calling_conv) bool;
pub type DeviceAttributesFn = extern "C" fn(Terminal, *mut c_void, *mut CDeviceAttributes) -> bool;

/// A single MIME representation in a clipboard write.
///
/// C: GhosttyClipboardContent -> RusttyClipboardContent (R2)
// pub const ClipboardContent = extern struct { mime: lib.String, data: lib.String };
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ClipboardContent {
    pub mime: LibString,
    pub data: LibString,
}

/// A protocol-neutral request to replace or clear clipboard contents.
/// The embedder answers by calling `reply` with the request before the
/// callback returns.
///
/// C: GhosttyClipboardWrite -> RusttyClipboardWrite (R2)
// pub const ClipboardWrite = extern struct { size, location, contents,
//     contents_len, name, granted, can_remember, ctx, reply };
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ClipboardWrite {
    pub size: usize,
    pub location: clipboard::Location,
    /// Null when `contents_len` is 0 (an empty write clears the destination).
    pub contents: *const ClipboardContent,
    pub contents_len: usize,
    pub name: LibString,
    pub granted: bool,
    pub can_remember: bool,
    /// Terminal-owned reply state; opaque to the embedder.
    // PORT NOTE (R9 callback-ctx): points at a STACK `ClipboardWriteCtx`
    // inside the write trampoline's frame; valid only for the duration of
    // the C callback (LIFETIMES.tsv: borrowed-during-call). Never retained,
    // never written through.
    pub ctx: *const c_void,
    pub reply: ClipboardWriteReplyFn,
}

/// The reply to a clipboard write request.
///
/// C: GhosttyClipboardWriteReply -> RusttyClipboardWriteReply (R2)
// pub const ClipboardWriteReply = extern struct { size, result, remember };
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ClipboardWriteReply {
    pub size: usize,
    pub result: clipboard::WriteStatus,
    pub remember: bool,
}

/// C function pointer type for replying to a clipboard write.
///
/// C: GhosttyClipboardWriteReplyFn -> RusttyClipboardWriteReplyFn (R2)
// pub const ClipboardWriteReplyFn = *const fn (*const ClipboardWrite, *const ClipboardWriteReply) callconv(lib.calling_conv) void;
pub type ClipboardWriteReplyFn = extern "C" fn(*const ClipboardWrite, *const ClipboardWriteReply);

/// The reply to a clipboard read request.
///
/// C: GhosttyClipboardReadReply -> RusttyClipboardReadReply (R2)
// pub const ClipboardReadReply = extern struct { size, result, contents,
//     contents_len, available, available_len, remember };
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ClipboardReadReply {
    pub size: usize,
    pub result: clipboard::ReadStatus,
    pub contents: *const ClipboardContent,
    pub contents_len: usize,
    pub available: *const LibString,
    pub available_len: usize,
    pub remember: bool,
}

/// A synchronous request to read clipboard contents. The embedder answers
/// by calling `reply` with the request before the callback returns.
///
/// C: GhosttyClipboardRead -> RusttyClipboardRead (R2)
// pub const ClipboardRead = extern struct { size, location, mimes, mimes_len,
//     list, name, granted, can_remember, ctx, reply };
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ClipboardRead {
    pub size: usize,
    pub location: clipboard::Location,
    pub mimes: *const LibString,
    pub mimes_len: usize,
    pub list: bool,
    pub name: LibString,
    pub granted: bool,
    pub can_remember: bool,
    /// Terminal-owned reply state; opaque to the embedder.
    // PORT NOTE (R9 callback-ctx): points at a STACK `ClipboardReadCtx`
    // inside the read trampoline's frame; valid only for the duration of
    // the C callback (LIFETIMES.tsv: borrowed-during-call).
    pub ctx: *const c_void,
    pub reply: ClipboardReadReplyFn,
}

/// C function pointer type for replying to a clipboard read.
///
/// C: GhosttyClipboardReadReplyFn -> RusttyClipboardReadReplyFn (R2)
// pub const ClipboardReadReplyFn = *const fn (*const ClipboardRead, *const ClipboardReadReply) callconv(lib.calling_conv) void;
pub type ClipboardReadReplyFn = extern "C" fn(*const ClipboardRead, *const ClipboardReadReply);

/// C function pointer type for the clipboard_write callback. The request
/// is borrowed for the callback duration and must be answered through
/// its reply function before the callback returns.
// pub const ClipboardWriteFn = *const fn (Terminal, ?*anyopaque, *const ClipboardWrite) callconv(lib.calling_conv) void;
//
// PORT NOTE (6959fd46c/7ae9b1113 end state): the callback no longer RETURNS a
// status — the write is answered through `request.reply` (an ABI BREAK of the
// pre-range typedef; header + typedef + trampoline + example land as one unit).
pub type ClipboardWriteFn = extern "C" fn(Terminal, *mut c_void, *const ClipboardWrite);

/// C function pointer type for the clipboard_read callback. The request
/// is borrowed for the callback duration and must be answered through
/// its reply function before the callback returns.
// pub const ClipboardReadFn = *const fn (Terminal, ?*anyopaque, *const ClipboardRead) callconv(lib.calling_conv) void;
pub type ClipboardReadFn = extern "C" fn(Terminal, *mut c_void, *const ClipboardRead);

/// A request to show a desktop notification (OSC 9 / OSC 777).
///
/// C: GhosttyTerminalDesktopNotification -> RusttyTerminalDesktopNotification (R2)
// pub const DesktopNotification = extern struct { size, title, body };
//
// PORT NOTE (byte-slice rule): `title`/`body` are `RusttyString`, i.e. a raw
// `(ptr, len)` pair over the stream's own parse buffer — BYTES, not text. The
// Rust side carries them as `&[u8]` end to end (`stream::ShowDesktopNotification`)
// and never converts to `String`: an OSC 9 payload is whatever the program
// wrote, and a lossy UTF-8 conversion would silently corrupt a non-UTF-8 title
// on the way to the embedder.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct DesktopNotification {
    pub size: usize,
    pub title: LibString,
    pub body: LibString,
}

/// C function pointer type for the desktop_notification callback. The request
/// and both strings are borrowed and only valid for the callback duration.
// pub const DesktopNotificationFn = *const fn (Terminal, ?*anyopaque, *const DesktopNotification) callconv(lib.calling_conv) void;
pub type DesktopNotificationFn = extern "C" fn(Terminal, *mut c_void, *const DesktopNotification);

/// C: GhosttyTerminalProgressState -> RusttyTerminalProgressState (R2) —
/// checked by the A5 enum-discriminant arm.
// pub const ProgressState = ... (the shared `osc.Command.ProgressReport.State`)
//
// PORT NOTE: `628adaf30`'s Zig `LibEnum` factory is structurally N/A here —
// rustty hand-authors each `#[repr(C)]` projection rather than generating it —
// but the ENUM that commit shares is `47d602c42`'s type, so it lands with the
// progress-report effect.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProgressState {
    Remove = 0,
    Set = 1,
    Error = 2,
    Indeterminate = 3,
    Pause = 4,
}

impl ProgressState {
    /// Project the pure-VT `osc::ProgressReportState` onto the C ABI enum.
    /// The discriminants are identical by construction (same key order).
    pub fn from_vt(s: rustty_vt::osc::ProgressReportState) -> ProgressState {
        use rustty_vt::osc::ProgressReportState as S;
        match s {
            S::Remove => ProgressState::Remove,
            S::Set => ProgressState::Set,
            S::Error => ProgressState::Error,
            S::Indeterminate => ProgressState::Indeterminate,
            S::Pause => ProgressState::Pause,
        }
    }
}

/// A progress report emitted by the running program (OSC 9;4).
///
/// C: GhosttyTerminalProgressReport -> RusttyTerminalProgressReport (R2)
// pub const ProgressReport = extern struct { size, state, progress };
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ProgressReport {
    pub size: usize,
    pub state: ProgressState,
    /// Progress percentage 0..=100, or `-1` when the report omitted it.
    pub progress: i8,
}

/// C function pointer type for the progress_report callback. The report is
/// borrowed and only valid for the callback duration.
// pub const ProgressReportFn = *const fn (Terminal, ?*anyopaque, *const ProgressReport) callconv(lib.calling_conv) void;
pub type ProgressReportFn = extern "C" fn(Terminal, *mut c_void, *const ProgressReport);

/// A borrowed unsupported string sequence.
///
/// C: GhosttyTerminalUnknownStringSequence -> RusttyTerminalUnknownStringSequence (R2)
// pub const UnknownStringSequence = extern struct {
//     truncated: bool,
//     content: lib.String,
// };
#[repr(C)]
#[derive(Clone, Copy)]
pub struct UnknownStringSequence {
    pub truncated: bool,
    pub content: LibString,
}

/// C: GhosttyTerminalUnknownSequenceTag -> RusttyTerminalUnknownSequenceTag (R2)
/// — checked by the A5 enum-discriminant arm.
// pub const Tag = lib.Enum(lib.target, &.{"apc"});
//
// PORT NOTE: the reference nests `Tag` inside `UnknownSequence`; Rust types
// cannot nest, so it lives at module scope (the vt `UnknownSequence` flattening
// precedent).
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnknownSequenceTag {
    Apc = 0,
}

/// C: GhosttyTerminalUnknownSequenceValue -> RusttyTerminalUnknownSequenceValue (R2)
// const c_union = lib.TaggedUnion(lib.target, @This(), .{ .padding = [16]u64 });
// pub const CValue = c_union.CValue;
//
// A future borrowed CSI payload may need parameter, separator, and
// intermediate arrays. Reserve 128 bytes so that representation and
// other structured sequence types can be added without an ABI break.
#[repr(C)]
#[derive(Clone, Copy)]
pub union UnknownSequenceValue {
    pub apc: UnknownStringSequence,
    pub _padding: [u64; 16],
}

/// An unsupported terminal sequence reported to the C callback.
///
/// C: GhosttyTerminalUnknownSequence -> RusttyTerminalUnknownSequence (R2)
// pub const UnknownSequence = union(Tag) { apc: UnknownStringSequence, ... };
// pub const C = c_union.C;
//
// PORT NOTE: the reference declares the tagged Zig union and derives the C
// projection through the `lib.TaggedUnion` factory; rustty hand-authors the
// projection (`{ tag, value }`) like `ScrollViewportC`. The value crosses the
// ABI outbound-only (by const pointer to the callback), so the tag field keeps
// its typed enum form. `UnknownSequence.cval` is the trampoline's constructor
// of this struct.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct UnknownSequence {
    pub tag: UnknownSequenceTag,
    pub value: UnknownSequenceValue,
}

// BUILD-failure gate (D2): pin the `[16]u64` padding size, the natural
// alignment, and the `c_int`-width tag. Drift here fails the build.
const _: () = assert!(size_of::<UnknownSequenceValue>() == size_of::<[u64; 16]>());
const _: () = assert!(align_of::<UnknownSequenceValue>() == 8);
const _: () = assert!(size_of::<UnknownSequenceTag>() == size_of::<c_int>());

/// C function pointer type for the unknown_sequence callback. The request
/// and its content are borrowed for the callback duration.
// pub const UnknownSequenceFn = *const fn (Terminal, ?*anyopaque, *const UnknownSequence.C) callconv(lib.calling_conv) void;
pub type UnknownSequenceFn = extern "C" fn(Terminal, *mut c_void, *const UnknownSequence);

/// A terminal mode and boolean value used for mode configuration.
///
/// C: GhosttyTerminalModeConfig -> RusttyTerminalModeConfig (R2)
// pub const ModeConfig = extern struct {
//     mode: modes.ModeTag.Backing,
//     value: bool,
// };
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ModeConfig {
    pub mode: modes::ModeTagBacking,
    pub value: bool,
}

impl ModeConfig {
    // fn toMode(self: ModeConfig) ?modes.Mode {
    //     const tag: modes.ModeTag = @bitCast(self.mode);
    //     return modes.modeFromInt(tag.value, tag.ansi);
    // }
    fn to_mode(self) -> Option<modes::Mode> {
        let tag = modes::ModeTag::from_backing(self.mode);
        modes::mode_from_int(tag.value, tag.ansi)
    }
}

/// C-compatible device attributes struct.
/// C: GhosttyDeviceAttributes -> RusttyDeviceAttributes (R2)
// pub const CDeviceAttributes = extern struct { primary, secondary, tertiary };
#[repr(C)]
#[derive(Clone, Copy)]
pub struct CDeviceAttributes {
    pub primary: CDeviceAttributesPrimary,
    pub secondary: CDeviceAttributesSecondary,
    pub tertiary: CDeviceAttributesTertiary,
}

// PORT NOTE: the reference nests `Primary`/`Secondary`/`Tertiary` inside
// `CDeviceAttributes`. Rust can't nest a type in a struct value namespace, so
// they are hoisted to siblings with the identical `extern` layout.

// pub const Primary = extern struct { conformance_level: u16, features: [64]u16, num_features: usize };
#[repr(C)]
#[derive(Clone, Copy)]
pub struct CDeviceAttributesPrimary {
    pub conformance_level: u16,
    pub features: [u16; 64],
    pub num_features: usize,
}

// pub const Secondary = extern struct { device_type: u16, firmware_version: u16, rom_cartridge: u16 };
#[repr(C)]
#[derive(Clone, Copy)]
pub struct CDeviceAttributesSecondary {
    pub device_type: u16,
    pub firmware_version: u16,
    pub rom_cartridge: u16,
}

// pub const Tertiary = extern struct { unit_id: u32 };
#[repr(C)]
#[derive(Clone, Copy)]
pub struct CDeviceAttributesTertiary {
    pub unit_id: u32,
}

// PORT NOTE (D3 container_of recovery): the reference trampolines recover the
// owning `*TerminalWrapper` from the `*Handler` via two nested
// `@fieldParentPtr`s: `Handler -> Stream` (the handler is `stream.handler`) and
// `Stream -> TerminalWrapper` (the stream is `wrapper.stream`). rustty collapses
// this into a single byte-offset subtraction off the `&mut Handler` address.
// The offset is the SUM of the two field offsets (`TerminalWrapper.stream` and
// `Stream.handler`) — the cross-crate, generic `Stream<'static>` makes a single
// nested `offset_of!(TerminalWrapper, stream.handler)` path fragile, so we sum
// the two leaf offsets (each is a plain in-crate field access).
//
// SB SOUNDNESS: the recovered `*mut TerminalWrapper` is used ONLY to read its
// SIBLING fields (`effects`, `terminal`) through the raw pointer; we never form
// an overlapping `&mut TerminalWrapper` while the caller's `&mut Handler` is
// still live (the `Handler` lives INSIDE the wrapper, so a `&mut TerminalWrapper`
// would alias it). All sibling reads go through raw pointer projection.
//
// PROVENANCE: a pointer derived (by offset subtraction) from the caller's
// `&mut Handler` would, under Stacked Borrows, only carry provenance over the
// Handler's bytes — reading the SIBLING `effects`/`terminal` through it is UB.
// `new_` therefore `expose_provenance()`s the whole `TerminalWrapper` allocation
// once; here we recover the wrapper by computing its ADDRESS (pure integer math
// off the handler address — no pointer provenance involved) and reconstituting a
// pointer over the exposed allocation via `with_exposed_provenance_mut`. This is
// the same exposed-provenance bridge `hyperlink.rs` uses for its stored page
// pointers, and it gives the recovered pointer whole-allocation provenance so the
// sibling reads are SB-sound.
///
/// Fetches a `TerminalWrapper` reference from a `Handler`.
#[inline]
fn wrapper_from_handler(handler: *mut Handler<'static>) -> *mut TerminalWrapper {
    // const stream_ptr: *Stream = @fieldParentPtr("handler", handler);
    // const wrapper: *TerminalWrapper = @fieldParentPtr("stream", stream_ptr);
    let off = offset_of!(TerminalWrapper, stream) + offset_of!(Stream<'static>, handler);
    // Compute the wrapper base ADDRESS from the handler address (integer math
    // only), then reconstitute a pointer over the exposed wrapper allocation.
    let wrapper_addr = (handler as *mut u8).addr() - off;
    std::ptr::with_exposed_provenance_mut::<TerminalWrapper>(wrapper_addr)
}

// /// Opaque context behind ClipboardWrite.ctx for the reply trampoline.
// const ClipboardWriteCtx = struct {
//     write: clipboard.Write,
// };
//
// PORT NOTE (R9 callback-ctx): the reference nests this in `Effects`; a Rust
// `impl` cannot hold a struct declaration, so it sits at module scope.
// Stack-lifetime: `ClipboardWrite.ctx` points at this inside the trampoline
// frame and the C callback must call `reply` before returning (documented
// contract). Only ever read back as `*const`.
//
// PORT NOTE (soundness, parity S166-F1): `handler` is not in the reference.
// `clipboard::Write::reply` takes the handler the effect callback was given
// (see its note), and the C reply arrives through `ClipboardWrite.reply`
// with only this ctx, so the trampoline stores a pointer derived from its own
// `&mut Handler` argument here. The trampoline does not touch its `handler`
// again until the C callback has returned, so the reply's use of this
// pointer is a use of a child of that argument's tag.
struct ClipboardWriteCtx<'a> {
    write: clipboard::Write<'a>,
    handler: *mut Handler<'static>,
}

// /// Opaque context behind ClipboardRead.ctx for the reply trampoline.
// const ClipboardReadCtx = struct {
//     read: clipboard.Read,
//     wrapper: *TerminalWrapper,
// };
//
// PORT NOTE (R9 callback-ctx): stack-lifetime, like `ClipboardWriteCtx` above
// (and module-scoped for the same reason). `wrapper` is carried so the reply
// trampoline can allocate through `ctx.wrapper.terminal.gpa()` (rustty reads
// the same allocator through `handler.terminal`, so the field is a parity
// carry; parity tail S7-4); it is a raw pointer,
// never re-borrowed as `&mut` (the wrapper's stream-handler call chain is
// live on the same wrapper — the interleaved-alias shape of the
// `shaper-hook-sb-clean-interleave` memory).
//
// PORT NOTE (soundness, parity S166-F1): `handler` as in `ClipboardWriteCtx`.
struct ClipboardReadCtx<'a> {
    read: clipboard::Read<'a>,
    #[allow(dead_code)]
    wrapper: *mut TerminalWrapper,
    handler: *mut Handler<'static>,
}

impl Effects {
    // fn writePtyTrampoline(handler: *Handler, data: [:0]const u8) void {
    //     const stream_ptr: *Stream = @fieldParentPtr("handler", handler);
    //     const wrapper: *TerminalWrapper = @fieldParentPtr("stream", stream_ptr);
    //     const func = wrapper.effects.write_pty orelse return;
    //     func(@ptrCast(wrapper), wrapper.effects.userdata, data.ptr, data.len);
    // }
    pub(crate) fn write_pty_trampoline(handler: &mut Handler<'static>, data: &[u8]) {
        let wrapper = wrapper_from_handler(handler);
        // SAFETY: `wrapper` is the live owning `TerminalWrapper`; we read its
        // `effects` sibling (not the `&mut Handler` we were handed) via raw ptr.
        let func = match unsafe { (*wrapper).effects.write_pty } {
            Some(f) => f,
            None => return,
        };
        let userdata = unsafe { (*wrapper).effects.userdata };
        func(wrapper, userdata, data.as_ptr(), data.len());
    }

    // fn bellTrampoline(handler: *Handler) void {
    //     ... const func = wrapper.effects.bell orelse return;
    //     func(@ptrCast(wrapper), wrapper.effects.userdata);
    // }
    pub(crate) fn bell_trampoline(handler: &mut Handler<'static>) {
        let wrapper = wrapper_from_handler(handler);
        let func = match unsafe { (*wrapper).effects.bell } {
            Some(f) => f,
            None => return,
        };
        let userdata = unsafe { (*wrapper).effects.userdata };
        func(wrapper, userdata);
    }

    // fn colorSchemeTrampoline(handler: *Handler) ?device_status.ColorScheme {
    //     ... const func = wrapper.effects.color_scheme orelse return null;
    //     var scheme: device_status.ColorScheme = undefined;
    //     if (func(@ptrCast(wrapper), wrapper.effects.userdata, &scheme)) return scheme;
    //     return null;
    // }
    pub(crate) fn color_scheme_trampoline(
        handler: &mut Handler<'static>,
    ) -> Option<device_status::ColorScheme> {
        let wrapper = wrapper_from_handler(handler);
        let func = unsafe { (*wrapper).effects.color_scheme }?;
        let userdata = unsafe { (*wrapper).effects.userdata };
        // var scheme: device_status.ColorScheme = undefined;
        let mut scheme = core::mem::MaybeUninit::<device_status::ColorScheme>::uninit();
        if func(wrapper, userdata, scheme.as_mut_ptr()) {
            return Some(unsafe { scheme.assume_init() });
        }
        None
    }

    // fn deviceAttributesTrampoline(handler: *Handler) device_attributes.Attributes {
    //     ... const func = wrapper.effects.device_attributes_cb orelse return .{};
    //     var c_attrs: CDeviceAttributes = undefined;
    //     if (!func(@ptrCast(wrapper), wrapper.effects.userdata, &c_attrs)) return .{};
    //     const n: usize = @min(c_attrs.primary.num_features, 64);
    //     for (0..n) |i| wrapper.effects.da_features_buf[i] = @enumFromInt(c_attrs.primary.features[i]);
    //     return .{ .primary = ..., .secondary = ..., .tertiary = ... };
    // }
    pub(crate) fn device_attributes_trampoline(
        handler: &mut Handler<'static>,
    ) -> device_attributes::Attributes {
        let wrapper = wrapper_from_handler(handler);
        let func = match unsafe { (*wrapper).effects.device_attributes_cb } {
            Some(f) => f,
            None => return device_attributes::Attributes::default(),
        };
        let userdata = unsafe { (*wrapper).effects.userdata };

        // Get our attributes from the callback.
        // var c_attrs: CDeviceAttributes = undefined;
        let mut c_attrs = core::mem::MaybeUninit::<CDeviceAttributes>::uninit();
        if !func(wrapper, userdata, c_attrs.as_mut_ptr()) {
            return device_attributes::Attributes::default();
        }
        let c_attrs = unsafe { c_attrs.assume_init() };

        // Note below we use a lot of enumFromInt but its always safe
        // because all our types are non-exhaustive enums.

        // const n: usize = @min(c_attrs.primary.num_features, 64);
        let n: usize = core::cmp::min(c_attrs.primary.num_features, 64);
        // for (0..n) |i| wrapper.effects.da_features_buf[i] = @enumFromInt(c_attrs.primary.features[i]);
        for i in 0..n {
            // SAFETY: `da_features_buf` is a sibling field of the recovered
            // wrapper; mutate it via the raw ptr (never via the live
            // `&mut Handler`).
            unsafe {
                (*wrapper).effects.da_features_buf[i] =
                    feature_from_int(c_attrs.primary.features[i]);
            }
        }

        // PORT NOTE: the reference returns `features = da_features_buf[0..n]`, a
        // slice into the wrapper's long-lived scratch buffer. The Rust
        // `Primary.features` field is `&'static [Feature]`; the buffer lives as
        // long as the heap `TerminalWrapper`, and `reportDeviceAttributes` reads
        // the slice immediately (before any re-entrant call), so promoting the
        // borrow to `'static` here is sound (matches the reference's reliance on
        // the buffer outliving the trampoline return).
        let features: &'static [device_attributes::Feature] =
            unsafe { core::slice::from_raw_parts((*wrapper).effects.da_features_buf.as_ptr(), n) };

        device_attributes::Attributes {
            primary: device_attributes::Primary {
                conformance_level: conformance_level_from_int(c_attrs.primary.conformance_level),
                features,
            },
            secondary: device_attributes::Secondary {
                device_type: device_type_from_int(c_attrs.secondary.device_type),
                firmware_version: c_attrs.secondary.firmware_version,
                rom_cartridge: c_attrs.secondary.rom_cartridge,
            },
            tertiary: device_attributes::Tertiary {
                unit_id: c_attrs.tertiary.unit_id,
            },
        }
    }

    // fn enquiryTrampoline(handler: *Handler) []const u8 {
    //     ... const func = wrapper.effects.enquiry orelse return "";
    //     const result = func(@ptrCast(wrapper), wrapper.effects.userdata);
    //     if (result.len == 0) return "";
    //     return result.ptr[0..result.len];
    // }
    pub(crate) fn enquiry_trampoline(handler: &mut Handler<'static>) -> &'static [u8] {
        let wrapper = wrapper_from_handler(handler);
        let func = match unsafe { (*wrapper).effects.enquiry } {
            Some(f) => f,
            None => return b"",
        };
        let userdata = unsafe { (*wrapper).effects.userdata };
        let result = func(wrapper, userdata);
        if result.len == 0 {
            return b"";
        }
        // return result.ptr[0..result.len];
        // SAFETY: the callback contract is that `result.ptr[0..result.len]` is
        // valid for the lifetime of the call; the response is read immediately
        // by `reportEnquiry` before any re-entrant call.
        unsafe { core::slice::from_raw_parts(result.ptr, result.len) }
    }

    // fn xtversionTrampoline(handler: *Handler) []const u8 { ... identical shape ... }
    pub(crate) fn xtversion_trampoline(handler: &mut Handler<'static>) -> &'static [u8] {
        let wrapper = wrapper_from_handler(handler);
        let func = match unsafe { (*wrapper).effects.xtversion } {
            Some(f) => f,
            None => return b"",
        };
        let userdata = unsafe { (*wrapper).effects.userdata };
        let result = func(wrapper, userdata);
        if result.len == 0 {
            return b"";
        }
        unsafe { core::slice::from_raw_parts(result.ptr, result.len) }
    }

    // fn titleChangedTrampoline(handler: *Handler) void { ... }
    pub(crate) fn title_changed_trampoline(handler: &mut Handler<'static>) {
        let wrapper = wrapper_from_handler(handler);
        let func = match unsafe { (*wrapper).effects.title_changed } {
            Some(f) => f,
            None => return,
        };
        let userdata = unsafe { (*wrapper).effects.userdata };
        func(wrapper, userdata);
    }

    // fn pwdChangedTrampoline(handler: *Handler) void { ... }
    pub(crate) fn pwd_changed_trampoline(handler: &mut Handler<'static>) {
        let wrapper = wrapper_from_handler(handler);
        let func = match unsafe { (*wrapper).effects.pwd_changed } {
            Some(f) => f,
            None => return,
        };
        let userdata = unsafe { (*wrapper).effects.userdata };
        func(wrapper, userdata);
    }

    // fn sizeTrampoline(handler: *Handler) ?size_report.Size {
    //     ... const func = wrapper.effects.size_cb orelse return null;
    //     var s: size_report.Size = undefined;
    //     if (func(@ptrCast(wrapper), wrapper.effects.userdata, &s)) return s;
    //     return null;
    // }
    pub(crate) fn size_trampoline(handler: &mut Handler<'static>) -> Option<size_report::Size> {
        let wrapper = wrapper_from_handler(handler);
        let func = unsafe { (*wrapper).effects.size_cb }?;
        let userdata = unsafe { (*wrapper).effects.userdata };
        let mut s = core::mem::MaybeUninit::<size_report::Size>::uninit();
        if func(wrapper, userdata, s.as_mut_ptr()) {
            return Some(unsafe { s.assume_init() });
        }
        None
    }

    // fn clipboardWriteTrampoline(handler: *Handler, write: clipboard.Write) void {
    //     const wrapper = TerminalWrapper.fromHandler(handler);
    //     const func = wrapper.effects.clipboard_write orelse
    //         return write.reply(.unsupported);
    //     ...stack-first (<=4) content conversion, gpa fallback...
    //     const ctx: ClipboardWriteCtx = .{ .write = write };
    //     const request: ClipboardWrite = .{ ..., .ctx = &ctx, .reply = &clipboardWriteReplyTrampoline };
    //     func(@ptrCast(wrapper), wrapper.effects.userdata, &request);
    // }
    pub(crate) fn clipboard_write_trampoline(
        handler: &mut Handler<'static>,
        write: clipboard::Write<'_>,
    ) {
        let wrapper = wrapper_from_handler(handler);
        let func = match unsafe { (*wrapper).effects.clipboard_write } {
            Some(f) => f,
            None => return write.reply(handler, clipboard::WriteResult::Unsupported),
        };
        let userdata = unsafe { (*wrapper).effects.userdata };

        // Most protocols currently produce one representation, so keep that
        // path allocation-free while supporting arbitrary multi-MIME writes.
        // (parity tail S7-4: past 4 the array is allocated through the
        // terminal's `gpa()`, fallibly: `catch return write.reply(.io_error)`.)
        // .mime = .init(content.mime), .data = .init(content.data)
        let to_c = |content: &clipboard::Content<'_>| ClipboardContent {
            mime: LibString::init(content.mime),
            data: LibString::init(content.data),
        };

        let mut stack_contents: [ClipboardContent; 4] = [ClipboardContent {
            mime: LibString {
                ptr: std::ptr::null(),
                len: 0,
            },
            data: LibString {
                ptr: std::ptr::null(),
                len: 0,
            },
        }; 4];
        let mut heap_contents: rustty_vt::alloc::AVec<ClipboardContent>;
        let contents: &[ClipboardContent] = if write.contents.len() <= stack_contents.len() {
            for (dst, content) in stack_contents.iter_mut().zip(write.contents) {
                *dst = to_c(content);
            }
            &stack_contents[..write.contents.len()]
        } else {
            // wrapper.terminal.gpa().alloc(ClipboardContent, write.contents.len)
            //     catch return write.reply(.io_error);
            // defer wrapper.terminal.gpa().free(contents);
            // SAFETY: `handler.terminal` is the live terminal; raw reads.
            let gpa = unsafe { (*(*handler.terminal).screens.active).alloc };
            heap_contents = rustty_vt::alloc::AVec::new_in(gpa);
            if heap_contents
                .try_reserve_exact(write.contents.len())
                .is_err()
            {
                return write.reply(handler, clipboard::WriteResult::IoError);
            }
            for content in write.contents {
                heap_contents.push(to_c(content));
            }
            &heap_contents
        };

        // const ctx: ClipboardWriteCtx = .{ .write = write };
        // (Derived last: nothing touches `handler` again until `func` returns.)
        let ctx = ClipboardWriteCtx {
            write,
            handler: core::ptr::from_mut(handler),
        };
        let request = ClipboardWrite {
            size: std::mem::size_of::<ClipboardWrite>(),
            location: write.location,
            contents: if contents.is_empty() {
                std::ptr::null()
            } else {
                contents.as_ptr()
            },
            contents_len: contents.len(),
            name: LibString::init(write.name),
            granted: write.granted,
            can_remember: write.can_remember,
            ctx: &ctx as *const ClipboardWriteCtx<'_> as *const c_void,
            reply: Self::clipboard_write_reply_trampoline,
        };
        func(wrapper, userdata, &request);
    }

    // fn clipboardWriteReplyTrampoline(
    //     request: *const ClipboardWrite,
    //     reply: *const ClipboardWriteReply,
    // ) callconv(lib.calling_conv) void { ... }
    //
    // # Safety (extern "C", DG5 abort-on-unwind): `request` is the borrowed
    // request whose `ctx` points at the live stack `ClipboardWriteCtx` (the C
    // callback contract: reply before returning); `reply` is borrowed for this
    // call only.
    extern "C" fn clipboard_write_reply_trampoline(
        request: *const ClipboardWrite,
        reply: *const ClipboardWriteReply,
    ) {
        use clipboard::WriteStatus;
        // const ctx: *const ClipboardWriteCtx = @ptrCast(@alignCast(request.ctx));
        // SAFETY: `request.ctx` is the trampoline frame's `ClipboardWriteCtx`,
        // still live because the callback has not returned (contract above).
        // Read-only access through `*const` — never a `&mut`.
        let ctx: *const ClipboardWriteCtx<'_> =
            unsafe { (*request).ctx as *const ClipboardWriteCtx<'_> };
        // const write = ctx.write;
        let write = unsafe { (*ctx).write };
        // SAFETY: `ctx.handler` was derived from the trampoline's live
        // `&mut Handler`, which is not used while the C callback runs (see
        // `ClipboardWriteCtx`); this is the only reference made from it.
        let handler = unsafe { &mut *(*ctx).handler };
        // switch (reply.result) { ... }
        //
        // (`.unsupported, _ =>` — an out-of-range C value is well-defined by
        // the open status type and maps to unsupported, like the reference's
        // catch-all arm.)
        match unsafe { (*reply).result } {
            WriteStatus::Success => write.reply(
                handler,
                clipboard::WriteResult::Success(clipboard::WriteResultSuccess {
                    remember: unsafe { (*reply).remember },
                }),
            ),
            WriteStatus::Denied => write.reply(handler, clipboard::WriteResult::Denied),
            WriteStatus::Busy => write.reply(handler, clipboard::WriteResult::Busy),
            WriteStatus::InvalidData => write.reply(handler, clipboard::WriteResult::InvalidData),
            WriteStatus::IoError => write.reply(handler, clipboard::WriteResult::IoError),
            _ => write.reply(handler, clipboard::WriteResult::Unsupported),
        }
    }

    // fn clipboardReadTrampoline(handler: *Handler, read: clipboard.Read) void {
    //     const wrapper = TerminalWrapper.fromHandler(handler);
    //     const func = wrapper.effects.clipboard_read orelse return;
    //     ...stackFallback(128) mime conversion...
    //     const ctx: ClipboardReadCtx = .{ .read = read, .wrapper = wrapper };
    //     const request: ClipboardRead = .{ ..., .ctx = &ctx, .reply = &clipboardReadReplyTrampoline };
    //     func(@ptrCast(wrapper), wrapper.effects.userdata, &request);
    // }
    pub(crate) fn clipboard_read_trampoline(
        handler: &mut Handler<'static>,
        read: clipboard::Read<'_>,
    ) {
        let wrapper = wrapper_from_handler(handler);
        let Some(func) = (unsafe { (*wrapper).effects.clipboard_read }) else {
            return;
        };
        let userdata = unsafe { (*wrapper).effects.userdata };

        // Requests carry a handful of MIME types, so keep the common case
        // allocation-free. On OOM the request goes unanswered and the
        // handler replies with an empty clipboard.
        // var sfa = std.heap.stackFallback(128, wrapper.terminal.gpa());
        // const alloc = sfa.get();
        // const mimes = alloc.alloc(lib.String, read.mimes.len) catch {
        //     log.warn("out of memory converting clipboard read request", .{});
        //     return;
        // };
        // defer alloc.free(mimes);
        // (parity tail S7-4: `alloc::StackFallback` over the terminal's `gpa()`.)
        // SAFETY: `handler.terminal` is the live terminal; raw reads.
        let gpa = unsafe { (*(*handler.terminal).screens.active).alloc };
        let sfa = rustty_vt::alloc::StackFallback::<128>::new(gpa);
        let mut mimes: allocator_api2::vec::Vec<LibString, &rustty_vt::alloc::StackFallback<128>> =
            allocator_api2::vec::Vec::new_in(&sfa);
        if mimes.try_reserve_exact(read.mimes.len()).is_err() {
            log::warn!(target: LOG_SCOPE, "out of memory converting clipboard read request");
            return;
        }
        for m in read.mimes {
            mimes.push(LibString::init(m));
        }

        // const ctx: ClipboardReadCtx = .{ .read = read, .wrapper = wrapper };
        // (Derived last: nothing touches `handler` again until `func` returns.)
        let ctx = ClipboardReadCtx {
            read,
            wrapper,
            handler: core::ptr::from_mut(handler),
        };
        let request = ClipboardRead {
            size: std::mem::size_of::<ClipboardRead>(),
            location: read.location,
            mimes: if mimes.is_empty() {
                std::ptr::null()
            } else {
                mimes.as_ptr()
            },
            mimes_len: mimes.len(),
            list: read.list,
            name: LibString::init(read.name),
            granted: read.granted,
            can_remember: read.can_remember,
            ctx: &ctx as *const ClipboardReadCtx<'_> as *const c_void,
            reply: Self::clipboard_read_reply_trampoline,
        };
        func(wrapper, userdata, &request);
    }

    // fn clipboardReadReplyTrampoline(
    //     request: *const ClipboardRead,
    //     reply: *const ClipboardReadReply,
    // ) callconv(lib.calling_conv) void { ... }
    //
    // # Safety (extern "C", DG5): `request.ctx` is the live stack
    // `ClipboardReadCtx`; `reply` and everything it points to are borrowed for
    // this call only.
    extern "C" fn clipboard_read_reply_trampoline(
        request: *const ClipboardRead,
        reply: *const ClipboardReadReply,
    ) {
        use clipboard::ReadStatus;
        // const ctx: *const ClipboardReadCtx = @ptrCast(@alignCast(request.ctx));
        // SAFETY: see `clipboard_write_reply_trampoline` — the ctx is the
        // trampoline frame's stack value, live until the callback returns.
        let ctx: *const ClipboardReadCtx<'_> =
            unsafe { (*request).ctx as *const ClipboardReadCtx<'_> };
        // const read = ctx.read;
        let read = unsafe { (*ctx).read };
        // SAFETY: as in `clipboard_write_reply_trampoline`.
        let handler = unsafe { &mut *(*ctx).handler };
        // switch (reply.result) {
        //     .success => {},
        //     .denied => return read.reply(.denied), ...
        //     .unsupported, _ => return read.reply(.unsupported),
        // }
        match unsafe { (*reply).result } {
            ReadStatus::Success => {}
            ReadStatus::Denied => return read.reply(handler, clipboard::ReadResult::Denied),
            ReadStatus::Busy => return read.reply(handler, clipboard::ReadResult::Busy),
            ReadStatus::IoError => return read.reply(handler, clipboard::ReadResult::IoError),
            _ => return read.reply(handler, clipboard::ReadResult::Unsupported),
        }

        // const c_contents: []const ClipboardContent = if (reply.contents) |ptr| ptr[0..reply.contents_len] else &.{};
        // SAFETY: the reply arrays are borrowed C memory, valid for this call
        // (caller contract).
        let c_contents: &[ClipboardContent] = if unsafe { (*reply).contents }.is_null() {
            &[]
        } else {
            unsafe { std::slice::from_raw_parts((*reply).contents, (*reply).contents_len) }
        };
        // const c_available: []const lib.String = if (reply.available) |ptr| ptr[0..reply.available_len] else &.{};
        let c_available: &[LibString] = if unsafe { (*reply).available }.is_null() {
            &[]
        } else {
            unsafe { std::slice::from_raw_parts((*reply).available, (*reply).available_len) }
        };

        // Most replies carry one representation, so keep that path
        // allocation-free while supporting arbitrary multi-MIME replies.
        // On OOM we don't reply and the handler answers with an empty
        // clipboard.
        // var sfa = std.heap.stackFallback(256, ctx.wrapper.terminal.gpa());
        // const alloc = sfa.get();
        // const contents = alloc.alloc(clipboard.Content, c_contents.len) catch {
        //     log.warn("out of memory converting clipboard read reply", .{});
        //     return;
        // };
        // defer alloc.free(contents);
        // ...
        // const available = alloc.alloc([]const u8, c_available.len) catch { ...same... };
        // defer alloc.free(available);
        // (parity tail S7-4: `alloc::StackFallback` over the terminal's
        // `gpa()`, read through the handler's raw terminal pointer: no wrapper
        // deref and no `&mut` alias of the live call chain.)
        // SAFETY: `handler.terminal` is the live terminal; raw reads.
        let gpa = unsafe { (*(*handler.terminal).screens.active).alloc };
        let sfa = rustty_vt::alloc::StackFallback::<256>::new(gpa);
        let mut contents: allocator_api2::vec::Vec<
            clipboard::Content<'_>,
            &rustty_vt::alloc::StackFallback<256>,
        > = allocator_api2::vec::Vec::new_in(&sfa);
        if contents.try_reserve_exact(c_contents.len()).is_err() {
            log::warn!(target: LOG_SCOPE, "out of memory converting clipboard read reply");
            return;
        }
        // SAFETY: each `LibString`/`ClipboardContent` names borrowed C memory
        // valid for this call.
        for c_content in c_contents {
            contents.push(clipboard::Content {
                mime: unsafe { std::slice::from_raw_parts(c_content.mime.ptr, c_content.mime.len) },
                data: unsafe { std::slice::from_raw_parts(c_content.data.ptr, c_content.data.len) },
            });
        }
        let mut available: allocator_api2::vec::Vec<&[u8], &rustty_vt::alloc::StackFallback<256>> =
            allocator_api2::vec::Vec::new_in(&sfa);
        if available.try_reserve_exact(c_available.len()).is_err() {
            log::warn!(target: LOG_SCOPE, "out of memory converting clipboard read reply");
            return;
        }
        for c_mime in c_available {
            available.push(unsafe { std::slice::from_raw_parts(c_mime.ptr, c_mime.len) });
        }

        // read.reply(.{ .success = .{ .contents = contents, .available = available, .remember = reply.remember } });
        read.reply(
            handler,
            clipboard::ReadResult::Success(clipboard::ReadResultSuccess {
                contents: &contents,
                available: &available,
                remember: unsafe { (*reply).remember },
            }),
        );
    }

    // fn desktopNotificationTrampoline(handler: *Handler, notification: Action.ShowDesktopNotification) void {
    //     const stream_ptr: *Stream = @fieldParentPtr("handler", handler);
    //     const wrapper: *TerminalWrapper = @fieldParentPtr("stream", stream_ptr);
    //     const func = wrapper.effects.desktop_notification orelse return;
    //     const request: DesktopNotification = .{ ... };
    //     func(@ptrCast(wrapper), wrapper.effects.userdata, &request);
    // }
    pub(crate) fn desktop_notification_trampoline(
        handler: &mut Handler<'static>,
        notification: rustty_vt::stream::ShowDesktopNotification<'_>,
    ) {
        let wrapper = wrapper_from_handler(handler);
        let Some(func) = (unsafe { (*wrapper).effects.desktop_notification }) else {
            return;
        };
        let userdata = unsafe { (*wrapper).effects.userdata };

        // The `size` field is the sized-struct contract: the embedder may only
        // read fields present within it. It must be `size_of` the struct rustty
        // actually built, never a literal.
        let request = DesktopNotification {
            size: std::mem::size_of::<DesktopNotification>(),
            // .title = .init(notification.title), .body = .init(notification.body)
            title: LibString::init(notification.title),
            body: LibString::init(notification.body),
        };
        func(wrapper, userdata, &request);
    }

    // fn progressReportTrampoline(handler: *Handler, report: osc.Command.ProgressReport) void {
    //     ... same @fieldParentPtr recovery ...
    // }
    pub(crate) fn progress_report_trampoline(
        handler: &mut Handler<'static>,
        report: rustty_vt::osc::ProgressReport,
    ) {
        let wrapper = wrapper_from_handler(handler);
        let Some(func) = (unsafe { (*wrapper).effects.progress_report }) else {
            return;
        };
        let userdata = unsafe { (*wrapper).effects.userdata };

        // PORT NOTE: the `-1 == omitted` encoding and the 0..=100 clamp are
        // `ProgressReport.cval()` in rustty-vt (osc/mod.rs), already ported;
        // reuse it rather than re-deriving the encoding here.
        let cval = report.cval();
        let request = ProgressReport {
            size: std::mem::size_of::<ProgressReport>(),
            state: ProgressState::from_vt(report.state),
            progress: cval.progress,
        };
        func(wrapper, userdata, &request);
    }

    // fn unknownSequenceTrampoline(
    //     handler: *Handler,
    //     sequence: Handler.UnknownSequence,
    // ) void {
    //     const wrapper = TerminalWrapper.fromHandler(handler);
    //     const func = wrapper.effects.unknown_sequence orelse return;
    //     const value = UnknownSequence.cval(switch (sequence) {
    //         .apc => |apc_value| .{
    //             .apc = .{
    //                 .truncated = apc_value.truncated,
    //                 .content = .{
    //                     .ptr = apc_value.content.ptr,
    //                     .len = apc_value.content.len,
    //                 },
    //             },
    //         },
    //     });
    //     func(@ptrCast(wrapper), wrapper.effects.userdata, &value);
    // }
    pub(crate) fn unknown_sequence_trampoline(
        handler: &mut Handler<'static>,
        sequence: rustty_vt::stream_terminal::UnknownSequence<'_>,
    ) {
        let wrapper = wrapper_from_handler(handler);
        let Some(func) = (unsafe { (*wrapper).effects.unknown_sequence }) else {
            return;
        };
        let userdata = unsafe { (*wrapper).effects.userdata };
        // PORT NOTE: `UnknownSequence.cval(...)` — rustty hand-authors the
        // TaggedUnion C projection, so `cval` is this literal construction
        // (tag + payload zero-extended into the padded union).
        let value = match sequence {
            rustty_vt::stream_terminal::UnknownSequence::Apc(apc_value) => UnknownSequence {
                tag: UnknownSequenceTag::Apc,
                value: UnknownSequenceValue {
                    apc: UnknownStringSequence {
                        truncated: apc_value.truncated,
                        // .content = .init(apc_value.content)
                        content: LibString::init(apc_value.content),
                    },
                },
            },
        };
        func(wrapper, userdata, &value);
    }
}

// PORT NOTE: the reference uses `@enumFromInt` on the DA enums (always safe
// because they are non-exhaustive). rustty's ported enums carry an explicit
// catch-all variant (`Other(u16)`), so these helpers reproduce `@enumFromInt`:
// known codes map to their named variant, every other value rides the catch-all.
// They are NOT a rustty-vt change (R3) — purely the vt-c-local int->enum bridge.
fn feature_from_int(v: u16) -> device_attributes::Feature {
    use device_attributes::Feature;
    match v {
        1 => Feature::Columns132,
        2 => Feature::Printer,
        3 => Feature::Regis,
        4 => Feature::Sixel,
        6 => Feature::SelectiveErase,
        8 => Feature::UserDefinedKeys,
        9 => Feature::NationalReplacement,
        15 => Feature::TechnicalCharacters,
        16 => Feature::Locator,
        17 => Feature::TerminalState,
        18 => Feature::Windowing,
        21 => Feature::HorizontalScrolling,
        22 => Feature::AnsiColor,
        28 => Feature::RectangularEditing,
        29 => Feature::AnsiTextLocator,
        52 => Feature::Clipboard,
        other => Feature::Other(other),
    }
}

fn conformance_level_from_int(v: u16) -> device_attributes::ConformanceLevel {
    use device_attributes::ConformanceLevel;
    match v {
        1 => ConformanceLevel::VT100,
        4 => ConformanceLevel::VT132,
        6 => ConformanceLevel::VT102,
        7 => ConformanceLevel::VT131,
        12 => ConformanceLevel::VT125,
        62 => ConformanceLevel::Level2,
        63 => ConformanceLevel::Level3,
        64 => ConformanceLevel::Level4,
        65 => ConformanceLevel::Level5,
        other => ConformanceLevel::Other(other),
    }
}

fn device_type_from_int(v: u16) -> device_attributes::DeviceType {
    use device_attributes::DeviceType;
    match v {
        0 => DeviceType::VT100,
        1 => DeviceType::VT220,
        2 => DeviceType::VT240,
        18 => DeviceType::VT330,
        19 => DeviceType::VT340,
        24 => DeviceType::VT320,
        32 => DeviceType::VT382,
        41 => DeviceType::VT420,
        61 => DeviceType::VT510,
        64 => DeviceType::VT520,
        65 => DeviceType::VT525,
        other => DeviceType::Other(other),
    }
}

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
        unsafe { (*terminal_).stream.next_slice(continuation) };
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
    match unsafe { (*terminal_).stream.write_continuation(&mut exported) } {
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
/// This function consumes `io` on every path. The decoded terminal is
/// transferred only after its final heap address has been allocated; its
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

// pub fn vt_write(terminal_: Terminal, ptr: [*]const u8, len: usize) ... {
//     const wrapper = terminal_ orelse return;
//     wrapper.stream.nextSlice(ptr[0..len]);
// }
/// # Safety
/// `terminal_`, if non-null, must be a live terminal handle; `ptr` must point to
/// `len` readable bytes.
pub unsafe fn vt_write(terminal_: Terminal, ptr: *const u8, len: usize) {
    // const wrapper = terminal_ orelse return;
    if terminal_.is_null() {
        return;
    }
    // wrapper.stream.nextSlice(ptr[0..len]);
    // SAFETY: `terminal_` is a live wrapper; `ptr[0..len]` is readable (caller
    // contract). The stream's handler aliases this wrapper's terminal.
    let input = unsafe { core::slice::from_raw_parts(ptr, len) };
    unsafe { (*terminal_).stream.next_slice(input) };
}

// pub fn vt_write_until_ground(
//     terminal_: Terminal,
//     ptr_: ?[*]const u8,
//     len: usize,
//     out_consumed_: ?*usize,
// ) callconv(lib.calling_conv) Result {
/// # Safety
/// `terminal_`, if non-null, must be a live terminal handle; `ptr_`, if
/// non-null, must point to `len` readable bytes; `out_consumed_`, if non-null,
/// must be writable.
pub unsafe fn vt_write_until_ground(
    terminal_: Terminal,
    ptr_: *const u8,
    len: usize,
    out_consumed_: *mut usize,
) -> Result {
    // const out_consumed = out_consumed_ orelse return .invalid_value;
    // out_consumed.* = 0;
    if out_consumed_.is_null() {
        return Result::InvalidValue;
    }
    // SAFETY: `out_consumed_` is non-null and writable (caller contract).
    unsafe { *out_consumed_ = 0 };

    // const wrapper = terminal_ orelse return .invalid_value;
    if terminal_.is_null() {
        return Result::InvalidValue;
    }
    // const input: []const u8 = if (ptr_) |ptr| ptr[0..len] else if (len == 0) "" else
    //     return .invalid_value;
    let input: &[u8] = if !ptr_.is_null() {
        // SAFETY: `ptr_[0..len]` is readable (caller contract).
        unsafe { core::slice::from_raw_parts(ptr_, len) }
    } else if len == 0 {
        b""
    } else {
        return Result::InvalidValue;
    };

    // if (wrapper.stream.nextSliceUntilGround(input)) |consumed| { ... }
    // SAFETY: `terminal_` is a live wrapper; the stream's handler aliases this
    // wrapper's terminal.
    if let Some(consumed) = unsafe { (*terminal_).stream.next_slice_until_ground(input) } {
        unsafe { *out_consumed_ = consumed };
        return Result::Success;
    }

    // out_consumed.* = len;
    unsafe { *out_consumed_ = len };
    Result::NoValue
}

// pub const ContinuationWriteError = error{
//     InvalidValue,
//     WriteFailed,
//     ContinuationDisabled,
//     ContinuationUnavailable,
// };
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContinuationWriteError {
    InvalidValue,
    WriteFailed,
    ContinuationDisabled,
    ContinuationUnavailable,
}

/// Write the exact replay-safe continuation for a C terminal to a Zig writer.
/// Snapshot encoding uses this helper so it can preflight continuation state
/// without converting its allocator or writer through the C ABI.
///
/// # Safety
/// `terminal_`, if non-null, must be a live terminal handle.
pub unsafe fn continuation_write_io(
    terminal_: Terminal,
    writer: &mut dyn rustty_vt::stream_continuation::ByteWriter,
) -> std::result::Result<(), ContinuationWriteError> {
    // Keep handle validation here so the three public output forms and the
    // snapshot encoder all use exactly the same continuation preflight.
    // const wrapper = terminal_ orelse return error.InvalidValue;
    if terminal_.is_null() {
        return Err(ContinuationWriteError::InvalidValue);
    }

    // Stream owns the tracker and distinguishes disabled tracking from a
    // tracker that lost bytes after allocation failure or limit overflow.
    // try wrapper.stream.writeContinuation(writer);
    // SAFETY: `terminal_` is a live wrapper (caller contract).
    unsafe { (*terminal_).stream.write_continuation(writer) }.map_err(|err| match err {
        rustty_vt::stream::WriteContinuationError::WriteFailed(_) => {
            ContinuationWriteError::WriteFailed
        }
        rustty_vt::stream::WriteContinuationError::ContinuationDisabled => {
            ContinuationWriteError::ContinuationDisabled
        }
        rustty_vt::stream::WriteContinuationError::ContinuationUnavailable => {
            ContinuationWriteError::ContinuationUnavailable
        }
    })
}

// pub const ContinuationAllocError = error{
//     InvalidValue,
//     OutOfMemory,
//     ContinuationDisabled,
//     ContinuationUnavailable,
// };
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContinuationAllocError {
    InvalidValue,
    OutOfMemory,
    ContinuationDisabled,
    ContinuationUnavailable,
}

/// Return an allocator-owned copy of the terminal's replay-safe continuation.
/// The caller owns the returned bytes.
///
/// If `allow_untracked_ground` is true, disabled tracking is accepted only
/// when the stream is provably at ground and an owned empty slice is returned.
// pub fn continuationAllocIo(
//     terminal_: Terminal,
//     alloc: std.mem.Allocator,
//     allow_untracked_ground: bool,
// ) ContinuationAllocError![]u8 {
//
// PORT NOTE (parity tail S7-8): the reference's `alloc`-owned slice is an
// `AVec<u8>` bound to `alloc`, built by an allocating writer over `alloc` and
// handed out by `toOwnedSlice` (exactly sized); dropping it is `alloc.free`.
///
/// # Safety
/// `terminal_`, if non-null, must be a live terminal handle.
pub unsafe fn continuation_alloc_io(
    terminal_: Terminal,
    alloc: rustty_vt::alloc::VtAllocator,
    allow_untracked_ground: bool,
) -> std::result::Result<rustty_vt::alloc::AVec<u8>, ContinuationAllocError> {
    // const wrapper = terminal_ orelse return error.InvalidValue;
    if terminal_.is_null() {
        return Err(ContinuationAllocError::InvalidValue);
    }
    // if (allow_untracked_ground and wrapper.stream.continuation == null and
    //     wrapper.stream.ground())
    // { return alloc.dupe(u8, "") catch error.OutOfMemory; }
    // SAFETY: `terminal_` is a live wrapper (caller contract); shared reads.
    if allow_untracked_ground
        && unsafe { (*terminal_).stream.continuation.is_none() }
        && unsafe { (*terminal_).stream.ground() }
    {
        // `alloc.dupe(u8, "")`: an empty slice, no allocator call.
        return Ok(rustty_vt::alloc::AVec::new_in(alloc));
    }

    // The allocating writer gives snapshot encoding and the public allocation
    // API one common way to obtain an exact owned continuation.
    // var aw: std.Io.Writer.Allocating = .init(alloc);
    // defer aw.deinit();
    let mut aw = rustty_vt::alloc::AllocatingWriter::init(alloc);

    // A write failure from an Allocating writer can only be allocation failure;
    // no external callback participates in this path.
    // SAFETY: `terminal_` is a live wrapper (caller contract).
    unsafe { continuation_write_io(terminal_, &mut aw) }.map_err(|err| match err {
        ContinuationWriteError::InvalidValue => ContinuationAllocError::InvalidValue,
        ContinuationWriteError::WriteFailed => ContinuationAllocError::OutOfMemory,
        ContinuationWriteError::ContinuationDisabled => {
            ContinuationAllocError::ContinuationDisabled
        }
        ContinuationWriteError::ContinuationUnavailable => {
            ContinuationAllocError::ContinuationUnavailable
        }
    })?;

    // Transfer the buffer out before the deferred writer cleanup runs.
    // return aw.toOwnedSlice() catch error.OutOfMemory;
    aw.to_owned_slice()
        .map_err(|_| ContinuationAllocError::OutOfMemory)
}

/// Map errors that are intrinsic to continuation export. Public callback
/// failures receive finer classification in `continuation_write` below.
fn continuation_error_result(err: ContinuationWriteError) -> Result {
    match err {
        ContinuationWriteError::InvalidValue => Result::InvalidValue,
        ContinuationWriteError::WriteFailed => Result::IoError,
        ContinuationWriteError::ContinuationDisabled => Result::InvalidValue,
        ContinuationWriteError::ContinuationUnavailable => Result::InvalidValue,
    }
}

// pub fn continuation_write(
//     terminal_: Terminal,
//     writer: c_io.Writer,
// ) callconv(lib.calling_conv) Result {
/// # Safety
/// `terminal_`, if non-null, must be a live terminal handle; `writer`'s
/// callback/userdata contract is the caller's.
pub unsafe fn continuation_write(terminal_: Terminal, writer: crate::io::Writer) -> Result {
    // Reject the missing callback before invoking the common helper so this is
    // classified as a bad argument rather than a write failure.
    if writer.write.is_none() {
        return Result::InvalidValue;
    }

    // The callback was validated above, so invalid_write can only mean output
    // accounting overflow. Keep that distinct from callback rejection.
    // var buffer: [c_io.WriterAdapter.recommended_buffer_len]u8 = undefined;
    // var adapter: c_io.WriterAdapter = .initBuffered(writer, &buffer);
    let mut adapter = crate::io::WriterAdapter::init_buffered(writer);
    // write: { ... }
    let failed = 'write: {
        // SAFETY: `terminal_` validity is the caller's contract.
        match unsafe { continuation_write_io(terminal_, &mut adapter) } {
            Ok(()) => {}
            Err(ContinuationWriteError::WriteFailed) => break 'write true,
            Err(err) => return continuation_error_result(err),
        }
        if adapter.flush_buffered().is_err() {
            break 'write true;
        }
        return Result::Success;
    };
    debug_assert!(failed);

    if adapter.invalid_write {
        return Result::LimitExceeded;
    }
    if adapter.callback_failed {
        return Result::IoError;
    }
    continuation_error_result(ContinuationWriteError::WriteFailed)
}

// pub fn continuation_buf(
//     terminal_: Terminal,
//     out_: ?[*]u8,
//     out_len: usize,
//     out_written_: ?*usize,
// ) callconv(lib.calling_conv) Result {
/// # Safety
/// `terminal_`, if non-null, must be a live terminal handle; `out_`, if
/// non-null, must point to `out_len` writable bytes; `out_written_`, if
/// non-null, must be writable.
pub unsafe fn continuation_buf(
    terminal_: Terminal,
    out_: *mut u8,
    out_len: usize,
    out_written_: *mut usize,
) -> Result {
    // const out_written = out_written_ orelse return .invalid_value;
    if out_written_.is_null() {
        return Result::InvalidValue;
    }
    // All failure paths leave deterministic output metadata.
    // SAFETY: `out_written_` is non-null and writable (caller contract).
    unsafe { *out_written_ = 0 };
    if out_.is_null() && out_len != 0 {
        return Result::InvalidValue;
    }

    if out_.is_null() {
        // A null/zero destination is the explicit size-query form. Discarding
        // runs the real exporter, so disabled or unavailable tracking is still
        // detected before a required length is reported.
        // var discarding: std.Io.Writer.Discarding = .init(&.{});
        let mut discarding = CountingWriter::new();
        // SAFETY: `terminal_` validity is the caller's contract.
        if let Err(err) = unsafe { continuation_write_io(terminal_, &mut discarding) } {
            return continuation_error_result(err);
        }
        unsafe { *out_written_ = discarding.count };
        return Result::OutOfSpace;
    }

    // Fixed writers report WriteFailed when capacity is exhausted. The stream
    // exporter itself remains all-or-nothing from the API's perspective.
    // var writer: std.Io.Writer = .fixed(out_.?[0..out_len]);
    // SAFETY: `out_[0..out_len]` is writable (caller contract).
    let dest = unsafe { core::slice::from_raw_parts_mut(out_, out_len) };
    let mut writer = FixedWriter::new(dest);
    // SAFETY: `terminal_` validity is the caller's contract.
    match unsafe { continuation_write_io(terminal_, &mut writer) } {
        Ok(()) => {}
        Err(ContinuationWriteError::WriteFailed) => {
            // Re-run against a counter to return the full required capacity,
            // not merely the prefix that fit in the caller's buffer.
            let mut discarding = CountingWriter::new();
            // SAFETY: as above.
            if let Err(count_err) = unsafe { continuation_write_io(terminal_, &mut discarding) } {
                return continuation_error_result(count_err);
            }
            unsafe { *out_written_ = discarding.count };
            return Result::OutOfSpace;
        }
        Err(err) => return continuation_error_result(err),
    }

    // `end` is the initialized prefix of the fixed destination.
    unsafe { *out_written_ = writer.end };
    Result::Success
}

// pub fn continuation_alloc(
//     terminal_: Terminal,
//     alloc_: ?*const CAllocator,
//     out_ptr_: ?*?[*]u8,
//     out_len_: ?*usize,
// ) callconv(lib.calling_conv) Result {
/// # Safety
/// `terminal_`, if non-null, must be a live terminal handle; `alloc_`, if
/// provided, must reference a valid `CAllocator`; `out_ptr_`/`out_len_`, if
/// non-null, must be writable.
pub unsafe fn continuation_alloc(
    terminal_: Terminal,
    alloc_: Option<&CAllocator>,
    out_ptr_: *mut *mut u8,
    out_len_: *mut usize,
) -> Result {
    // const out_ptr = out_ptr_ orelse return .invalid_value;
    // const out_len = out_len_ orelse return .invalid_value;
    if out_ptr_.is_null() || out_len_.is_null() {
        return Result::InvalidValue;
    }
    // Make ownership unambiguous even if validation or allocation fails.
    // SAFETY: both outputs are non-null and writable (caller contract).
    unsafe {
        *out_ptr_ = std::ptr::null_mut();
        *out_len_ = 0;
    }

    // Resolve NULL to librustty-vt's default allocator before entering the
    // shared Zig allocation path.
    // SAFETY: a non-NULL `alloc_` is a valid `RusttyAllocator` for the call.
    let alloc = unsafe { rustty_vt::alloc::default(alloc_) };
    // SAFETY: `terminal_` validity is the caller's contract.
    let bytes = match unsafe { continuation_alloc_io(terminal_, alloc, false) } {
        Ok(bytes) => bytes,
        Err(err) => {
            return match err {
                ContinuationAllocError::InvalidValue => Result::InvalidValue,
                ContinuationAllocError::OutOfMemory => Result::OutOfMemory,
                ContinuationAllocError::ContinuationDisabled => Result::InvalidValue,
                ContinuationAllocError::ContinuationUnavailable => Result::InvalidValue,
            }
        }
    };

    // Ownership crosses the ABI here; callers release this exact pointer and
    // length with rustty_free and the same allocator selection.
    // An idle parser has no continuation. Never export the empty Zig slice's
    // sentinel pointer to a foreign runtime.
    // PORT NOTE (parity tail S7-8): the bytes are the allocating writer's own
    // buffer on the caller's allocator (`toOwnedSlice`); an empty
    // continuation made no allocator call, and `into_c_bytes` exports it as
    // NULL (`dd2edd760`).
    let (dst, len) = crate::allocator::into_c_bytes(bytes);
    // out_ptr.* = if (bytes.len == 0) null else bytes.ptr;
    // out_len.* = bytes.len;
    unsafe {
        *out_ptr_ = dst;
        *out_len_ = len;
    }
    Result::Success
}

// fn continuationMaxBytes(wrapper: *const TerminalWrapper) usize {
/// # Safety
/// `wrapper` must be a live terminal wrapper pointer.
unsafe fn continuation_max_bytes(wrapper: *const TerminalWrapper) -> usize {
    // Absence of a tracker is the public representation of disabled tracking.
    // SAFETY: `wrapper` is live (caller contract); shared read of its stream.
    match unsafe { (*wrapper).stream.continuation.as_ref() } {
        Some(tracker) => tracker.max_bytes,
        None => 0,
    }
}

/// Change continuation tracking policy without disturbing normal VT parser
/// state. Bytes which were not retained while disabled or after exceeding an
/// earlier cap cannot be reconstructed: export remains unavailable until a
/// later feed reaches ground or contains a new replay start.
// fn setContinuationMaxBytes(wrapper: *TerminalWrapper, max_bytes: usize) void {
/// # Safety
/// `wrapper` must be a live terminal wrapper pointer.
unsafe fn set_continuation_max_bytes(wrapper: *mut TerminalWrapper, max_bytes: usize) {
    if max_bytes == 0 {
        // Disabling releases retained bytes immediately. Parser and UTF-8 state
        // continue normally; only future replay/export information is lost.
        // if (wrapper.stream.continuation) |*tracker| tracker.deinit();
        // wrapper.stream.continuation = null;
        // SAFETY: `wrapper` is live (caller contract).
        unsafe {
            if let Some(tracker) = (*wrapper).stream.continuation.as_mut() {
                tracker.deinit();
            }
            (*wrapper).stream.continuation = None;
        }
        return;
    }

    // SAFETY: `wrapper` is live (caller contract).
    if let Some(tracker) = unsafe { (*wrapper).stream.continuation.as_mut() } {
        // Changing a live cap preserves retained bytes when they still fit.
        tracker.max_bytes = max_bytes;
        if tracker.bytes.len() > max_bytes {
            // Once a retained prefix is discarded it cannot be reconstructed
            // from parser state alone. Mark it broken until Stream observes a
            // ground state or a new replay-safe sequence start.
            tracker.bytes.clear_retaining_capacity();
            tracker.broken = true;
        }
        return;
    }

    // Enabling from zero starts an empty tracker owned by the terminal's
    // allocator. It can immediately track only if no earlier bytes are needed.
    // wrapper.stream.continuation = .init(wrapper.terminal.gpa(), max_bytes);
    // SAFETY: `wrapper` is live (caller contract); raw reads of its terminal.
    unsafe {
        let gpa = (*(*(*wrapper).terminal).screens.active).alloc;
        (*wrapper).stream.continuation = Some(rustty_vt::stream_continuation::Tracker::init(
            gpa, max_bytes,
        ));
        if !(*wrapper).stream.ground() {
            // The parser was already mid-sequence while tracking was disabled, so
            // exporting now would omit an unknown prefix.
            (*wrapper).stream.continuation.as_mut().unwrap().broken = true;
        }
    }
}

// pub fn compression_activity(terminal_: Terminal, out_activity_: ?*u64) ... Result {
//     const t: *ZigTerminal = (terminal_ orelse return .invalid_value).terminal;
//     const out_activity = out_activity_ orelse return .invalid_value;
//     out_activity.* = t.compressionActivity();
//     return .success;
// }
/// # Safety
/// `terminal_`, if non-null, must be a live terminal handle; `out_activity_`, if
/// non-null, must be a valid writable pointer to a `u64`.
pub unsafe fn compression_activity(terminal_: Terminal, out_activity_: *mut u64) -> Result {
    // const t: *ZigTerminal = (terminal_ orelse return .invalid_value).terminal;
    if terminal_.is_null() {
        return Result::InvalidValue;
    }
    // SAFETY: `terminal_` is a live wrapper; `terminal` is its owned heap
    // terminal.
    let t: *const ZigTerminal = unsafe { (*terminal_).terminal };
    // const out_activity = out_activity_ orelse return .invalid_value;
    if out_activity_.is_null() {
        return Result::InvalidValue;
    }
    // out_activity.* = t.compressionActivity();
    // SAFETY: `out_activity_` is non-null and writable (caller contract).
    unsafe { *out_activity_ = (*t).compression_activity() };
    // return .success;
    Result::Success
}

// pub fn compress(terminal_: Terminal, mode_: c_int, out_result_: ?*CompressionResult) ... Result {
//     const t: *ZigTerminal = (terminal_ orelse return .invalid_value).terminal;
//     const out_result = out_result_ orelse return .invalid_value;
//     const mode = std.meta.intToEnum(CompressionMode, mode_) catch
//         return .invalid_value;
//
//     out_result.* = t.compress(mode);
//     return .success;
// }
/// # Safety
/// `terminal_`, if non-null, must be a live terminal handle; `out_result_`, if
/// non-null, must be a valid writable pointer to a `CompressionResult`.
pub unsafe fn compress(
    terminal_: Terminal,
    mode_: c_int,
    out_result_: *mut CompressionResult,
) -> Result {
    // const t: *ZigTerminal = (terminal_ orelse return .invalid_value).terminal;
    if terminal_.is_null() {
        return Result::InvalidValue;
    }
    // SAFETY: `terminal_` is a live wrapper.
    let t: *mut ZigTerminal = unsafe { (*terminal_).terminal };
    // const out_result = out_result_ orelse return .invalid_value;
    if out_result_.is_null() {
        return Result::InvalidValue;
    }
    // const mode = std.meta.intToEnum(CompressionMode, mode_) catch
    //     return .invalid_value;
    //
    // PORT NOTE (soundness): the reference's `intToEnum` is a checked
    // conversion. rustty must NOT `transmute` an unchecked `c_int` into the
    // `#[repr(C)]` enum — an out-of-range discriminant is instant UB — so the
    // conversion is an explicit exhaustive `match` on the incoming integer,
    // which is the sound analog and the only place the C-ABI open-enum value is
    // narrowed to the closed Rust enum.
    let mode = match mode_ {
        0 => CompressionMode::Incremental,
        1 => CompressionMode::Full,
        _ => return Result::InvalidValue,
    };

    // out_result.* = t.compress(mode);
    // SAFETY: `out_result_` is non-null and writable (caller contract).
    unsafe { *out_result_ = (*t).compress(mode) };
    // return .success;
    Result::Success
}

/// C: GhosttyTerminalOption -> RusttyTerminalOption (R2)
// pub const Option = enum(c_int) { ... };
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Option_ {
    Userdata = 0,
    WritePty = 1,
    Bell = 2,
    Enquiry = 3,
    Xtversion = 4,
    TitleChanged = 5,
    SizeCb = 6,
    ColorScheme = 7,
    DeviceAttributes = 8,
    Title = 9,
    Pwd = 10,
    ColorForeground = 11,
    ColorBackground = 12,
    ColorCursor = 13,
    ColorPalette = 14,
    KittyImageStorageLimit = 15,
    KittyImageMediumFile = 16,
    KittyImageMediumTempFile = 17,
    KittyImageMediumSharedMem = 18,
    ApcMaxBytes = 19,
    ApcMaxBytesKitty = 20,
    Selection = 21,
    DefaultCursorStyle = 22,
    DefaultCursorBlink = 23,
    GlyphProtocol = 24,
    PwdChanged = 25,
    ClipboardWrite = 26,
    ScrollbackMaxBytes = 27,
    ScrollbackMaxLines = 28,
    /// Input type: `DesktopNotificationFn` (oracle c3655ba25).
    DesktopNotification = 29,
    /// Input type: `ProgressReportFn` (oracle 47d602c42).
    ProgressReport = 30,
    /// Input type: `*const usize` (oracle d7bb4b863).
    ContinuationMaxBytes = 31,
    /// Input type: `*const bool` (oracle 38e891e6c).
    TitleReport = 32,
    /// Input type: `*const ModeConfig` (oracle cfc19e805).
    ModeDefault = 33,
    /// Input type: `*const ModeConfig` (oracle cfc19e805).
    Mode = 34,
    /// Input type: `UnknownSequenceFn` (oracle 6b990de5b).
    UnknownSequence = 35,
    /// Input type: `*const usize` (oracle 6b990de5b).
    UnknownMaxBytes = 36,
    /// Input type: `*const lib.String` (oracle 74efadb44).
    TerminfoName = 37,
    /// Input type: `ClipboardReadFn` (oracle e03475c0c).
    ClipboardRead = 38,
    /// Input type: `*const usize` (oracle 5350d4a5f/70f006575).
    ClipboardWriteMaxBytes = 39,
    // /// Input type expected for setting the option.
    // pub fn InType(comptime self: Option) type {
    //     return switch (self) {
    //         .userdata => ?*const anyopaque,
    //         .write_pty => ?Effects.WritePtyFn,
    //         .bell => ?Effects.BellFn,
    //         .color_scheme => ?Effects.ColorSchemeFn,
    //         .device_attributes => ?Effects.DeviceAttributesFn,
    //         .enquiry => ?Effects.EnquiryFn,
    //         .xtversion => ?Effects.XtversionFn,
    //         .title_changed => ?Effects.TitleChangedFn,
    //         .pwd_changed => ?Effects.PwdChangedFn,
    //         .size_cb => ?Effects.SizeFn,
    //         .clipboard_write => ?Effects.ClipboardWriteFn,
    //         .clipboard_read => ?Effects.ClipboardReadFn,
    //         .unknown_sequence => ?Effects.UnknownSequenceFn,
    //         .title, .pwd, .terminfo_name => ?*const lib.String,
    //         .color_foreground, .color_background, .color_cursor => ?*const color.RGB.C,
    //         .color_palette => ?*const color.PaletteC,
    //         .kitty_image_storage_limit => ?*const u64,
    //         .kitty_image_medium_file,
    //         .kitty_image_medium_shared_mem,
    //         .glyph_protocol,
    //         .title_report,
    //         => ?*const bool,
    //         .kitty_image_medium_temp_file => ?*const lib.String,
    //         .apc_max_bytes,
    //         .apc_max_bytes_kitty,
    //         .scrollback_max_bytes,
    //         .scrollback_max_lines,
    //         .continuation_max_bytes,
    //         .unknown_max_bytes,
    //         .clipboard_write_max_bytes,
    //         => ?*const usize,
    //         .selection => ?*const selection_c.CSelection,
    //         .default_cursor_style => ?*const TerminalCursorStyle,
    //         .default_cursor_blink => ?*const bool,
    //         .mode, .mode_default => ?*const ModeConfig,
    //     };
    // }
    //
    // PORT NOTE: `InType` is a comptime type-returning fn (the input type expected
    // for setting each option). rustty erases `value` to `*const c_void` at the FFI
    // boundary and reads it back as the per-arm type inside `set_typed`, so this
    // comptime projection has no Rust analog; it is preserved verbatim as the
    // reference contract for each arm's expected pointee type.
}

/// Zig `@intCast` from a C-ABI `uint64_t` into a narrower destination, with the
/// out-of-range value taken to the destination's maximum instead of trapping.
///
/// PORT NOTE (task R13-P, `UPSTREAM-DEFECTS.md` UD63): generic over the
/// destination so the saturating arm is testable on a 64-bit host, where
/// `usize::try_from(u64)` can never fail. The only production caller is the
/// `kitty_image_storage_limit` setter.
fn saturating_from_u64<T: TryFrom<u64>>(v: u64, max: T) -> T {
    T::try_from(v).unwrap_or(max)
}

// pub fn set(terminal_: Terminal, option: Option, value: ?*const anyopaque) ... {
//     if (comptime std.debug.runtime_safety) { _ = intToEnum(Option, ...) catch { ... return .invalid_value; }; }
//     const wrapper = terminal_ orelse return .invalid_value;
//     return switch (option) {
//         inline else => |comptime_option| setTyped(wrapper, comptime_option, @ptrCast(@alignCast(value))),
//     };
// }
//
// PORT NOTE: the `intToEnum(Option, ...)` runtime-safety guard on the raw `c_int`
// crossing the C boundary lives in the lib.rs `extern "C"` wrapper (the cell/key
// pattern); here `option` is already a valid `#[repr(C)] Option_`. The `inline
// else => setTyped` comptime dispatch becomes a direct `set_typed` call.
//
// PORT NOTE (formerly PARTIAL FILL): the `.selection` arm landed first (it is
// exercised by `selection.zig`'s `selection_format_alloc uses active
// selection` test); the get/set sub-batch has since landed every other option
// arm — no `todo!("vt-c set <option>")` remains (VALIDATION_REPORT M3).
/// # Safety
/// `terminal_`, if non-null, must be a live terminal handle; `value`, if
/// non-null, must point to storage of the type required by `option`
/// (`Option_::in_type`).
pub unsafe fn set(terminal_: Terminal, option: Option_, value: *const c_void) -> Result {
    // const wrapper = terminal_ orelse return .invalid_value;
    if terminal_.is_null() {
        return Result::InvalidValue;
    }
    // inline else => |comptime_option| setTyped(wrapper, comptime_option, @ptrCast(@alignCast(value)))
    unsafe { set_typed(terminal_, option, value) }
}

// fn setTyped(wrapper: *TerminalWrapper, comptime option: Option, value: option.InType()) Result {
//     switch (option) { ...; .selection => { ... }; ... }
//     return .success;
// }
//
// # Safety
// `terminal_`, if non-null, is a live wrapper; `value` points at storage of the
// type required by `option`.
unsafe fn set_typed(terminal_: Terminal, option: Option_, value: *const c_void) -> Result {
    // PORT NOTE (SOUNDNESS): the reference is `setTyped(wrapper, ...)`, mutating
    // the public fields `wrapper.effects`, `wrapper.terminal.*`, and
    // `wrapper.stream.handler.*`. rustty reaches each through RAW pointers off
    // `terminal_` (the `*mut TerminalWrapper`) and forms a `&mut` only to the
    // disjoint sub-field it writes — never an overlapping `&mut TerminalWrapper`
    // or `&mut ZigTerminal` that would invalidate the stream handler's stored
    // alias of the terminal (the rust-mut-args-strongly-protected discipline).
    // `value`, for callback-fn arms, IS the C function pointer (the reference's
    // `?Effects.WritePtyFn` etc.); for data arms it points at the typed value.
    let t: *mut ZigTerminal = unsafe { (*terminal_).terminal };

    match option {
        // .userdata => wrapper.effects.userdata = @constCast(value),
        Option_::Userdata => unsafe {
            (*terminal_).effects.userdata = value as *mut c_void;
        },
        // .write_pty => wrapper.effects.write_pty = value,
        Option_::WritePty => unsafe {
            (*terminal_).effects.write_pty =
                std::mem::transmute::<*const c_void, Option<WritePtyFn>>(value);
        },
        // .bell => wrapper.effects.bell = value,
        Option_::Bell => unsafe {
            (*terminal_).effects.bell = std::mem::transmute::<*const c_void, Option<BellFn>>(value);
        },
        // .color_scheme => wrapper.effects.color_scheme = value,
        Option_::ColorScheme => unsafe {
            (*terminal_).effects.color_scheme =
                std::mem::transmute::<*const c_void, Option<ColorSchemeFn>>(value);
        },
        // .device_attributes => wrapper.effects.device_attributes_cb = value,
        Option_::DeviceAttributes => unsafe {
            (*terminal_).effects.device_attributes_cb =
                std::mem::transmute::<*const c_void, Option<DeviceAttributesFn>>(value);
        },
        // .enquiry => wrapper.effects.enquiry = value,
        Option_::Enquiry => unsafe {
            (*terminal_).effects.enquiry =
                std::mem::transmute::<*const c_void, Option<EnquiryFn>>(value);
        },
        // .xtversion => wrapper.effects.xtversion = value,
        Option_::Xtversion => unsafe {
            (*terminal_).effects.xtversion =
                std::mem::transmute::<*const c_void, Option<XtversionFn>>(value);
        },
        // .title_changed => wrapper.effects.title_changed = value,
        Option_::TitleChanged => unsafe {
            (*terminal_).effects.title_changed =
                std::mem::transmute::<*const c_void, Option<TitleChangedFn>>(value);
        },
        // .pwd_changed => wrapper.effects.pwd_changed = value,
        Option_::PwdChanged => unsafe {
            (*terminal_).effects.pwd_changed =
                std::mem::transmute::<*const c_void, Option<PwdChangedFn>>(value);
        },
        // .size_cb => wrapper.effects.size_cb = value,
        Option_::SizeCb => unsafe {
            (*terminal_).effects.size_cb =
                std::mem::transmute::<*const c_void, Option<SizeFn>>(value);
        },
        // .clipboard_write => {
        //     wrapper.effects.clipboard_write = value;
        //     wrapper.stream.handler.effects.clipboard_write = if (value != null)
        //         &Effects.clipboardWriteTrampoline
        //     else
        //         null;
        // },
        Option_::ClipboardWrite => unsafe {
            (*terminal_).effects.clipboard_write =
                std::mem::transmute::<*const c_void, Option<ClipboardWriteFn>>(value);
            // The trampoline is installed dynamically to preserve the
            // handler's null fast path (a null Zig-level effect makes Kitty
            // clipboard writes fail up front instead of spooling a
            // transaction that can never commit; see Effects).
            (*terminal_).stream.handler.effects.clipboard_write = if !value.is_null() {
                Some(Effects::clipboard_write_trampoline)
            } else {
                None
            };
        },
        // .clipboard_read => {
        //     wrapper.effects.clipboard_read = value;
        //     wrapper.stream.handler.effects.clipboard_read = if (value != null)
        //         &Effects.clipboardReadTrampoline
        //     else
        //         null;
        // },
        Option_::ClipboardRead => unsafe {
            (*terminal_).effects.clipboard_read =
                std::mem::transmute::<*const c_void, Option<ClipboardReadFn>>(value);
            // Installed dynamically to keep the handler's null fast path
            // (reads stay denied without a callback; see Effects).
            (*terminal_).stream.handler.effects.clipboard_read = if !value.is_null() {
                Some(Effects::clipboard_read_trampoline)
            } else {
                None
            };
        },

        // .unknown_sequence => {
        //     wrapper.effects.unknown_sequence = value;
        //     wrapper.stream.handler.unknown_sequence = if (value != null)
        //         &Effects.unknownSequenceTrampoline
        //     else
        //         null;
        // },
        Option_::UnknownSequence => unsafe {
            (*terminal_).effects.unknown_sequence =
                std::mem::transmute::<*const c_void, Option<UnknownSequenceFn>>(value);
            // The trampoline is installed dynamically to preserve the
            // handler's null fast path.
            (*terminal_).stream.handler.unknown_sequence = if !value.is_null() {
                Some(Effects::unknown_sequence_trampoline)
            } else {
                None
            };
        },

        // .title_report => wrapper.stream.handler.title_report = if (value) |ptr|
        //     ptr.*
        // else
        //     false,
        Option_::TitleReport => {
            let ptr = value as *const bool;
            let v = if !ptr.is_null() {
                // SAFETY: `value` points to a readable `bool` (caller contract).
                unsafe { *ptr }
            } else {
                false
            };
            // SAFETY: raw place write off the handle (no wrapper-level `&mut`).
            unsafe { (*terminal_).stream.handler.title_report = v };
        }

        // .desktop_notification => wrapper.effects.desktop_notification = value,
        Option_::DesktopNotification => unsafe {
            (*terminal_).effects.desktop_notification =
                std::mem::transmute::<*const c_void, Option<DesktopNotificationFn>>(value);
        },

        // .progress_report => wrapper.effects.progress_report = value,
        Option_::ProgressReport => unsafe {
            (*terminal_).effects.progress_report =
                std::mem::transmute::<*const c_void, Option<ProgressReportFn>>(value);
        },

        // .title => {
        //     const str = if (value) |v| v.ptr[0..v.len] else "";
        //     wrapper.terminal.setTitle(str) catch return .out_of_memory;
        // },
        Option_::Title => {
            // const str = if (value) |v| v.ptr[0..v.len] else "";
            //
            // PORT NOTE (WS8 T7): the caller is allowed to hand back the bytes
            // `rustty_terminal_title()` returned, i.e. the terminal's OWN
            // buffer. We therefore keep the source as a RAW pointer and use the
            // alias-tolerant `set_title_raw`; forming a `&[u8]` here and
            // passing it to `set_title` would make the source a strongly
            // protected argument, and writing into it would be Stacked-Borrows
            // UB (memory: `rust-mut-args-strongly-protected`).
            let ptr = value as *const LibString;
            let (src, src_len): (*const u8, usize) = if !ptr.is_null() {
                let s = unsafe { *ptr };
                (s.ptr, s.len)
            } else {
                (b"".as_ptr(), 0)
            };
            // wrapper.terminal.setTitle(str) catch return .out_of_memory;
            // SAFETY: write the `title` sub-field of the terminal via the raw ptr;
            // `set_title_raw` takes `&mut Terminal` over a region disjoint from
            // the stream handler's alias usage (no concurrent `vt_write` is
            // live), and tolerates `src` overlapping the terminal's title.
            if unsafe { (*t).set_title_raw(src, src_len) }.is_err() {
                return Result::OutOfMemory;
            }
        }
        // .pwd => {
        //     const str = if (value) |v| v.ptr[0..v.len] else "";
        //     wrapper.terminal.setPwd(str) catch return .out_of_memory;
        // },
        Option_::Pwd => {
            // See the WS8 T7 PORT NOTE on `Option_::Title` above — the source
            // may alias the terminal's own pwd buffer, so it stays raw.
            let ptr = value as *const LibString;
            let (src, src_len): (*const u8, usize) = if !ptr.is_null() {
                let s = unsafe { *ptr };
                (s.ptr, s.len)
            } else {
                (b"".as_ptr(), 0)
            };
            if unsafe { (*t).set_pwd_raw(src, src_len) }.is_err() {
                return Result::OutOfMemory;
            }
        }

        // .terminfo_name => {
        //     const str = if (value) |v| v.ptr[0..v.len] else "";
        //     if (str.len > wrapper.terminfo_name_buf.len) return .invalid_value;
        //     @memcpy(wrapper.terminfo_name_buf[0..str.len], str);
        //     wrapper.stream.handler.terminfo_name = if (str.len > 0)
        //         wrapper.terminfo_name_buf[0..str.len]
        //     else
        //         null;
        // },
        Option_::TerminfoName => {
            // const str = if (value) |v| v.ptr[0..v.len] else "";
            let ptr = value as *const LibString;
            let (src, src_len): (*const u8, usize) = if !ptr.is_null() {
                let s = unsafe { *ptr };
                (s.ptr, s.len)
            } else {
                (b"".as_ptr(), 0)
            };
            // if (str.len > wrapper.terminfo_name_buf.len) return .invalid_value;
            //
            // The bounds check FIRST, then a copy of EXACTLY `str.len` bytes
            // (the `960c2cca5` overflow-class contract).
            if src_len > rustty_vt::stream_terminal::MAX_TERMINFO_NAME_BYTES {
                return Result::InvalidValue;
            }
            // @memcpy(wrapper.terminfo_name_buf[0..str.len], str);
            //
            // SB (SOUNDNESS): the buffer is written via a raw place projection
            // off the wrapper base and the handler's published slice is rebuilt
            // from the SAME projection — no wrapper-level `&mut` is formed, so
            // the stream handler's other self-referential aliases survive (see
            // the `terminfo_name_buf` field note).
            let buf: *mut u8 =
                unsafe { core::ptr::addr_of_mut!((*terminal_).terminfo_name_buf) } as *mut u8;
            // SAFETY: `src` has `src_len` readable bytes (caller contract);
            // `src_len <= MAX_TERMINFO_NAME_BYTES` was checked above; the C
            // caller's string cannot alias this internal buffer (it is never
            // exposed), so the ranges do not overlap (`@memcpy`'s contract).
            unsafe { core::ptr::copy_nonoverlapping(src, buf, src_len) };
            // wrapper.stream.handler.terminfo_name = ...;
            // SAFETY: the published slice borrows the heap-stable wrapper
            // buffer; it lives exactly as long as the wrapper (freed in `free`
            // together with the stream).
            unsafe {
                (*terminal_).stream.handler.terminfo_name = if src_len > 0 {
                    Some(core::slice::from_raw_parts(buf as *const u8, src_len))
                } else {
                    None
                };
            }
        }

        // .color_foreground => {
        //     wrapper.terminal.colors.foreground.default = if (value) |v| .fromC(v.*) else null;
        //     wrapper.terminal.flags.dirty.palette = true;
        // },
        Option_::ColorForeground => unsafe {
            let ptr = value as *const crate::color::RgbC;
            (*t).colors.foreground.default = if !ptr.is_null() { Some(*ptr) } else { None };
            (*t).flags.dirty.palette = true;
        },
        // .color_background => { ... },
        Option_::ColorBackground => unsafe {
            let ptr = value as *const crate::color::RgbC;
            (*t).colors.background.default = if !ptr.is_null() { Some(*ptr) } else { None };
            (*t).flags.dirty.palette = true;
        },
        // .color_cursor => { ... },
        Option_::ColorCursor => unsafe {
            let ptr = value as *const crate::color::RgbC;
            (*t).colors.cursor.default = if !ptr.is_null() { Some(*ptr) } else { None };
            (*t).flags.dirty.palette = true;
        },
        // .color_palette => {
        //     wrapper.terminal.colors.palette.changeDefault(
        //         if (value) |v| color.paletteZval(v) else color.default,
        //     );
        //     wrapper.terminal.flags.dirty.palette = true;
        // },
        Option_::ColorPalette => unsafe {
            let ptr = value as *const crate::color::PaletteC;
            let def = if !ptr.is_null() {
                crate::color::palette_zval(ptr)
            } else {
                rustty_vt::color::DEFAULT
            };
            (*t).colors.palette.change_default(def);
            (*t).flags.dirty.palette = true;
        },

        // .kitty_image_storage_limit => {
        //     if (comptime !build_options.kitty_graphics) return .success;
        //     const limit: usize = if (value) |v| @intCast(v.*) else 0;
        //     var it = wrapper.terminal.screens.all.iterator();
        //     while (it.next()) |entry| {
        //         const screen = entry.value.*;
        //         screen.kitty_images.setLimit(screen.io, screen.alloc, screen, limit);
        //     }
        // },
        Option_::KittyImageStorageLimit => {
            // if (comptime !build_options.kitty_graphics) return .success;
            if !rustty_vt::options::KITTY_GRAPHICS {
                return Result::Success;
            }
            // const limit: usize = if (value) |v| @intCast(v.*) else 0;
            // PORT NOTE (W33-B07): `@intCast` is a CHECKED narrowing — a
            // safety-checked build traps when the `uint64_t` limit exceeds
            // `usize` (only reachable where `usize` is 32-bit). Measured with
            // Zig 0.16.0 (`r13p-reference-probe.zig`, the same cast narrowed to
            // u32): Debug and ReleaseSafe panic "integer does not fit in
            // destination type", ReleaseFast wraps (4294967296 -> 0).
            //
            // PORT NOTE: reference defect (R13), `UPSTREAM-DEFECTS.md` UD63. The
            // value is a C-ABI caller's `uint64_t`, not an invariant, and this
            // is an `extern "C"` setter, so a panic aborts. A limit above the
            // address space is a request for "no practical limit", so it
            // saturates; the earlier `.unwrap()` trapped in EVERY profile,
            // which was stricter than the reference's own safe builds.
            let ptr = value as *const u64;
            let limit: usize = if !ptr.is_null() {
                saturating_from_u64(unsafe { *ptr }, usize::MAX)
            } else {
                0
            };
            // var it = wrapper.terminal.screens.all.iterator();
            // while (it.next()) |entry| {
            //     const screen = entry.value.*;
            //     screen.kitty_images.setLimit(screen.io, screen.alloc, screen, limit);
            // }
            // (`4371871bc` made `setLimit` infallible; the `catch return
            // .out_of_memory` is gone upstream.)
            for slot in unsafe { (*t).screens.all.iter() } {
                if let Some(screen) = *slot {
                    // PORT NOTE: `setLimit(&mut self, alloc, s: *mut Screen, limit)`
                    // aliases `screen.kitty_images` (the receiver) with `screen`
                    // (the `s` arg). Mirror `ScreenSet::init`: MOVE `kitty_images`
                    // out, run `set_limit` against the raw screen pointer, then
                    // move it back.
                    let alloc = unsafe { (*screen).alloc };
                    let mut images = core::mem::take(unsafe { &mut (*screen).kitty_images });
                    images.set_limit(alloc, screen, limit);
                    unsafe { (*screen).kitty_images = images };
                }
            }
        }
        // .kitty_image_medium_file,
        // .kitty_image_medium_shared_mem,
        // => {
        //     if (comptime !build_options.kitty_graphics) return .success;
        //     const val = (value orelse return .success).*;
        //     var it = wrapper.terminal.screens.all.iterator();
        //     while (it.next()) |entry| {
        //         const screen = entry.value.*;
        //         switch (option) {
        //             .kitty_image_medium_file => screen.kitty_images.image_limits.file = val,
        //             .kitty_image_medium_shared_mem => screen.kitty_images.image_limits.shared_memory = val,
        //             else => unreachable,
        //         }
        //     }
        // },
        // PORT NOTE (oracle `e8525c0fd`, `P3-W11 T1`): `kitty_image_medium_temp_file`
        // LEFT this multi-arm group when its value type changed from `bool*` to
        // `RusttyString*`; it now has its own arm below.
        Option_::KittyImageMediumFile | Option_::KittyImageMediumSharedMem => {
            if !rustty_vt::options::KITTY_GRAPHICS {
                return Result::Success;
            }
            // const val = (value orelse return .success).*;
            let ptr = value as *const bool;
            if ptr.is_null() {
                return Result::Success;
            }
            let val = unsafe { *ptr };
            for slot in unsafe { (*t).screens.all.iter() } {
                if let Some(screen) = *slot {
                    let limits = unsafe { &mut (*screen).kitty_images.image_limits };
                    match option {
                        Option_::KittyImageMediumFile => limits.file = val,
                        Option_::KittyImageMediumSharedMem => limits.shared_memory = val,
                        _ => unreachable!(),
                    }
                }
            }
        }

        // .kitty_image_medium_temp_file => {
        //     if (comptime !build_options.kitty_graphics) return .success;
        //     if (value) |v| {
        //         if (v.len > wrapper.tmp_dir_path.len) return .out_of_memory;
        //         @memcpy(wrapper.tmp_dir_path[0..v.len], v.ptr[0..v.len]);
        //         var it = wrapper.terminal.screens.all.iterator();
        //         while (it.next()) |entry| {
        //             const screen = entry.value.*;
        //             screen.kitty_images.image_limits.temporary_file = .{
        //                 .enabled = .{ .directory = wrapper.tmp_dir_path[0..v.len] },
        //             };
        //         }
        //     } else {
        //         var it = wrapper.terminal.screens.all.iterator();
        //         while (it.next()) |entry| {
        //             const screen = entry.value.*;
        //             screen.kitty_images.image_limits.temporary_file = .disabled;
        //         }
        //     }
        // },
        Option_::KittyImageMediumTempFile => {
            if !rustty_vt::options::KITTY_GRAPHICS {
                return Result::Success;
            }
            // const alloc = wrapper.terminal.gpa();
            // SAFETY: `(*terminal_).alloc` is the live C-resolved allocator
            // (F-008), valid until `free` reclaims it.
            let alloc: &dyn ZigAllocator = unsafe { &*(*terminal_).alloc };

            let ptr = value as *const LibString;
            if ptr.is_null() {
                // `else` arm: a NULL value pointer DISABLES the medium.
                //
                // SAFETY (Stacked Borrows discipline, path clear): every
                // screen is switched to `Disabled` FIRST, so none of them
                // still borrows `wrapper.tmp_dir_path` by the time it is
                // freed just below — matching upstream's
                // `if (wrapper.tmp_dir_path) |old| alloc.free(old);` AFTER
                // its own screen loop.
                for slot in unsafe { (*t).screens.all.iter() } {
                    if let Some(screen) = *slot {
                        unsafe {
                            (*screen).kitty_images.image_limits.temporary_file =
                                TemporaryFileLimit::Disabled;
                        }
                    }
                }
                // if (wrapper.tmp_dir_path) |old| alloc.free(old);
                // wrapper.tmp_dir_path = null;
                if let Some(old) = unsafe { (*terminal_).tmp_dir_path.take() } {
                    alloc.free_bytes(old);
                }
                return Result::Success;
            }

            // PORT NOTE (oracle `960c2cca5`/`105d0a545`, P7-W2 T3): the
            // reference copies the caller's bytes into
            // `wrapper.tmp_dir_path`, now an allocator-owned `?[]u8` (was a
            // fixed `[max_path_bytes]u8` buffer before `105d0a545`) whose
            // slice the limits then BORROW. `960c2cca5` fixed a `@memcpy(&buf,
            // v.ptr[0..v.len])` whose destination was the WHOLE buffer while
            // the source was `v.len` bytes — a length mismatch; that fix's
            // observable contract still applies here:
            //   (a) the length is bounds-checked FIRST, against
            //       `MAX_PATH_BYTES`, returning `RUSTTY_OUT_OF_MEMORY`;
            //   (b) EXACTLY `v.len` bytes are copied — never more.
            //
            // DECISIONS B6 (faithful): the wrapper allocates exactly ONE copy
            // (`dir` below), through the CALLER's allocator (F-008), and
            // every screen's `Limits` borrows it — not an independent
            // per-screen copy, and not a Rust-global-heap allocation.
            let v = unsafe { &*ptr };
            let len = v.len;
            // if (v.len > wrapper.tmp_dir_path.len) return .out_of_memory;
            if len > MAX_PATH_BYTES {
                return Result::OutOfMemory;
            }
            // const path = alloc.dupe(u8, v.ptr[0..v.len]) catch return .out_of_memory;
            let new_ptr = alloc.alloc_bytes(len);
            if new_ptr.is_null() {
                return Result::OutOfMemory;
            }
            if len > 0 {
                // SAFETY: `new_ptr` is a freshly allocated, non-overlapping
                // `len`-byte buffer (caller contract guarantees `v.ptr` is
                // readable for `len` bytes).
                unsafe { core::ptr::copy_nonoverlapping(v.ptr, new_ptr, len) };
            }
            let dir = ZigSlice::new(new_ptr, len);
            // var it = wrapper.terminal.screens.all.iterator();
            // while (it.next()) |entry| { ... screen.kitty_images.image_limits.temporary_file = .{ .enabled = .{ .directory = path } }; }
            //
            // SAFETY (Stacked Borrows discipline, path set twice): every
            // screen is pointed at the NEW `dir` here, BEFORE the wrapper's
            // OLD copy is freed below — so a screen updated earlier in this
            // very loop never observes a dangling pointer, and neither does
            // one updated on a LATER call (the old copy this call is about to
            // replace was already superseded for every screen by the time the
            // PREVIOUS call returned).
            for slot in unsafe { (*t).screens.all.iter() } {
                if let Some(screen) = *slot {
                    unsafe {
                        (*screen).kitty_images.image_limits.temporary_file =
                            TemporaryFileLimit::Enabled {
                                // SAFETY: `dir` is the wrapper's own copy,
                                // stored into `(*terminal_).tmp_dir_path`
                                // just below and kept alive until it is
                                // replaced or freed by a LATER call (or by
                                // `free`, which tears down every screen
                                // first) — never while this screen still
                                // holds this borrow.
                                directory: DirBorrow::new(std::ptr::slice_from_raw_parts(
                                    dir.ptr, dir.len,
                                )),
                            };
                    }
                }
            }
            // Every screen points at the new copy now so the previous
            // one can be released.
            // if (wrapper.tmp_dir_path) |old| alloc.free(old);
            // wrapper.tmp_dir_path = path;
            if let Some(old) = unsafe { (*terminal_).tmp_dir_path.replace(dir) } {
                alloc.free_bytes(old);
            }
        }

        // .apc_max_bytes => {
        //     wrapper.stream.handler.apc_handler.max_bytes = if (value) |ptr|
        //         .initFull(ptr.*)
        //     else
        //         .{};
        // },
        Option_::ApcMaxBytes => {
            // PORT NOTE: the reference `EnumMap` ports to the always-full
            // `[usize; Protocol::COUNT]` array (apc.rs). `.initFull(ptr.*)` sets
            // every protocol to the value; the `else .{}` branch (an EnumMap with
            // no entries) is, under the always-full port, the field's struct
            // default — `default_max_bytes` per protocol.
            let handler = unsafe { &mut (*terminal_).stream.handler };
            let ptr = value as *const usize;
            if !ptr.is_null() {
                let v = unsafe { *ptr };
                handler.apc_handler.max_bytes = [v; rustty_vt::apc::Protocol::COUNT];
            } else {
                handler.apc_handler.max_bytes = [
                    rustty_vt::apc::Protocol::default_max_bytes(rustty_vt::apc::Protocol::Kitty),
                    rustty_vt::apc::Protocol::default_max_bytes(rustty_vt::apc::Protocol::Glyph),
                ];
            }
        }
        // .apc_max_bytes_kitty => {
        //     if (value) |ptr| {
        //         wrapper.stream.handler.apc_handler.max_bytes.put(.kitty, ptr.*);
        //     } else {
        //         wrapper.stream.handler.apc_handler.max_bytes.remove(.kitty);
        //     }
        // },
        Option_::ApcMaxBytesKitty => {
            // PORT NOTE: `.put(.kitty, v)` sets the kitty slot. `.remove(.kitty)`
            // removes the entry so the later `.get(.kitty) orelse
            // defaultMaxBytes(.kitty)` falls back to the default; under the
            // always-full array port that is exactly resetting the slot to
            // `defaultMaxBytes(.kitty)`.
            let handler = unsafe { &mut (*terminal_).stream.handler };
            let ptr = value as *const usize;
            let kitty = rustty_vt::apc::Protocol::Kitty as usize;
            if !ptr.is_null() {
                handler.apc_handler.max_bytes[kitty] = unsafe { *ptr };
            } else {
                handler.apc_handler.max_bytes[kitty] =
                    rustty_vt::apc::Protocol::default_max_bytes(rustty_vt::apc::Protocol::Kitty);
            }
        }

        // .glyph_protocol => {
        //     const enabled = (value orelse return .success).*;
        //     wrapper.stream.handler.apc_handler.enable(.glyph, enabled);
        //     if (!enabled) wrapper.terminal.glyph_glossary.clearAndFree(wrapper.terminal.gpa());
        // },
        Option_::GlyphProtocol => {
            // const enabled = (value orelse return .success).*;
            let ptr = value as *const bool;
            if ptr.is_null() {
                return Result::Success;
            }
            let enabled = unsafe { *ptr };
            // wrapper.stream.handler.apc_handler.enable(.glyph, enabled);
            unsafe {
                (*terminal_)
                    .stream
                    .handler
                    .apc_handler
                    .enable(rustty_vt::apc::Protocol::Glyph, enabled)
            };
            // if (!enabled) wrapper.terminal.glyph_glossary.clearAndFree(wrapper.terminal.gpa());
            if !enabled {
                // PORT NOTE: rustty's `Glossary::clear_and_free` carries its own
                // allocator (the `gpa()` thread is internal); the reference passes
                // `wrapper.terminal.gpa()` explicitly.
                unsafe { (*t).glyph_glossary.clear_and_free() };
            }
        }

        // .selection => {
        //     if (value) |ptr| {
        //         const sel = ptr.toZig() orelse return .invalid_value;
        //         wrapper.terminal.screens.active.select(sel) catch return .out_of_memory;
        //     } else {
        //         wrapper.terminal.screens.active.clearSelection();
        //     }
        // },
        Option_::Selection => {
            let active: *mut rustty_vt::screen::Screen = unsafe { (*t).screens.active };
            // if (value) |ptr| { ... } else { ... }
            let ptr = value as *const crate::selection::CSelection;
            if !ptr.is_null() {
                // const sel = ptr.toZig() orelse return .invalid_value;
                let sel = match unsafe { (*ptr).to_zig() } {
                    Some(s) => s,
                    None => return Result::InvalidValue,
                };
                // wrapper.terminal.screens.active.select(sel) catch return .out_of_memory;
                if unsafe { (*active).select(Some(sel)) }.is_err() {
                    return Result::OutOfMemory;
                }
            } else {
                // wrapper.terminal.screens.active.clearSelection();
                unsafe { (*active).clear_selection() };
            }
        }

        // .default_cursor_style => {
        //     const style = (if (value) |ptr| ptr.* else TerminalCursorStyle.block).toZig() orelse return .invalid_value;
        //     wrapper.terminal.setDefaultCursorStyle(style);
        // },
        Option_::DefaultCursorStyle => {
            // const style = (if (value) |ptr| ptr.* else .block).toZig() orelse return .invalid_value;
            //
            // D11: the incoming value is read as the raw `c_int` it is on the
            // wire, NOT as the closed Rust `TerminalCursorStyle` — the oracle's
            // enum is non-exhaustive, so any `c_int` is representable there,
            // while a Rust closed-enum read of an out-of-range discriminant is
            // UB. `from_c_int` reproduces the reference `_ => null` arm.
            let ptr = value as *const c_int;
            let raw = if !ptr.is_null() {
                unsafe { *ptr }
            } else {
                TerminalCursorStyle::Block as c_int
            };
            let style =
                match TerminalCursorStyle::from_c_int(raw).and_then(TerminalCursorStyle::to_zig) {
                    Some(s) => s,
                    None => return Result::InvalidValue,
                };
            // wrapper.terminal.setDefaultCursorStyle(style);
            //
            // PORT NOTE (oracle c594031d5): the C API no longer writes the
            // handler's `default_cursor_style` (that field is gone) nor pokes
            // the active screen's cursor style; `Terminal` owns both and its
            // setter applies the change only while the cursor is still on its
            // defaults.
            unsafe { (*t).set_default_cursor_style(style) };
        }
        // .default_cursor_blink => {
        //     const blink = if (value) |ptr| ptr.* else false;
        //     wrapper.terminal.setDefaultCursorBlink(blink);
        // },
        Option_::DefaultCursorBlink => {
            // const blink = if (value) |ptr| ptr.* else false;
            let ptr = value as *const bool;
            let blink = if !ptr.is_null() {
                unsafe { *ptr }
            } else {
                false
            };
            // wrapper.terminal.setDefaultCursorBlink(blink);
            //
            // PORT NOTE: the reference's `blink` here is a plain `bool` (the C
            // option is a `bool*`), widened to `Terminal.Cursor.default_blink`'s
            // `?bool` at the setter boundary — a C caller can never select the
            // `null` "emulator default" arm through this option.
            unsafe { (*t).set_default_cursor_blink(Some(blink)) };
        }

        // .scrollback_max_bytes => wrapper.terminal.setScrollbackMaxBytes(
        //     if (value) |ptr| ptr.* else null,
        // ),
        Option_::ScrollbackMaxBytes => {
            let ptr = value as *const usize;
            let max = if ptr.is_null() {
                None
            } else {
                Some(unsafe { *ptr })
            };
            unsafe { (*t).set_scrollback_max_bytes(max) };
        }

        // .scrollback_max_lines => wrapper.terminal.setScrollbackMaxLines(
        //     if (value) |ptr| ptr.* else null,
        // ),
        Option_::ScrollbackMaxLines => {
            let ptr = value as *const usize;
            let max = if ptr.is_null() {
                None
            } else {
                Some(unsafe { *ptr })
            };
            unsafe { (*t).set_scrollback_max_lines(max) };
        }

        // .continuation_max_bytes => setContinuationMaxBytes(
        //     wrapper,
        //     if (value) |ptr| ptr.* else default_continuation_max_bytes,
        // ),
        Option_::ContinuationMaxBytes => {
            let ptr = value as *const usize;
            let max_bytes = if !ptr.is_null() {
                // SAFETY: `value` points to a readable `usize` (caller contract).
                unsafe { *ptr }
            } else {
                DEFAULT_CONTINUATION_MAX_BYTES
            };
            // SAFETY: `terminal_` is a live wrapper (caller contract).
            unsafe { set_continuation_max_bytes(terminal_, max_bytes) };
        }

        // .unknown_max_bytes => wrapper.stream.handler.apc_handler.unknown_max_bytes =
        //     if (value) |ptr| ptr.* else 0,
        Option_::UnknownMaxBytes => {
            let ptr = value as *const usize;
            let v = if !ptr.is_null() {
                // SAFETY: `value` points to a readable `usize` (caller contract).
                unsafe { *ptr }
            } else {
                0
            };
            // SAFETY: raw place write off the handle (no wrapper-level `&mut`).
            unsafe { (*terminal_).stream.handler.apc_handler.unknown_max_bytes = v };
        }

        // .clipboard_write_max_bytes => wrapper.stream.handler.kitty_clipboard_write_max_bytes =
        //     if (value) |ptr| ptr.* else kitty_clipboard.max_write_size,
        Option_::ClipboardWriteMaxBytes => {
            let ptr = value as *const usize;
            let v = if !ptr.is_null() {
                // SAFETY: `value` points to a readable `usize` (caller contract).
                unsafe { *ptr }
            } else {
                rustty_vt::kitty::clipboard::max_write_size
            };
            // SAFETY: raw place write off the handle (no wrapper-level `&mut`).
            unsafe { (*terminal_).stream.handler.kitty_clipboard_write_max_bytes = v };
        }

        // .mode, .mode_default => {
        //     const config = (value orelse return .invalid_value).*;
        //     const mode = config.toMode() orelse return .invalid_value;
        //     switch (option) {
        //         .mode => wrapper.terminal.modes.set(mode, config.value),
        //         .mode_default => {
        //             if (!modes.defaultConfigurable(mode)) return .invalid_value;
        //             wrapper.terminal.modes.setDefault(mode, config.value);
        //         },
        //         else => unreachable,
        //     }
        // },
        Option_::Mode | Option_::ModeDefault => {
            // const config = (value orelse return .invalid_value).*;
            let ptr = value as *const ModeConfig;
            if ptr.is_null() {
                return Result::InvalidValue;
            }
            // SAFETY: `value` points to a readable `ModeConfig` (caller contract).
            let config = unsafe { *ptr };
            // const mode = config.toMode() orelse return .invalid_value;
            let Some(mode) = config.to_mode() else {
                return Result::InvalidValue;
            };
            match option {
                // .mode => wrapper.terminal.modes.set(mode, config.value),
                // SAFETY: raw place access to the owned heap terminal.
                Option_::Mode => unsafe { (*t).modes.set(mode, config.value) },
                Option_::ModeDefault => {
                    // if (!modes.defaultConfigurable(mode)) return .invalid_value;
                    if !modes::default_configurable(mode) {
                        return Result::InvalidValue;
                    }
                    // wrapper.terminal.modes.setDefault(mode, config.value);
                    // SAFETY: raw place access to the owned heap terminal.
                    unsafe { (*t).modes.set_default(mode, config.value) };
                }
                // else => unreachable,
                _ => unreachable!(),
            }
        }
    }

    // return .success;
    Result::Success
}

/// C: GhosttyTerminalCursorStyle -> RusttyTerminalCursorStyle (R2)
// pub const TerminalCursorStyle = enum(c_int) { bar = 0, block = 1, underline = 2, block_hollow = 3, _ };
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalCursorStyle {
    Bar = 0,
    Block = 1,
    Underline = 2,
    BlockHollow = 3,
}

impl TerminalCursorStyle {
    /// Narrow a wire `c_int` to a declared enumerator, `None` for anything else.
    ///
    /// PORT NOTE (D11): the reference enum is non-exhaustive (`_`), so every
    /// `c_int` is a representable value in Zig and `toZig`'s `_ => null` arm is
    /// what rejects unknown discriminants. Rust has no non-exhaustive
    /// `#[repr(C)]` enum, so the narrowing must happen at the read: this is a
    /// soundness requirement of the Rust representation (loading an
    /// out-of-range discriminant into the closed enum is UB), not a behavioural
    /// divergence. The public header declares
    /// `RUSTTY_TERMINAL_CURSOR_STYLE_MAX_VALUE = RUSTTY_ENUM_MAX_VALUE`
    /// (INT_MAX), so out-of-range values are reachable from a conforming
    /// caller and must return `.invalid_value` exactly as the oracle does.
    fn from_c_int(value: c_int) -> Option<Self> {
        match value {
            0 => Some(TerminalCursorStyle::Bar),
            1 => Some(TerminalCursorStyle::Block),
            2 => Some(TerminalCursorStyle::Underline),
            3 => Some(TerminalCursorStyle::BlockHollow),
            // `_ => null`
            _ => None,
        }
    }

    // fn toZig(self: TerminalCursorStyle) ?Screen.CursorStyle {
    //     return switch (self) {
    //         .bar => .bar,
    //         .block => .block,
    //         .underline => .underline,
    //         .block_hollow => .block_hollow,
    //         _ => null,
    //     };
    // }
    // PORT NOTE: the reference enum is non-exhaustive (`_`), mapping any unknown
    // discriminant to `null`. The Rust `#[repr(C)]` enum carries only the four
    // named variants, so that arm lives in `from_c_int` above, which is what
    // callers must use to build this type out of a wire `c_int`. The signature
    // stays `Option` so the call site is a verbatim `orelse return
    // .invalid_value`.
    fn to_zig(self) -> Option<rustty_vt::screen::CursorStyle> {
        match self {
            // .bar => .bar,
            TerminalCursorStyle::Bar => Some(rustty_vt::screen::CursorStyle::Bar),
            // .block => .block,
            TerminalCursorStyle::Block => Some(rustty_vt::screen::CursorStyle::Block),
            // .underline => .underline,
            TerminalCursorStyle::Underline => Some(rustty_vt::screen::CursorStyle::Underline),
            // .block_hollow => .block_hollow,
            TerminalCursorStyle::BlockHollow => Some(rustty_vt::screen::CursorStyle::BlockHollow),
        }
    }
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
        (*terminal_)
            .stream
            .handler
            .resize(rustty_vt::terminal::Resize {
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
//     const t: *ZigTerminal = (terminal_ orelse return).terminal;
//     t.fullReset();
// }
/// # Safety
/// `terminal_`, if non-null, must be a live terminal handle.
pub unsafe fn reset(terminal_: Terminal) {
    if terminal_.is_null() {
        return;
    }
    // SAFETY: `terminal_` is a live wrapper.
    let t: *mut ZigTerminal = unsafe { (*terminal_).terminal };
    unsafe { (*t).full_reset() };
}

// (cfc19e805: `mode_get`/`mode_set` were REMOVED — replaced by the
// `.mode`/`.mode_default` option arms in `set_typed` and the `.mode`
// `TerminalData` arm in `get_typed`.)

/// C: GhosttyKittyGraphics -> RusttyKittyGraphics (R2)
// pub const KittyGraphics = kitty_gfx_c.KittyGraphics;
pub type KittyGraphics = KittyGraphicsHandle;

/// C: GhosttyTerminalScreen -> RusttyTerminalScreen (R2)
// pub const TerminalScreen = ScreenSet.Key;
pub type TerminalScreen = ScreenSetKey;

/// C: GhosttyTerminalScrollbar -> RusttyTerminalScrollbar (R2)
// pub const TerminalScrollbar = PageList.Scrollbar.C;
pub type TerminalScrollbar = ScrollbarC;

/// C: GhosttyTerminalData -> RusttyTerminalData (R2)
// pub const TerminalData = enum(c_int) { ... };
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalData {
    Invalid = 0,
    Cols = 1,
    Rows = 2,
    CursorX = 3,
    CursorY = 4,
    CursorPendingWrap = 5,
    ActiveScreen = 6,
    CursorVisible = 7,
    KittyKeyboardFlags = 8,
    Scrollbar = 9,
    CursorStyle = 10,
    MouseTracking = 11,
    Title = 12,
    Pwd = 13,
    TotalRows = 14,
    ScrollbackRows = 15,
    WidthPx = 16,
    HeightPx = 17,
    ColorForeground = 18,
    ColorBackground = 19,
    ColorCursor = 20,
    ColorPalette = 21,
    ColorForegroundDefault = 22,
    ColorBackgroundDefault = 23,
    ColorCursorDefault = 24,
    ColorPaletteDefault = 25,
    KittyImageStorageLimit = 26,
    KittyImageMediumFile = 27,
    KittyImageMediumTempFile = 28,
    KittyImageMediumSharedMem = 29,
    KittyGraphics = 30,
    Selection = 31,
    ViewportActive = 32,
    VtProcessingError = 33,
    ScrollbackMaxBytes = 34,
    ScrollbackMaxLines = 35,
    /// Output type: `usize` (oracle d7bb4b863).
    ContinuationMaxBytes = 36,
    /// Output type: `ModeConfig` (input/output — oracle cfc19e805).
    Mode = 37,
    /// Output type: `bool` (oracle a69a591af).
    VtGround = 38,
    /// Output type: `bool` (oracle bdb566068).
    CursorAtPrompt = 39,
    /// Output type: `usize` (oracle 5350d4a5f/70f006575).
    ClipboardWriteMaxBytes = 40,
    /// The mouse pointer shape requested by the application through OSC 22.
    ///
    /// Initially `mouse::Shape::Text`. Excludes host hover overrides.
    ///
    /// Output type: `mouse::Shape` (oracle 78494bad8/67fb0c940).
    MouseShape = 41,
    // /// Output type expected for querying the data of the given kind.
    // pub fn OutType(comptime self: TerminalData) type {
    //     return switch (self) {
    //         .invalid => void,
    //         .cols, .rows, .cursor_x, .cursor_y => size.CellCountInt,
    //         .cursor_pending_wrap,
    //         .cursor_visible,
    //         .mouse_tracking,
    //         .viewport_active,
    //         .vt_processing_error,
    //         .vt_ground,
    //         .cursor_at_prompt,
    //         => bool,
    //         .mouse_shape => mouse.Shape,
    //         .active_screen => TerminalScreen,
    //         .kitty_keyboard_flags => u8,
    //         .scrollbar => TerminalScrollbar,
    //         .cursor_style => style_c.Style,
    //         .title, .pwd => lib.String,
    //         .total_rows,
    //         .scrollback_rows,
    //         .scrollback_max_bytes,
    //         .scrollback_max_lines,
    //         .continuation_max_bytes,
    //         .clipboard_write_max_bytes,
    //         => usize,
    //         .width_px, .height_px => u32,
    //         .color_foreground,
    //         .color_background,
    //         .color_cursor,
    //         .color_foreground_default,
    //         .color_background_default,
    //         .color_cursor_default,
    //         => color.RGB.C,
    //         .color_palette, .color_palette_default => color.PaletteC,
    //         .kitty_image_storage_limit => u64,
    //         .kitty_image_medium_file,
    //         .kitty_image_medium_shared_mem,
    //         => bool,
    //         .kitty_image_medium_temp_file => lib.String,
    //         .kitty_graphics => KittyGraphics,
    //         .selection => selection_c.CSelection,
    //         .mode => ModeConfig,
    //     };
    // }
    // (`.continuation_max_bytes => usize` rides Bundle S.)
    //
    // PORT NOTE: `OutType` is a comptime type-returning fn (the output type expected
    // for querying each data kind). rustty erases `out` to `*mut c_void` at the FFI
    // boundary and writes it back as the per-arm type inside `get_typed`, so this
    // comptime projection has no Rust analog; it is preserved verbatim as the
    // reference contract for each arm's expected pointee type.
}

// D11 (P3-W6 T2), see `render::Data::from_c_int`: proves reading this value out
// of C memory as a `c_int` leaves the ABI unchanged (the published header
// declares `const RusttyTerminalData *keys` for `get_multi`).
const _: () = assert!(std::mem::size_of::<TerminalData>() == std::mem::size_of::<c_int>());

// PORT NOTE: range-checked `c_int -> enum` projection for the terminal get
// guard (the `std.meta.intToEnum` guard on the raw `c_int` crossing the C
// boundary). `TerminalData` is contiguous TODAY but has not always been.
//
// It went non-contiguous at `P3-W3` T5 (oracle `a27e04e8f` added
// `scrollback_max_bytes = 34` / `scrollback_max_lines = 35` while `33`,
// `vt_processing_error`, was not yet ported) and is CONTIGUOUS AGAIN as of
// `P3-W4` T5, which lands 33 (oracle `439d35e27`). A plain `0..N` range check
// would `transmute` a hole into an UNINHABITED enum value — instant UB — so
// holes are rejected explicitly. The MECHANISM stays even though the list is
// now empty: 36+ may reopen a hole, and re-deriving it from scratch under time
// pressure is exactly how that UB gets reintroduced.
// Bundle M reopened the holes at 36/38; Bundle S closed them again
// (`continuation_max_bytes` = 36, `vt_ground` = 38) — the list is empty but
// the MECHANISM stays.
const TERMINAL_DATA_MAX: c_int = 40;
const TERMINAL_DATA_HOLES: &[c_int] = &[];

impl TerminalData {
    /// Narrow an inbound C integer to a `TerminalData`, mirroring
    /// `std.meta.intToEnum`; `None` outside the declared range (or on a hole).
    /// `Invalid = 0` is IN range and keeps its `.invalid_value` disposition
    /// inside `get`.
    // PORT NOTE (W35-B08): the reference's guard is
    // `std.enums.fromInt(TerminalData, @intFromEnum(data))`. This used to live
    // in `lib.rs` (`terminal_data_from_c_int`), reachable only from the single
    // `rustty_terminal_get` wrapper; it lives here now so `get_multi` reaches
    // it per array element too, exactly as the reference's `get_multi` reaches
    // the guard by calling `get`.
    pub fn from_c_int(v: c_int) -> Option<TerminalData> {
        if (0..=TERMINAL_DATA_MAX).contains(&v) && !TERMINAL_DATA_HOLES.contains(&v) {
            // SAFETY: `v` is a valid `TerminalData` discriminant (checked
            // above).
            Some(unsafe { std::mem::transmute::<c_int, TerminalData>(v) })
        } else {
            None
        }
    }
}

// pub fn get(terminal_: Terminal, data: TerminalData, out: ?*anyopaque) ... {
//     if (comptime std.debug.runtime_safety) {
//         _ = std.meta.intToEnum(TerminalData, @intFromEnum(data)) catch { ... return .invalid_value; };
//     }
//     return switch (data) {
//         .invalid => .invalid_value,
//         inline else => |comptime_data| getTyped(terminal_, comptime_data, @ptrCast(@alignCast(out))),
//     };
// }
//
// PORT NOTE: the reference `intToEnum` runtime-safety guard lives on the raw
// `c_int` crossing the C boundary (`get_checked` below, shared by the
// `extern "C"` wrapper in lib.rs AND by `get_multi`); a valid `TerminalData`
// here cannot be out of range. The `inline else => getTyped` comptime dispatch
// becomes a direct `get_typed` call (the typed `out` cast is performed per-arm
// inside `get_typed`).
/// # Safety
/// `terminal_`, if non-null, must be a live terminal handle; `out`, if non-null,
/// must point to storage of the type required by `data` (`TerminalData::out_type`).
pub unsafe fn get(terminal_: Terminal, data: TerminalData, out: *mut c_void) -> Result {
    // const out_ptr = out orelse return .invalid_value;
    if out.is_null() {
        return Result::InvalidValue;
    }
    // .invalid => .invalid_value,
    if matches!(data, TerminalData::Invalid) {
        return Result::InvalidValue;
    }
    // inline else => |comptime_data| getTyped(terminal_, comptime_data, @ptrCast(@alignCast(out_ptr)))
    unsafe { get_typed(terminal_, data, out) }
}

// PORT NOTE (W35-B08): the reference carries the runtime-safety guard INSIDE
// `get`, so every caller reaches it — including `get_multi`, which calls `get`
// per array element. rustty's `get` takes the already-narrowed `TerminalData`
// (a `#[repr(C)]` enum cannot even hold an out-of-range tag), so the guard
// lives here, in the raw-`c_int` entry point shared by the `extern "C"` wrapper
// in `lib.rs` AND by `get_multi` — the same "every path goes through the guard"
// shape as the reference.
/// # Safety
/// `terminal_`, if non-null, must be a live terminal handle; `out`, if non-null,
/// must point to storage of the type required by `data`.
pub unsafe fn get_checked(terminal_: Terminal, data: c_int, out: *mut c_void) -> Result {
    // _ = std.meta.intToEnum(TerminalData, @intFromEnum(data)) catch {
    //     log.warn("terminal_get invalid data value={d}", .{@intFromEnum(data)});
    //     return .invalid_value;
    // };
    let Some(d) = TerminalData::from_c_int(data) else {
        crate::sys::log_fn(
            crate::sys::StdLevel::Warn,
            LOG_SCOPE,
            format_args!("terminal_get invalid data value={data}"),
        );
        return Result::InvalidValue;
    };
    // SAFETY: `terminal_`/`out` validity is the caller's contract.
    unsafe { get(terminal_, d, out) }
}

// fn getTyped(terminal_: Terminal, comptime data: TerminalData, out: *data.OutType()) Result {
//     const t: *ZigTerminal = (terminal_ orelse return .invalid_value).terminal;
//     switch (data) { ... }
//     return .success;
// }
//
// PORT NOTE: the reference is a `comptime`-monomorphized switch whose `out` is
// typed per arm (`*data.OutType()`). rustty erases `out` to `*mut c_void` and
// casts to the arm's concrete pointer type before the write. Arms whose
// `OutType` projects a vt-c type (`color.RGB.C`/`PaletteC`, `style_c.Style`,
// `selection_c.CSelection`, kitty image/graphics) landed with the get/set
// sub-batch — every arm below is live (VALIDATION_REPORT M3).
//
// # Safety
// `terminal_`, if non-null, is a live wrapper; `out` points at storage of the
// type required by `data`.
unsafe fn get_typed(terminal_: Terminal, data: TerminalData, out: *mut c_void) -> Result {
    // const t: *ZigTerminal = (terminal_ orelse return .invalid_value).terminal;
    if terminal_.is_null() {
        return Result::InvalidValue;
    }
    // PORT NOTE (SOUNDNESS): the reference reads through `*ZigTerminal` and its
    // `screens.active: *Screen`. We mirror that with RAW pointers and never form
    // a `&mut ZigTerminal`/`&mut Screen`: the stream handler aliases this same
    // terminal via a stored raw pointer (created in `vt_handler`), and a Unique
    // (`&mut`) retag over the terminal would invalidate that aliasing tag and
    // make the next `vt_write` UB (the rust-mut-args-strongly-protected rule).
    // All reads here are shared (`&`/scalar) through the raw ptr; the active
    // screen is reached via the raw `screens.active` pointer.
    let t: *mut ZigTerminal = unsafe { (*terminal_).terminal };
    let active: *mut rustty_vt::screen::Screen = unsafe { (*t).screens.active };

    match data {
        // .invalid => return .invalid_value,
        TerminalData::Invalid => return Result::InvalidValue,
        // .cols => out.* = t.cols,
        TerminalData::Cols => unsafe { *(out as *mut size::CellCountInt) = (*t).cols },
        // .rows => out.* = t.rows,
        TerminalData::Rows => unsafe { *(out as *mut size::CellCountInt) = (*t).rows },
        // .cursor_x => out.* = t.screens.active.cursor.x,
        TerminalData::CursorX => unsafe { *(out as *mut size::CellCountInt) = (*active).cursor.x },
        // .cursor_y => out.* = t.screens.active.cursor.y,
        TerminalData::CursorY => unsafe { *(out as *mut size::CellCountInt) = (*active).cursor.y },
        // .cursor_pending_wrap => out.* = t.screens.active.cursor.pending_wrap,
        TerminalData::CursorPendingWrap => unsafe {
            *(out as *mut bool) = (*active).cursor.pending_wrap
        },
        // .active_screen => out.* = t.screens.active_key,
        TerminalData::ActiveScreen => unsafe {
            *(out as *mut TerminalScreen) = (*t).screens.active_key
        },
        // .cursor_visible => out.* = t.modes.get(.cursor_visible),
        TerminalData::CursorVisible => unsafe {
            *(out as *mut bool) = (*t).modes.get(modes::Mode::CursorVisible)
        },
        // .kitty_keyboard_flags => out.* = @as(u8, t.screens.active.kitty_keyboard.current().int()),
        TerminalData::KittyKeyboardFlags => unsafe {
            *(out as *mut u8) = (*active).kitty_keyboard.current().int()
        },
        // .mouse_tracking => out.* = t.modes.get(.mouse_event_x10) or ... or any,
        TerminalData::MouseTracking => unsafe {
            *(out as *mut bool) = (*t).modes.get(modes::Mode::MouseEventX10)
                || (*t).modes.get(modes::Mode::MouseEventNormal)
                || (*t).modes.get(modes::Mode::MouseEventButton)
                || (*t).modes.get(modes::Mode::MouseEventAny)
        },
        // .total_rows => out.* = t.screens.active.pages.total_rows,
        TerminalData::TotalRows => unsafe { *(out as *mut usize) = (*active).pages.total_rows },
        // .scrollback_rows => out.* = t.screens.active.pages.total_rows - t.rows,
        TerminalData::ScrollbackRows => {
            let total = unsafe { (*active).pages.total_rows };
            let rows = unsafe { (*t).rows } as usize;
            unsafe { *(out as *mut usize) = total - rows };
        }
        // .width_px => out.* = t.width_px,
        TerminalData::WidthPx => unsafe { *(out as *mut u32) = (*t).width_px },
        // .height_px => out.* = t.height_px,
        TerminalData::HeightPx => unsafe { *(out as *mut u32) = (*t).height_px },
        // .viewport_active => out.* = t.screens.active.pages.viewport == .active,
        TerminalData::ViewportActive => unsafe {
            *(out as *mut bool) = matches!(
                (*active).pages.viewport,
                rustty_vt::pagelist::Viewport::Active
            )
        },
        // .vt_processing_error => out.* = wrapper.stream.handler.semantic_failure,
        TerminalData::VtProcessingError => unsafe {
            *(out as *mut bool) = (*terminal_).stream.handler.semantic_failure
        },

        // .scrollback_max_bytes => {
        //     const max = t.screens.get(.primary).?.pages.limits.bytes.explicit;
        //     if (max == std.math.maxInt(usize)) return .no_value;
        //     out.* = max;
        // },
        TerminalData::ScrollbackMaxBytes => {
            let primary: *mut rustty_vt::screen::Screen = unsafe {
                (*t).screens
                    .get(rustty_vt::screen_set::Key::Primary)
                    .unwrap()
            };
            let max = unsafe { (*primary).pages.limits.bytes.explicit };
            if max == usize::MAX {
                return Result::NoValue;
            }
            unsafe { *(out as *mut usize) = max };
        }
        // .scrollback_max_lines => { ... same, .lines.explicit ... },
        TerminalData::ScrollbackMaxLines => {
            let primary: *mut rustty_vt::screen::Screen = unsafe {
                (*t).screens
                    .get(rustty_vt::screen_set::Key::Primary)
                    .unwrap()
            };
            let max = unsafe { (*primary).pages.limits.lines.explicit };
            if max == usize::MAX {
                return Result::NoValue;
            }
            unsafe { *(out as *mut usize) = max };
        }

        // .continuation_max_bytes => out.* = continuationMaxBytes(wrapper),
        TerminalData::ContinuationMaxBytes => {
            // SAFETY: `terminal_` is a live wrapper (caller contract); `out`
            // points to a writable `usize`.
            unsafe { *(out as *mut usize) = continuation_max_bytes(terminal_) };
        }

        // .vt_ground => out.* = wrapper.stream.ground(),
        TerminalData::VtGround => unsafe { *(out as *mut bool) = (*terminal_).stream.ground() },

        // .mode => {
        //     const mode = out.toMode() orelse return .invalid_value;
        //     out.value = t.modes.get(mode);
        // },
        TerminalData::Mode => {
            // PORT NOTE: `.mode` is the one IN/OUT arm — the caller initializes
            // `mode` and receives `value`.
            // SAFETY: `out` points to a readable+writable `ModeConfig` (caller
            // contract; `get` already rejected a null `out`).
            let config = unsafe { &mut *(out as *mut ModeConfig) };
            let Some(mode) = config.to_mode() else {
                return Result::InvalidValue;
            };
            // SAFETY: shared read of the owned heap terminal's modes.
            config.value = unsafe { (*t).modes.get(mode) };
        }

        // .cursor_at_prompt => out.* = t.cursorIsAtPrompt(),
        TerminalData::CursorAtPrompt => {
            // SAFETY: `cursor_is_at_prompt` takes `&mut Terminal`; the transient
            // field-level autoref off the raw handle dies within the statement
            // (the rust-mut-args-strongly-protected shape).
            unsafe { *(out as *mut bool) = (*t).cursor_is_at_prompt() };
        }

        // .clipboard_write_max_bytes => out.* = wrapper.stream.handler.kitty_clipboard_write_max_bytes,
        TerminalData::ClipboardWriteMaxBytes => unsafe {
            *(out as *mut usize) = (*terminal_).stream.handler.kitty_clipboard_write_max_bytes
        },

        // .mouse_shape => out.* = t.mouse_shape,
        TerminalData::MouseShape => {
            // SAFETY: shared read of the owned heap terminal's mouse_shape
            // field via the raw ptr (no `&mut` retag of the handler-aliased
            // terminal).
            unsafe { *(out as *mut rustty_vt::mouse::Shape) = (*t).mouse_shape };
        }

        // .scrollbar => out.* = t.screens.active.pages.scrollbar().cval(),
        TerminalData::Scrollbar => unsafe {
            *(out as *mut TerminalScrollbar) = (*active).pages.scrollbar().cval()
        },
        // .cursor_style => out.* = .fromStyle(t.screens.active.cursor.style),
        TerminalData::CursorStyle => unsafe {
            *(out as *mut crate::style::Style) =
                crate::style::Style::from_style((*active).cursor.style)
        },
        // .title => {
        //     const title = t.getTitle() orelse "";
        //     out.* = .{ .ptr = title.ptr, .len = title.len };
        // },
        TerminalData::Title => {
            // SAFETY: shared read of the terminal's title via the raw ptr (no
            // `&mut` retag of the handler-aliased terminal).
            let title = unsafe { (*t).get_title() }.unwrap_or(b"");
            unsafe { *(out as *mut LibString) = LibString::init(title) };
        }
        // .pwd => {
        //     const pwd = t.getPwd() orelse "";
        //     out.* = .{ .ptr = pwd.ptr, .len = pwd.len };
        // },
        TerminalData::Pwd => {
            let pwd = unsafe { (*t).get_pwd() }.unwrap_or(b"");
            unsafe { *(out as *mut LibString) = LibString::init(pwd) };
        }
        // .color_foreground => out.* = (t.colors.foreground.get() orelse return .no_value).cval(),
        TerminalData::ColorForeground => {
            let rgb = match unsafe { (*t).colors.foreground.get() } {
                Some(c) => c,
                None => return Result::NoValue,
            };
            unsafe { *(out as *mut crate::color::RgbC) = rgb };
        }
        // .color_background => out.* = (t.colors.background.get() orelse return .no_value).cval(),
        TerminalData::ColorBackground => {
            let rgb = match unsafe { (*t).colors.background.get() } {
                Some(c) => c,
                None => return Result::NoValue,
            };
            unsafe { *(out as *mut crate::color::RgbC) = rgb };
        }
        // .color_cursor => out.* = (t.colors.cursor.get() orelse return .no_value).cval(),
        TerminalData::ColorCursor => {
            let rgb = match unsafe { (*t).colors.cursor.get() } {
                Some(c) => c,
                None => return Result::NoValue,
            };
            unsafe { *(out as *mut crate::color::RgbC) = rgb };
        }
        // .color_foreground_default => out.* = (t.colors.foreground.default orelse return .no_value).cval(),
        TerminalData::ColorForegroundDefault => {
            let rgb = match unsafe { (*t).colors.foreground.default } {
                Some(c) => c,
                None => return Result::NoValue,
            };
            unsafe { *(out as *mut crate::color::RgbC) = rgb };
        }
        // .color_background_default => out.* = (t.colors.background.default orelse return .no_value).cval(),
        TerminalData::ColorBackgroundDefault => {
            let rgb = match unsafe { (*t).colors.background.default } {
                Some(c) => c,
                None => return Result::NoValue,
            };
            unsafe { *(out as *mut crate::color::RgbC) = rgb };
        }
        // .color_cursor_default => out.* = (t.colors.cursor.default orelse return .no_value).cval(),
        TerminalData::ColorCursorDefault => {
            let rgb = match unsafe { (*t).colors.cursor.default } {
                Some(c) => c,
                None => return Result::NoValue,
            };
            unsafe { *(out as *mut crate::color::RgbC) = rgb };
        }
        // .color_palette => out.* = color.paletteCval(&t.colors.palette.current),
        TerminalData::ColorPalette => unsafe {
            *(out as *mut crate::color::PaletteC) =
                crate::color::palette_cval(&(*t).colors.palette.current)
        },
        // .color_palette_default => out.* = color.paletteCval(&t.colors.palette.original),
        TerminalData::ColorPaletteDefault => unsafe {
            *(out as *mut crate::color::PaletteC) =
                crate::color::palette_cval(&(*t).colors.palette.original)
        },
        // .kitty_image_storage_limit => {
        //     if (comptime !build_options.kitty_graphics) return .no_value;
        //     out.* = @intCast(t.screens.active.kitty_images.total_limit);
        // },
        TerminalData::KittyImageStorageLimit => {
            if !rustty_vt::options::KITTY_GRAPHICS {
                return Result::NoValue;
            }
            unsafe { *(out as *mut u64) = (*active).kitty_images.total_limit as u64 };
        }
        // .kitty_image_medium_file => {
        //     if (comptime !build_options.kitty_graphics) return .no_value;
        //     out.* = t.screens.active.kitty_images.image_limits.file;
        // },
        TerminalData::KittyImageMediumFile => {
            if !rustty_vt::options::KITTY_GRAPHICS {
                return Result::NoValue;
            }
            unsafe { *(out as *mut bool) = (*active).kitty_images.image_limits.file };
        }
        // .kitty_image_medium_temp_file => {
        //     if (comptime !build_options.kitty_graphics) return .no_value;
        //     const dir = switch (t.screens.active.kitty_images.image_limits.temporary_file) {
        //         .enabled => |d| d.directory,
        //         .disabled => "",
        //     };
        //     out.* = .init(dir);
        // },
        TerminalData::KittyImageMediumTempFile => {
            if !rustty_vt::options::KITTY_GRAPHICS {
                return Result::NoValue;
            }
            // PORT NOTE (byte-slice rule + borrow contract): `RusttyString` is
            // documented as BORROWED — the pointer is valid for as long as the
            // producing object is unchanged. Here it points into
            // `wrapper.tmp_dir_path` (via the active screen's borrowed
            // `DirBorrow`), exactly as upstream's does. `DirBorrow::as_slice`
            // is safe because the active screen's `directory`, when present,
            // borrows `(*terminal_).tmp_dir_path` (P7-W2 T3), which is still
            // live — nothing between the last `set` and this `get` can have
            // dropped or replaced it (the C API is single-threaded per
            // terminal, and only `set`/`free` touch that field).
            let dir: &[u8] = match unsafe { &(*active).kitty_images.image_limits.temporary_file } {
                TemporaryFileLimit::Enabled { directory } => directory.as_slice(),
                TemporaryFileLimit::Disabled => b"",
            };
            unsafe { *(out as *mut LibString) = LibString::init(dir) };
        }
        // .kitty_image_medium_shared_mem => {
        //     if (comptime !build_options.kitty_graphics) return .no_value;
        //     out.* = t.screens.active.kitty_images.image_limits.shared_memory;
        // },
        TerminalData::KittyImageMediumSharedMem => {
            if !rustty_vt::options::KITTY_GRAPHICS {
                return Result::NoValue;
            }
            unsafe { *(out as *mut bool) = (*active).kitty_images.image_limits.shared_memory };
        }
        // .kitty_graphics => {
        //     if (comptime !build_options.kitty_graphics) return .no_value;
        //     out.* = &t.screens.active.kitty_images;
        // },
        TerminalData::KittyGraphics => {
            if !rustty_vt::options::KITTY_GRAPHICS {
                return Result::NoValue;
            }
            // out.* = &t.screens.active.kitty_images;
            // PORT NOTE: `KittyGraphics` is the opaque `*mut c_void` handle (see
            // kitty_graphics.rs); hand out the address of the active screen's
            // `kitty_images` storage.
            unsafe {
                *(out as *mut crate::kitty_graphics::KittyGraphics) =
                    core::ptr::addr_of_mut!((*active).kitty_images) as *mut c_void
            };
        }
        // .selection => out.* = selection_c.CSelection.fromZig(
        //     t.screens.active.selection orelse return .no_value,
        // ),
        TerminalData::Selection => {
            let sel = match unsafe { (*active).selection } {
                Some(s) => s,
                None => return Result::NoValue,
            };
            unsafe {
                *(out as *mut crate::selection::CSelection) =
                    crate::selection::CSelection::from_zig(sel)
            };
        }
    }

    Result::Success
}

// pub fn get_multi(terminal_, count, keys, values, out_written) ... {
//     const k = keys orelse return .invalid_value;
//     const v = values orelse return .invalid_value;
//     for (0..count) |i| {
//         const result = get(terminal_, k[i], v[i]);
//         if (result != .success) { if (out_written) |w| w.* = i; return result; }
//     }
//     if (out_written) |w| w.* = count;
//     return .success;
// }
/// # Safety
/// `terminal_`, if non-null, must be a live terminal handle; `keys`/`values`, if
/// non-null, must point to `count` valid elements; `out_written`, if non-null,
/// must be a valid writable pointer.
// PORT NOTE (W35-B08): `keys` is C memory typed `const RusttyTerminalData *` in
// the published header; each element is read as a raw `c_int` (ABI-identical —
// `TerminalData` is `#[repr(C)]`, asserted int-sized above) and range-checked
// by `get_checked`. Reading an element straight into a `TerminalData` would
// materialize an out-of-range discriminant, which is UB in Rust in EVERY build
// mode (where the reference merely loses its `runtime_safety` guard).
pub unsafe fn get_multi(
    terminal_: Terminal,
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
        // const result = get(terminal_, k[i], v[i]);
        // SAFETY: `keys`/`values` point at `count` valid elements (caller
        // contract).
        let key = unsafe { *keys.add(i) };
        let value = unsafe { *values.add(i) };
        let result = unsafe { get_checked(terminal_, key, value) };
        // if (result != .success) { if (out_written) |w| w.* = i; return result; }
        if result != Result::Success {
            if !out_written.is_null() {
                unsafe { *out_written = i };
            }
            return result;
        }
    }
    // if (out_written) |w| w.* = count;
    if !out_written.is_null() {
        unsafe { *out_written = count };
    }
    Result::Success
}

// pub fn grid_ref(terminal_: Terminal, pt: point.Point.C, out_ref: ?*grid_ref_c.CGridRef) ... {
//     const t: *ZigTerminal = (terminal_ orelse return .invalid_value).terminal;
//     const zig_pt: point.Point = .fromC(pt);
//     const p = t.screens.active.pages.pin(zig_pt) orelse return .invalid_value;
//     if (out_ref) |out| out.* = grid_ref_c.CGridRef.fromPin(p);
//     return .success;
// }
/// # Safety
/// `terminal_`, if non-null, must be a live terminal handle; `out_ref`, if
/// non-null, must be a valid writable pointer to a `CGridRef`.
pub unsafe fn grid_ref(terminal_: Terminal, pt: PointC, out_ref: *mut CGridRef) -> Result {
    // const t: *ZigTerminal = (terminal_ orelse return .invalid_value).terminal;
    if terminal_.is_null() {
        return Result::InvalidValue;
    }
    // SAFETY: `terminal_` is a live wrapper; reads go through raw ptrs (never a
    // `&mut ZigTerminal`/`&mut Screen` that would invalidate the stream handler's
    // aliasing tag — the rust-mut-args-strongly-protected discipline).
    let t: *mut ZigTerminal = unsafe { (*terminal_).terminal };
    let active: *mut rustty_vt::screen::Screen = unsafe { (*t).screens.active };
    // const zig_pt: point.Point = .fromC(pt);
    // D11 (P3-W6 T2): `pt.tag` is the raw C integer; `from_c` narrows it.
    let zig_pt = match pt.from_c() {
        Some(p) => p,
        None => return Result::InvalidValue,
    };
    // const p = t.screens.active.pages.pin(zig_pt) orelse return .invalid_value;
    let p = match unsafe { (*active).pages.pin(zig_pt) } {
        Some(p) => p,
        None => return Result::InvalidValue,
    };
    // if (out_ref) |out| out.* = grid_ref_c.CGridRef.fromPin(p);
    if !out_ref.is_null() {
        // SAFETY: `out_ref` is a valid writable `CGridRef`.
        unsafe { *out_ref = CGridRef::from_pin(p) };
    }
    // return .success;
    Result::Success
}

// pub fn grid_ref_track(terminal_, pt: point.Point.C, out_ref: ?*grid_ref_tracked_c.CTrackedGridRef) ... {
//     const wrapper = terminal_ orelse return .invalid_value;
//     const out = out_ref orelse return .invalid_value;
//     out.* = null;
//     const t: *ZigTerminal = wrapper.terminal;
//     const list = &t.screens.active.pages;
//     const p = list.pin(.fromC(pt)) orelse return .invalid_value;
//     const tracked_pin = list.trackPin(p) catch return .out_of_memory;
//     const alloc = t.gpa();
//     const ref = alloc.create(grid_ref_tracked_c.TrackedGridRef) catch { list.untrackPin(tracked_pin); return .out_of_memory; };
//     ref.* = .{ .alloc, .terminal = wrapper, .screen_key = t.screens.active_key, .screen_generation = ..., .pin = tracked_pin };
//     wrapper.tracked_grid_refs.putNoClobber(alloc, ref, {}) catch { list.untrackPin(tracked_pin); alloc.destroy(ref); return .out_of_memory; };
//     out.* = ref;
//     return .success;
// }
/// # Safety
/// `terminal_`, if non-null, must be a live terminal handle; `out_ref`, if
/// non-null, must be a valid writable pointer to a `CTrackedGridRef`.
pub unsafe fn grid_ref_track(
    terminal_: Terminal,
    pt: PointC,
    out_ref: *mut CTrackedGridRef,
) -> Result {
    // const wrapper = terminal_ orelse return .invalid_value;
    if terminal_.is_null() {
        return Result::InvalidValue;
    }
    // const out = out_ref orelse return .invalid_value;
    if out_ref.is_null() {
        return Result::InvalidValue;
    }
    // out.* = null;
    // SAFETY: `out_ref` is a non-null valid writable `CTrackedGridRef`.
    unsafe { *out_ref = std::ptr::null_mut() };

    // const t: *ZigTerminal = wrapper.terminal;
    // SAFETY: `terminal_` is a live wrapper; raw-ptr reads only.
    let t: *mut ZigTerminal = unsafe { (*terminal_).terminal };
    let active: *mut rustty_vt::screen::Screen = unsafe { (*t).screens.active };
    // const list = &t.screens.active.pages;
    let list: *mut rustty_vt::pagelist::PageList =
        unsafe { core::ptr::addr_of_mut!((*active).pages) };
    // const p = list.pin(.fromC(pt)) orelse return .invalid_value;
    // D11 (P3-W6 T2): `pt.tag` is the raw C integer; `from_c` narrows it.
    let zig_pt = match pt.from_c() {
        Some(p) => p,
        None => return Result::InvalidValue,
    };
    let p = match unsafe { (*list).pin(zig_pt) } {
        Some(p) => p,
        None => return Result::InvalidValue,
    };
    // const tracked_pin = list.trackPin(p) catch return .out_of_memory;
    let tracked_pin = match unsafe { (*list).track_pin(p) } {
        Ok(tp) => tp,
        Err(_) => return Result::OutOfMemory,
    };

    // const alloc = t.gpa();
    // PORT NOTE (parity S179-F3, S7-3): the reference's `t.gpa()` is the
    // caller's allocator (`t.init(alloc, ..)`), and since S7-3 so is the
    // port's (the tracked pin above came from the terminal's pin pool on it).
    // The ref takes the wrapper's copy of that C allocator
    // (`TerminalWrapper.c_alloc`), read through the raw wrapper pointer (no
    // reference to `*t` is formed; Miri SB+TB), and keeps its own copy, so it
    // can be freed after the terminal.
    let alloc: Option<CAllocator> = unsafe { (*terminal_).c_alloc };
    let active_key = unsafe { (*t).screens.active_key };
    let generation = unsafe { (*t).screens.generation(active_key) };
    // const ref = alloc.create(grid_ref_tracked_c.TrackedGridRef) catch { list.untrackPin(tracked_pin); return .out_of_memory; };
    // ref.* = .{ .alloc, .terminal = wrapper, .screen_key, .screen_generation, .pin = tracked_pin };
    // SAFETY: `alloc`, if set, is the caller's allocator (C contract: valid
    // for as long as objects created with it live).
    let ref_ptr: *mut TrackedGridRef = unsafe {
        crate::cwrap::create(
            alloc.as_ref(),
            TrackedGridRef {
                alloc,
                terminal: terminal_,
                screen_key: active_key,
                screen_generation: generation,
                pin: tracked_pin,
            },
        )
    };
    if ref_ptr.is_null() {
        // SAFETY: `list` is the live active PageList that tracked the pin.
        unsafe { (*list).untrack_pin(tracked_pin) };
        return Result::OutOfMemory;
    }

    // Store the tracked ref in the terminal so that when we free
    // the terminal the tracked ref can be detached safely.
    // wrapper.tracked_grid_refs.putNoClobber(alloc, ref, {}) catch {
    //     list.untrackPin(tracked_pin);
    //     alloc.destroy(ref);
    //     return .out_of_memory;
    // };
    // PORT NOTE (parity S179-F3, parity tail S7-4): `try_put_no_clobber` is
    // the fallible `putNoClobber` (the key is a fresh heap pointer, so it is
    // absent), so the reference's `catch` arm is reachable, as in
    // `search::new` (S4-F3). The registry grows through the terminal's
    // allocator, bound in `wrap`.
    #[cfg(test)]
    let reserved = track_tw_check().and_then(|()| unsafe {
        (*terminal_)
            .tracked_grid_refs
            .try_put_no_clobber(ref_ptr, ())
            .map_err(|_| TrackRegistryFull)
    });
    #[cfg(not(test))]
    let reserved = unsafe {
        (*terminal_)
            .tracked_grid_refs
            .try_put_no_clobber(ref_ptr, ())
    };
    if reserved.is_err() {
        // SAFETY: as above; `ref_ptr` was just created with `alloc` and has
        // not been handed out.
        unsafe { (*list).untrack_pin(tracked_pin) };
        unsafe { crate::cwrap::destroy(alloc.as_ref(), ref_ptr) };
        return Result::OutOfMemory;
    }

    // out.* = ref;
    unsafe { *out_ref = ref_ptr };
    // return .success;
    Result::Success
}

// PORT NOTE: tripwire fail-point for the tracked-ref registry's growth in
// `grid_ref_track` (`wrapper.tracked_grid_refs.putNoClobber(alloc, ref, {})`),
// standing in for `std.testing.FailingAllocator`, as `search.rs`'s
// `REGISTER_TW` did for S4-F3 until parity tail S7-6 drove that arm with a
// `FailingAllocator` under the terminal (parity S179-F3).
#[cfg(test)]
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) enum TrackFp {
    Reserve,
}

/// The tripwire's error value (Zig `Allocator.Error`).
#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TrackRegistryFull;

#[cfg(test)]
thread_local! {
    pub(crate) static TRACK_TW: core::cell::RefCell<
        rustty_util::tripwire::Module<TrackFp, TrackRegistryFull>,
    > = core::cell::RefCell::new(rustty_util::tripwire::module());
}

#[cfg(test)]
#[inline]
fn track_tw_check() -> core::result::Result<(), TrackRegistryFull> {
    TRACK_TW.with(|tw| tw.borrow_mut().check(TrackFp::Reserve))
}

// pub fn point_from_grid_ref(terminal_, ref: *const grid_ref_c.CGridRef, tag: point.Tag, out: ?*point.Coordinate) ... {
//     const t: *ZigTerminal = (terminal_ orelse return .invalid_value).terminal;
//     const p = ref.toPin() orelse return .invalid_value;
//     const pt = t.screens.active.pages.pointFromPin(tag, p) orelse return .no_value;
//     if (out) |o| o.* = pt.coord();
//     return .success;
// }
/// # Safety
/// `terminal_`, if non-null, must be a live terminal handle; `ref` must point to
/// a valid `CGridRef`; `out`, if non-null, must be a valid writable pointer to a
/// `Coordinate`.
pub unsafe fn point_from_grid_ref(
    terminal_: Terminal,
    ref_: *const CGridRef,
    tag: Tag,
    out: *mut Coordinate,
) -> Result {
    // const t: *ZigTerminal = (terminal_ orelse return .invalid_value).terminal;
    if terminal_.is_null() {
        return Result::InvalidValue;
    }
    // SAFETY: `terminal_` is a live wrapper; raw-ptr reads only.
    let t: *mut ZigTerminal = unsafe { (*terminal_).terminal };
    let active: *mut rustty_vt::screen::Screen = unsafe { (*t).screens.active };
    // const p = ref.toPin() orelse return .invalid_value;
    let p = match unsafe { (*ref_).to_pin() } {
        Some(p) => p,
        None => return Result::InvalidValue,
    };
    // const pt = t.screens.active.pages.pointFromPin(tag, p) orelse return .no_value;
    let pt = match unsafe { (*active).pages.point_from_pin(tag, p) } {
        Some(pt) => pt,
        None => return Result::NoValue,
    };
    // if (out) |o| o.* = pt.coord();
    if !out.is_null() {
        // SAFETY: `out` is a valid writable `Coordinate`.
        unsafe { *out = pt.coord() };
    }
    // return .success;
    Result::Success
}

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
mod tests {
    use super::*;
    use crate::test_util::with_test_allocator;

    // PORT NOTE: the reference passes `&lib.alloc.test_allocator` to `new`.
    // rustty has no global test allocator (leaks are caught by Miri); the parity
    // analog is `with_test_allocator`, a C `Allocator` backed by the process
    // global. The reference `t.?.terminal.plainString(testing.allocator)` becomes
    // `(*(*t).terminal).plain_string(Global)`; the returned `Vec<u8>` frees on
    // drop (the reference's `testing.allocator.free(str)`).

    // Shared scaffold for the reference's repeated
    //   var t: Terminal = null;
    //   try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
    // preamble. It returns the `new` Result UNASSERTED so the reference's
    // `expectEqual(Result.success, ...)` ASSERTION lives in each test body
    // (assertion-for-assertion; G2 counts source-level asserts per test).
    fn new_t(
        alloc: &CAllocator,
        cols: size::CellCountInt,
        rows: size::CellCountInt,
    ) -> (Result, Terminal) {
        let mut t: Terminal = std::ptr::null_mut();
        let r = unsafe { new(alloc as *const CAllocator, &mut t, cols, rows) };
        (r, t)
    }

    fn plain_string(t: Terminal) -> Vec<u8> {
        // SAFETY: `t` is a live terminal from `new_t`.
        let zt = unsafe { &mut *(*t).terminal };
        zt.plain_string(rustty_vt::alloc::Global).unwrap()
    }

    // test "new/free"
    #[test]
    fn new_free() {
        with_test_allocator(|alloc| {
            // var t: Terminal = null;
            let mut t: Terminal = std::ptr::null_mut();
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, 80, 24));
            assert_eq!(Result::Success, unsafe {
                new(alloc as *const CAllocator, &mut t, 80, 24)
            });
            // try testing.expect(t != null);
            assert!(!t.is_null());
            // free(t);
            unsafe { free(t) };
        });
    }

    // PORT NOTE (F-008 proving tests, Rust-only — the reference has no unit
    // test for this; the contract is `new_`'s `alloc.create(...) catch return
    // error.OutOfMemory` + `free`'s `alloc.destroy`): the caller-supplied C
    // allocator must OWN the wrapper-level lifecycle storage.
    #[test]
    fn new_free_uses_caller_allocator() {
        let stats = crate::test_util::CountingCAllocator::new();
        let calloc = stats.c_allocator();
        let mut t: Terminal = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe {
            new(&calloc as *const CAllocator, &mut t, 80, 24)
        });
        assert!(!t.is_null());
        // At least the ZigTerminal + TerminalWrapper storage (Ghostty:
        // `alloc.create(ZigTerminal)` + `alloc.create(TerminalWrapper)`).
        assert!(
            stats.alloc_calls.get() >= 2,
            "terminal/wrapper storage must come from the caller allocator (got {} allocs)",
            stats.alloc_calls.get()
        );
        unsafe { free(t) };
        // free must return every caller-allocator byte (`alloc.destroy` x2).
        assert!(stats.free_calls.get() >= 2);
        assert_eq!(
            0,
            stats.live_bytes.get(),
            "free must balance the caller allocator"
        );
    }

    // Parity S174-F3 (no reference test): the stream's OSC capture allocates
    // through the terminal's caller allocator (`.initAlloc(alloc, handler)`),
    // so a long OSC 52 grows through it and `free` returns every byte.
    #[test]
    fn stream_osc_capture_uses_the_caller_allocator() {
        let stats = crate::test_util::CountingCAllocator::new();
        let calloc = stats.c_allocator();
        let mut t: Terminal = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe {
            new(&calloc as *const CAllocator, &mut t, 80, 24)
        });
        let before = stats.alloc_calls.get();
        let mut osc = b"\x1B]52;c;".to_vec();
        osc.extend(std::iter::repeat_n(b'A', rustty_vt::osc::MAX_BUF * 2));
        osc.extend_from_slice(b"\x1B\\");
        vtw(t, &osc);
        // The capture's initial MAX_BUF and at least one growth.
        assert!(
            stats.alloc_calls.get() >= before + 2,
            "the OSC capture went elsewhere ({} -> {} allocs)",
            before,
            stats.alloc_calls.get()
        );
        unsafe { free(t) };
        assert_eq!(0, stats.live_bytes.get());
    }

    #[test]
    fn new_rejecting_allocator_returns_out_of_memory() {
        let stats = crate::test_util::CountingCAllocator::new();
        stats.reject.set(true);
        let calloc = stats.c_allocator();
        let mut t: Terminal = std::ptr::null_mut();
        // Ghostty: a rejecting caller allocator makes `alloc.create` fail and
        // `new` return `.out_of_memory` with `result.* = null` — NOT a silent
        // success on some other heap.
        assert_eq!(Result::OutOfMemory, unsafe {
            new(&calloc as *const CAllocator, &mut t, 80, 24)
        });
        assert!(t.is_null());
        assert!(stats.alloc_calls.get() >= 1);
        assert_eq!(0, stats.live_bytes.get());
    }

    // test "new invalid value"
    #[test]
    fn new_invalid_value() {
        with_test_allocator(|alloc| {
            let mut t: Terminal = std::ptr::null_mut();
            // cols == 0
            assert_eq!(Result::InvalidValue, unsafe {
                new(alloc as *const CAllocator, &mut t, 0, 24)
            });
            assert!(t.is_null());

            // rows == 0
            assert_eq!(Result::InvalidValue, unsafe {
                new(alloc as *const CAllocator, &mut t, 80, 0)
            });
            assert!(t.is_null());
        });
    }

    // fn testEnableContinuation(terminal: Terminal) !void {
    fn test_enable_continuation(terminal: Terminal) {
        let limit: usize = 1024;
        assert_eq!(Result::Success, unsafe {
            set(
                terminal,
                Option_::ContinuationMaxBytes,
                &limit as *const usize as *const c_void,
            )
        });
    }

    // test "continuation option and data"
    #[test]
    fn continuation_option_and_data() {
        with_test_allocator(|alloc| {
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);

            let mut value: usize = 0;
            assert_eq!(Result::Success, unsafe {
                get(
                    t,
                    TerminalData::ContinuationMaxBytes,
                    &mut value as *mut usize as *mut c_void,
                )
            });
            assert_eq!(DEFAULT_CONTINUATION_MAX_BYTES, value);

            let custom: usize = 1234;
            assert_eq!(Result::Success, unsafe {
                set(
                    t,
                    Option_::ContinuationMaxBytes,
                    &custom as *const usize as *const c_void,
                )
            });
            assert_eq!(Result::Success, unsafe {
                get(
                    t,
                    TerminalData::ContinuationMaxBytes,
                    &mut value as *mut usize as *mut c_void,
                )
            });
            assert_eq!(custom, value);

            assert_eq!(Result::Success, unsafe {
                set(t, Option_::ContinuationMaxBytes, std::ptr::null())
            });
            assert_eq!(Result::Success, unsafe {
                get(
                    t,
                    TerminalData::ContinuationMaxBytes,
                    &mut value as *mut usize as *mut c_void,
                )
            });
            assert_eq!(DEFAULT_CONTINUATION_MAX_BYTES, value);

            let disabled: usize = 0;
            assert_eq!(Result::Success, unsafe {
                set(
                    t,
                    Option_::ContinuationMaxBytes,
                    &disabled as *const usize as *const c_void,
                )
            });
            assert_eq!(Result::Success, unsafe {
                get(
                    t,
                    TerminalData::ContinuationMaxBytes,
                    &mut value as *mut usize as *mut c_void,
                )
            });
            assert_eq!(0usize, value);

            let mut written: usize = 99;
            assert_eq!(Result::InvalidValue, unsafe {
                continuation_buf(t, std::ptr::null_mut(), 0, &mut written)
            });
            assert_eq!(0usize, written);
            unsafe { free(t) };
        });
    }

    // test "continuation buffer and allocator export exact suffix"
    #[test]
    fn continuation_buffer_and_allocator_export_exact_suffix() {
        with_test_allocator(|alloc| {
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);
            test_enable_continuation(t);

            vtw(t, b"A\x1b[31");

            let mut required: usize = 0;
            assert_eq!(Result::OutOfSpace, unsafe {
                continuation_buf(t, std::ptr::null_mut(), 0, &mut required)
            });
            assert_eq!(4usize, required);

            let mut short = [0u8; 2];
            assert_eq!(Result::OutOfSpace, unsafe {
                continuation_buf(t, short.as_mut_ptr(), short.len(), &mut required)
            });
            assert_eq!(4usize, required);

            let mut buf = [0u8; 8];
            let mut written: usize = 0;
            assert_eq!(Result::Success, unsafe {
                continuation_buf(t, buf.as_mut_ptr(), buf.len(), &mut written)
            });
            assert_eq!(b"\x1b[31", &buf[..written]);

            let mut out_ptr: *mut u8 = std::ptr::null_mut();
            let mut out_len: usize = 0;
            assert_eq!(Result::Success, unsafe {
                continuation_alloc(t, Some(alloc), &mut out_ptr, &mut out_len)
            });
            // const allocated = (out_ptr orelse return error.TestExpectedEqual)[0..out_len];
            assert!(!out_ptr.is_null());
            // SAFETY: `out_ptr[0..out_len]` is the allocation returned above.
            let allocated = unsafe { core::slice::from_raw_parts(out_ptr, out_len) };
            // try testing.expectEqualStrings("\x1b[31", allocated);
            assert_eq!(b"\x1b[31", allocated);
            // defer lib.alloc.default(&test_allocator).free(allocated);
            rustty_lib::allocator::default(Some(alloc)).free(
                rustty_lib::allocator::ZigSlice::new(out_ptr, out_len),
                rustty_lib::allocator::Alignment::from_byte_units(1),
                0,
            );

            vtw(t, b"m");
            assert_eq!(Result::OutOfSpace, unsafe {
                continuation_buf(t, std::ptr::null_mut(), 0, &mut required)
            });
            assert_eq!(0usize, required);

            // Completing the sequence must return an empty, allocation-free result.
            // const failing: CAllocator = .fromZig(&std.mem.Allocator.failing);
            let failing_zig = rustty_lib::allocator::FailingZigAllocator;
            let failing_ref = rustty_lib::allocator::ZigAllocatorRef::new(&failing_zig);
            let failing = rustty_lib::allocator::Allocator::from_zig(&failing_ref);
            // try testing.expectEqual(Result.success, continuation_alloc(t, &failing, &out_ptr, &out_len));
            assert_eq!(Result::Success, unsafe {
                continuation_alloc(t, Some(&failing), &mut out_ptr, &mut out_len)
            });
            // defer @import("allocator.zig").free(&failing, out_ptr, out_len);
            // PORT NOTE: the deferred free runs after the assertions below.
            // try testing.expectEqual(null, out_ptr);
            assert!(out_ptr.is_null());
            // try testing.expectEqual(@as(usize, 0), out_len);
            assert_eq!(0usize, out_len);
            unsafe { crate::allocator::free(Some(&failing), out_ptr, out_len) };
            unsafe { free(t) };
        });
    }

    // test "continuation export tracks split UTF-8"
    #[test]
    fn continuation_export_tracks_split_utf8() {
        with_test_allocator(|alloc| {
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);
            test_enable_continuation(t);

            let prefix = [0xF0u8, 0x9F];
            vtw(t, &prefix);

            let mut buf = [0u8; 4];
            let mut written: usize = 0;
            assert_eq!(Result::Success, unsafe {
                continuation_buf(t, buf.as_mut_ptr(), buf.len(), &mut written)
            });
            assert_eq!(&prefix[..], &buf[..written]);

            let suffix = [0x98u8, 0x80];
            vtw(t, &suffix);
            assert_eq!(Result::OutOfSpace, unsafe {
                continuation_buf(t, std::ptr::null_mut(), 0, &mut written)
            });
            assert_eq!(0usize, written);
            unsafe { free(t) };
        });
    }

    // test "continuation callback writer reports success and failure"
    #[test]
    fn continuation_callback_writer_reports_success_and_failure() {
        struct Sink {
            bytes: [u8; 8],
            len: usize,
        }
        extern "C" fn sink_write(userdata: *mut c_void, data: *const u8, len: usize) -> bool {
            let sink = unsafe { &mut *(userdata as *mut Sink) };
            if len > sink.bytes.len() - sink.len {
                return false;
            }
            unsafe {
                std::ptr::copy_nonoverlapping(data, sink.bytes[sink.len..].as_mut_ptr(), len)
            };
            sink.len += len;
            true
        }
        extern "C" fn sink_fail(_: *mut c_void, _: *const u8, _: usize) -> bool {
            false
        }

        with_test_allocator(|alloc| {
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);
            test_enable_continuation(t);
            vtw(t, b"\x1b[31");

            let mut sink = Sink {
                bytes: [0; 8],
                len: 0,
            };
            assert_eq!(Result::Success, unsafe {
                continuation_write(
                    t,
                    crate::io::Writer {
                        write: Some(sink_write),
                        userdata: &mut sink as *mut Sink as *mut c_void,
                    },
                )
            });
            assert_eq!(b"\x1b[31", &sink.bytes[..sink.len]);

            assert_eq!(Result::IoError, unsafe {
                continuation_write(
                    t,
                    crate::io::Writer {
                        write: Some(sink_fail),
                        userdata: std::ptr::null_mut(),
                    },
                )
            });
            assert_eq!(Result::InvalidValue, unsafe {
                continuation_write(t, crate::io::Writer::default())
            });
            assert_eq!(Result::InvalidValue, unsafe {
                continuation_write(
                    std::ptr::null_mut(),
                    crate::io::Writer {
                        write: Some(sink_fail),
                        userdata: std::ptr::null_mut(),
                    },
                )
            });
            unsafe { free(t) };
        });
    }

    // test "continuation runtime reconfiguration recovers safely"
    #[test]
    fn continuation_runtime_reconfiguration_recovers_safely() {
        with_test_allocator(|alloc| {
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);

            let enough: usize = 8;
            assert_eq!(Result::Success, unsafe {
                set(
                    t,
                    Option_::ContinuationMaxBytes,
                    &enough as *const usize as *const c_void,
                )
            });
            vtw(t, b"\x1b[123");
            let too_small: usize = 2;
            assert_eq!(Result::Success, unsafe {
                set(
                    t,
                    Option_::ContinuationMaxBytes,
                    &too_small as *const usize as *const c_void,
                )
            });

            let mut written: usize = 99;
            assert_eq!(Result::InvalidValue, unsafe {
                continuation_buf(t, std::ptr::null_mut(), 0, &mut written)
            });
            assert_eq!(0usize, written);

            // Raising the limit cannot reconstruct discarded bytes, but a new ESC is
            // a complete replay start and repairs tracking without first grounding.
            assert_eq!(Result::Success, unsafe {
                set(
                    t,
                    Option_::ContinuationMaxBytes,
                    &enough as *const usize as *const c_void,
                )
            });
            vtw(t, b"\x1b[");
            let mut buf = [0u8; 8];
            assert_eq!(Result::Success, unsafe {
                continuation_buf(t, buf.as_mut_ptr(), buf.len(), &mut written)
            });
            assert_eq!(b"\x1b[", &buf[..written]);

            let disabled: usize = 0;
            assert_eq!(Result::Success, unsafe {
                set(
                    t,
                    Option_::ContinuationMaxBytes,
                    &disabled as *const usize as *const c_void,
                )
            });
            assert_eq!(Result::Success, unsafe {
                set(
                    t,
                    Option_::ContinuationMaxBytes,
                    &enough as *const usize as *const c_void,
                )
            });
            assert_eq!(Result::InvalidValue, unsafe {
                continuation_buf(t, std::ptr::null_mut(), 0, &mut written)
            });

            // Completing the sequence reaches ground and resets the broken marker.
            vtw(t, b"m");
            assert_eq!(Result::OutOfSpace, unsafe {
                continuation_buf(t, std::ptr::null_mut(), 0, &mut written)
            });
            assert_eq!(0usize, written);
            unsafe { free(t) };
        });
    }

    // test "continuation internal allocation and restoration"
    #[test]
    fn continuation_internal_allocation_and_restoration() {
        with_test_allocator(|alloc| {
            let (new_result, source) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);
            test_enable_continuation(source);
            vtw(source, b"\x1b[31");

            // const alloc = lib.alloc.default(&lib.alloc.test_allocator);
            // const bytes = try continuationAllocIo(source, alloc, false);
            let default_alloc = unsafe { rustty_vt::alloc::default(Some(alloc)) };
            let bytes = unsafe { continuation_alloc_io(source, default_alloc, false) }.unwrap();
            assert_eq!(b"\x1b[31", bytes.as_slice());

            let (new_result, restored) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);
            test_enable_continuation(restored);
            unsafe { restore_continuation(restored, &bytes) }.unwrap();

            let reexported =
                unsafe { continuation_alloc_io(restored, default_alloc, false) }.unwrap();
            assert_eq!(bytes, reexported);

            let (new_result, invalid) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);
            test_enable_continuation(invalid);
            assert_eq!(Err(RestoreContinuationError::InvalidContinuation), unsafe {
                restore_continuation(invalid, b"A")
            });
            unsafe { free(invalid) };
            unsafe { free(restored) };
            unsafe { free(source) };
        });
    }

    // test "set scrollback limits"
    #[test]
    fn set_scrollback_limits() {
        with_test_allocator(|alloc| {
            let (new_result, t) = new_t(alloc, 80, 3);
            assert_eq!(Result::Success, new_result);

            // const primary = t.?.terminal.screens.get(.primary).?;
            let primary: *mut rustty_vt::screen::Screen = unsafe {
                (*(*t).terminal)
                    .screens
                    .get(rustty_vt::screen_set::Key::Primary)
                    .unwrap()
            };

            let max_bytes: usize = 0;
            assert_eq!(Result::Success, unsafe {
                set(
                    t,
                    Option_::ScrollbackMaxBytes,
                    &max_bytes as *const usize as *const c_void,
                )
            });
            assert_eq!(0usize, unsafe { (*primary).pages.limits.bytes.explicit });
            assert!(unsafe { (*primary).no_scrollback });

            assert_eq!(Result::Success, unsafe {
                set(t, Option_::ScrollbackMaxBytes, std::ptr::null())
            });
            assert_eq!(usize::MAX, unsafe {
                (*primary).pages.limits.bytes.explicit
            });
            assert!(!unsafe { (*primary).no_scrollback });

            let max_lines: usize = 12;
            assert_eq!(Result::Success, unsafe {
                set(
                    t,
                    Option_::ScrollbackMaxLines,
                    &max_lines as *const usize as *const c_void,
                )
            });
            assert_eq!(max_lines, unsafe { (*primary).pages.limits.lines.explicit });

            assert_eq!(Result::Success, unsafe {
                set(t, Option_::ScrollbackMaxLines, std::ptr::null())
            });
            assert_eq!(usize::MAX, unsafe {
                (*primary).pages.limits.lines.explicit
            });

            unsafe { free(t) };
        });
    }

    // test "get configured scrollback limits"
    #[test]
    fn get_configured_scrollback_limits() {
        with_test_allocator(|alloc| {
            let (new_result, t) = new_t(alloc, 80, 3);
            assert_eq!(Result::Success, new_result);

            let mut value: usize = 0;
            assert_eq!(Result::Success, unsafe {
                get(
                    t,
                    TerminalData::ScrollbackMaxBytes,
                    &mut value as *mut usize as *mut c_void,
                )
            });
            assert_eq!(10_000usize, value);
            assert_eq!(Result::NoValue, unsafe {
                get(
                    t,
                    TerminalData::ScrollbackMaxLines,
                    &mut value as *mut usize as *mut c_void,
                )
            });

            let max_bytes: usize = 0;
            let max_lines: usize = 12;
            assert_eq!(Result::Success, unsafe {
                set(
                    t,
                    Option_::ScrollbackMaxBytes,
                    &max_bytes as *const usize as *const c_void,
                )
            });
            assert_eq!(Result::Success, unsafe {
                set(
                    t,
                    Option_::ScrollbackMaxLines,
                    &max_lines as *const usize as *const c_void,
                )
            });
            assert_eq!(Result::Success, unsafe {
                get(
                    t,
                    TerminalData::ScrollbackMaxBytes,
                    &mut value as *mut usize as *mut c_void,
                )
            });
            assert_eq!(max_bytes, value);
            assert_eq!(Result::Success, unsafe {
                get(
                    t,
                    TerminalData::ScrollbackMaxLines,
                    &mut value as *mut usize as *mut c_void,
                )
            });
            assert_eq!(max_lines, value);

            // The configured limits belong to the primary screen and remain
            // readable while an alternate screen is active.
            unsafe { vt_write(t, b"\x1b[?1049h".as_ptr(), 8) };
            assert_eq!(Result::Success, unsafe {
                get(
                    t,
                    TerminalData::ScrollbackMaxBytes,
                    &mut value as *mut usize as *mut c_void,
                )
            });
            assert_eq!(max_bytes, value);
            assert_eq!(Result::Success, unsafe {
                get(
                    t,
                    TerminalData::ScrollbackMaxLines,
                    &mut value as *mut usize as *mut c_void,
                )
            });
            assert_eq!(max_lines, value);

            assert_eq!(Result::Success, unsafe {
                set(t, Option_::ScrollbackMaxBytes, std::ptr::null())
            });
            assert_eq!(Result::Success, unsafe {
                set(t, Option_::ScrollbackMaxLines, std::ptr::null())
            });
            assert_eq!(Result::NoValue, unsafe {
                get(
                    t,
                    TerminalData::ScrollbackMaxBytes,
                    &mut value as *mut usize as *mut c_void,
                )
            });
            assert_eq!(Result::NoValue, unsafe {
                get(
                    t,
                    TerminalData::ScrollbackMaxLines,
                    &mut value as *mut usize as *mut c_void,
                )
            });

            unsafe { free(t) };
        });
    }

    // test "free null"
    #[test]
    fn free_null() {
        unsafe { free(std::ptr::null_mut()) };
    }

    // test "scroll_viewport"
    #[test]
    fn scroll_viewport_test() {
        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 5, 2);
            assert_eq!(Result::Success, new_result);

            let mut viewport_active: bool = false;
            assert_eq!(Result::Success, unsafe {
                get(
                    t,
                    TerminalData::ViewportActive,
                    &mut viewport_active as *mut bool as *mut c_void,
                )
            });
            assert!(viewport_active);

            // Write "hello" on the first line
            unsafe { vt_write(t, b"hello".as_ptr(), 5) };

            // Push "hello" into scrollback with 3 newlines (index = ESC D)
            unsafe { vt_write(t, b"\x1bD\x1bD\x1bD".as_ptr(), 6) };
            // Viewport should be empty now since hello scrolled off
            assert_eq!(b"", plain_string(t).as_slice());

            // Scroll to top: "hello" should be visible again
            unsafe {
                scroll_viewport(
                    t,
                    ScrollViewportC {
                        tag: ScrollViewportTag::Top as c_int,
                        value: ScrollViewportCValue { _padding: [0; 2] },
                    },
                )
            };
            assert_eq!(Result::Success, unsafe {
                get(
                    t,
                    TerminalData::ViewportActive,
                    &mut viewport_active as *mut bool as *mut c_void,
                )
            });
            assert!(!viewport_active);
            assert_eq!(b"hello", plain_string(t).as_slice());

            // Scroll to bottom: viewport should be empty again
            unsafe {
                scroll_viewport(
                    t,
                    ScrollViewportC {
                        tag: ScrollViewportTag::Bottom as c_int,
                        value: ScrollViewportCValue { _padding: [0; 2] },
                    },
                )
            };
            assert_eq!(Result::Success, unsafe {
                get(
                    t,
                    TerminalData::ViewportActive,
                    &mut viewport_active as *mut bool as *mut c_void,
                )
            });
            assert!(viewport_active);
            assert_eq!(b"", plain_string(t).as_slice());

            // Scroll up by delta to bring "hello" back into view
            unsafe {
                scroll_viewport(
                    t,
                    ScrollViewportC {
                        tag: ScrollViewportTag::Delta as c_int,
                        value: ScrollViewportCValue { delta: -3 },
                    },
                )
            };
            assert_eq!(Result::Success, unsafe {
                get(
                    t,
                    TerminalData::ViewportActive,
                    &mut viewport_active as *mut bool as *mut c_void,
                )
            });
            assert!(!viewport_active);
            assert_eq!(b"hello", plain_string(t).as_slice());

            unsafe { free(t) };
        });
    }

    // test "scroll_viewport null"
    #[test]
    fn scroll_viewport_null() {
        unsafe {
            scroll_viewport(
                std::ptr::null_mut(),
                ScrollViewportC {
                    tag: ScrollViewportTag::Top as c_int,
                    value: ScrollViewportCValue { _padding: [0; 2] },
                },
            )
        };
        // scroll_viewport(null, .{ .tag = .row, .value = .{ .row = 1 } });
        //
        // FIX (S180-F1): the second call was dropped. It is the one whose union
        // payload is READ, so it is the call that exercises the null guard for a
        // tag the implementation must decode.
        unsafe {
            scroll_viewport(
                std::ptr::null_mut(),
                ScrollViewportC {
                    tag: ScrollViewportTag::Row as c_int,
                    value: ScrollViewportCValue { row: 1 },
                },
            )
        };
    }

    // PROVING TEST (W35-B07; inverted by task R13-D, UD113): no reference
    // analog. The reference switches directly on the typed tag, so its
    // safety-checked builds panic "switch on corrupt value" on an undeclared
    // tag while ReleaseFast takes an arbitrary arm. Under R13 the port takes
    // the defined silent return in EVERY build, debug included (this test used
    // to require a debug-build panic). `catch_unwind` (not `#[should_panic]`)
    // so a regression still frees the terminal and fails on the assertion.
    #[test]
    fn scroll_viewport_invalid_tag() {
        with_test_allocator(|alloc| {
            let (new_result, t) = new_t(alloc, 5, 2);
            assert_eq!(Result::Success, new_result);

            // A valid call must still work: "hello" into scrollback, then Top
            // brings it back into view.
            unsafe { vt_write(t, b"hello".as_ptr(), 5) };
            unsafe { vt_write(t, b"\x1bD\x1bD\x1bD".as_ptr(), 6) };
            assert_eq!(b"", plain_string(t).as_slice());
            unsafe {
                scroll_viewport(
                    t,
                    ScrollViewportC {
                        tag: ScrollViewportTag::Top as c_int,
                        value: ScrollViewportCValue { _padding: [0; 2] },
                    },
                )
            };
            assert_eq!(b"hello", plain_string(t).as_slice());

            // Undeclared tags, including negative ones and the first value past
            // `row`: every build returns silently with the viewport untouched.
            for tag in [c_int::MAX, -1, ScrollViewportTag::Row as c_int + 1] {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
                    scroll_viewport(
                        t,
                        ScrollViewportC {
                            tag,
                            value: ScrollViewportCValue { _padding: [0; 2] },
                        },
                    )
                }));
                assert!(
                    result.is_ok(),
                    "an undeclared tag ({tag}) from the C caller must not panic (R13, UD113)"
                );
                // The viewport is untouched: "hello" still visible.
                assert_eq!(b"hello", plain_string(t).as_slice());
            }

            // The positive control after the invalid calls: a declared tag
            // still acts (Bottom scrolls "hello" back out of view).
            unsafe {
                scroll_viewport(
                    t,
                    ScrollViewportC {
                        tag: ScrollViewportTag::Bottom as c_int,
                        value: ScrollViewportCValue { _padding: [0; 2] },
                    },
                )
            };
            assert_eq!(b"", plain_string(t).as_slice());

            unsafe { free(t) };
        });
    }

    // test "scroll_viewport row" (oracle 3a2e28329)
    #[test]
    fn scroll_viewport_row_test() {
        with_test_allocator(|alloc| {
            let (new_result, t) = new_t(alloc, 5, 2);
            assert_eq!(Result::Success, new_result);

            // Write 4 rows so rows "1" and "2" are pushed into scrollback:
            // total rows is 4, viewport length is 2.
            unsafe { vt_write(t, b"1\r\n2\r\n3\r\n4".as_ptr(), 10) };

            let mut viewport_active: bool = false;
            assert_eq!(Result::Success, unsafe {
                get(
                    t,
                    TerminalData::ViewportActive,
                    &mut viewport_active as *mut bool as *mut c_void,
                )
            });
            assert!(viewport_active);

            // Row 0 is the top of the scrollback.
            unsafe {
                scroll_viewport(
                    t,
                    ScrollViewportC {
                        tag: ScrollViewportTag::Row as c_int,
                        value: ScrollViewportCValue { row: 0 },
                    },
                )
            };
            assert_eq!(Result::Success, unsafe {
                get(
                    t,
                    TerminalData::ViewportActive,
                    &mut viewport_active as *mut bool as *mut c_void,
                )
            });
            assert!(!viewport_active);
            assert_eq!(b"1\n2", plain_string(t).as_slice());

            // An absolute row within the scrollback becomes the first visible
            // row and round-trips through the scrollbar offset.
            unsafe {
                scroll_viewport(
                    t,
                    ScrollViewportC {
                        tag: ScrollViewportTag::Row as c_int,
                        value: ScrollViewportCValue { row: 1 },
                    },
                )
            };
            assert_eq!(b"2\n3", plain_string(t).as_slice());

            let mut scrollbar_data: TerminalScrollbar = TerminalScrollbar {
                total: 0,
                offset: 0,
                len: 0,
            };
            assert_eq!(Result::Success, unsafe {
                get(
                    t,
                    TerminalData::Scrollbar,
                    &mut scrollbar_data as *mut TerminalScrollbar as *mut c_void,
                )
            });
            assert_eq!(4, scrollbar_data.total);
            assert_eq!(1, scrollbar_data.offset);
            assert_eq!(2, scrollbar_data.len);

            // A row past the end clamps to the active area.
            unsafe {
                scroll_viewport(
                    t,
                    ScrollViewportC {
                        tag: ScrollViewportTag::Row as c_int,
                        value: ScrollViewportCValue { row: 9999 },
                    },
                )
            };
            assert_eq!(Result::Success, unsafe {
                get(
                    t,
                    TerminalData::ViewportActive,
                    &mut viewport_active as *mut bool as *mut c_void,
                )
            });
            assert!(viewport_active);
            assert_eq!(b"3\n4", plain_string(t).as_slice());
            assert_eq!(Result::Success, unsafe {
                get(
                    t,
                    TerminalData::Scrollbar,
                    &mut scrollbar_data as *mut TerminalScrollbar as *mut c_void,
                )
            });
            assert_eq!(2, scrollbar_data.offset);

            unsafe { free(t) };
        });
    }

    // test "scroll_viewport row alt screen" (oracle 3a2e28329)
    #[test]
    fn scroll_viewport_row_alt_screen_test() {
        with_test_allocator(|alloc| {
            let (new_result, t) = new_t(alloc, 5, 2);
            assert_eq!(Result::Success, new_result);

            // Enter the alternate screen, which has no scrollback.
            unsafe { vt_write(t, b"\x1b[?1049h".as_ptr(), 8) };
            let mut screen: TerminalScreen = TerminalScreen::Primary;
            assert_eq!(Result::Success, unsafe {
                get(
                    t,
                    TerminalData::ActiveScreen,
                    &mut screen as *mut TerminalScreen as *mut c_void,
                )
            });
            assert_eq!(TerminalScreen::Alternate, screen);

            // Scrolling to any row keeps the viewport on the active area.
            let mut viewport_active: bool = false;
            unsafe {
                scroll_viewport(
                    t,
                    ScrollViewportC {
                        tag: ScrollViewportTag::Row as c_int,
                        value: ScrollViewportCValue { row: 0 },
                    },
                )
            };
            assert_eq!(Result::Success, unsafe {
                get(
                    t,
                    TerminalData::ViewportActive,
                    &mut viewport_active as *mut bool as *mut c_void,
                )
            });
            assert!(viewport_active);
            unsafe {
                scroll_viewport(
                    t,
                    ScrollViewportC {
                        tag: ScrollViewportTag::Row as c_int,
                        value: ScrollViewportCValue { row: 9999 },
                    },
                )
            };
            assert_eq!(Result::Success, unsafe {
                get(
                    t,
                    TerminalData::ViewportActive,
                    &mut viewport_active as *mut bool as *mut c_void,
                )
            });
            assert!(viewport_active);

            // With no scrollback the scrollbar covers exactly the active area.
            let mut scrollbar_data: TerminalScrollbar = TerminalScrollbar {
                total: 0,
                offset: 0,
                len: 0,
            };
            assert_eq!(Result::Success, unsafe {
                get(
                    t,
                    TerminalData::Scrollbar,
                    &mut scrollbar_data as *mut TerminalScrollbar as *mut c_void,
                )
            });
            assert_eq!(2, scrollbar_data.total);
            assert_eq!(0, scrollbar_data.offset);
            assert_eq!(2, scrollbar_data.len);

            unsafe { free(t) };
        });
    }

    // test "compression invalid arguments" (oracle 172f15da3)
    #[test]
    fn compression_invalid_arguments() {
        with_test_allocator(|alloc| {
            // var activity: u64 = undefined;
            // var compression_result: CompressionResult = undefined;
            let mut activity: u64 = 0;
            let mut compression_result = CompressionResult::Unsupported;

            // try testing.expectEqual(Result.invalid_value, compression_activity(null, &activity));
            assert_eq!(Result::InvalidValue, unsafe {
                compression_activity(std::ptr::null_mut(), &mut activity)
            });
            // try testing.expectEqual(Result.invalid_value, compress(null, @intFromEnum(.incremental), &compression_result));
            assert_eq!(Result::InvalidValue, unsafe {
                compress(
                    std::ptr::null_mut(),
                    CompressionMode::Incremental as c_int,
                    &mut compression_result,
                )
            });

            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);

            // try testing.expectEqual(Result.invalid_value, compression_activity(t, null));
            assert_eq!(Result::InvalidValue, unsafe {
                compression_activity(t, std::ptr::null_mut())
            });
            // try testing.expectEqual(Result.invalid_value, compress(t, @intFromEnum(.incremental), null));
            assert_eq!(Result::InvalidValue, unsafe {
                compress(
                    t,
                    CompressionMode::Incremental as c_int,
                    std::ptr::null_mut(),
                )
            });
            // try testing.expectEqual(Result.invalid_value, compress(t, -1, &compression_result));
            assert_eq!(Result::InvalidValue, unsafe {
                compress(t, -1, &mut compression_result)
            });

            // defer free(t);
            unsafe { free(t) };
        });
    }

    // test "compression activity and incremental scheduling" (oracle 172f15da3)
    #[test]
    #[cfg_attr(
        miri,
        ignore = "MIRI EXCEPTION (rustty-vt-c, parity tail C-1): intractable under Miri, 4,000 lines of history written through the terminal, then an incremental compression pass; measured past a 20-minute bound; runs natively and under ASan (see MIRI_EXCEPTIONS.md)"
    )]
    fn compression_activity_and_incremental_scheduling() {
        with_test_allocator(|alloc| {
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);

            // var initial_activity: u64 = undefined;
            // try testing.expectEqual(Result.success, compression_activity(t, &initial_activity));
            let mut initial_activity: u64 = 0;
            assert_eq!(Result::Success, unsafe {
                compression_activity(t, &mut initial_activity)
            });

            // const line = "repeated and compressible terminal history\r\n";
            // const repeat = 4_000;
            // const input = try testing.allocator.alloc(u8, line.len * repeat);
            // for (0..repeat) |i| @memcpy(input[i * line.len ..][0..line.len], line);
            // vt_write(t, input.ptr, input.len);
            let line: &[u8] = b"repeated and compressible terminal history\r\n";
            const REPEAT: usize = 4_000;
            let mut input: Vec<u8> = Vec::with_capacity(line.len() * REPEAT);
            for _ in 0..REPEAT {
                input.extend_from_slice(line);
            }
            unsafe { vt_write(t, input.as_ptr(), input.len()) };

            // var activity: u64 = undefined;
            // try testing.expectEqual(Result.success, compression_activity(t, &activity));
            // try testing.expect(activity != initial_activity);
            let mut activity: u64 = 0;
            assert_eq!(Result::Success, unsafe {
                compression_activity(t, &mut activity)
            });
            assert_ne!(activity, initial_activity);

            // var compression_result: CompressionResult = undefined;
            // for (0..1_000) |_| { ... } else return error.TestUnexpectedResult;
            let mut compression_result = CompressionResult::Unsupported;
            let mut completed = false;
            for _ in 0..1_000 {
                assert_eq!(Result::Success, unsafe {
                    compress(
                        t,
                        CompressionMode::Incremental as c_int,
                        &mut compression_result,
                    )
                });

                match compression_result {
                    // .pending => continue,
                    CompressionResult::Pending => continue,
                    // .complete => break,
                    CompressionResult::Complete => {
                        completed = true;
                        break;
                    }
                    // .unsupported => unreachable,
                    CompressionResult::Unsupported => {
                        unreachable!("test builds always support strict reclamation")
                    }
                }
            }
            // The reference's `else` clause on the `for` loop: falling out
            // without a `break` is `error.TestUnexpectedResult`.
            assert!(completed);

            // Compression changes storage representation, not the activity token.
            // var final_activity: u64 = undefined;
            // try testing.expectEqual(Result.success, compression_activity(t, &final_activity));
            // try testing.expectEqual(activity, final_activity);
            let mut final_activity: u64 = 0;
            assert_eq!(Result::Success, unsafe {
                compression_activity(t, &mut final_activity)
            });
            assert_eq!(activity, final_activity);

            // try testing.expectEqual(Result.success, compress(t, @intFromEnum(.full), &compression_result));
            // try testing.expectEqual(CompressionResult.complete, compression_result);
            assert_eq!(Result::Success, unsafe {
                compress(t, CompressionMode::Full as c_int, &mut compression_result)
            });
            assert_eq!(CompressionResult::Complete, compression_result);

            // defer free(t);
            unsafe { free(t) };
        });
    }

    // test "reset"
    #[test]
    fn reset_test() {
        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);

            unsafe { vt_write(t, b"Hello".as_ptr(), 5) };
            unsafe { reset(t) };

            assert_eq!(b"", plain_string(t).as_slice());
            unsafe { free(t) };
        });
    }

    // test "reset null"
    #[test]
    fn reset_null() {
        unsafe { reset(std::ptr::null_mut()) };
    }

    // test "resize"
    #[test]
    fn resize_test() {
        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);

            assert_eq!(Result::Success, unsafe { resize(t, 40, 12, 9, 18) });
            assert_eq!(40, unsafe { (*(*t).terminal).cols });
            assert_eq!(12, unsafe { (*(*t).terminal).rows });
            unsafe { free(t) };
        });
    }

    // test "resize null"
    #[test]
    fn resize_null() {
        assert_eq!(Result::InvalidValue, unsafe {
            resize(std::ptr::null_mut(), 80, 24, 9, 18)
        });
    }

    // test "resize invalid value"
    #[test]
    fn resize_invalid_value() {
        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);

            assert_eq!(Result::InvalidValue, unsafe { resize(t, 0, 24, 9, 18) });
            assert_eq!(Result::InvalidValue, unsafe { resize(t, 80, 0, 9, 18) });
            unsafe { free(t) };
        });
    }

    // test "resize shrinks both axes with cursor at bottom"
    #[test]
    fn resize_shrinks_both_axes_with_cursor_at_bottom() {
        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);

            // CSI 24;1H -> park the cursor on the bottom row (1-based).
            let mv = b"\x1b[24;1H";
            unsafe { vt_write(t, mv.as_ptr(), mv.len()) };

            // Shrink both axes; pre-resize cursor.y sits past the new bottom row.
            // Previously this underflowed in PageList.resizeCols.
            assert_eq!(Result::Success, unsafe { resize(t, 79, 23, 8, 16) });
            assert_eq!(79, unsafe { (*(*t).terminal).cols });
            assert_eq!(23, unsafe { (*(*t).terminal).rows });
            unsafe { free(t) };
        });
    }

    // test "set and get mode"
    #[test]
    fn set_and_get_mode() {
        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);

            // DEC mode 25 (cursor_visible) defaults to true
            // const cursor_visible: modes.ModeTag.Backing = @bitCast(modes.ModeTag{ .value = 25, .ansi = false });
            let cursor_visible = modes::ModeTag {
                value: 25,
                ansi: false,
            }
            .to_backing();
            // var config: ModeConfig = .{ .mode = cursor_visible, .value = undefined };
            let mut config = ModeConfig {
                mode: cursor_visible,
                value: false,
            };
            // try testing.expectEqual(Result.success, get(t, .mode, @ptrCast(&config)));
            assert_eq!(Result::Success, unsafe {
                get(
                    t,
                    TerminalData::Mode,
                    &mut config as *mut ModeConfig as *mut c_void,
                )
            });
            // try testing.expect(config.value);
            assert!(config.value);

            // Set it to false
            config.value = false;
            assert_eq!(Result::Success, unsafe {
                set(
                    t,
                    Option_::Mode,
                    &config as *const ModeConfig as *const c_void,
                )
            });
            assert_eq!(Result::Success, unsafe {
                get(
                    t,
                    TerminalData::Mode,
                    &mut config as *mut ModeConfig as *mut c_void,
                )
            });
            assert!(!config.value);

            // ANSI mode 4 (insert) defaults to false
            let insert = modes::ModeTag {
                value: 4,
                ansi: true,
            }
            .to_backing();
            config.mode = insert;
            assert_eq!(Result::Success, unsafe {
                get(
                    t,
                    TerminalData::Mode,
                    &mut config as *mut ModeConfig as *mut c_void,
                )
            });
            assert!(!config.value);

            config.value = true;
            assert_eq!(Result::Success, unsafe {
                set(
                    t,
                    Option_::Mode,
                    &config as *const ModeConfig as *const c_void,
                )
            });
            assert_eq!(Result::Success, unsafe {
                get(
                    t,
                    TerminalData::Mode,
                    &mut config as *mut ModeConfig as *mut c_void,
                )
            });
            assert!(config.value);
            unsafe { free(t) };
        });
    }

    // test "set mode default updates current and reset value"
    #[test]
    fn set_mode_default_updates_current_and_reset_value() {
        with_test_allocator(|alloc| {
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);

            // const tag: modes.ModeTag.Backing = @bitCast(modes.ModeTag{ .value = 2027, .ansi = false });
            let tag = modes::ModeTag {
                value: 2027,
                ansi: false,
            }
            .to_backing();
            // var config: ModeConfig = .{ .mode = tag, .value = true };
            let mut config = ModeConfig {
                mode: tag,
                value: true,
            };
            assert_eq!(Result::Success, unsafe {
                set(
                    t,
                    Option_::ModeDefault,
                    &config as *const ModeConfig as *const c_void,
                )
            });
            // try testing.expect(t.?.terminal.modes.get(.grapheme_cluster));
            assert!(unsafe { (*(*t).terminal).modes.get(modes::Mode::GraphemeCluster) });
            // try testing.expect(t.?.terminal.modes.default.grapheme_cluster);
            assert!(unsafe {
                (*(*t).terminal)
                    .modes
                    .default
                    .get_mode(modes::Mode::GraphemeCluster)
            });

            config.value = false;
            assert_eq!(Result::Success, unsafe {
                set(
                    t,
                    Option_::Mode,
                    &config as *const ModeConfig as *const c_void,
                )
            });
            assert!(!unsafe { (*(*t).terminal).modes.get(modes::Mode::GraphemeCluster) });
            assert!(unsafe {
                (*(*t).terminal)
                    .modes
                    .default
                    .get_mode(modes::Mode::GraphemeCluster)
            });

            // Setting the default also unconditionally replaces the current value.
            config.value = true;
            assert_eq!(Result::Success, unsafe {
                set(
                    t,
                    Option_::ModeDefault,
                    &config as *const ModeConfig as *const c_void,
                )
            });
            assert!(unsafe { (*(*t).terminal).modes.get(modes::Mode::GraphemeCluster) });

            config.value = false;
            assert_eq!(Result::Success, unsafe {
                set(
                    t,
                    Option_::Mode,
                    &config as *const ModeConfig as *const c_void,
                )
            });
            // vt_write(t, "\x1bc", 2);
            let ris = b"\x1bc";
            unsafe { vt_write(t, ris.as_ptr(), ris.len()) };
            assert!(unsafe { (*(*t).terminal).modes.get(modes::Mode::GraphemeCluster) });

            config.value = false;
            assert_eq!(Result::Success, unsafe {
                set(
                    t,
                    Option_::ModeDefault,
                    &config as *const ModeConfig as *const c_void,
                )
            });
            assert!(!unsafe { (*(*t).terminal).modes.get(modes::Mode::GraphemeCluster) });
            assert!(!unsafe {
                (*(*t).terminal)
                    .modes
                    .default
                    .get_mode(modes::Mode::GraphemeCluster)
            });
            unsafe { free(t) };
        });
    }

    // test "set mode default validates configuration"
    #[test]
    fn set_mode_default_validates_configuration() {
        with_test_allocator(|alloc| {
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);

            // try testing.expectEqual(Result.invalid_value, set(t, .mode_default, null));
            assert_eq!(Result::InvalidValue, unsafe {
                set(t, Option_::ModeDefault, std::ptr::null())
            });

            // var config: ModeConfig = .{ .mode = @bitCast(modes.ModeTag{ .value = 9999, .ansi = false }), .value = true };
            let mut config = ModeConfig {
                mode: modes::ModeTag {
                    value: 9999,
                    ansi: false,
                }
                .to_backing(),
                value: true,
            };
            assert_eq!(Result::InvalidValue, unsafe {
                set(
                    t,
                    Option_::ModeDefault,
                    &config as *const ModeConfig as *const c_void,
                )
            });

            // config.mode = @bitCast(modes.ModeTag{ .value = 1047, .ansi = false });
            config.mode = modes::ModeTag {
                value: 1047,
                ansi: false,
            }
            .to_backing();
            assert_eq!(Result::InvalidValue, unsafe {
                set(
                    t,
                    Option_::ModeDefault,
                    &config as *const ModeConfig as *const c_void,
                )
            });
            unsafe { free(t) };
        });
    }

    // test "set and get mode null"
    #[test]
    fn set_and_get_mode_null() {
        // const tag: modes.ModeTag.Backing = @bitCast(modes.ModeTag{ .value = 25, .ansi = false });
        let tag = modes::ModeTag {
            value: 25,
            ansi: false,
        }
        .to_backing();
        // var config: ModeConfig = .{ .mode = tag, .value = true };
        let mut config = ModeConfig {
            mode: tag,
            value: true,
        };
        // try testing.expectEqual(Result.invalid_value, set(null, .mode, @ptrCast(&config)));
        assert_eq!(Result::InvalidValue, unsafe {
            set(
                std::ptr::null_mut(),
                Option_::Mode,
                &config as *const ModeConfig as *const c_void,
            )
        });
        // try testing.expectEqual(Result.invalid_value, get(null, .mode, @ptrCast(&config)));
        assert_eq!(Result::InvalidValue, unsafe {
            get(
                std::ptr::null_mut(),
                TerminalData::Mode,
                &mut config as *mut ModeConfig as *mut c_void,
            )
        });
    }

    // test "set and get mode validate configuration"
    #[test]
    fn set_and_get_mode_validate_configuration() {
        with_test_allocator(|alloc| {
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);

            // try testing.expectEqual(Result.invalid_value, set(t, .mode, null));
            assert_eq!(Result::InvalidValue, unsafe {
                set(t, Option_::Mode, std::ptr::null())
            });
            // try testing.expectEqual(Result.invalid_value, get(t, .mode, null));
            assert_eq!(Result::InvalidValue, unsafe {
                get(t, TerminalData::Mode, std::ptr::null_mut())
            });

            // var config: ModeConfig = .{ .mode = @bitCast(modes.ModeTag{ .value = 9999, .ansi = false }), .value = true };
            let mut config = ModeConfig {
                mode: modes::ModeTag {
                    value: 9999,
                    ansi: false,
                }
                .to_backing(),
                value: true,
            };
            assert_eq!(Result::InvalidValue, unsafe {
                set(
                    t,
                    Option_::Mode,
                    &config as *const ModeConfig as *const c_void,
                )
            });
            assert_eq!(Result::InvalidValue, unsafe {
                get(
                    t,
                    TerminalData::Mode,
                    &mut config as *mut ModeConfig as *mut c_void,
                )
            });
            unsafe { free(t) };
        });
    }

    // test "vt_write"
    #[test]
    fn vt_write_test() {
        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);

            unsafe { vt_write(t, b"Hello".as_ptr(), 5) };

            assert_eq!(b"Hello", plain_string(t).as_slice());
            unsafe { free(t) };
        });
    }

    // test "vt_write_until_ground result contract"
    #[test]
    fn vt_write_until_ground_result_contract() {
        with_test_allocator(|alloc| {
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);

            // Input remains untouched when the stream is already at ground.
            let mut consumed: usize = 99;
            let untouched = b"unprocessed";
            assert_eq!(Result::Success, unsafe {
                vt_write_until_ground(t, untouched.as_ptr(), untouched.len(), &mut consumed)
            });
            assert_eq!(0usize, consumed);

            // Complete a split CSI and stop before inspecting the printable suffix.
            vtw(t, b"\x1b[31");
            let input = b"mABC\x1b[";
            assert_eq!(Result::Success, unsafe {
                vt_write_until_ground(t, input.as_ptr(), input.len(), &mut consumed)
            });
            assert_eq!(1usize, consumed);
            assert!(unsafe { (*t).stream.ground() });

            assert_eq!(b"", plain_string(t).as_slice());

            // The untouched suffix can be processed after work at the boundary.
            unsafe { vt_write(t, input.as_ptr().add(consumed), input.len() - consumed) };
            assert_eq!(b"ABC", plain_string(t).as_slice());
            assert!(!unsafe { (*t).stream.ground() });

            // Exhausting input while unfinished is distinct from reaching ground on
            // the final byte, even though both consume the entire input.
            assert_eq!(Result::NoValue, unsafe {
                vt_write_until_ground(t, b"123".as_ptr(), 3, &mut consumed)
            });
            assert_eq!(3usize, consumed);
            assert!(!unsafe { (*t).stream.ground() });

            assert_eq!(Result::Success, unsafe {
                vt_write_until_ground(t, b"m".as_ptr(), 1, &mut consumed)
            });
            assert_eq!(1usize, consumed);
            assert!(unsafe { (*t).stream.ground() });
            unsafe { free(t) };
        });
    }

    // test "vt_write_until_ground handles UTF-8 and abort boundaries"
    #[test]
    fn vt_write_until_ground_handles_utf8_and_abort_boundaries() {
        with_test_allocator(|alloc| {
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);

            let mut ground = false;
            assert_eq!(Result::Success, unsafe {
                get(
                    t,
                    TerminalData::VtGround,
                    &mut ground as *mut bool as *mut c_void,
                )
            });
            assert!(ground);

            vtw(t, &[0xF0]);
            assert_eq!(Result::Success, unsafe {
                get(
                    t,
                    TerminalData::VtGround,
                    &mut ground as *mut bool as *mut c_void,
                )
            });
            assert!(!ground);

            let utf8_suffix = [0x9Fu8, 0x98, 0x84];
            let mut consumed: usize = 99;
            assert_eq!(Result::Success, unsafe {
                vt_write_until_ground(t, utf8_suffix.as_ptr(), utf8_suffix.len(), &mut consumed)
            });
            assert_eq!(utf8_suffix.len(), consumed);
            assert!(unsafe { (*t).stream.ground() });

            // A malformed continuation resets the decoder and reaches ground after
            // processing the retry byte.
            vtw(t, &[0xE0, 0xA0]);
            assert_eq!(Result::Success, unsafe {
                vt_write_until_ground(t, b"A".as_ptr(), 1, &mut consumed)
            });
            assert_eq!(1usize, consumed);
            assert!(unsafe { (*t).stream.ground() });

            vtw(t, b"\x1b[123");
            assert_eq!(Result::Success, unsafe {
                vt_write_until_ground(t, [0x18u8].as_ptr(), 1, &mut consumed)
            });
            assert_eq!(1usize, consumed);
            assert!(unsafe { (*t).stream.ground() });
            unsafe { free(t) };
        });
    }

    // test "vt_write_until_ground invokes effects only for consumed input"
    #[test]
    fn vt_write_until_ground_invokes_effects_only_for_consumed_input() {
        static mut UG_BELL_COUNT: usize = 0;
        extern "C" fn bell(_t: Terminal, _ud: *mut c_void) {
            unsafe { UG_BELL_COUNT += 1 };
        }

        with_test_allocator(|alloc| {
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);

            unsafe { UG_BELL_COUNT = 0 };
            assert_eq!(Result::Success, unsafe {
                set(t, Option_::Bell, bell as *const c_void)
            });

            vtw(t, b"\x1b[31");
            let input = b"m\x07";
            let mut consumed: usize = 99;
            assert_eq!(Result::Success, unsafe {
                vt_write_until_ground(t, input.as_ptr(), input.len(), &mut consumed)
            });
            assert_eq!(1usize, consumed);
            assert_eq!(0usize, unsafe { UG_BELL_COUNT });

            unsafe { vt_write(t, input.as_ptr().add(consumed), input.len() - consumed) };
            assert_eq!(1usize, unsafe { UG_BELL_COUNT });
            unsafe { free(t) };
        });
    }

    // test "vt_write_until_ground validates arguments"
    #[test]
    fn vt_write_until_ground_validates_arguments() {
        with_test_allocator(|alloc| {
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);

            let mut consumed: usize = 99;
            assert_eq!(Result::InvalidValue, unsafe {
                vt_write_until_ground(std::ptr::null_mut(), b"x".as_ptr(), 1, &mut consumed)
            });
            assert_eq!(0usize, consumed);

            consumed = 99;
            assert_eq!(Result::InvalidValue, unsafe {
                vt_write_until_ground(t, std::ptr::null(), 1, &mut consumed)
            });
            assert_eq!(0usize, consumed);

            assert_eq!(Result::InvalidValue, unsafe {
                vt_write_until_ground(t, b"".as_ptr(), 0, std::ptr::null_mut())
            });

            // NULL represents a valid empty slice. While unfinished it consumes zero
            // bytes and reports that no ground boundary was found.
            vtw(t, b"\x1b[");
            consumed = 99;
            assert_eq!(Result::NoValue, unsafe {
                vt_write_until_ground(t, std::ptr::null(), 0, &mut consumed)
            });
            assert_eq!(0usize, consumed);
            unsafe { free(t) };
        });
    }

    // test "vt_write split escape sequence"
    #[test]
    fn vt_write_split_escape_sequence() {
        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);

            // Write "Hello" in bold by splitting the CSI bold sequence across two writes.
            // ESC [ 1 m  = bold on, ESC [ 0 m = reset
            // Split ESC from the rest of the CSI sequence.
            unsafe { vt_write(t, b"Hello \x1b".as_ptr(), 7) };
            unsafe { vt_write(t, b"[1mBold\x1b[0m".as_ptr(), 10) };

            // If the escape sequence leaked, we'd see "[1mBold" as literal text.
            assert_eq!(b"Hello Bold", plain_string(t).as_slice());
            unsafe { free(t) };
        });
    }

    // test "vt_write split combining mark after base at right edge"
    #[test]
    fn vt_write_split_combining_mark_after_base_at_right_edge() {
        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 2, 2);
            assert_eq!(Result::Success, new_result);

            // Put "å" in the final column, then send its combining low line in a
            // separate write so the mark arrives while the cursor has a pending wrap.
            unsafe { vt_write(t, "xå".as_bytes().as_ptr(), 3) };
            unsafe { vt_write(t, b"\xcc\xb2".as_ptr(), 2) };

            assert_eq!("xå̲".as_bytes(), plain_string(t).as_slice());
            unsafe { free(t) };
        });
    }

    // test "get cols and rows"
    #[test]
    fn get_cols_and_rows() {
        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);

            let mut cols: size::CellCountInt = 0;
            let mut rows: size::CellCountInt = 0;
            assert_eq!(Result::Success, unsafe {
                get(t, TerminalData::Cols, &mut cols as *mut _ as *mut c_void)
            });
            assert_eq!(Result::Success, unsafe {
                get(t, TerminalData::Rows, &mut rows as *mut _ as *mut c_void)
            });
            assert_eq!(80, cols);
            assert_eq!(24, rows);
            unsafe { free(t) };
        });
    }

    // test "get cursor position"
    #[test]
    fn get_cursor_position() {
        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);

            unsafe { vt_write(t, b"Hello".as_ptr(), 5) };

            let mut x: size::CellCountInt = 0;
            let mut y: size::CellCountInt = 0;
            assert_eq!(Result::Success, unsafe {
                get(t, TerminalData::CursorX, &mut x as *mut _ as *mut c_void)
            });
            assert_eq!(Result::Success, unsafe {
                get(t, TerminalData::CursorY, &mut y as *mut _ as *mut c_void)
            });
            assert_eq!(5, x);
            assert_eq!(0, y);
            unsafe { free(t) };
        });
    }

    // test "get vt_processing_error"
    //
    // Ported as written (parity tail S7-3): the active screen's allocator is
    // swapped for `std.mem.Allocator.failing` (`rustty_vt::alloc::failing()`)
    // around the write, as the reference swaps in `testing.failing_allocator`.
    // (A `set_title` tripwire stood in for it while `title` lived on the Rust
    // heap.)
    #[test]
    fn get_vt_processing_error() {
        with_test_allocator(|alloc| {
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);

            let mut processing_error: bool = true;
            assert_eq!(Result::Success, unsafe {
                get(
                    t,
                    TerminalData::VtProcessingError,
                    &mut processing_error as *mut _ as *mut c_void,
                )
            });
            assert!(!processing_error);

            // Force a non-graceful terminal-owned update failure through the
            // public VT write path.
            {
                // const alloc = t.?.terminal.screens.active.alloc;
                // t.?.terminal.screens.active.alloc = testing.failing_allocator;
                // defer t.?.terminal.screens.active.alloc = alloc;
                // SAFETY: `t` is live; the active screen is a stable heap
                // pointer owned by the terminal, written through a raw place.
                let active = unsafe { (*zig_terminal(t)).screens.active };
                let alloc = unsafe { (*active).alloc };
                unsafe { (*active).alloc = rustty_vt::alloc::failing() };

                vtw(t, b"\x1B]2;unavailable\x1B\\");

                unsafe { (*active).alloc = alloc };
            }

            assert_eq!(Result::Success, unsafe {
                get(
                    t,
                    TerminalData::VtProcessingError,
                    &mut processing_error as *mut _ as *mut c_void,
                )
            });
            assert!(processing_error);

            // rustty_terminal_reset() does NOT clear it.
            unsafe { reset(t) };
            assert_eq!(Result::Success, unsafe {
                get(
                    t,
                    TerminalData::VtProcessingError,
                    &mut processing_error as *mut _ as *mut c_void,
                )
            });
            assert!(processing_error);

            unsafe { free(t) };
        });
    }

    // test "get null"
    #[test]
    fn get_null() {
        let mut cols: size::CellCountInt = 0;
        assert_eq!(Result::InvalidValue, unsafe {
            get(
                std::ptr::null_mut(),
                TerminalData::Cols,
                &mut cols as *mut _ as *mut c_void,
            )
        });
    }

    // test "get cursor_visible"
    #[test]
    fn get_cursor_visible() {
        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);

            let mut visible: bool = false;
            assert_eq!(Result::Success, unsafe {
                get(
                    t,
                    TerminalData::CursorVisible,
                    &mut visible as *mut bool as *mut c_void,
                )
            });
            assert!(visible);

            // DEC mode 25 controls cursor visibility
            // var config: ModeConfig = .{ .mode = @bitCast(modes.ModeTag{ .value = 25, .ansi = false }), .value = false };
            let config = ModeConfig {
                mode: modes::ModeTag {
                    value: 25,
                    ansi: false,
                }
                .to_backing(),
                value: false,
            };
            // try testing.expectEqual(Result.success, set(t, .mode, @ptrCast(&config)));
            assert_eq!(Result::Success, unsafe {
                set(
                    t,
                    Option_::Mode,
                    &config as *const ModeConfig as *const c_void,
                )
            });
            assert_eq!(Result::Success, unsafe {
                get(
                    t,
                    TerminalData::CursorVisible,
                    &mut visible as *mut bool as *mut c_void,
                )
            });
            assert!(!visible);
            unsafe { free(t) };
        });
    }

    // test "get cursor_at_prompt"
    #[test]
    fn get_cursor_at_prompt() {
        with_test_allocator(|alloc| {
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);

            // var at_prompt: bool = undefined;
            let mut at_prompt: bool = false;
            // try testing.expectEqual(Result.success, get(t, .cursor_at_prompt, @ptrCast(&at_prompt)));
            assert_eq!(Result::Success, unsafe {
                get(
                    t,
                    TerminalData::CursorAtPrompt,
                    &mut at_prompt as *mut bool as *mut c_void,
                )
            });
            // try testing.expect(!at_prompt);
            assert!(!at_prompt);

            // const prompt_start = "\x1b]133;A\x07";
            let prompt_start = b"\x1b]133;A\x07";
            unsafe { vt_write(t, prompt_start.as_ptr(), prompt_start.len()) };
            assert_eq!(Result::Success, unsafe {
                get(
                    t,
                    TerminalData::CursorAtPrompt,
                    &mut at_prompt as *mut bool as *mut c_void,
                )
            });
            // try testing.expect(at_prompt);
            assert!(at_prompt);

            // const alternate_screen = "\x1b[?1049h";
            let alternate_screen = b"\x1b[?1049h";
            unsafe { vt_write(t, alternate_screen.as_ptr(), alternate_screen.len()) };
            assert_eq!(Result::Success, unsafe {
                get(
                    t,
                    TerminalData::CursorAtPrompt,
                    &mut at_prompt as *mut bool as *mut c_void,
                )
            });
            // try testing.expect(!at_prompt);
            assert!(!at_prompt);
            unsafe { free(t) };
        });
    }

    // test "get active_screen"
    #[test]
    fn get_active_screen() {
        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);

            let mut screen: TerminalScreen = TerminalScreen::Primary;
            assert_eq!(Result::Success, unsafe {
                get(
                    t,
                    TerminalData::ActiveScreen,
                    &mut screen as *mut TerminalScreen as *mut c_void,
                )
            });
            assert_eq!(TerminalScreen::Primary, screen);
            unsafe { free(t) };
        });
    }

    // test "get kitty_keyboard_flags"
    #[test]
    fn get_kitty_keyboard_flags() {
        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);

            let mut flags: u8 = 0;
            assert_eq!(Result::Success, unsafe {
                get(
                    t,
                    TerminalData::KittyKeyboardFlags,
                    &mut flags as *mut u8 as *mut c_void,
                )
            });
            assert_eq!(0, flags);

            // Push kitty flags via VT sequence: CSI > 3 u (push disambiguate | report_events)
            unsafe { vt_write(t, b"\x1b[>3u".as_ptr(), 5) };

            assert_eq!(Result::Success, unsafe {
                get(
                    t,
                    TerminalData::KittyKeyboardFlags,
                    &mut flags as *mut u8 as *mut c_void,
                )
            });
            assert_eq!(3, flags);
            unsafe { free(t) };
        });
    }

    // test "get mouse_tracking"
    #[test]
    fn get_mouse_tracking() {
        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);

            let mut tracking: bool = false;
            let get_tracking = |t: Terminal, tracking: &mut bool| unsafe {
                get(
                    t,
                    TerminalData::MouseTracking,
                    tracking as *mut bool as *mut c_void,
                )
            };
            assert_eq!(Result::Success, get_tracking(t, &mut tracking));
            assert!(!tracking);

            // PORT NOTE (cfc19e805): mode writes go through `set(.mode)` with a
            // `ModeConfig`; this closure is the `set(t, .mode, @ptrCast(&config))`
            // call shared by every step below.
            let set_mode = |t: Terminal, value: u16, on: bool| {
                let config = ModeConfig {
                    mode: modes::ModeTag { value, ansi: false }.to_backing(),
                    value: on,
                };
                unsafe {
                    set(
                        t,
                        Option_::Mode,
                        &config as *const ModeConfig as *const c_void,
                    )
                }
            };

            // Enable X10 mouse (DEC mode 9)
            // var config: ModeConfig = .{ .mode = @bitCast(modes.ModeTag{ .value = 9, .ansi = false }), .value = true };
            assert_eq!(Result::Success, set_mode(t, 9, true));
            assert_eq!(Result::Success, get_tracking(t, &mut tracking));
            assert!(tracking);

            // Disable X10, enable normal mouse (DEC mode 1000)
            assert_eq!(Result::Success, set_mode(t, 9, false));
            assert_eq!(Result::Success, set_mode(t, 1000, true));
            assert_eq!(Result::Success, get_tracking(t, &mut tracking));
            assert!(tracking);

            // Disable normal, enable button mouse (DEC mode 1002)
            assert_eq!(Result::Success, set_mode(t, 1000, false));
            assert_eq!(Result::Success, set_mode(t, 1002, true));
            assert_eq!(Result::Success, get_tracking(t, &mut tracking));
            assert!(tracking);

            // Disable button, enable any mouse (DEC mode 1003)
            assert_eq!(Result::Success, set_mode(t, 1002, false));
            assert_eq!(Result::Success, set_mode(t, 1003, true));
            assert_eq!(Result::Success, get_tracking(t, &mut tracking));
            assert!(tracking);

            // Disable all - should be false again
            assert_eq!(Result::Success, set_mode(t, 1003, false));
            assert_eq!(Result::Success, get_tracking(t, &mut tracking));
            assert!(!tracking);
            unsafe { free(t) };
        });
    }

    // test "get total_rows"
    #[test]
    fn get_total_rows() {
        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);

            let mut total: usize = 0;
            assert_eq!(Result::Success, unsafe {
                get(
                    t,
                    TerminalData::TotalRows,
                    &mut total as *mut usize as *mut c_void,
                )
            });
            assert!(total >= 24);
            unsafe { free(t) };
        });
    }

    // test "get scrollback_rows"
    #[test]
    fn get_scrollback_rows() {
        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 3);
            assert_eq!(Result::Success, new_result);

            let mut scrollback: usize = 0;
            assert_eq!(Result::Success, unsafe {
                get(
                    t,
                    TerminalData::ScrollbackRows,
                    &mut scrollback as *mut usize as *mut c_void,
                )
            });
            assert_eq!(0usize, scrollback);

            // Write enough lines to push content into scrollback
            unsafe {
                vt_write(
                    t,
                    b"line1\r\nline2\r\nline3\r\nline4\r\nline5\r\n".as_ptr(),
                    34,
                )
            };

            assert_eq!(Result::Success, unsafe {
                get(
                    t,
                    TerminalData::ScrollbackRows,
                    &mut scrollback as *mut usize as *mut c_void,
                )
            });
            assert_eq!(2usize, scrollback);
            unsafe { free(t) };
        });
    }

    // test "get invalid"
    #[test]
    fn get_invalid() {
        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);

            assert_eq!(Result::InvalidValue, unsafe {
                get(t, TerminalData::Invalid, std::ptr::null_mut())
            });
            unsafe { free(t) };
        });
    }

    // test "get_multi null keys returns invalid_value"
    #[test]
    fn get_multi_null_keys_returns_invalid_value() {
        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);

            let mut cols: size::CellCountInt = 0;
            let mut values: [*mut c_void; 1] = [&mut cols as *mut _ as *mut c_void];
            assert_eq!(Result::InvalidValue, unsafe {
                get_multi(
                    t,
                    1,
                    std::ptr::null(),
                    values.as_mut_ptr(),
                    std::ptr::null_mut(),
                )
            });
            unsafe { free(t) };
        });
    }

    // test "get mouse_shape"
    #[test]
    fn get_mouse_shape() {
        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&lib.alloc.test_allocator, &t, 80, 24));
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);

            // var shape: mouse.Shape = undefined;
            let mut shape: rustty_vt::mouse::Shape = rustty_vt::mouse::Shape::Default;
            // try testing.expectEqual(Result.success, get(t, .mouse_shape, @ptrCast(&shape)));
            assert_eq!(Result::Success, unsafe {
                get(
                    t,
                    TerminalData::MouseShape,
                    &mut shape as *mut rustty_vt::mouse::Shape as *mut c_void,
                )
            });
            // try testing.expectEqual(mouse.Shape.text, shape);
            assert_eq!(rustty_vt::mouse::Shape::Text, shape);

            // const cases = .{ ... };
            let cases: &[(&[u8], rustty_vt::mouse::Shape)] = &[
                (b"\x1b]22;pointer\x07", rustty_vt::mouse::Shape::Pointer),
                (
                    b"\x1b]22;crosshair\x1b\\",
                    rustty_vt::mouse::Shape::Crosshair,
                ),
                // Invalid names leave the last accepted shape unchanged.
                (
                    b"\x1b]22;not-a-pointer-shape\x07",
                    rustty_vt::mouse::Shape::Crosshair,
                ),
                // Hyperlinks don't override the application's requested shape.
                (
                    b"\x1b]8;;https://example.com\x1b\\link\x1b]8;;\x1b\\",
                    rustty_vt::mouse::Shape::Crosshair,
                ),
                (b"\x1b]22;default\x07", rustty_vt::mouse::Shape::Default),
                (b"\x1b]22;text\x07", rustty_vt::mouse::Shape::Text),
            ];
            // inline for (cases) |case| { ... }
            for (data, want) in cases {
                unsafe { vt_write(t, data.as_ptr(), data.len()) };
                assert_eq!(Result::Success, unsafe {
                    get(
                        t,
                        TerminalData::MouseShape,
                        &mut shape as *mut rustty_vt::mouse::Shape as *mut c_void,
                    )
                });
                assert_eq!(*want, shape);
            }

            // An incomplete OSC must not update the shape before its terminator.
            let prefix = b"\x1b]22;wait";
            unsafe { vt_write(t, prefix.as_ptr(), prefix.len()) };
            assert_eq!(Result::Success, unsafe {
                get(
                    t,
                    TerminalData::MouseShape,
                    &mut shape as *mut rustty_vt::mouse::Shape as *mut c_void,
                )
            });
            assert_eq!(rustty_vt::mouse::Shape::Text, shape);
            let terminator = b"\x07";
            unsafe { vt_write(t, terminator.as_ptr(), terminator.len()) };
            assert_eq!(Result::Success, unsafe {
                get(
                    t,
                    TerminalData::MouseShape,
                    &mut shape as *mut rustty_vt::mouse::Shape as *mut c_void,
                )
            });
            assert_eq!(rustty_vt::mouse::Shape::Wait, shape);
            unsafe { free(t) };
        });
    }

    // PORT NOTE (D3 SEAM soundness probe, Rust-only addition): the reference
    // exercises the effect trampolines through the C `set` API (a later
    // sub-batch). To validate the `container_of` recovery (`wrapper_from_handler`)
    // in THIS sub-batch, we install a C `write_pty` callback by writing the vt-c
    // `effects.write_pty` field directly (the same field `setTyped(.write_pty)`
    // will write), then drive a VT query (DSR operating status `CSI 5 n`) that
    // round-trips through the always-installed trampoline and unconditionally
    // writes `"\x1b[0n"` back. This is the Miri soundness gate for the raw-ptr
    // sibling recovery off the `&mut Handler`.
    static mut WRITE_PTY_CAPTURE: Vec<u8> = Vec::new();

    extern "C" fn capture_write_pty(_t: Terminal, _ud: *mut c_void, ptr: *const u8, len: usize) {
        let data = unsafe { core::slice::from_raw_parts(ptr, len) };
        unsafe {
            #[allow(static_mut_refs)]
            WRITE_PTY_CAPTURE.extend_from_slice(data);
        }
    }

    #[test]
    fn write_pty_trampoline_round_trip() {
        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);
            unsafe {
                #[allow(static_mut_refs)]
                WRITE_PTY_CAPTURE.clear();
            }

            // Install the C callback directly on the vt-c effects (the field the
            // C `set(.write_pty)` will populate).
            unsafe { (*t).effects.write_pty = Some(capture_write_pty) };

            // DSR operating status "CSI 5 n" writes "\x1b[0n" via write_pty.
            unsafe { vt_write(t, b"\x1b[5n".as_ptr(), 4) };

            let captured = unsafe {
                #[allow(static_mut_refs)]
                WRITE_PTY_CAPTURE.clone()
            };
            assert_eq!(b"\x1b[0n", captured.as_slice());
            unsafe { free(t) };
        });
    }

    // PORT NOTE (test helpers): the reference builds `point.Point.C` literals
    // inline (`.{ .tag = .active, .value = .{ .active = .{ .x, .y } } }`). rustty's
    // `PointC` is a hand-authored tagged union; these helpers construct it for the
    // active/viewport/screen tags used by the ported tests.
    fn pt_active(x: size::CellCountInt, y: u32) -> PointC {
        PointC {
            tag: crate::point::TagC::Active as c_int,
            value: crate::point::PointCValue {
                active: Coordinate { x, y },
            },
        }
    }
    fn pt_viewport(x: size::CellCountInt, y: u32) -> PointC {
        PointC {
            tag: crate::point::TagC::Viewport as c_int,
            value: crate::point::PointCValue {
                viewport: Coordinate { x, y },
            },
        }
    }
    fn pt_screen(x: size::CellCountInt, y: u32) -> PointC {
        PointC {
            tag: crate::point::TagC::Screen as c_int,
            value: crate::point::PointCValue {
                screen: Coordinate { x, y },
            },
        }
    }

    // Convenience: write a byte slice through `vt_write`.
    fn vtw(t: Terminal, bytes: &[u8]) {
        unsafe { vt_write(t, bytes.as_ptr(), bytes.len()) };
    }

    // test "set default cursor style and blink"
    #[test]
    fn set_default_cursor_style_and_blink() {
        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);

            // var default_style: TerminalCursorStyle = .bar;
            let mut default_style = TerminalCursorStyle::Bar;
            // var default_blink = true;
            let mut default_blink = true;
            assert_eq!(Result::Success, unsafe {
                set(
                    t,
                    Option_::DefaultCursorStyle,
                    &mut default_style as *mut _ as *const c_void,
                )
            });
            assert_eq!(Result::Success, unsafe {
                set(
                    t,
                    Option_::DefaultCursorBlink,
                    &mut default_blink as *mut _ as *const c_void,
                )
            });

            // Setting defaults applies them immediately while the cursor is still default.
            assert_eq!(rustty_vt::screen::CursorStyle::Bar, unsafe {
                (*(*(*t).terminal).screens.active).cursor.cursor_style
            });
            assert!(unsafe { (*(*t).terminal).modes.get(modes::Mode::CursorBlinking) });

            // An explicit DECSCUSR style overrides the configured defaults.
            vtw(t, b"\x1b[2 q");
            assert_eq!(rustty_vt::screen::CursorStyle::Block, unsafe {
                (*(*(*t).terminal).screens.active).cursor.cursor_style
            });
            assert!(!unsafe { (*(*t).terminal).modes.get(modes::Mode::CursorBlinking) });

            // Changing defaults does not override an explicit cursor style.
            default_style = TerminalCursorStyle::Underline;
            assert_eq!(Result::Success, unsafe {
                set(
                    t,
                    Option_::DefaultCursorStyle,
                    &mut default_style as *mut _ as *const c_void,
                )
            });
            assert_eq!(rustty_vt::screen::CursorStyle::Block, unsafe {
                (*(*(*t).terminal).screens.active).cursor.cursor_style
            });
            assert!(!unsafe { (*(*t).terminal).modes.get(modes::Mode::CursorBlinking) });

            // DECSCUSR reset restores the configured default style and blink.
            vtw(t, b"\x1b[0 q");
            assert_eq!(rustty_vt::screen::CursorStyle::Underline, unsafe {
                (*(*(*t).terminal).screens.active).cursor.cursor_style
            });
            assert!(unsafe { (*(*t).terminal).modes.get(modes::Mode::CursorBlinking) });

            // RIS also restores cursor defaults from Terminal-owned state.
            vtw(t, b"\x1b[2 q");
            unsafe { reset(t) };
            assert_eq!(rustty_vt::screen::CursorStyle::Underline, unsafe {
                (*(*(*t).terminal).screens.active).cursor.cursor_style
            });
            assert!(unsafe { (*(*t).terminal).modes.get(modes::Mode::CursorBlinking) });
            unsafe { free(t) };
        });
    }

    // Rust-only regression test (no oracle counterpart): the oracle's
    // `TerminalCursorStyle` is a non-exhaustive `enum(c_int)`, so an unknown
    // discriminant reaches `toZig`'s `_ => null` arm and yields
    // `.invalid_value`. The header exposes
    // `RUSTTY_TERMINAL_CURSOR_STYLE_MAX_VALUE` (= INT_MAX), so a conforming
    // caller can send one. This pins that the closed Rust enum never sees such
    // a value (which would be UB) and that the option is rejected, not applied.
    #[test]
    fn set_default_cursor_style_rejects_unknown_enum_values() {
        with_test_allocator(|alloc| {
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);

            // Control: a declared enumerator is accepted and applied.
            let mut style = TerminalCursorStyle::Underline;
            assert_eq!(Result::Success, unsafe {
                set(
                    t,
                    Option_::DefaultCursorStyle,
                    &mut style as *mut _ as *const c_void,
                )
            });
            assert_eq!(rustty_vt::screen::CursorStyle::Underline, unsafe {
                (*(*(*t).terminal).screens.active).cursor.cursor_style
            });

            // Out-of-range discriminants: the header sentinel and the first
            // unnamed value. Both are rejected and change nothing.
            for raw in [c_int::MAX, 4, -1] {
                let mut raw = raw;
                assert_eq!(Result::InvalidValue, unsafe {
                    set(
                        t,
                        Option_::DefaultCursorStyle,
                        &mut raw as *mut c_int as *const c_void,
                    )
                });
                assert_eq!(rustty_vt::screen::CursorStyle::Underline, unsafe {
                    (*(*(*t).terminal).screens.active).cursor.cursor_style
                });
            }

            unsafe { free(t) };
        });
    }

    // test "set and get selection"
    #[test]
    fn set_and_get_selection() {
        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);

            vtw(t, b"Hello");

            // var start_ref: grid_ref_c.CGridRef = .{};
            let mut start_ref = CGridRef::default();
            assert_eq!(Result::Success, unsafe {
                grid_ref(t, pt_active(0, 0), &mut start_ref)
            });

            let mut end_ref = CGridRef::default();
            assert_eq!(Result::Success, unsafe {
                grid_ref(t, pt_active(4, 0), &mut end_ref)
            });

            let mut out = crate::selection::CSelection::default();
            assert_eq!(Result::NoValue, unsafe {
                get(
                    t,
                    TerminalData::Selection,
                    &mut out as *mut _ as *mut c_void,
                )
            });

            let sel = crate::selection::CSelection {
                start: start_ref,
                end: end_ref,
                rectangle: true,
                ..crate::selection::CSelection::default()
            };
            assert_eq!(Result::Success, unsafe {
                set(t, Option_::Selection, &sel as *const _ as *const c_void)
            });
            assert!(unsafe {
                (*(*(*t).terminal).screens.active)
                    .selection
                    .unwrap()
                    .tracked()
            });

            assert_eq!(Result::Success, unsafe {
                get(
                    t,
                    TerminalData::Selection,
                    &mut out as *mut _ as *mut c_void,
                )
            });
            assert!(out.start.to_pin().unwrap().eql(start_ref.to_pin().unwrap()));
            assert!(out.end.to_pin().unwrap().eql(end_ref.to_pin().unwrap()));
            assert!(out.rectangle);

            // set(t, .selection, null)
            assert_eq!(Result::Success, unsafe {
                set(t, Option_::Selection, std::ptr::null())
            });
            assert!(unsafe { (*(*(*t).terminal).screens.active).selection.is_none() });
            assert_eq!(Result::NoValue, unsafe {
                get(
                    t,
                    TerminalData::Selection,
                    &mut out as *mut _ as *mut c_void,
                )
            });
            unsafe { free(t) };
        });
    }

    // test "selection derivation helpers"
    #[test]
    fn selection_derivation_helpers() {
        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);

            vtw(t, b"  Hello  \r\nWorld");

            let mut out = crate::selection::CSelection::default();

            let mut word_ref = CGridRef::default();
            assert_eq!(Result::Success, unsafe {
                grid_ref(t, pt_active(3, 0), &mut word_ref)
            });

            let mut empty_ref = CGridRef::default();
            assert_eq!(Result::Success, unsafe {
                grid_ref(t, pt_active(20, 0), &mut empty_ref)
            });

            let mut line_ref = CGridRef::default();
            assert_eq!(Result::Success, unsafe {
                grid_ref(t, pt_active(0, 0), &mut line_ref)
            });

            let mut word_opts = crate::selection::SelectWordOptions {
                size: size_of::<crate::selection::SelectWordOptions>(),
                ref_: word_ref,
                boundary_codepoints: std::ptr::null(),
                boundary_codepoints_len: 0,
            };
            assert_eq!(Result::Success, unsafe {
                crate::selection::word(t, &word_opts, &mut out)
            });
            assert_eq!(2u16, out.start.to_pin().unwrap().x);
            assert_eq!(6u16, out.end.to_pin().unwrap().x);

            word_opts.ref_ = empty_ref;
            assert_eq!(Result::NoValue, unsafe {
                crate::selection::word(t, &word_opts, &mut out)
            });

            let mut between_start_ref = CGridRef::default();
            assert_eq!(Result::Success, unsafe {
                grid_ref(t, pt_active(20, 1), &mut between_start_ref)
            });

            let mut between_end_ref = CGridRef::default();
            assert_eq!(Result::Success, unsafe {
                grid_ref(t, pt_active(0, 1), &mut between_end_ref)
            });

            let word_between_opts = crate::selection::SelectWordBetweenOptions {
                size: size_of::<crate::selection::SelectWordBetweenOptions>(),
                start: between_start_ref,
                end: between_end_ref,
                boundary_codepoints: std::ptr::null(),
                boundary_codepoints_len: 0,
            };
            assert_eq!(Result::Success, unsafe {
                crate::selection::word_between(t, &word_between_opts, &mut out)
            });
            assert_eq!(0u16, out.start.to_pin().unwrap().x);
            assert_eq!(1u16, out.start.to_pin().unwrap().y);
            assert_eq!(4u16, out.end.to_pin().unwrap().x);
            assert_eq!(1u16, out.end.to_pin().unwrap().y);

            let mut line_opts = crate::selection::SelectLineOptions {
                size: size_of::<crate::selection::SelectLineOptions>(),
                ref_: line_ref,
                whitespace: std::ptr::null(),
                whitespace_len: 0,
                semantic_prompt_boundary: false,
            };
            assert_eq!(Result::Success, unsafe {
                crate::selection::line(t, &line_opts, &mut out)
            });
            assert_eq!(2u16, out.start.to_pin().unwrap().x);
            assert_eq!(6u16, out.end.to_pin().unwrap().x);

            assert_eq!(Result::Success, unsafe {
                crate::selection::all(t, &mut out)
            });
            assert_eq!(2u16, out.start.to_pin().unwrap().x);
            assert_eq!(0u16, out.start.to_pin().unwrap().y);
            assert_eq!(4u16, out.end.to_pin().unwrap().x);
            assert_eq!(1u16, out.end.to_pin().unwrap().y);

            assert_eq!(Result::NoValue, unsafe {
                crate::selection::output(t, line_ref, &mut out)
            });

            line_opts.size = size_of::<usize>() - 1;
            assert_eq!(Result::InvalidValue, unsafe {
                crate::selection::line(t, &line_opts, &mut out)
            });
            assert_eq!(Result::InvalidValue, unsafe {
                crate::selection::word(t, std::ptr::null(), &mut out)
            });
            assert_eq!(Result::InvalidValue, unsafe {
                crate::selection::word(t, &word_opts, std::ptr::null_mut())
            });
            assert_eq!(Result::InvalidValue, unsafe {
                crate::selection::word_between(t, std::ptr::null(), &mut out)
            });
            assert_eq!(Result::InvalidValue, unsafe {
                crate::selection::word_between(t, &word_between_opts, std::ptr::null_mut())
            });
            unsafe { free(t) };
        });
    }

    // test "selection_adjust mutates snapshot end"
    #[test]
    fn selection_adjust_mutates_snapshot_end() {
        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);

            vtw(t, b"Hello");

            let mut start_ref = CGridRef::default();
            assert_eq!(Result::Success, unsafe {
                grid_ref(t, pt_active(0, 0), &mut start_ref)
            });

            let mut end_ref = CGridRef::default();
            assert_eq!(Result::Success, unsafe {
                grid_ref(t, pt_active(1, 0), &mut end_ref)
            });

            let mut sel = crate::selection::CSelection {
                start: start_ref,
                end: end_ref,
                ..crate::selection::CSelection::default()
            };
            assert_eq!(Result::Success, unsafe {
                crate::selection::adjust(t, &mut sel, crate::selection::Adjustment::Right)
            });
            assert_eq!(0u16, sel.start.to_pin().unwrap().x);
            assert_eq!(2u16, sel.end.to_pin().unwrap().x);

            assert_eq!(Result::Success, unsafe {
                crate::selection::adjust(t, &mut sel, crate::selection::Adjustment::Left)
            });
            assert_eq!(0u16, sel.start.to_pin().unwrap().x);
            assert_eq!(1u16, sel.end.to_pin().unwrap().x);

            sel = crate::selection::CSelection {
                start: end_ref,
                end: start_ref,
                ..crate::selection::CSelection::default()
            };
            assert_eq!(Result::Success, unsafe {
                crate::selection::adjust(t, &mut sel, crate::selection::Adjustment::Right)
            });
            assert_eq!(1u16, sel.start.to_pin().unwrap().x);
            assert_eq!(1u16, sel.end.to_pin().unwrap().x);
            unsafe { free(t) };
        });
    }

    // test "selection_order and selection_ordered"
    #[test]
    fn selection_order_and_selection_ordered() {
        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);

            vtw(t, b"Hello\r\nWorld");

            let mut start_ref = CGridRef::default();
            assert_eq!(Result::Success, unsafe {
                grid_ref(t, pt_active(3, 0), &mut start_ref)
            });

            let mut end_ref = CGridRef::default();
            assert_eq!(Result::Success, unsafe {
                grid_ref(t, pt_active(1, 1), &mut end_ref)
            });

            let sel = crate::selection::CSelection {
                start: start_ref,
                end: end_ref,
                rectangle: true,
                ..crate::selection::CSelection::default()
            };

            let mut order = crate::selection::Order::Forward;
            assert_eq!(Result::Success, unsafe {
                crate::selection::order(t, &sel, &mut order)
            });
            assert_eq!(crate::selection::Order::MirroredForward, order);

            let mut out = crate::selection::CSelection::default();
            assert_eq!(Result::Success, unsafe {
                crate::selection::ordered(t, &sel, crate::selection::Order::Forward, &mut out)
            });
            assert_eq!(1u16, out.start.to_pin().unwrap().x);
            assert_eq!(0u16, out.start.to_pin().unwrap().y);
            assert_eq!(3u16, out.end.to_pin().unwrap().x);
            assert_eq!(1u16, out.end.to_pin().unwrap().y);
            assert!(out.rectangle);

            assert_eq!(Result::Success, unsafe {
                crate::selection::ordered(t, &sel, crate::selection::Order::Reverse, &mut out)
            });
            assert_eq!(3u16, out.start.to_pin().unwrap().x);
            assert_eq!(1u16, out.start.to_pin().unwrap().y);
            assert_eq!(1u16, out.end.to_pin().unwrap().x);
            assert_eq!(0u16, out.end.to_pin().unwrap().y);
            assert!(out.rectangle);
            unsafe { free(t) };
        });
    }

    // test "selection_contains"
    #[test]
    fn selection_contains_test() {
        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);

            vtw(t, b"Hello\r\nWorld");

            let mut start_ref = CGridRef::default();
            assert_eq!(Result::Success, unsafe {
                grid_ref(t, pt_active(3, 0), &mut start_ref)
            });

            let mut end_ref = CGridRef::default();
            assert_eq!(Result::Success, unsafe {
                grid_ref(t, pt_active(1, 1), &mut end_ref)
            });

            let linear = crate::selection::CSelection {
                start: start_ref,
                end: end_ref,
                ..crate::selection::CSelection::default()
            };

            let mut contains = false;
            assert_eq!(Result::Success, unsafe {
                crate::selection::contains(t, &linear, pt_active(4, 0), &mut contains)
            });
            assert!(contains);

            assert_eq!(Result::Success, unsafe {
                crate::selection::contains(t, &linear, pt_active(2, 0), &mut contains)
            });
            assert!(!contains);

            let rectangle = crate::selection::CSelection {
                start: start_ref,
                end: end_ref,
                rectangle: true,
                ..crate::selection::CSelection::default()
            };

            assert_eq!(Result::Success, unsafe {
                crate::selection::contains(t, &rectangle, pt_active(2, 0), &mut contains)
            });
            assert!(contains);
            unsafe { free(t) };
        });
    }

    // test "selection_equal"
    #[test]
    fn selection_equal_test() {
        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);
            // try testing.expectEqual(Result.success, new(&test_allocator, &other_t, .{ ... }));
            let (new_result, other_t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);

            vtw(t, b"Hello");
            vtw(other_t, b"Hello");

            let mut start_ref = CGridRef::default();
            assert_eq!(Result::Success, unsafe {
                grid_ref(t, pt_active(0, 0), &mut start_ref)
            });

            let mut end_ref = CGridRef::default();
            assert_eq!(Result::Success, unsafe {
                grid_ref(t, pt_active(1, 0), &mut end_ref)
            });

            let mut other_end_ref = CGridRef::default();
            assert_eq!(Result::Success, unsafe {
                grid_ref(t, pt_active(2, 0), &mut other_end_ref)
            });

            let mut cross_terminal_ref = CGridRef::default();
            assert_eq!(Result::Success, unsafe {
                grid_ref(other_t, pt_active(1, 0), &mut cross_terminal_ref)
            });

            let sel = crate::selection::CSelection {
                start: start_ref,
                end: end_ref,
                ..crate::selection::CSelection::default()
            };
            let equal_sel = crate::selection::CSelection {
                start: start_ref,
                end: end_ref,
                ..crate::selection::CSelection::default()
            };
            let different_endpoint = crate::selection::CSelection {
                start: start_ref,
                end: other_end_ref,
                ..crate::selection::CSelection::default()
            };
            let different_rectangle = crate::selection::CSelection {
                start: start_ref,
                end: end_ref,
                rectangle: true,
                ..crate::selection::CSelection::default()
            };
            let cross_terminal = crate::selection::CSelection {
                start: start_ref,
                end: cross_terminal_ref,
                ..crate::selection::CSelection::default()
            };

            let mut equal = false;
            assert_eq!(Result::Success, unsafe {
                crate::selection::equal(t, &sel, &equal_sel, &mut equal)
            });
            assert!(equal);

            assert_eq!(Result::Success, unsafe {
                crate::selection::equal(t, &sel, &different_endpoint, &mut equal)
            });
            assert!(!equal);

            assert_eq!(Result::Success, unsafe {
                crate::selection::equal(t, &sel, &different_rectangle, &mut equal)
            });
            assert!(!equal);

            assert_eq!(Result::Success, unsafe {
                crate::selection::equal(t, &sel, &cross_terminal, &mut equal)
            });
            assert!(!equal);
            assert_eq!(Result::InvalidValue, unsafe {
                crate::selection::equal(t, &sel, &equal_sel, std::ptr::null_mut())
            });
            unsafe { free(t) };
            unsafe { free(other_t) };
        });
    }

    // test "selection_order invalid values"
    #[test]
    fn selection_order_invalid_values() {
        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);

            let mut order = crate::selection::Order::Forward;
            assert_eq!(Result::InvalidValue, unsafe {
                crate::selection::order(std::ptr::null_mut(), std::ptr::null(), &mut order)
            });
            assert_eq!(Result::InvalidValue, unsafe {
                crate::selection::order(t, std::ptr::null(), &mut order)
            });
            unsafe { free(t) };
        });
    }

    // test "grid_ref"
    #[test]
    fn grid_ref_test() {
        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);

            vtw(t, b"Hello");

            let mut out_ref = CGridRef::default();
            assert_eq!(Result::Success, unsafe {
                grid_ref(t, pt_active(0, 0), &mut out_ref)
            });

            // Extract cell from grid ref and verify it contains 'H'
            let mut out_cell: crate::cell::CCell = 0;
            assert_eq!(Result::Success, unsafe {
                crate::grid_ref::grid_ref_cell(&out_ref, &mut out_cell)
            });

            let mut cp: u32 = 0;
            assert_eq!(Result::Success, unsafe {
                crate::cell::get(
                    out_cell,
                    crate::cell::CellData::Codepoint,
                    &mut cp as *mut _ as *mut c_void,
                )
            });
            assert_eq!(b'H' as u32, cp);
            unsafe { free(t) };
        });
    }

    // test "grid_ref null terminal"
    #[test]
    fn grid_ref_null_terminal() {
        let mut out_ref = CGridRef::default();
        assert_eq!(Result::InvalidValue, unsafe {
            grid_ref(std::ptr::null_mut(), pt_active(0, 0), &mut out_ref)
        });
    }

    // test "point_from_grid_ref roundtrip active"
    #[test]
    fn point_from_grid_ref_roundtrip_active() {
        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);

            vtw(t, b"Hello");

            // Get a grid ref at (2, 0) in active coords
            let mut ref_ = CGridRef::default();
            assert_eq!(Result::Success, unsafe {
                grid_ref(t, pt_active(2, 0), &mut ref_)
            });

            // Convert back to active coords
            let mut coord = Coordinate { x: 0, y: 0 };
            assert_eq!(Result::Success, unsafe {
                point_from_grid_ref(t, &ref_, Tag::Active, &mut coord)
            });
            assert_eq!(2 as size::CellCountInt, coord.x);
            assert_eq!(0u32, coord.y);
            unsafe { free(t) };
        });
    }

    // test "point_from_grid_ref roundtrip viewport"
    #[test]
    fn point_from_grid_ref_roundtrip_viewport() {
        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);

            vtw(t, b"Hello");

            let mut ref_ = CGridRef::default();
            assert_eq!(Result::Success, unsafe {
                grid_ref(t, pt_viewport(0, 0), &mut ref_)
            });

            let mut coord = Coordinate { x: 0, y: 0 };
            assert_eq!(Result::Success, unsafe {
                point_from_grid_ref(t, &ref_, Tag::Viewport, &mut coord)
            });
            assert_eq!(0 as size::CellCountInt, coord.x);
            assert_eq!(0u32, coord.y);
            unsafe { free(t) };
        });
    }

    // test "point_from_grid_ref history ref to active returns no_value"
    #[test]
    fn point_from_grid_ref_history_ref_to_active_returns_no_value() {
        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 4);
            assert_eq!(Result::Success, new_result);

            // Write enough lines to push content into scrollback
            for _ in 0..10 {
                vtw(t, b"line\n");
            }

            // Get a ref to the first line (now in scrollback)
            let mut ref_ = CGridRef::default();
            assert_eq!(Result::Success, unsafe {
                grid_ref(t, pt_screen(0, 0), &mut ref_)
            });

            // Should succeed for screen coords
            let mut coord = Coordinate { x: 0, y: 0 };
            assert_eq!(Result::Success, unsafe {
                point_from_grid_ref(t, &ref_, Tag::Screen, &mut coord)
            });
            assert_eq!(0u32, coord.y);

            // Should fail for active coords (it's in scrollback)
            assert_eq!(Result::NoValue, unsafe {
                point_from_grid_ref(t, &ref_, Tag::Active, &mut coord)
            });
            unsafe { free(t) };
        });
    }

    // test "point_from_grid_ref null terminal"
    #[test]
    fn point_from_grid_ref_null_terminal() {
        let ref_ = CGridRef::default();
        assert_eq!(Result::InvalidValue, unsafe {
            point_from_grid_ref(
                std::ptr::null_mut(),
                &ref_,
                Tag::Active,
                std::ptr::null_mut(),
            )
        });
    }

    // test "point_from_grid_ref null node"
    #[test]
    fn point_from_grid_ref_null_node() {
        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);

            let ref_ = CGridRef::default();
            assert_eq!(Result::InvalidValue, unsafe {
                point_from_grid_ref(t, &ref_, Tag::Active, std::ptr::null_mut())
            });
            unsafe { free(t) };
        });
    }

    // PORT NOTE (callback test scaffolding): the reference declares a per-test
    // `const S = struct { var last_data: ?[]u8 = ...; fn writePty(...) {...} }`
    // capturing into file-local `var`s with `testing.allocator.dupe`. The rustty
    // analog is a per-test module-scope `static mut` plus an `extern "C" fn`; the
    // capture buffer is an owned `Vec<u8>` (freed on test exit). `#[allow(
    // static_mut_refs)]` mirrors the existing trampoline probe above. Each test's
    // statics are isolated to its own `mod`-like block so the parallel test runner
    // never aliases them.

    // test "set write_pty callback DECRQM" (renamed by oracle 9dc0d974e)
    #[test]
    fn set_write_pty_callback_decrqm() {
        static mut LAST_DATA: Option<Vec<u8>> = None;
        static mut LAST_USERDATA: *mut c_void = std::ptr::null_mut();
        static mut CALLS: usize = 0;
        extern "C" fn write_pty(_t: Terminal, ud: *mut c_void, ptr: *const u8, len: usize) {
            let data = unsafe { core::slice::from_raw_parts(ptr, len) }.to_vec();
            unsafe {
                LAST_DATA = Some(data);
                LAST_USERDATA = ud;
                CALLS += 1;
            }
        }

        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);
            unsafe {
                LAST_DATA = None;
                LAST_USERDATA = std::ptr::null_mut();
                CALLS = 0;
            }

            // Set userdata and write_pty callback
            let mut sentinel: u8 = 42;
            assert_eq!(Result::Success, unsafe {
                set(
                    t,
                    Option_::Userdata,
                    &mut sentinel as *mut u8 as *const c_void,
                )
            });
            assert_eq!(Result::Success, unsafe {
                set(t, Option_::WritePty, write_pty as *const c_void)
            });

            // DECRQM for wraparound mode (mode 7, set by default) should trigger write_pty
            vtw(t, b"\x1B[?7$p");
            assert!(unsafe { (*std::ptr::addr_of!(LAST_DATA)).as_ref() }.is_some());
            assert_eq!(
                b"\x1B[?7;1$y".as_slice(),
                unsafe { (*std::ptr::addr_of!(LAST_DATA)).as_ref() }
                    .unwrap()
                    .as_slice()
            );
            assert_eq!(&mut sentinel as *mut u8 as *mut c_void, unsafe {
                LAST_USERDATA
            });
            assert_eq!(1, unsafe { CALLS });

            let query = b"\x1B[4h\x1B[4$p";
            vtw(t, query);
            assert_eq!(2, unsafe { CALLS });
            assert_eq!(
                b"\x1B[4;1$y".as_slice(),
                unsafe { (*std::ptr::addr_of!(LAST_DATA)).as_ref() }
                    .unwrap()
                    .as_slice()
            );
            assert_eq!(&mut sentinel as *mut u8 as *mut c_void, unsafe {
                LAST_USERDATA
            });
            unsafe { free(t) };
        });
    }

    // test "write_pty receives DECRQSS response" (oracle 39ae85f04)
    //
    // Area C §6.5 recorded a BOUNDED UNCERTAINTY: nobody had traced whether the
    // C `vt_write` path delivers a DECRQSS reply through the `write_pty`
    // trampoline. This test answers it definitively — and it does, so no
    // additional C-side plumbing was needed beyond the `dcs_handler` field.
    #[test]
    fn write_pty_receives_decrqss_response() {
        static mut LAST_DATA: Option<Vec<u8>> = None;
        extern "C" fn write_pty(_t: Terminal, _ud: *mut c_void, ptr: *const u8, len: usize) {
            let data = unsafe { core::slice::from_raw_parts(ptr, len) }.to_vec();
            unsafe { LAST_DATA = Some(data) };
        }

        with_test_allocator(|alloc| {
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);
            unsafe { LAST_DATA = None };

            assert_eq!(Result::Success, unsafe {
                set(t, Option_::WritePty, write_pty as *const c_void)
            });

            vtw(t, b"\x1B[1m\x1BP$qm\x1B\\");
            assert_eq!(
                b"\x1BP1$r0;1m\x1B\\".as_slice(),
                unsafe { (*std::ptr::addr_of!(LAST_DATA)).as_ref() }
                    .unwrap()
                    .as_slice()
            );
            unsafe { free(t) };
        });
    }

    // test "write_pty receives OSC color query response" (oracle 14c829883)
    #[test]
    fn write_pty_receives_osc_color_query_response() {
        static mut LAST_DATA: Option<Vec<u8>> = None;
        extern "C" fn write_pty(_t: Terminal, _ud: *mut c_void, ptr: *const u8, len: usize) {
            let data = unsafe { core::slice::from_raw_parts(ptr, len) }.to_vec();
            unsafe {
                LAST_DATA = Some(data);
            }
        }

        with_test_allocator(|alloc| {
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);
            unsafe {
                LAST_DATA = None;
            }

            assert_eq!(Result::Success, unsafe {
                set(t, Option_::WritePty, write_pty as *const c_void)
            });

            let set_fg = b"\x1B]10;rgb:01/02/03\x1B\\";
            vtw(t, set_fg);
            assert!(unsafe { (*std::ptr::addr_of!(LAST_DATA)).as_ref() }.is_none());

            let query_fg = b"\x1B]10;?\x1B\\";
            vtw(t, query_fg);
            assert!(unsafe { (*std::ptr::addr_of!(LAST_DATA)).as_ref() }.is_some());
            assert_eq!(
                b"\x1B]10;rgb:0101/0202/0303\x1B\\".as_slice(),
                unsafe { (*std::ptr::addr_of!(LAST_DATA)).as_ref() }
                    .unwrap()
                    .as_slice()
            );

            unsafe { free(t) };
        });
    }

    // test "set write_pty without callback ignores queries"
    #[test]
    fn set_write_pty_without_callback_ignores_queries() {
        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);
            // Without setting a callback, DECRQM should be silently ignored (no crash)
            vtw(t, b"\x1B[?7$p");
            vtw(t, b"\x1B[4$p");
            unsafe { free(t) };
        });
    }

    // test "set write_pty null clears callback"
    #[test]
    fn set_write_pty_null_clears_callback() {
        static mut CALLED: bool = false;
        extern "C" fn write_pty(_t: Terminal, _ud: *mut c_void, _ptr: *const u8, _len: usize) {
            unsafe { CALLED = true };
        }

        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);
            unsafe { CALLED = false };

            // Set then clear the callback
            assert_eq!(Result::Success, unsafe {
                set(t, Option_::WritePty, write_pty as *const c_void)
            });
            assert_eq!(Result::Success, unsafe {
                set(t, Option_::WritePty, std::ptr::null())
            });

            vtw(t, b"\x1B[?7$p");
            assert!(!unsafe { CALLED });
            unsafe { free(t) };
        });
    }

    // test "set bell callback"
    #[test]
    fn set_bell_callback() {
        static mut BELL_COUNT: usize = 0;
        static mut LAST_USERDATA: *mut c_void = std::ptr::null_mut();
        extern "C" fn bell(_t: Terminal, ud: *mut c_void) {
            unsafe {
                BELL_COUNT += 1;
                LAST_USERDATA = ud;
            }
        }

        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);
            unsafe {
                BELL_COUNT = 0;
                LAST_USERDATA = std::ptr::null_mut();
            }

            // Set userdata and bell callback
            let mut sentinel: u8 = 99;
            assert_eq!(Result::Success, unsafe {
                set(
                    t,
                    Option_::Userdata,
                    &mut sentinel as *mut u8 as *const c_void,
                )
            });
            assert_eq!(Result::Success, unsafe {
                set(t, Option_::Bell, bell as *const c_void)
            });

            // Single BEL
            vtw(t, b"\x07");
            assert_eq!(1usize, unsafe { BELL_COUNT });
            assert_eq!(&mut sentinel as *mut u8 as *mut c_void, unsafe {
                LAST_USERDATA
            });

            // Multiple BELs
            vtw(t, b"\x07\x07");
            assert_eq!(3usize, unsafe { BELL_COUNT });
            unsafe { free(t) };
        });
    }

    // test "bell without callback is silent"
    #[test]
    fn bell_without_callback_is_silent() {
        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);
            // BEL without a callback should not crash
            vtw(t, b"\x07");
            unsafe { free(t) };
        });
    }

    // test "set enquiry callback"
    #[test]
    fn set_enquiry_callback() {
        static mut LAST_DATA: Option<Vec<u8>> = None;
        extern "C" fn write_pty(_t: Terminal, _ud: *mut c_void, ptr: *const u8, len: usize) {
            let data = unsafe { core::slice::from_raw_parts(ptr, len) }.to_vec();
            unsafe { LAST_DATA = Some(data) };
        }
        const RESPONSE: &[u8] = b"OK";
        extern "C" fn enquiry(_t: Terminal, _ud: *mut c_void) -> LibString {
            LibString {
                ptr: RESPONSE.as_ptr(),
                len: RESPONSE.len(),
            }
        }

        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);
            unsafe { LAST_DATA = None };

            assert_eq!(Result::Success, unsafe {
                set(t, Option_::WritePty, write_pty as *const c_void)
            });
            assert_eq!(Result::Success, unsafe {
                set(t, Option_::Enquiry, enquiry as *const c_void)
            });

            // ENQ (0x05) should trigger the enquiry callback and write response via write_pty
            vtw(t, b"\x05");
            assert!(unsafe { (*std::ptr::addr_of!(LAST_DATA)).as_ref() }.is_some());
            assert_eq!(
                b"OK".as_slice(),
                unsafe { (*std::ptr::addr_of!(LAST_DATA)).as_ref() }
                    .unwrap()
                    .as_slice()
            );
            unsafe { free(t) };
        });
    }

    // test "enquiry without callback is silent"
    #[test]
    fn enquiry_without_callback_is_silent() {
        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);
            // ENQ without a callback should not crash
            vtw(t, b"\x05");
            unsafe { free(t) };
        });
    }

    // test "set xtversion callback"
    #[test]
    fn set_xtversion_callback() {
        static mut LAST_DATA: Option<Vec<u8>> = None;
        extern "C" fn write_pty(_t: Terminal, _ud: *mut c_void, ptr: *const u8, len: usize) {
            let data = unsafe { core::slice::from_raw_parts(ptr, len) }.to_vec();
            unsafe { LAST_DATA = Some(data) };
        }
        const VERSION: &[u8] = b"myterm 1.0";
        extern "C" fn xtversion(_t: Terminal, _ud: *mut c_void) -> LibString {
            LibString {
                ptr: VERSION.as_ptr(),
                len: VERSION.len(),
            }
        }

        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);
            unsafe { LAST_DATA = None };

            assert_eq!(Result::Success, unsafe {
                set(t, Option_::WritePty, write_pty as *const c_void)
            });
            assert_eq!(Result::Success, unsafe {
                set(t, Option_::Xtversion, xtversion as *const c_void)
            });

            // XTVERSION: CSI > q
            vtw(t, b"\x1B[>q");
            assert!(unsafe { (*std::ptr::addr_of!(LAST_DATA)).as_ref() }.is_some());
            // Response should be DCS >| version ST
            assert_eq!(
                b"\x1BP>|myterm 1.0\x1B\\".as_slice(),
                unsafe { (*std::ptr::addr_of!(LAST_DATA)).as_ref() }
                    .unwrap()
                    .as_slice()
            );
            unsafe { free(t) };
        });
    }

    // test "xtversion without callback reports default"
    #[test]
    fn xtversion_without_callback_reports_default() {
        static mut LAST_DATA: Option<Vec<u8>> = None;
        extern "C" fn write_pty(_t: Terminal, _ud: *mut c_void, ptr: *const u8, len: usize) {
            let data = unsafe { core::slice::from_raw_parts(ptr, len) }.to_vec();
            unsafe { LAST_DATA = Some(data) };
        }

        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);
            unsafe { LAST_DATA = None };

            // Set write_pty but not xtversion — should get default "libghostty"
            assert_eq!(Result::Success, unsafe {
                set(t, Option_::WritePty, write_pty as *const c_void)
            });

            vtw(t, b"\x1B[>q");
            assert!(unsafe { (*std::ptr::addr_of!(LAST_DATA)).as_ref() }.is_some());
            // PORT NOTE: the reference default version string is "libghostty";
            // rustty retargets it to "librustty" (the rename rule, see
            // `report_xtversion`), so the INDEPENDENT expected value is "librustty".
            assert_eq!(
                b"\x1BP>|librustty\x1B\\".as_slice(),
                unsafe { (*std::ptr::addr_of!(LAST_DATA)).as_ref() }
                    .unwrap()
                    .as_slice()
            );
            unsafe { free(t) };
        });
    }

    // test "set terminfo_name option"
    #[test]
    fn set_terminfo_name_option() {
        static mut LAST_DATA: Option<Vec<u8>> = None;
        fn deinit() {
            // if (last_data) |d| testing.allocator.free(d);
            unsafe { LAST_DATA = None };
        }
        fn last_data() -> Option<&'static Vec<u8>> {
            unsafe { (*std::ptr::addr_of!(LAST_DATA)).as_ref() }
        }
        extern "C" fn write_pty(_t: Terminal, _ud: *mut c_void, ptr: *const u8, len: usize) {
            let data = unsafe { core::slice::from_raw_parts(ptr, len) }.to_vec();
            unsafe { LAST_DATA = Some(data) };
        }

        with_test_allocator(|alloc| {
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);
            unsafe { LAST_DATA = None };
            // defer S.deinit();

            assert_eq!(Result::Success, unsafe {
                set(t, Option_::WritePty, write_pty as *const c_void)
            });

            // While no name is set the query goes unanswered; other capabilities
            // are still served from the static map.
            // const query = "\x1BP+q" ++ std.fmt.bytesToHex("TN", .upper) ++ "\x1B\\";
            let query = b"\x1BP+q544E\x1B\\";
            vtw(t, query);
            // try testing.expect(S.last_data == null);
            assert!(last_data().is_none());
            // const co_query = "\x1BP+q" ++ std.fmt.bytesToHex("Co", .upper) ++ "\x1B\\";
            let co_query = b"\x1BP+q436F\x1B\\";
            vtw(t, co_query);
            // try testing.expectEqualStrings("\x1BP1+r" ++ hex("Co") ++ "=" ++ hex("256") ++ "\x1B\\", S.last_data.?);
            assert_eq!(
                b"\x1BP1+r436F=323536\x1B\\".as_slice(),
                last_data().unwrap().as_slice()
            );
            deinit();

            // The name is copied, so the caller's buffer can go away afterwards.
            // var name: [14]u8 = "xterm-256color".*;
            let mut name: [u8; 14] = *b"xterm-256color";
            let value = LibString {
                ptr: name.as_ptr(),
                len: name.len(),
            };
            assert_eq!(Result::Success, unsafe {
                set(
                    t,
                    Option_::TerminfoName,
                    &value as *const LibString as *const c_void,
                )
            });
            // @memset(&name, 'z');
            name.fill(b'z');

            vtw(t, query);
            // try testing.expect(S.last_data != null);
            assert!(last_data().is_some());
            // try testing.expectEqualStrings("\x1BP1+r" ++ hex("TN") ++ "=" ++ hex("xterm-256color") ++ "\x1B\\", S.last_data.?);
            assert_eq!(
                b"\x1BP1+r544E=787465726D2D323536636F6C6F72\x1B\\".as_slice(),
                last_data().unwrap().as_slice()
            );

            // Clearing with NULL leaves the query unanswered again.
            deinit();
            assert_eq!(Result::Success, unsafe {
                set(t, Option_::TerminfoName, std::ptr::null())
            });
            vtw(t, query);
            assert!(last_data().is_none());

            // Names beyond the maximum are rejected rather than truncated.
            // const long: [Handler.max_terminfo_name_bytes + 1]u8 = @splat('a');
            let long = [b'a'; rustty_vt::stream_terminal::MAX_TERMINFO_NAME_BYTES + 1];
            let long_value = LibString {
                ptr: long.as_ptr(),
                len: long.len(),
            };
            assert_eq!(Result::InvalidValue, unsafe {
                set(
                    t,
                    Option_::TerminfoName,
                    &long_value as *const LibString as *const c_void,
                )
            });
            unsafe { free(t) };
        });
    }

    // test "set title_changed callback"
    #[test]
    fn set_title_changed_callback() {
        static mut TITLE_COUNT: usize = 0;
        static mut LAST_USERDATA: *mut c_void = std::ptr::null_mut();
        extern "C" fn title_changed(_t: Terminal, ud: *mut c_void) {
            unsafe {
                TITLE_COUNT += 1;
                LAST_USERDATA = ud;
            }
        }

        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);
            unsafe {
                TITLE_COUNT = 0;
                LAST_USERDATA = std::ptr::null_mut();
            }

            let mut sentinel: u8 = 77;
            assert_eq!(Result::Success, unsafe {
                set(
                    t,
                    Option_::Userdata,
                    &mut sentinel as *mut u8 as *const c_void,
                )
            });
            assert_eq!(Result::Success, unsafe {
                set(t, Option_::TitleChanged, title_changed as *const c_void)
            });

            // OSC 2 ; title ST — set window title
            vtw(t, b"\x1B]2;Hello\x1B\\");
            assert_eq!(1usize, unsafe { TITLE_COUNT });
            assert_eq!(&mut sentinel as *mut u8 as *mut c_void, unsafe {
                LAST_USERDATA
            });

            // Another title change
            vtw(t, b"\x1B]2;World\x1B\\");
            assert_eq!(2usize, unsafe { TITLE_COUNT });
            unsafe { free(t) };
        });
    }

    // test "title_changed without callback is silent"
    #[test]
    fn title_changed_without_callback_is_silent() {
        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);
            // OSC 2 without a callback should not crash
            vtw(t, b"\x1B]2;Hello\x1B\\");
            unsafe { free(t) };
        });
    }

    // test "kitty_image_medium_temp_file empty output"
    #[test]
    #[cfg_attr(
        not(feature = "kitty-graphics"),
        ignore = "SkipZigTest: needs the kitty-graphics vt feature (R14)"
    )]
    fn kitty_image_medium_temp_file_empty_output() {
        // if (comptime !build_options.kitty_graphics) return error.SkipZigTest;
        // PORT NOTE (R14): the skip is the `cfg_attr(ignore)` above.
        with_test_allocator(|alloc| {
            // var t: Terminal = null;
            // try testing.expectEqual(Result.success, new(&lib.alloc.test_allocator, &t, 80, 24));
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);
            // defer free(t);
            struct FreeTerminal(Terminal);
            impl Drop for FreeTerminal {
                fn drop(&mut self) {
                    unsafe { free(self.0) };
                }
            }
            let _free_t = FreeTerminal(t);

            // const empty: lib.String = .{ .ptr = "", .len = 0 };
            let empty = LibString {
                ptr: rustty_lib::EMPTY.as_ptr(),
                len: 0,
            };
            // try testing.expectEqual(Result.success, set(t, .kitty_image_medium_temp_file, &empty));
            assert_eq!(Result::Success, unsafe {
                set(
                    t,
                    Option_::KittyImageMediumTempFile,
                    core::ptr::addr_of!(empty) as *const c_void,
                )
            });
            // var result: lib.String = undefined;
            let mut result = LibString {
                ptr: core::ptr::null(),
                len: usize::MAX,
            };
            // try testing.expectEqual(Result.success, get(t, .kitty_image_medium_temp_file, &result));
            assert_eq!(Result::Success, unsafe {
                get(
                    t,
                    TerminalData::KittyImageMediumTempFile,
                    core::ptr::addr_of_mut!(result) as *mut c_void,
                )
            });
            // try testing.expectEqual(@as(usize, 0), result.len);
            assert_eq!(0usize, result.len);
            // try testing.expectEqual(@as([*]const u8, ""), result.ptr);
            // PORT NOTE (DECISIONS B1): Zig's `""` is `rustty_lib::EMPTY`.
            assert_eq!(rustty_lib::EMPTY.as_ptr(), result.ptr);
        });
    }

    // test "set pwd_changed callback"
    #[test]
    fn set_pwd_changed_callback() {
        static mut PWD_COUNT: usize = 0;
        static mut LAST_USERDATA: *mut c_void = std::ptr::null_mut();
        extern "C" fn pwd_changed(_t: Terminal, ud: *mut c_void) {
            unsafe {
                PWD_COUNT += 1;
                LAST_USERDATA = ud;
            }
        }

        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);
            unsafe {
                PWD_COUNT = 0;
                LAST_USERDATA = std::ptr::null_mut();
            }

            let mut sentinel: u8 = 88;
            assert_eq!(Result::Success, unsafe {
                set(
                    t,
                    Option_::Userdata,
                    &mut sentinel as *mut u8 as *const c_void,
                )
            });
            assert_eq!(Result::Success, unsafe {
                set(t, Option_::PwdChanged, pwd_changed as *const c_void)
            });

            // OSC 7 ; file:///tmp ST — report pwd
            vtw(t, b"\x1B]7;file:///tmp\x1B\\");
            assert_eq!(1usize, unsafe { PWD_COUNT });
            assert_eq!(&mut sentinel as *mut u8 as *mut c_void, unsafe {
                LAST_USERDATA
            });
            assert_eq!(b"file:///tmp".as_slice(), unsafe {
                (*zig_terminal(t)).get_pwd().unwrap()
            });

            // Another pwd change
            vtw(t, b"\x1B]7;file:///home/user\x1B\\");
            assert_eq!(2usize, unsafe { PWD_COUNT });
            assert_eq!(b"file:///home/user".as_slice(), unsafe {
                (*zig_terminal(t)).get_pwd().unwrap()
            });
            unsafe { free(t) };
        });
    }

    // test "set desktop_notification callback" (oracle c3655ba25)
    #[test]
    fn set_desktop_notification_callback() {
        static mut COUNT: usize = 0;
        static mut LAST_TERMINAL: Terminal = std::ptr::null_mut();
        static mut LAST_USERDATA: *mut c_void = std::ptr::null_mut();
        static mut LAST_SIZE: usize = 0;
        // PER THE BYTE-SLICE RULE: `Vec<u8>`, never `String`. The oracle keeps
        // `[64]u8` + a length; the borrowed slices are raw pty bytes.
        static mut LAST_TITLE: Vec<u8> = Vec::new();
        static mut LAST_BODY: Vec<u8> = Vec::new();

        extern "C" fn desktop_notification(
            terminal_: Terminal,
            ud: *mut c_void,
            n: *const DesktopNotification,
        ) {
            unsafe {
                COUNT += 1;
                LAST_TERMINAL = terminal_;
                LAST_USERDATA = ud;
                LAST_SIZE = (*n).size;
                LAST_TITLE = std::slice::from_raw_parts((*n).title.ptr, (*n).title.len).to_vec();
                LAST_BODY = std::slice::from_raw_parts((*n).body.ptr, (*n).body.len).to_vec();
            }
        }

        with_test_allocator(|alloc| {
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);
            unsafe {
                COUNT = 0;
                LAST_TERMINAL = std::ptr::null_mut();
                LAST_USERDATA = std::ptr::null_mut();
            }

            let mut sentinel: u8 = 88;
            assert_eq!(Result::Success, unsafe {
                set(
                    t,
                    Option_::Userdata,
                    &mut sentinel as *mut u8 as *const c_void,
                )
            });
            assert_eq!(Result::Success, unsafe {
                set(
                    t,
                    Option_::DesktopNotification,
                    desktop_notification as *const c_void,
                )
            });

            // Split OSC 777 across writes to exercise the persistent VT parser.
            vtw(t, b"\x1B]777;notify;Codex;");
            assert_eq!(0usize, unsafe { COUNT });
            vtw(t, b"Needs attention\x1B\\");
            assert_eq!(1usize, unsafe { COUNT });
            assert_eq!(t, unsafe { LAST_TERMINAL });
            assert_eq!(&mut sentinel as *mut u8 as *mut c_void, unsafe {
                LAST_USERDATA
            });
            // The sized-struct contract: `size` is `size_of` the struct rustty
            // built, so a header/library ABI drift shows up here.
            assert_eq!(std::mem::size_of::<DesktopNotification>(), unsafe {
                LAST_SIZE
            });
            assert_eq!(b"Codex".as_slice(), unsafe {
                (&*std::ptr::addr_of!(LAST_TITLE)).as_slice()
            });
            assert_eq!(b"Needs attention".as_slice(), unsafe {
                (&*std::ptr::addr_of!(LAST_BODY)).as_slice()
            });

            // OSC 9 has no title and preserves its body.
            vtw(t, b"\x1B]9;Build complete\x07");
            assert_eq!(2usize, unsafe { COUNT });
            assert_eq!(b"".as_slice(), unsafe {
                (&*std::ptr::addr_of!(LAST_TITLE)).as_slice()
            });
            assert_eq!(b"Build complete".as_slice(), unsafe {
                (&*std::ptr::addr_of!(LAST_BODY)).as_slice()
            });

            // Removing the callback takes effect immediately.
            assert_eq!(Result::Success, unsafe {
                set(t, Option_::DesktopNotification, std::ptr::null())
            });
            vtw(t, b"\x1B]9;Build complete\x07");
            assert_eq!(2usize, unsafe { COUNT });

            unsafe { free(t) };
        });
    }

    // test "set progress_report callback" (oracle 47d602c42)
    #[test]
    fn set_progress_report_callback() {
        static mut COUNT: usize = 0;
        static mut LAST_TERMINAL: Terminal = std::ptr::null_mut();
        static mut LAST_USERDATA: *mut c_void = std::ptr::null_mut();
        static mut LAST_SIZE: usize = 0;
        static mut LAST_STATE: ProgressState = ProgressState::Remove;
        static mut LAST_PROGRESS: i8 = 0;

        extern "C" fn progress_report(
            terminal_: Terminal,
            ud: *mut c_void,
            r: *const ProgressReport,
        ) {
            unsafe {
                COUNT += 1;
                LAST_TERMINAL = terminal_;
                LAST_USERDATA = ud;
                LAST_SIZE = (*r).size;
                LAST_STATE = (*r).state;
                LAST_PROGRESS = (*r).progress;
            }
        }

        with_test_allocator(|alloc| {
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);
            // S.count = 0; S.last_userdata = null; S.last_size = 0;
            // S.last_state = .remove; S.last_progress = -1;
            unsafe {
                COUNT = 0;
                LAST_TERMINAL = std::ptr::null_mut();
                LAST_USERDATA = std::ptr::null_mut();
                LAST_SIZE = 0;
                LAST_STATE = ProgressState::Remove;
                LAST_PROGRESS = -1;
            }

            let mut sentinel: u8 = 88;
            assert_eq!(Result::Success, unsafe {
                set(
                    t,
                    Option_::Userdata,
                    &mut sentinel as *mut u8 as *const c_void,
                )
            });
            assert_eq!(Result::Success, unsafe {
                set(t, Option_::ProgressReport, progress_report as *const c_void)
            });

            // const cases = [_]struct { sequence, state, progress }{
            //     .{ "\x1B]9;4;0;\x1B\\",   .remove,        -1 },
            //     .{ "\x1B]9;4;1;42\x07",   .set,           42 },
            //     .{ "\x1B]9;4;2;7\x1B\\",  .@"error",       7 },
            //     .{ "\x1B]9;4;3\x1B\\",    .indeterminate, -1 },
            //     .{ "\x1B]9;4;4;75\x1B\\", .pause,         75 },
            // };
            // Every state the ABI can carry, including the BEL-terminated form.
            let cases: [(&[u8], ProgressState, i8); 5] = [
                (b"\x1B]9;4;0;\x1B\\", ProgressState::Remove, -1),
                (b"\x1B]9;4;1;42\x07", ProgressState::Set, 42),
                (b"\x1B]9;4;2;7\x1B\\", ProgressState::Error, 7),
                (b"\x1B]9;4;3\x1B\\", ProgressState::Indeterminate, -1),
                (b"\x1B]9;4;4;75\x1B\\", ProgressState::Pause, 75),
            ];

            // for (cases, 1..) |case, expected_count| { ... }
            for (i, (sequence, state, progress)) in cases.iter().enumerate() {
                let expected_count = i + 1;
                // Split at the midpoint to exercise the persistent VT parser.
                let midpoint = sequence.len() / 2;
                vtw(t, &sequence[..midpoint]);
                assert_eq!(expected_count - 1, unsafe { COUNT });
                vtw(t, &sequence[midpoint..]);
                assert_eq!(expected_count, unsafe { COUNT });
                assert_eq!(t, unsafe { LAST_TERMINAL });
                assert_eq!(&mut sentinel as *mut u8 as *mut c_void, unsafe {
                    LAST_USERDATA
                });
                // The sized-struct contract: a header/library ABI drift shows here.
                assert_eq!(std::mem::size_of::<ProgressReport>(), unsafe { LAST_SIZE });
                assert_eq!(*state, unsafe { LAST_STATE });
                // An omitted percentage crosses the ABI as -1, not as 0.
                assert_eq!(*progress, unsafe { LAST_PROGRESS });
            }

            // Removing the callback takes effect immediately.
            assert_eq!(Result::Success, unsafe {
                set(t, Option_::ProgressReport, std::ptr::null())
            });
            vtw(t, b"\x1B]9;4;1;90\x1B\\");
            assert_eq!(cases.len(), unsafe { COUNT });

            unsafe { free(t) };
        });
    }

    // test "set unknown_sequence callback"
    #[test]
    fn set_unknown_sequence_callback() {
        static mut COUNT: usize = 0;
        static mut LAST_TERMINAL: Terminal = std::ptr::null_mut();
        static mut LAST_USERDATA: *mut c_void = std::ptr::null_mut();
        static mut LAST_TAG: UnknownSequenceTag = UnknownSequenceTag::Apc;
        static mut LAST_TRUNCATED: bool = false;
        static mut CONTENT: [u8; 64] = [0; 64];
        static mut CONTENT_LEN: usize = 0;

        extern "C" fn unknown_sequence(
            terminal_: Terminal,
            ud: *mut c_void,
            sequence: *const UnknownSequence,
        ) {
            unsafe {
                COUNT += 1;
                LAST_TERMINAL = terminal_;
                LAST_USERDATA = ud;
                LAST_TAG = (*sequence).tag;
                // const apc_value = sequence.value.apc;
                let apc_value = (*sequence).value.apc;
                LAST_TRUNCATED = apc_value.truncated;
                // content_len = @min(apc_value.content.len, content.len);
                CONTENT_LEN = core::cmp::min(apc_value.content.len, 64);
                // @memcpy(content[0..content_len], apc_value.content.ptr[0..content_len]);
                core::ptr::copy_nonoverlapping(
                    apc_value.content.ptr,
                    core::ptr::addr_of_mut!(CONTENT) as *mut u8,
                    CONTENT_LEN,
                );
            }
        }
        fn content() -> &'static [u8] {
            unsafe {
                core::slice::from_raw_parts(core::ptr::addr_of!(CONTENT) as *const u8, CONTENT_LEN)
            }
        }

        with_test_allocator(|alloc| {
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);
            // S.count = 0; ... (the reference resets its statics)
            unsafe {
                COUNT = 0;
                LAST_TERMINAL = std::ptr::null_mut();
                LAST_USERDATA = std::ptr::null_mut();
                LAST_TAG = UnknownSequenceTag::Apc;
                LAST_TRUNCATED = false;
                CONTENT_LEN = 0;
            }

            // var sentinel: u8 = 101;
            let mut sentinel: u8 = 101;
            assert_eq!(Result::Success, unsafe {
                set(
                    t,
                    Option_::Userdata,
                    &mut sentinel as *mut u8 as *const c_void,
                )
            });

            // const max_bytes: usize = 8;
            let max_bytes: usize = 8;
            assert_eq!(Result::Success, unsafe {
                set(
                    t,
                    Option_::UnknownMaxBytes,
                    &max_bytes as *const usize as *const c_void,
                )
            });
            // try testing.expectEqual(max_bytes, t.?.stream.handler.apc_handler.unknown_max_bytes);
            assert_eq!(max_bytes, unsafe {
                (*t).stream.handler.apc_handler.unknown_max_bytes
            });

            // A byte limit without a callback performs no external effect.
            // const before_callback = "\x1B_abc;xy\x1B\\";
            let before_callback = b"\x1B_abc;xy\x1B\\";
            vtw(t, before_callback);
            assert_eq!(0, unsafe { COUNT });
            // try testing.expect(t.?.stream.handler.unknown_sequence == null);
            assert!(unsafe { (*t).stream.handler.unknown_sequence }.is_none());

            assert_eq!(Result::Success, unsafe {
                set(
                    t,
                    Option_::UnknownSequence,
                    unknown_sequence as *const c_void,
                )
            });
            // try testing.expect(t.?.stream.handler.unknown_sequence != null);
            assert!(unsafe { (*t).stream.handler.unknown_sequence }.is_some());

            // Split a complete APC across writes to exercise persistent parser state.
            // const seq_a = "\x1B_abc;"; const seq_b = "xy\x1B\\";
            let seq_a = b"\x1B_abc;";
            let seq_b = b"xy\x1B\\";
            vtw(t, seq_a);
            assert_eq!(0, unsafe { COUNT });
            vtw(t, seq_b);
            assert_eq!(1, unsafe { COUNT });
            // try testing.expectEqual(t, S.last_terminal);
            assert_eq!(t, unsafe { LAST_TERMINAL });
            // try testing.expectEqual(@as(?*anyopaque, @ptrCast(&sentinel)), S.last_userdata);
            assert_eq!(&mut sentinel as *mut u8 as *mut c_void, unsafe {
                LAST_USERDATA
            });
            // try testing.expectEqual(UnknownSequence.Tag.apc, S.last_tag);
            assert_eq!(UnknownSequenceTag::Apc, unsafe { LAST_TAG });
            // try testing.expect(!S.last_truncated);
            assert!(!unsafe { LAST_TRUNCATED });
            // try testing.expectEqualStrings("abc;xy", S.content[0..S.content_len]);
            assert_eq!(b"abc;xy".as_slice(), content());

            // Content beyond the generic limit is omitted and marked truncated.
            // const truncated = "\x1B_abcdefghijkl\x1B\\";
            let truncated = b"\x1B_abcdefghijkl\x1B\\";
            vtw(t, truncated);
            assert_eq!(2, unsafe { COUNT });
            assert!(unsafe { LAST_TRUNCATED });
            // try testing.expectEqualStrings("abcdefgh", S.content[0..S.content_len]);
            assert_eq!(b"abcdefgh".as_slice(), content());

            // CAN aborts the APC and must not invoke the callback.
            // const aborted = "\x1B_abcdef\x18";
            let aborted = b"\x1B_abcdef\x18";
            vtw(t, aborted);
            assert_eq!(2, unsafe { COUNT });

            // Clearing the callback restores the null fast path immediately.
            assert_eq!(Result::Success, unsafe {
                set(t, Option_::UnknownSequence, std::ptr::null())
            });
            assert!(unsafe { (*t).stream.handler.unknown_sequence }.is_none());
            vtw(t, before_callback);
            assert_eq!(2, unsafe { COUNT });

            // A NULL limit disables capture even after reinstalling the callback.
            assert_eq!(Result::Success, unsafe {
                set(
                    t,
                    Option_::UnknownSequence,
                    unknown_sequence as *const c_void,
                )
            });
            assert_eq!(Result::Success, unsafe {
                set(t, Option_::UnknownMaxBytes, std::ptr::null())
            });
            // try testing.expectEqual(@as(usize, 0), t.?.stream.handler.apc_handler.unknown_max_bytes);
            assert_eq!(0, unsafe {
                (*t).stream.handler.apc_handler.unknown_max_bytes
            });
            vtw(t, before_callback);
            assert_eq!(2, unsafe { COUNT });

            unsafe { free(t) };
        });
    }

    // test "set clipboard_write callback" (oracle 634ef7198; 7ae9b1113 end
    // state: reply-based, with name/granted/can_remember)
    #[test]
    fn set_clipboard_write_callback() {
        static mut COUNT: usize = 0;
        static mut LAST_TERMINAL: Terminal = std::ptr::null_mut();
        static mut LAST_USERDATA: *mut c_void = std::ptr::null_mut();
        static mut LAST_SIZE: usize = 0;
        static mut LAST_LOCATION: clipboard::Location = clipboard::Location::Standard;
        static mut LAST_CONTENTS_NULL: bool = false;
        static mut LAST_CONTENTS_LEN: usize = 0;
        static mut LAST_MIMES: Vec<Vec<u8>> = Vec::new();
        static mut LAST_DATA: Vec<Vec<u8>> = Vec::new();
        static mut LAST_NAME_LEN: usize = 0;
        static mut LAST_GRANTED: bool = true;
        static mut LAST_CAN_REMEMBER: bool = true;
        static mut NEXT_RESULT: clipboard::WriteStatus = clipboard::WriteStatus::Success;
        static mut REPLY_RESULT: Option<clipboard::WriteResult> = None;

        // const Reply = struct { var last: ?clipboard.Write.Result = null; ... };
        // Captures the trampoline's reply for the direct-invoke checks below.
        fn capture_reply(_: *mut c_void, _: *mut c_void, result: clipboard::WriteResult) {
            unsafe { REPLY_RESULT = Some(result) };
        }

        extern "C" fn clipboard_write(
            terminal_: Terminal,
            ud: *mut c_void,
            request: *const ClipboardWrite,
        ) {
            unsafe {
                COUNT += 1;
                LAST_TERMINAL = terminal_;
                LAST_USERDATA = ud;
                LAST_SIZE = (*request).size;
                LAST_LOCATION = (*request).location;
                LAST_CONTENTS_NULL = (*request).contents.is_null();
                LAST_CONTENTS_LEN = (*request).contents_len;
                LAST_NAME_LEN = (*request).name.len;
                LAST_GRANTED = (*request).granted;
                LAST_CAN_REMEMBER = (*request).can_remember;
                let mut mimes: Vec<Vec<u8>> = Vec::new();
                let mut data: Vec<Vec<u8>> = Vec::new();
                if !(*request).contents.is_null() {
                    let contents =
                        std::slice::from_raw_parts((*request).contents, (*request).contents_len);
                    for c in contents {
                        mimes.push(std::slice::from_raw_parts(c.mime.ptr, c.mime.len).to_vec());
                        data.push(std::slice::from_raw_parts(c.data.ptr, c.data.len).to_vec());
                    }
                }
                LAST_MIMES = mimes;
                LAST_DATA = data;

                // request.reply(request, &.{ .size = ..., .result = next_result, .remember = false });
                ((*request).reply)(
                    request,
                    &ClipboardWriteReply {
                        size: std::mem::size_of::<ClipboardWriteReply>(),
                        result: NEXT_RESULT,
                        remember: false,
                    },
                );
            }
        }

        with_test_allocator(|alloc| {
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);
            unsafe {
                COUNT = 0;
                LAST_TERMINAL = std::ptr::null_mut();
                LAST_USERDATA = std::ptr::null_mut();
                NEXT_RESULT = clipboard::WriteStatus::Denied;
                REPLY_RESULT = None;
            }

            let mut sentinel: u8 = 88;
            assert_eq!(Result::Success, unsafe {
                set(
                    t,
                    Option_::Userdata,
                    &mut sentinel as *mut u8 as *const c_void,
                )
            });
            assert_eq!(Result::Success, unsafe {
                set(t, Option_::ClipboardWrite, clipboard_write as *const c_void)
            });

            // Split OSC 52 write whose decoded payload contains an embedded NUL.
            vtw(t, b"\x1B]52;c;aGVs");
            vtw(t, b"bG8Ad29ybGQ=\x1B\\");
            assert_eq!(1, unsafe { COUNT });
            assert_eq!(t, unsafe { LAST_TERMINAL });
            assert_eq!(&mut sentinel as *mut u8 as *mut c_void, unsafe {
                LAST_USERDATA
            });
            assert_eq!(std::mem::size_of::<ClipboardWrite>(), unsafe { LAST_SIZE });
            assert_eq!(clipboard::Location::Standard, unsafe { LAST_LOCATION });
            assert!(!unsafe { LAST_CONTENTS_NULL });
            assert_eq!(1, unsafe { LAST_CONTENTS_LEN });
            assert_eq!(b"text/plain".as_slice(), unsafe {
                (&*std::ptr::addr_of!(LAST_MIMES))[0].as_slice()
            });
            assert_eq!(b"hello\x00world".as_slice(), unsafe {
                (&*std::ptr::addr_of!(LAST_DATA))[0].as_slice()
            });

            // OSC 52 carries no program identity or password grant state.
            assert_eq!(0, unsafe { LAST_NAME_LEN });
            assert!(!unsafe { LAST_GRANTED });
            assert!(!unsafe { LAST_CAN_REMEMBER });

            // OSC 52 destinations are normalized rather than exposed as wire bytes.
            for (selector, expected) in [
                (b's', clipboard::Location::Selection),
                (b'p', clipboard::Location::Primary),
                (b'q', clipboard::Location::Standard),
            ] {
                let seq = [
                    0x1B, b']', b'5', b'2', b';', selector, b';', b'e', b'A', b'=', b'=', 0x1B,
                    b'\\',
                ];
                vtw(t, &seq);
                assert_eq!(expected, unsafe { LAST_LOCATION });
            }
            assert_eq!(4, unsafe { COUNT });

            // An empty content list is a clear and retains a null descriptor pointer.
            vtw(t, b"\x1B]52;s;\x1B\\");
            assert_eq!(5, unsafe { COUNT });
            assert_eq!(clipboard::Location::Selection, unsafe { LAST_LOCATION });
            assert!(unsafe { LAST_CONTENTS_NULL });
            assert_eq!(0, unsafe { LAST_CONTENTS_LEN });

            // Read requests and malformed base64 must never reach the callback.
            vtw(t, b"\x1B]52;c;?\x1B\\");
            vtw(t, b"\x1B]52;c;%%%\x1B\\");
            assert_eq!(5, unsafe { COUNT });

            // iTerm2 Copy reaches the same normalized callback.
            vtw(t, b"\x1B]1337;Copy=:aVRlcm0=\x1B\\");
            assert_eq!(6, unsafe { COUNT });
            assert_eq!(clipboard::Location::Standard, unsafe { LAST_LOCATION });
            assert_eq!(b"text/plain".as_slice(), unsafe {
                (&*std::ptr::addr_of!(LAST_MIMES))[0].as_slice()
            });
            assert_eq!(b"iTerm".as_slice(), unsafe {
                (&*std::ptr::addr_of!(LAST_DATA))[0].as_slice()
            });

            // Every representation is converted, and callback replies propagate
            // back through the C trampoline for protocols that can acknowledge
            // writes.
            // (Direct invoke of the INSTALLED handler effect — the dynamic
            // trampoline — mirroring the oracle's
            // `handler.effects.clipboard_write.?(handler, ...)`.)
            let internal_contents = [
                clipboard::Content {
                    mime: b"text/plain",
                    data: b"plain",
                },
                clipboard::Content {
                    mime: b"application/octet-stream",
                    data: b"a\x00b",
                },
                clipboard::Content {
                    mime: b"text/html",
                    data: b"<b>plain</b>",
                },
                clipboard::Content {
                    mime: b"text/rtf",
                    data: b"{\\rtf1 plain}",
                },
                clipboard::Content {
                    mime: b"image/png",
                    data: b"\x89PNG",
                },
            ];
            unsafe { NEXT_RESULT = clipboard::WriteStatus::Busy };
            // const handler = &t.?.stream.handler;
            // handler.effects.clipboard_write.?(handler, .{ ... });
            //
            // SB: take the handler address ONCE via a raw place projection;
            // the `.reply_ctx = handler` pointer and the `&mut` argument must
            // not be two separate overlapping retags.
            let installed = unsafe { (*t).stream.handler.effects.clipboard_write }.unwrap();
            let handler_ptr: *mut Handler<'static> =
                unsafe { std::ptr::addr_of_mut!((*t).stream.handler) };
            installed(
                unsafe { &mut *handler_ptr },
                clipboard::Write {
                    location: clipboard::Location::Primary,
                    contents: &internal_contents,
                    name: b"app",
                    granted: true,
                    can_remember: true,
                    reply_ctx: handler_ptr as *mut c_void,
                    reply_fn: capture_reply,
                },
            );
            // try testing.expect(Reply.last.? == .busy);
            assert_eq!(Some(clipboard::WriteResult::Busy), unsafe { REPLY_RESULT });
            assert_eq!(3, unsafe { LAST_NAME_LEN });
            assert!(unsafe { LAST_GRANTED });
            assert!(unsafe { LAST_CAN_REMEMBER });
            assert_eq!(7, unsafe { COUNT });
            assert_eq!(5, unsafe { LAST_CONTENTS_LEN });
            assert_eq!(b"application/octet-stream".as_slice(), unsafe {
                (&*std::ptr::addr_of!(LAST_MIMES))[1].as_slice()
            });
            assert_eq!(b"a\x00b".as_slice(), unsafe {
                (&*std::ptr::addr_of!(LAST_DATA))[1].as_slice()
            });
            assert_eq!(b"image/png".as_slice(), unsafe {
                (&*std::ptr::addr_of!(LAST_MIMES))[4].as_slice()
            });
            assert_eq!(b"\x89PNG".as_slice(), unsafe {
                (&*std::ptr::addr_of!(LAST_DATA))[4].as_slice()
            });

            // Removing the callback takes effect immediately and uninstalls
            // the trampoline.
            assert_eq!(Result::Success, unsafe {
                set(t, Option_::ClipboardWrite, std::ptr::null())
            });
            // try testing.expect(t.?.stream.handler.effects.clipboard_write == null);
            assert!(unsafe { (*t).stream.handler.effects.clipboard_write }.is_none());
            vtw(t, b"\x1B]52;c;eA==\x1B\\");
            assert_eq!(7, unsafe { COUNT });

            unsafe {
                LAST_MIMES = Vec::new();
                LAST_DATA = Vec::new();
                free(t);
            }
        });
    }

    // test "clipboard_write without callback is unsupported and silent"
    #[test]
    fn clipboard_write_without_callback_is_unsupported_and_silent() {
        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);

            // OSC 52 without a callback should not crash
            vtw(t, b"\x1B]52;c;aGVsbG8=\x1B\\");

            // No trampoline is installed until a callback is set, so the
            // stream skips clipboard work (and never spools a Kitty clipboard
            // transaction it can't deliver).
            // try testing.expect(t.?.stream.handler.effects.clipboard_write == null);
            assert!(unsafe { (*t).stream.handler.effects.clipboard_write }.is_none());

            unsafe { free(t) };
        });
    }

    // test "kitty clipboard write via C effects" (oracle 6959fd46c/4f49dc2b8/
    // 7ae9b1113 end state)
    #[test]
    fn kitty_clipboard_write_via_c_effects() {
        // const S = struct { var responses: [512]u8 = undefined; ... };
        // PORT NOTE: the fixed `[512]u8` capture buffers become `Vec`s (the
        // crate's test-capture convention); every assertion is carried.
        static mut RESPONSES: Vec<u8> = Vec::new();
        static mut WRITE_COUNT: usize = 0;
        static mut LAST_LOCATION: clipboard::Location = clipboard::Location::Standard;
        static mut LAST_CONTENTS_LEN: usize = 0;
        static mut LAST_MIMES: Vec<Vec<u8>> = Vec::new();
        static mut LAST_DATA: Vec<Vec<u8>> = Vec::new();
        static mut LAST_GRANTED: bool = true;
        static mut LAST_CAN_REMEMBER: bool = true;
        static mut NEXT_REMEMBER: bool = false;

        extern "C" fn write_pty(_: Terminal, _: *mut c_void, ptr: *const u8, len: usize) {
            unsafe {
                (*std::ptr::addr_of_mut!(RESPONSES))
                    .extend_from_slice(std::slice::from_raw_parts(ptr, len));
            }
        }

        extern "C" fn clipboard_write(_: Terminal, _: *mut c_void, request: *const ClipboardWrite) {
            unsafe {
                WRITE_COUNT += 1;
                LAST_LOCATION = (*request).location;
                LAST_CONTENTS_LEN = (*request).contents_len;
                let mut mimes: Vec<Vec<u8>> = Vec::new();
                let mut data: Vec<Vec<u8>> = Vec::new();
                if !(*request).contents.is_null() {
                    let contents =
                        std::slice::from_raw_parts((*request).contents, (*request).contents_len);
                    for c in contents {
                        mimes.push(std::slice::from_raw_parts(c.mime.ptr, c.mime.len).to_vec());
                        data.push(std::slice::from_raw_parts(c.data.ptr, c.data.len).to_vec());
                    }
                }
                LAST_MIMES = mimes;
                LAST_DATA = data;
                LAST_GRANTED = (*request).granted;
                LAST_CAN_REMEMBER = (*request).can_remember;
                ((*request).reply)(
                    request,
                    &ClipboardWriteReply {
                        size: std::mem::size_of::<ClipboardWriteReply>(),
                        result: clipboard::WriteStatus::Success,
                        remember: NEXT_REMEMBER,
                    },
                );
            }
        }

        // const R = struct { ... fn clipboardRead(...) ... };
        static mut R_COUNT: usize = 0;
        static mut R_LAST_MIMES_LEN: usize = 0;
        static mut R_LAST_MIME_IS_TEXT: bool = false;
        static mut R_LAST_LIST: bool = true;
        static mut R_LAST_NAME_LEN: usize = 0;
        static mut R_LAST_GRANTED: bool = true;
        static mut R_LAST_CAN_REMEMBER: bool = true;

        extern "C" fn clipboard_read(_: Terminal, _: *mut c_void, request: *const ClipboardRead) {
            unsafe {
                R_COUNT += 1;
                R_LAST_MIMES_LEN = (*request).mimes_len;
                R_LAST_MIME_IS_TEXT = (*request).mimes_len > 0 && {
                    let m = *(*request).mimes;
                    std::slice::from_raw_parts(m.ptr, m.len) == b"text/plain"
                };
                R_LAST_LIST = (*request).list;
                R_LAST_NAME_LEN = (*request).name.len;
                R_LAST_GRANTED = (*request).granted;
                R_LAST_CAN_REMEMBER = (*request).can_remember;

                let mime: &[u8] = b"text/plain";
                let data: &[u8] = b"hello";
                let contents = [ClipboardContent {
                    mime: LibString::init(mime),
                    data: LibString::init(data),
                }];
                ((*request).reply)(
                    request,
                    &ClipboardReadReply {
                        size: std::mem::size_of::<ClipboardReadReply>(),
                        result: clipboard::ReadStatus::Success,
                        contents: contents.as_ptr(),
                        contents_len: contents.len(),
                        available: std::ptr::null(),
                        available_len: 0,
                        remember: false,
                    },
                );
            }
        }

        with_test_allocator(|alloc| {
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);
            unsafe {
                RESPONSES = Vec::new();
                WRITE_COUNT = 0;
                LAST_MIMES = Vec::new();
                LAST_DATA = Vec::new();
                LAST_GRANTED = true;
                LAST_CAN_REMEMBER = true;
                NEXT_REMEMBER = false;
                R_COUNT = 0;
            }

            assert_eq!(Result::Success, unsafe {
                set(t, Option_::WritePty, write_pty as *const c_void)
            });
            assert_eq!(Result::Success, unsafe {
                set(t, Option_::ClipboardWrite, clipboard_write as *const c_void)
            });

            // A full OSC 5522 write transaction: begin, chunked data for two
            // representations, commit. Only the commit invokes the callback,
            // and its result maps to the DONE response.
            let seqs: [&[u8]; 5] = [
                b"\x1B]5522;type=write:id=c1\x1B\\",
                b"\x1B]5522;type=wdata:mime=dGV4dC9wbGFpbg==;R2hvc3Q=\x1B\\", // "Ghost"
                b"\x1B]5522;type=wdata:mime=dGV4dC9wbGFpbg==;dHk=\x1B\\",     // "ty"
                b"\x1B]5522;type=wdata:mime=dGV4dC9odG1s;PGI+aGk8L2I+\x1B\\", // "<b>hi</b>"
                b"\x1B]5522;type=wdata\x1B\\",
            ];
            for seq in seqs {
                vtw(t, seq);
            }

            assert_eq!(1, unsafe { WRITE_COUNT });
            assert_eq!(clipboard::Location::Standard, unsafe { LAST_LOCATION });
            assert_eq!(2, unsafe { LAST_CONTENTS_LEN });
            assert_eq!(b"text/plain".as_slice(), unsafe {
                (&*std::ptr::addr_of!(LAST_MIMES))[0].as_slice()
            });
            assert_eq!(b"Ghostty".as_slice(), unsafe {
                (&*std::ptr::addr_of!(LAST_DATA))[0].as_slice()
            });
            assert_eq!(b"text/html".as_slice(), unsafe {
                (&*std::ptr::addr_of!(LAST_MIMES))[1].as_slice()
            });
            assert_eq!(b"<b>hi</b>".as_slice(), unsafe {
                (&*std::ptr::addr_of!(LAST_DATA))[1].as_slice()
            });
            assert_eq!(
                b"\x1B]5522;type=write:status=DONE:id=c1\x1B\\".as_slice(),
                unsafe { (&*std::ptr::addr_of!(RESPONSES)).as_slice() }
            );
            assert!(!unsafe { LAST_GRANTED });
            assert!(!unsafe { LAST_CAN_REMEMBER });

            // Password grants round-trip through the C reply: the first pw'd
            // commit isn't granted and asks to remember, so the next one
            // arrives granted.
            unsafe {
                RESPONSES = Vec::new();
                NEXT_REMEMBER = true;
            }
            let grant_seqs: [&[u8]; 4] = [
                b"\x1B]5522;type=write:id=g1:pw=c2VjcmV0:name=YXBw\x1B\\",
                b"\x1B]5522;type=wdata\x1B\\",
                b"\x1B]5522;type=write:id=g2:pw=c2VjcmV0:name=YXBw\x1B\\",
                b"\x1B]5522;type=wdata\x1B\\",
            ];
            for seq in grant_seqs {
                vtw(t, seq);
            }
            assert_eq!(3, unsafe { WRITE_COUNT });
            assert!(unsafe { LAST_GRANTED });
            assert!(unsafe { LAST_CAN_REMEMBER });
            assert_eq!(
                b"\x1B]5522;type=write:status=DONE:id=g1\x1B\\\x1B]5522;type=write:status=DONE:id=g2\x1B\\".as_slice(),
                unsafe { (&*std::ptr::addr_of!(RESPONSES)).as_slice() }
            );
            unsafe { NEXT_REMEMBER = false };

            // Without a read callback reads are denied.
            unsafe { RESPONSES = Vec::new() };
            vtw(t, b"\x1B]5522;type=read:id=r1;dGV4dC9wbGFpbg==\x1B\\");
            assert_eq!(3, unsafe { WRITE_COUNT });
            assert_eq!(
                b"\x1B]5522;type=read:status=EPERM:id=r1\x1B\\".as_slice(),
                unsafe { (&*std::ptr::addr_of!(RESPONSES)).as_slice() }
            );

            // With a read callback the request is served through it.
            assert_eq!(Result::Success, unsafe {
                set(t, Option_::ClipboardRead, clipboard_read as *const c_void)
            });
            unsafe { RESPONSES = Vec::new() };
            // name="app" without a password: forwarded for prompts, not
            // rememberable.
            vtw(
                t,
                b"\x1B]5522;type=read:id=r2:name=YXBw;dGV4dC9wbGFpbg==\x1B\\",
            );
            assert_eq!(1, unsafe { R_COUNT });
            assert_eq!(1, unsafe { R_LAST_MIMES_LEN });
            assert!(unsafe { R_LAST_MIME_IS_TEXT });
            assert!(!unsafe { R_LAST_LIST });
            assert_eq!(3, unsafe { R_LAST_NAME_LEN });
            assert!(!unsafe { R_LAST_GRANTED });
            assert!(!unsafe { R_LAST_CAN_REMEMBER });
            assert_eq!(
                b"\x1B]5522;type=read:status=OK:id=r2\x1B\\\x1B]5522;type=read:status=DATA:id=r2:mime=dGV4dC9wbGFpbg==;aGVsbG8=\x1B\\\x1B]5522;type=read:status=DONE:id=r2\x1B\\".as_slice(),
                unsafe { (&*std::ptr::addr_of!(RESPONSES)).as_slice() }
            );

            // Without a clipboard callback the transaction fails up front.
            assert_eq!(Result::Success, unsafe {
                set(t, Option_::ClipboardWrite, std::ptr::null())
            });
            unsafe { RESPONSES = Vec::new() };
            vtw(t, b"\x1B]5522;type=write:id=c2\x1B\\");
            assert_eq!(
                b"\x1B]5522;type=write:status=ENOSYS:id=c2\x1B\\".as_slice(),
                unsafe { (&*std::ptr::addr_of!(RESPONSES)).as_slice() }
            );

            unsafe {
                RESPONSES = Vec::new();
                LAST_MIMES = Vec::new();
                LAST_DATA = Vec::new();
                free(t);
            }
        });
    }

    // test "set clipboard write max bytes" (oracle 5350d4a5f/70f006575 end
    // state: reject-not-truncate + configurable limit)
    #[test]
    fn set_clipboard_write_max_bytes() {
        static mut RESPONSES: Vec<u8> = Vec::new();
        static mut WRITE_COUNT: usize = 0;

        extern "C" fn write_pty(_: Terminal, _: *mut c_void, ptr: *const u8, len: usize) {
            unsafe {
                (*std::ptr::addr_of_mut!(RESPONSES))
                    .extend_from_slice(std::slice::from_raw_parts(ptr, len));
            }
        }

        extern "C" fn clipboard_write(_: Terminal, _: *mut c_void, request: *const ClipboardWrite) {
            unsafe {
                WRITE_COUNT += 1;
                ((*request).reply)(
                    request,
                    &ClipboardWriteReply {
                        size: std::mem::size_of::<ClipboardWriteReply>(),
                        result: clipboard::WriteStatus::Success,
                        remember: false,
                    },
                );
            }
        }

        with_test_allocator(|alloc| {
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);
            unsafe {
                RESPONSES = Vec::new();
                WRITE_COUNT = 0;
            }

            assert_eq!(Result::Success, unsafe {
                set(t, Option_::WritePty, write_pty as *const c_void)
            });
            assert_eq!(Result::Success, unsafe {
                set(t, Option_::ClipboardWrite, clipboard_write as *const c_void)
            });

            // The built-in default reads back.
            let mut max: usize = 0;
            assert_eq!(Result::Success, unsafe {
                get(
                    t,
                    TerminalData::ClipboardWriteMaxBytes,
                    &mut max as *mut usize as *mut c_void,
                )
            });
            assert_eq!(rustty_vt::kitty::clipboard::max_write_size, max);

            // Set a tiny limit; an oversized text write fails with EFBIG and
            // never reaches the callback.
            let limit: usize = 4;
            assert_eq!(Result::Success, unsafe {
                set(
                    t,
                    Option_::ClipboardWriteMaxBytes,
                    &limit as *const usize as *const c_void,
                )
            });
            assert_eq!(Result::Success, unsafe {
                get(
                    t,
                    TerminalData::ClipboardWriteMaxBytes,
                    &mut max as *mut usize as *mut c_void,
                )
            });
            assert_eq!(limit, max);

            let seqs: [&[u8]; 4] = [
                b"\x1B]5522;type=write:id=c1\x1B\\",
                b"\x1B]5522;type=wdata:mime=dGV4dC9wbGFpbg==;SGVsbA==\x1B\\", // "Hell"
                b"\x1B]5522;type=wdata:mime=dGV4dC9wbGFpbg==;bw==\x1B\\",     // "o"
                b"\x1B]5522;type=wdata\x1B\\",
            ];
            for seq in seqs {
                vtw(t, seq);
            }
            assert_eq!(0, unsafe { WRITE_COUNT });
            assert_eq!(
                b"\x1B]5522;type=write:status=EFBIG:id=c1\x1B\\".as_slice(),
                unsafe { (&*std::ptr::addr_of!(RESPONSES)).as_slice() }
            );

            // A NULL value reverts to the built-in default.
            assert_eq!(Result::Success, unsafe {
                set(t, Option_::ClipboardWriteMaxBytes, std::ptr::null())
            });
            assert_eq!(Result::Success, unsafe {
                get(
                    t,
                    TerminalData::ClipboardWriteMaxBytes,
                    &mut max as *mut usize as *mut c_void,
                )
            });
            assert_eq!(rustty_vt::kitty::clipboard::max_write_size, max);

            unsafe {
                RESPONSES = Vec::new();
                free(t);
            }
        });
    }

    // test "set clipboard_read callback" (oracle e03475c0c)
    #[test]
    fn set_clipboard_read_callback() {
        static mut LAST_DATA: Option<Vec<u8>> = None;
        static mut COUNT: usize = 0;
        static mut LAST_SIZE: usize = 0;
        static mut LAST_LOCATION: clipboard::Location = clipboard::Location::Standard;
        static mut LAST_MIMES_LEN: usize = 0;
        static mut LAST_MIME_IS_TEXT: bool = false;
        static mut LAST_LIST: bool = true;
        static mut LAST_NAME_LEN: usize = 1;
        static mut LAST_GRANTED: bool = true;
        static mut LAST_CAN_REMEMBER: bool = true;
        static mut RESULT: clipboard::ReadStatus = clipboard::ReadStatus::Success;

        extern "C" fn write_pty(_: Terminal, _: *mut c_void, ptr: *const u8, len: usize) {
            unsafe {
                LAST_DATA = Some(std::slice::from_raw_parts(ptr, len).to_vec());
            }
        }

        extern "C" fn clipboard_read(_: Terminal, _: *mut c_void, request: *const ClipboardRead) {
            unsafe {
                COUNT += 1;
                LAST_SIZE = (*request).size;
                LAST_LOCATION = (*request).location;
                LAST_MIMES_LEN = (*request).mimes_len;
                LAST_MIME_IS_TEXT = (*request).mimes_len > 0 && {
                    let m = *(*request).mimes;
                    std::slice::from_raw_parts(m.ptr, m.len) == b"text/plain"
                };
                LAST_LIST = (*request).list;
                LAST_NAME_LEN = (*request).name.len;
                LAST_GRANTED = (*request).granted;
                LAST_CAN_REMEMBER = (*request).can_remember;

                let mime: &[u8] = b"text/plain";
                let data: &[u8] = b"hello";
                let contents = [ClipboardContent {
                    mime: LibString::init(mime),
                    data: LibString::init(data),
                }];
                ((*request).reply)(
                    request,
                    &ClipboardReadReply {
                        size: std::mem::size_of::<ClipboardReadReply>(),
                        result: RESULT,
                        contents: contents.as_ptr(),
                        contents_len: contents.len(),
                        available: std::ptr::null(),
                        available_len: 0,
                        remember: false,
                    },
                );
            }
        }

        with_test_allocator(|alloc| {
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);
            unsafe {
                LAST_DATA = None;
                COUNT = 0;
                RESULT = clipboard::ReadStatus::Success;
            }

            assert_eq!(Result::Success, unsafe {
                set(t, Option_::WritePty, write_pty as *const c_void)
            });

            // Without a callback the handler effect is unset and reads are silent.
            assert!(unsafe { (*t).stream.handler.effects.clipboard_read }.is_none());
            vtw(t, b"\x1B]52;c;?\x1B\\");
            assert!(unsafe { (&*std::ptr::addr_of!(LAST_DATA)).is_none() });

            assert_eq!(Result::Success, unsafe {
                set(t, Option_::ClipboardRead, clipboard_read as *const c_void)
            });
            assert!(unsafe { (*t).stream.handler.effects.clipboard_read }.is_some());

            vtw(t, b"\x1B]52;p;?\x07");
            assert_eq!(1, unsafe { COUNT });
            assert_eq!(std::mem::size_of::<ClipboardRead>(), unsafe { LAST_SIZE });
            assert_eq!(clipboard::Location::Primary, unsafe { LAST_LOCATION });
            assert_eq!(1, unsafe { LAST_MIMES_LEN });
            assert!(unsafe { LAST_MIME_IS_TEXT });
            assert!(!unsafe { LAST_LIST });
            assert_eq!(0, unsafe { LAST_NAME_LEN });
            assert!(!unsafe { LAST_GRANTED });
            assert!(!unsafe { LAST_CAN_REMEMBER });
            assert_eq!(b"\x1B]52;p;aGVsbG8=\x07".as_slice(), unsafe {
                (&*std::ptr::addr_of!(LAST_DATA)).as_deref().unwrap()
            });

            // Denied replies with an empty clipboard.
            unsafe { RESULT = clipboard::ReadStatus::Denied };
            vtw(t, b"\x1B]52;c;?\x1B\\");
            assert_eq!(2, unsafe { COUNT });
            assert_eq!(b"\x1B]52;c;\x1B\\".as_slice(), unsafe {
                (&*std::ptr::addr_of!(LAST_DATA)).as_deref().unwrap()
            });

            // Clearing the callback uninstalls the handler effect.
            assert_eq!(Result::Success, unsafe {
                set(t, Option_::ClipboardRead, std::ptr::null())
            });
            assert!(unsafe { (*t).stream.handler.effects.clipboard_read }.is_none());

            unsafe {
                LAST_DATA = None;
                free(t);
            }
        });
    }

    // Parity S166-F1 (no reference test): the reference writes an OSC 52
    // read reply's response from inside the reply (`ClipboardReadState.reply`
    // -> `respond` -> `handler.writePty`), so a C embedder sees its
    // write_pty callback run before its `reply` call returns. The C reply
    // trampoline reaches the handler through the pointer the read trampoline
    // stored in `ClipboardReadCtx`.
    #[test]
    fn clipboard_read_c_reply_writes_the_response_inside_reply() {
        static mut LOG: Vec<u8> = Vec::new();
        static mut LOG_LEN_AFTER_REPLY: Option<usize> = None;

        extern "C" fn write_pty(_: Terminal, _: *mut c_void, ptr: *const u8, len: usize) {
            unsafe {
                (&mut *std::ptr::addr_of_mut!(LOG))
                    .extend_from_slice(std::slice::from_raw_parts(ptr, len));
            }
        }

        extern "C" fn clipboard_read(_: Terminal, _: *mut c_void, request: *const ClipboardRead) {
            unsafe {
                let contents = [ClipboardContent {
                    mime: LibString::init(b"text/plain"),
                    data: LibString::init(b"hello"),
                }];
                ((*request).reply)(
                    request,
                    &ClipboardReadReply {
                        size: std::mem::size_of::<ClipboardReadReply>(),
                        result: clipboard::ReadStatus::Success,
                        contents: contents.as_ptr(),
                        contents_len: contents.len(),
                        available: std::ptr::null(),
                        available_len: 0,
                        remember: false,
                    },
                );
                LOG_LEN_AFTER_REPLY = Some((&*std::ptr::addr_of!(LOG)).len());
            }
        }

        with_test_allocator(|alloc| {
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);
            unsafe {
                LOG = Vec::new();
                LOG_LEN_AFTER_REPLY = None;
            }
            assert_eq!(Result::Success, unsafe {
                set(t, Option_::WritePty, write_pty as *const c_void)
            });
            assert_eq!(Result::Success, unsafe {
                set(t, Option_::ClipboardRead, clipboard_read as *const c_void)
            });

            vtw(t, b"\x1B]52;c;?\x1B\\");
            let expected: &[u8] = b"\x1B]52;c;aGVsbG8=\x1B\\";
            assert_eq!(Some(expected.len()), unsafe { LOG_LEN_AFTER_REPLY });
            assert_eq!(expected, unsafe { (&*std::ptr::addr_of!(LOG)).as_slice() });

            unsafe {
                LOG = Vec::new();
                free(t);
            }
        });
    }

    // test "mode 2048 enable and disable use C callbacks" (oracle 07af4612b)
    #[test]
    fn mode_2048_enable_and_disable_use_c_callbacks() {
        static mut DATA: [u8; 128] = [0; 128];
        static mut LEN: usize = 0;
        static mut CALLS: usize = 0;

        extern "C" fn write_pty(_: Terminal, _: *mut c_void, ptr: *const u8, length: usize) {
            unsafe {
                let dst = std::ptr::addr_of_mut!(DATA) as *mut u8;
                std::ptr::copy_nonoverlapping(ptr, dst, length);
                LEN = length;
                CALLS += 1;
            }
        }

        extern "C" fn size_cb(
            _: Terminal,
            _: *mut c_void,
            out_size: *mut size_report::Size,
        ) -> bool {
            unsafe {
                *out_size = size_report::Size {
                    rows: 24,
                    columns: 80,
                    cell_width: 8,
                    cell_height: 16,
                };
            }
            true
        }

        with_test_allocator(|alloc| {
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);
            unsafe {
                LEN = 0;
                CALLS = 0;
            }

            assert_eq!(Result::Success, unsafe {
                set(t, Option_::WritePty, write_pty as *const c_void)
            });
            assert_eq!(Result::Success, unsafe {
                set(t, Option_::SizeCb, size_cb as *const c_void)
            });

            vtw(t, b"\x1B[?2048h");
            vtw(t, b"\x1B[?2048h");

            assert_eq!(2, unsafe { CALLS });
            assert_eq!(b"\x1B[48;24;80;384;640t".as_slice(), unsafe {
                &(&*std::ptr::addr_of!(DATA))[..LEN]
            });
            vtw(t, b"\x1B[?2048l");

            assert_eq!(2, unsafe { CALLS });
            // try testing.expect(!t.?.terminal.modes.get(.in_band_size_reports));
            assert!(!unsafe {
                (*(*t).terminal)
                    .modes
                    .get(rustty_vt::modes::Mode::InBandSizeReports)
            });

            unsafe { free(t) };
        });
    }

    // test "mode 2048 enable tolerates missing C callbacks" (oracle 07af4612b)
    #[test]
    fn mode_2048_enable_tolerates_missing_c_callbacks() {
        static mut WRITES: usize = 0;
        static mut SIZES: usize = 0;

        extern "C" fn write_pty(_: Terminal, _: *mut c_void, _: *const u8, _: usize) {
            unsafe { WRITES += 1 };
        }

        extern "C" fn size_cb(
            _: Terminal,
            _: *mut c_void,
            out_size: *mut size_report::Size,
        ) -> bool {
            unsafe {
                SIZES += 1;
                *out_size = size_report::Size {
                    rows: 24,
                    columns: 80,
                    cell_width: 8,
                    cell_height: 16,
                };
            }
            true
        }

        with_test_allocator(|alloc| {
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);
            unsafe {
                WRITES = 0;
                SIZES = 0;
            }

            assert_eq!(Result::Success, unsafe {
                set(t, Option_::WritePty, write_pty as *const c_void)
            });

            vtw(t, b"\x1B[?2048h");

            assert_eq!(0, unsafe { WRITES });
            assert!(unsafe {
                (*(*t).terminal)
                    .modes
                    .get(rustty_vt::modes::Mode::InBandSizeReports)
            });

            // var no_write: Terminal = null;
            let (no_write_result, no_write) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, no_write_result);
            assert_eq!(Result::Success, unsafe {
                set(no_write, Option_::SizeCb, size_cb as *const c_void)
            });
            vtw(no_write, b"\x1B[?2048h");
            assert_eq!(1, unsafe { SIZES });
            assert!(unsafe {
                (*(*no_write).terminal)
                    .modes
                    .get(rustty_vt::modes::Mode::InBandSizeReports)
            });

            unsafe {
                free(no_write);
                free(t);
            }
        });
    }

    // test "pwd_changed without callback is silent"
    #[test]
    fn pwd_changed_without_callback_is_silent() {
        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);
            // OSC 7 without a callback should not crash, but should still set the pwd
            vtw(t, b"\x1B]7;file:///tmp\x1B\\");
            assert_eq!(b"file:///tmp".as_slice(), unsafe {
                (*zig_terminal(t)).get_pwd().unwrap()
            });
            unsafe { free(t) };
        });
    }

    // test "set size callback"
    #[test]
    fn set_size_callback() {
        static mut LAST_DATA: Option<Vec<u8>> = None;
        extern "C" fn write_pty(_t: Terminal, _ud: *mut c_void, ptr: *const u8, len: usize) {
            let data = unsafe { core::slice::from_raw_parts(ptr, len) }.to_vec();
            unsafe { LAST_DATA = Some(data) };
        }
        extern "C" fn size_cb(
            _t: Terminal,
            _ud: *mut c_void,
            out_size: *mut size_report::Size,
        ) -> bool {
            unsafe {
                *out_size = size_report::Size {
                    rows: 24,
                    columns: 80,
                    cell_width: 8,
                    cell_height: 16,
                };
            }
            true
        }

        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);
            unsafe { LAST_DATA = None };

            assert_eq!(Result::Success, unsafe {
                set(t, Option_::WritePty, write_pty as *const c_void)
            });
            assert_eq!(Result::Success, unsafe {
                set(t, Option_::SizeCb, size_cb as *const c_void)
            });

            // CSI 18 t — report text area size in characters
            vtw(t, b"\x1B[18t");
            assert!(unsafe { (*std::ptr::addr_of!(LAST_DATA)).as_ref() }.is_some());
            assert_eq!(
                b"\x1b[8;24;80t".as_slice(),
                unsafe { (*std::ptr::addr_of!(LAST_DATA)).as_ref() }
                    .unwrap()
                    .as_slice()
            );
            unsafe { free(t) };
        });
    }

    // test "size without callback is silent"
    #[test]
    fn size_without_callback_is_silent() {
        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);
            // CSI 18 t without a size callback should not crash
            vtw(t, b"\x1B[18t");
            unsafe { free(t) };
        });
    }

    // test "set device_attributes callback primary"
    #[test]
    fn set_device_attributes_callback_primary() {
        static mut LAST_DATA: Option<Vec<u8>> = None;
        extern "C" fn write_pty(_t: Terminal, _ud: *mut c_void, ptr: *const u8, len: usize) {
            let data = unsafe { core::slice::from_raw_parts(ptr, len) }.to_vec();
            unsafe { LAST_DATA = Some(data) };
        }
        extern "C" fn da(_t: Terminal, _ud: *mut c_void, out: *mut CDeviceAttributes) -> bool {
            let mut features = [0u16; 64];
            features[0] = 22;
            features[1] = 52;
            unsafe {
                *out = CDeviceAttributes {
                    primary: CDeviceAttributesPrimary {
                        conformance_level: 64,
                        features,
                        num_features: 2,
                    },
                    secondary: CDeviceAttributesSecondary {
                        device_type: 1,
                        firmware_version: 10,
                        rom_cartridge: 0,
                    },
                    tertiary: CDeviceAttributesTertiary { unit_id: 0 },
                };
            }
            true
        }

        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);
            unsafe { LAST_DATA = None };

            assert_eq!(Result::Success, unsafe {
                set(t, Option_::WritePty, write_pty as *const c_void)
            });
            assert_eq!(Result::Success, unsafe {
                set(t, Option_::DeviceAttributes, da as *const c_void)
            });

            // CSI c — primary DA
            vtw(t, b"\x1B[c");
            assert!(unsafe { (*std::ptr::addr_of!(LAST_DATA)).as_ref() }.is_some());
            assert_eq!(
                b"\x1b[?64;22;52c".as_slice(),
                unsafe { (*std::ptr::addr_of!(LAST_DATA)).as_ref() }
                    .unwrap()
                    .as_slice()
            );
            unsafe { free(t) };
        });
    }

    // test "set device_attributes callback secondary"
    #[test]
    fn set_device_attributes_callback_secondary() {
        static mut LAST_DATA: Option<Vec<u8>> = None;
        extern "C" fn write_pty(_t: Terminal, _ud: *mut c_void, ptr: *const u8, len: usize) {
            let data = unsafe { core::slice::from_raw_parts(ptr, len) }.to_vec();
            unsafe { LAST_DATA = Some(data) };
        }
        extern "C" fn da(_t: Terminal, _ud: *mut c_void, out: *mut CDeviceAttributes) -> bool {
            let mut features = [0u16; 64];
            features[0] = 22;
            unsafe {
                *out = CDeviceAttributes {
                    primary: CDeviceAttributesPrimary {
                        conformance_level: 62,
                        features,
                        num_features: 1,
                    },
                    secondary: CDeviceAttributesSecondary {
                        device_type: 1,
                        firmware_version: 10,
                        rom_cartridge: 0,
                    },
                    tertiary: CDeviceAttributesTertiary { unit_id: 0 },
                };
            }
            true
        }

        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);
            unsafe { LAST_DATA = None };

            assert_eq!(Result::Success, unsafe {
                set(t, Option_::WritePty, write_pty as *const c_void)
            });
            assert_eq!(Result::Success, unsafe {
                set(t, Option_::DeviceAttributes, da as *const c_void)
            });

            // CSI > c — secondary DA
            vtw(t, b"\x1B[>c");
            assert!(unsafe { (*std::ptr::addr_of!(LAST_DATA)).as_ref() }.is_some());
            assert_eq!(
                b"\x1b[>1;10;0c".as_slice(),
                unsafe { (*std::ptr::addr_of!(LAST_DATA)).as_ref() }
                    .unwrap()
                    .as_slice()
            );
            unsafe { free(t) };
        });
    }

    // test "set device_attributes callback tertiary"
    #[test]
    fn set_device_attributes_callback_tertiary() {
        static mut LAST_DATA: Option<Vec<u8>> = None;
        extern "C" fn write_pty(_t: Terminal, _ud: *mut c_void, ptr: *const u8, len: usize) {
            let data = unsafe { core::slice::from_raw_parts(ptr, len) }.to_vec();
            unsafe { LAST_DATA = Some(data) };
        }
        extern "C" fn da(_t: Terminal, _ud: *mut c_void, out: *mut CDeviceAttributes) -> bool {
            unsafe {
                *out = CDeviceAttributes {
                    primary: CDeviceAttributesPrimary {
                        conformance_level: 62,
                        features: [0u16; 64],
                        num_features: 0,
                    },
                    secondary: CDeviceAttributesSecondary {
                        device_type: 1,
                        firmware_version: 0,
                        rom_cartridge: 0,
                    },
                    tertiary: CDeviceAttributesTertiary {
                        unit_id: 0xAABBCCDD,
                    },
                };
            }
            true
        }

        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);
            unsafe { LAST_DATA = None };

            assert_eq!(Result::Success, unsafe {
                set(t, Option_::WritePty, write_pty as *const c_void)
            });
            assert_eq!(Result::Success, unsafe {
                set(t, Option_::DeviceAttributes, da as *const c_void)
            });

            // CSI = c — tertiary DA
            vtw(t, b"\x1B[=c");
            assert!(unsafe { (*std::ptr::addr_of!(LAST_DATA)).as_ref() }.is_some());
            assert_eq!(
                b"\x1bP!|AABBCCDD\x1b\\".as_slice(),
                unsafe { (*std::ptr::addr_of!(LAST_DATA)).as_ref() }
                    .unwrap()
                    .as_slice()
            );
            unsafe { free(t) };
        });
    }

    // test "device_attributes without callback uses default"
    #[test]
    fn device_attributes_without_callback_uses_default() {
        static mut LAST_DATA: Option<Vec<u8>> = None;
        extern "C" fn write_pty(_t: Terminal, _ud: *mut c_void, ptr: *const u8, len: usize) {
            let data = unsafe { core::slice::from_raw_parts(ptr, len) }.to_vec();
            unsafe { LAST_DATA = Some(data) };
        }

        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);
            unsafe { LAST_DATA = None };

            assert_eq!(Result::Success, unsafe {
                set(t, Option_::WritePty, write_pty as *const c_void)
            });

            // Without setting a device_attributes callback, DA1 should return the default
            vtw(t, b"\x1B[c");
            assert!(unsafe { (*std::ptr::addr_of!(LAST_DATA)).as_ref() }.is_some());
            assert_eq!(
                b"\x1b[?62;22c".as_slice(),
                unsafe { (*std::ptr::addr_of!(LAST_DATA)).as_ref() }
                    .unwrap()
                    .as_slice()
            );
            unsafe { free(t) };
        });
    }

    // test "device_attributes callback returns false uses default"
    #[test]
    fn device_attributes_callback_returns_false_uses_default() {
        static mut LAST_DATA: Option<Vec<u8>> = None;
        extern "C" fn write_pty(_t: Terminal, _ud: *mut c_void, ptr: *const u8, len: usize) {
            let data = unsafe { core::slice::from_raw_parts(ptr, len) }.to_vec();
            unsafe { LAST_DATA = Some(data) };
        }
        extern "C" fn da(_t: Terminal, _ud: *mut c_void, _out: *mut CDeviceAttributes) -> bool {
            false
        }

        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);
            unsafe { LAST_DATA = None };

            assert_eq!(Result::Success, unsafe {
                set(t, Option_::WritePty, write_pty as *const c_void)
            });
            assert_eq!(Result::Success, unsafe {
                set(t, Option_::DeviceAttributes, da as *const c_void)
            });

            // Callback returns false, should use default response
            vtw(t, b"\x1B[c");
            assert!(unsafe { (*std::ptr::addr_of!(LAST_DATA)).as_ref() }.is_some());
            assert_eq!(
                b"\x1b[?62;22c".as_slice(),
                unsafe { (*std::ptr::addr_of!(LAST_DATA)).as_ref() }
                    .unwrap()
                    .as_slice()
            );
            unsafe { free(t) };
        });
    }

    // test "set and get title"
    #[test]
    fn set_and_get_title() {
        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);

            // No title set yet — should return empty string
            let mut title = LibString {
                ptr: std::ptr::null(),
                len: 0,
            };
            assert_eq!(Result::Success, unsafe {
                get(t, TerminalData::Title, &mut title as *mut _ as *mut c_void)
            });
            assert_eq!(0usize, title.len);

            // Set title via option
            let hello = LibString {
                ptr: b"Hello".as_ptr(),
                len: 5,
            };
            assert_eq!(Result::Success, unsafe {
                set(t, Option_::Title, &hello as *const _ as *const c_void)
            });

            assert_eq!(Result::Success, unsafe {
                get(t, TerminalData::Title, &mut title as *mut _ as *mut c_void)
            });
            assert_eq!(b"Hello".as_slice(), unsafe {
                core::slice::from_raw_parts(title.ptr, title.len)
            });

            // Overwrite title
            let world = LibString {
                ptr: b"World".as_ptr(),
                len: 5,
            };
            assert_eq!(Result::Success, unsafe {
                set(t, Option_::Title, &world as *const _ as *const c_void)
            });

            assert_eq!(Result::Success, unsafe {
                get(t, TerminalData::Title, &mut title as *mut _ as *mut c_void)
            });
            assert_eq!(b"World".as_slice(), unsafe {
                core::slice::from_raw_parts(title.ptr, title.len)
            });

            // Clear title with NULL
            assert_eq!(Result::Success, unsafe {
                set(t, Option_::Title, std::ptr::null())
            });

            assert_eq!(Result::Success, unsafe {
                get(t, TerminalData::Title, &mut title as *mut _ as *mut c_void)
            });
            assert_eq!(0usize, title.len);
            unsafe { free(t) };
        });
    }

    // test "set and get pwd"
    #[test]
    fn set_and_get_pwd() {
        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);

            // No pwd set yet — should return empty string
            let mut pwd = LibString {
                ptr: std::ptr::null(),
                len: 0,
            };
            assert_eq!(Result::Success, unsafe {
                get(t, TerminalData::Pwd, &mut pwd as *mut _ as *mut c_void)
            });
            assert_eq!(0usize, pwd.len);

            // Set pwd via option
            let home = LibString {
                ptr: b"/home/user".as_ptr(),
                len: 10,
            };
            assert_eq!(Result::Success, unsafe {
                set(t, Option_::Pwd, &home as *const _ as *const c_void)
            });

            assert_eq!(Result::Success, unsafe {
                get(t, TerminalData::Pwd, &mut pwd as *mut _ as *mut c_void)
            });
            assert_eq!(b"/home/user".as_slice(), unsafe {
                core::slice::from_raw_parts(pwd.ptr, pwd.len)
            });

            // Clear pwd with NULL
            assert_eq!(Result::Success, unsafe {
                set(t, Option_::Pwd, std::ptr::null())
            });

            assert_eq!(Result::Success, unsafe {
                get(t, TerminalData::Pwd, &mut pwd as *mut _ as *mut c_void)
            });
            assert_eq!(0usize, pwd.len);
            unsafe { free(t) };
        });
    }

    // test "get title set via vt_write"
    #[test]
    fn get_title_set_via_vt_write() {
        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);

            // Set title via OSC 2
            vtw(t, b"\x1B]2;VT Title\x1B\\");

            let mut title = LibString {
                ptr: std::ptr::null(),
                len: 0,
            };
            assert_eq!(Result::Success, unsafe {
                get(t, TerminalData::Title, &mut title as *mut _ as *mut c_void)
            });
            assert_eq!(b"VT Title".as_slice(), unsafe {
                core::slice::from_raw_parts(title.ptr, title.len)
            });
            unsafe { free(t) };
        });
    }

    // test "title report requires explicit opt in"
    #[test]
    fn title_report_requires_explicit_opt_in() {
        static mut LAST_DATA: Option<Vec<u8>> = None;
        fn deinit() {
            // if (last_data) |data| testing.allocator.free(data);
            unsafe { LAST_DATA = None };
        }
        fn last_data() -> Option<&'static Vec<u8>> {
            unsafe { (*std::ptr::addr_of!(LAST_DATA)).as_ref() }
        }
        extern "C" fn write_pty(_t: Terminal, _ud: *mut c_void, ptr: *const u8, len: usize) {
            let data = unsafe { core::slice::from_raw_parts(ptr, len) }.to_vec();
            unsafe { LAST_DATA = Some(data) };
        }

        with_test_allocator(|alloc| {
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);
            // S.last_data = null;
            unsafe { LAST_DATA = None };
            // defer S.deinit();

            assert_eq!(Result::Success, unsafe {
                set(t, Option_::WritePty, write_pty as *const c_void)
            });

            // const set_title = "\x1B]2;echo vulnerable\x1B\\";
            let set_title = b"\x1B]2;echo vulnerable\x1B\\";
            // const query_title = "\x1B[21t";
            let query_title = b"\x1B[21t";
            vtw(t, set_title);

            // WRITE_PTY alone must not enable the security-sensitive response.
            vtw(t, query_title);
            // try testing.expect(S.last_data == null);
            assert!(last_data().is_none());

            // const enabled = true;
            let enabled: bool = true;
            assert_eq!(Result::Success, unsafe {
                set(
                    t,
                    Option_::TitleReport,
                    &enabled as *const bool as *const c_void,
                )
            });
            vtw(t, query_title);
            // try testing.expectEqualStrings("\x1b]lecho vulnerable\x1b\\", S.last_data.?);
            assert_eq!(
                b"\x1b]lecho vulnerable\x1b\\".as_slice(),
                last_data().unwrap().as_slice()
            );

            // NULL restores the secure default.
            deinit();
            assert_eq!(Result::Success, unsafe {
                set(t, Option_::TitleReport, std::ptr::null())
            });
            vtw(t, query_title);
            assert!(last_data().is_none());

            unsafe { free(t) };
        });
    }

    // test "resize updates pixel dimensions"
    #[test]
    fn resize_updates_pixel_dimensions() {
        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);

            // Pixel geometry must still be applied when the cell dimensions match.
            assert_eq!(Result::Success, unsafe { resize(t, 80, 24, 9, 18) });

            let zt = unsafe { (*t).terminal };
            assert_eq!(80u32 * 9, unsafe { (*zt).width_px });
            assert_eq!(24u32 * 18, unsafe { (*zt).height_px });
            unsafe { free(t) };
        });
    }

    // test "resize pixel overflow saturates"
    #[test]
    fn resize_pixel_overflow_saturates() {
        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);

            assert_eq!(Result::Success, unsafe {
                resize(t, 100, 40, u32::MAX, u32::MAX)
            });

            let zt = unsafe { (*t).terminal };
            assert_eq!(u32::MAX, unsafe { (*zt).width_px });
            assert_eq!(u32::MAX, unsafe { (*zt).height_px });
            unsafe { free(t) };
        });
    }

    // test "resize disables synchronized output"
    #[test]
    fn resize_disables_synchronized_output() {
        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);

            let zt = unsafe { (*t).terminal };
            unsafe { (*zt).modes.set(modes::Mode::SynchronizedOutput, true) };

            // NEGATIVE-CONTROL ASSERTION (P3-W4 T2 Step 5): the clear must
            // happen EXACTLY ONCE. `89b103dd5` moves the clear out of this
            // wrapper and into `Terminal::resize`; a port that keeps both
            // double-clears (== 2, behaviourally invisible) and a port that
            // deletes both without the terminal-side move loses it entirely
            // (== 0). Only the count separates the three cases — the
            // `!get(SynchronizedOutput)` assertion below passes for both 1 and
            // 2. See the `sync_output_clear_count` PORT NOTE in vt/modes.rs.
            // The COUNTER is `cfg(debug_assertions)` in rustty-vt, so it must be
            // referenced under the same cfg here — an unconditional call does not
            // compile under `--release`. The behavioural assertions below stay in
            // BOTH profiles; only the count, which is what distinguishes 0/1/2
            // clears, is debug-only.
            #[cfg(debug_assertions)]
            rustty_vt::modes::sync_output_clear_reset();

            // The terminal-level reset must run even if grid work is unnecessary.
            assert_eq!(Result::Success, unsafe { resize(t, 80, 24, 9, 18) });
            assert!(!unsafe { (*zt).modes.get(modes::Mode::SynchronizedOutput) });
            #[cfg(debug_assertions)]
            assert_eq!(1, rustty_vt::modes::sync_output_clear_count());
            unsafe { free(t) };
        });
    }

    // test "resize sends in-band size report"
    #[test]
    fn resize_sends_in_band_size_report() {
        static mut LAST_DATA: Option<Vec<u8>> = None;
        extern "C" fn write_pty(_t: Terminal, _ud: *mut c_void, ptr: *const u8, len: usize) {
            let data = unsafe { core::slice::from_raw_parts(ptr, len) }.to_vec();
            unsafe { LAST_DATA = Some(data) };
        }

        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);
            unsafe { LAST_DATA = None };

            assert_eq!(Result::Success, unsafe {
                set(t, Option_::WritePty, write_pty as *const c_void)
            });

            // Enable in-band size reports (mode 2048)
            unsafe {
                (*(*t).terminal)
                    .modes
                    .set(modes::Mode::InBandSizeReports, true)
            };

            assert_eq!(Result::Success, unsafe { resize(t, 100, 40, 9, 18) });

            // Expected: \x1B[48;rows;cols;height_px;width_pxt
            // height_px = 40*18 = 720, width_px = 100*9 = 900
            assert!(unsafe { (*std::ptr::addr_of!(LAST_DATA)).as_ref() }.is_some());
            assert_eq!(
                b"\x1B[48;40;100;720;900t".as_slice(),
                unsafe { (*std::ptr::addr_of!(LAST_DATA)).as_ref() }
                    .unwrap()
                    .as_slice()
            );
            unsafe { free(t) };
        });
    }

    // test "resize no size report without mode 2048"
    #[test]
    fn resize_no_size_report_without_mode_2048() {
        static mut CALLED: bool = false;
        extern "C" fn write_pty(_t: Terminal, _ud: *mut c_void, _ptr: *const u8, _len: usize) {
            unsafe { CALLED = true };
        }

        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);
            unsafe { CALLED = false };

            assert_eq!(Result::Success, unsafe {
                set(t, Option_::WritePty, write_pty as *const c_void)
            });

            // in_band_size_reports is off by default
            assert_eq!(Result::Success, unsafe { resize(t, 100, 40, 9, 18) });
            assert!(!unsafe { CALLED });
            unsafe { free(t) };
        });
    }

    // test "resize in-band report without write_pty callback"
    #[test]
    fn resize_in_band_report_without_write_pty_callback() {
        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);

            // Enable mode 2048 but don't set a write_pty callback — should not crash
            unsafe {
                (*(*t).terminal)
                    .modes
                    .set(modes::Mode::InBandSizeReports, true)
            };
            assert_eq!(Result::Success, unsafe { resize(t, 100, 40, 9, 18) });
            unsafe { free(t) };
        });
    }

    // test "resize null terminal"
    #[test]
    fn resize_null_terminal() {
        assert_eq!(Result::InvalidValue, unsafe {
            resize(std::ptr::null_mut(), 100, 40, 9, 18)
        });
    }

    // test "resize zero cols"
    #[test]
    fn resize_zero_cols() {
        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);
            assert_eq!(Result::InvalidValue, unsafe { resize(t, 0, 40, 9, 18) });
            unsafe { free(t) };
        });
    }

    // test "resize zero rows"
    #[test]
    fn resize_zero_rows() {
        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);
            assert_eq!(Result::InvalidValue, unsafe { resize(t, 100, 0, 9, 18) });
            unsafe { free(t) };
        });
    }

    // test "grid_ref out of bounds"
    #[test]
    fn grid_ref_out_of_bounds() {
        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);

            let mut out_ref = CGridRef::default();
            assert_eq!(Result::InvalidValue, unsafe {
                grid_ref(t, pt_active(100, 0), &mut out_ref)
            });
            unsafe { free(t) };
        });
    }

    // test "set and get color_foreground"
    #[test]
    fn set_and_get_color_foreground() {
        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);

            // Initially unset
            let mut rgb = crate::color::RgbC { r: 0, g: 0, b: 0 };
            assert_eq!(Result::NoValue, unsafe {
                get(
                    t,
                    TerminalData::ColorForeground,
                    &mut rgb as *mut _ as *mut c_void,
                )
            });

            // Set a value
            let fg = crate::color::RgbC {
                r: 0xAA,
                g: 0xBB,
                b: 0xCC,
            };
            assert_eq!(Result::Success, unsafe {
                set(
                    t,
                    Option_::ColorForeground,
                    &fg as *const _ as *const c_void,
                )
            });
            assert_eq!(Result::Success, unsafe {
                get(
                    t,
                    TerminalData::ColorForeground,
                    &mut rgb as *mut _ as *mut c_void,
                )
            });
            assert_eq!(fg, rgb);

            // Clear with null
            assert_eq!(Result::Success, unsafe {
                set(t, Option_::ColorForeground, std::ptr::null())
            });
            assert_eq!(Result::NoValue, unsafe {
                get(
                    t,
                    TerminalData::ColorForeground,
                    &mut rgb as *mut _ as *mut c_void,
                )
            });
            unsafe { free(t) };
        });
    }

    // test "set and get color_background"
    #[test]
    fn set_and_get_color_background() {
        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);

            let mut rgb = crate::color::RgbC { r: 0, g: 0, b: 0 };
            assert_eq!(Result::NoValue, unsafe {
                get(
                    t,
                    TerminalData::ColorBackground,
                    &mut rgb as *mut _ as *mut c_void,
                )
            });

            let bg = crate::color::RgbC {
                r: 0x11,
                g: 0x22,
                b: 0x33,
            };
            assert_eq!(Result::Success, unsafe {
                set(
                    t,
                    Option_::ColorBackground,
                    &bg as *const _ as *const c_void,
                )
            });
            assert_eq!(Result::Success, unsafe {
                get(
                    t,
                    TerminalData::ColorBackground,
                    &mut rgb as *mut _ as *mut c_void,
                )
            });
            assert_eq!(bg, rgb);

            assert_eq!(Result::Success, unsafe {
                set(t, Option_::ColorBackground, std::ptr::null())
            });
            assert_eq!(Result::NoValue, unsafe {
                get(
                    t,
                    TerminalData::ColorBackground,
                    &mut rgb as *mut _ as *mut c_void,
                )
            });
            unsafe { free(t) };
        });
    }

    // test "set and get color_cursor"
    #[test]
    fn set_and_get_color_cursor() {
        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);

            let mut rgb = crate::color::RgbC { r: 0, g: 0, b: 0 };
            assert_eq!(Result::NoValue, unsafe {
                get(
                    t,
                    TerminalData::ColorCursor,
                    &mut rgb as *mut _ as *mut c_void,
                )
            });

            let cur = crate::color::RgbC {
                r: 0xFF,
                g: 0x00,
                b: 0x88,
            };
            assert_eq!(Result::Success, unsafe {
                set(t, Option_::ColorCursor, &cur as *const _ as *const c_void)
            });
            assert_eq!(Result::Success, unsafe {
                get(
                    t,
                    TerminalData::ColorCursor,
                    &mut rgb as *mut _ as *mut c_void,
                )
            });
            assert_eq!(cur, rgb);

            assert_eq!(Result::Success, unsafe {
                set(t, Option_::ColorCursor, std::ptr::null())
            });
            assert_eq!(Result::NoValue, unsafe {
                get(
                    t,
                    TerminalData::ColorCursor,
                    &mut rgb as *mut _ as *mut c_void,
                )
            });
            unsafe { free(t) };
        });
    }

    // test "set and get color_palette"
    #[test]
    fn set_and_get_color_palette() {
        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);

            // Get default palette
            let mut palette: crate::color::PaletteC =
                [crate::color::RgbC { r: 0, g: 0, b: 0 }; 256];
            assert_eq!(Result::Success, unsafe {
                get(
                    t,
                    TerminalData::ColorPalette,
                    palette.as_mut_ptr() as *mut c_void,
                )
            });
            assert_eq!(rustty_vt::color::DEFAULT[0], palette[0]);

            // Set custom palette
            let mut custom: crate::color::PaletteC =
                unsafe { crate::color::palette_cval(&rustty_vt::color::DEFAULT) };
            custom[0] = crate::color::RgbC {
                r: 0x12,
                g: 0x34,
                b: 0x56,
            };
            assert_eq!(Result::Success, unsafe {
                set(t, Option_::ColorPalette, custom.as_ptr() as *const c_void)
            });
            assert_eq!(Result::Success, unsafe {
                get(
                    t,
                    TerminalData::ColorPalette,
                    palette.as_mut_ptr() as *mut c_void,
                )
            });
            assert_eq!(custom[0], palette[0]);

            // Reset with null restores default
            assert_eq!(Result::Success, unsafe {
                set(t, Option_::ColorPalette, std::ptr::null())
            });
            assert_eq!(Result::Success, unsafe {
                get(
                    t,
                    TerminalData::ColorPalette,
                    palette.as_mut_ptr() as *mut c_void,
                )
            });
            assert_eq!(rustty_vt::color::DEFAULT[0], palette[0]);
            unsafe { free(t) };
        });
    }

    // test "get color default vs effective with override"
    #[test]
    fn get_color_default_vs_effective_with_override() {
        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);

            let zt = unsafe { (*t).terminal };
            let mut rgb = crate::color::RgbC { r: 0, g: 0, b: 0 };

            // Set defaults
            let fg = crate::color::RgbC {
                r: 0xAA,
                g: 0xBB,
                b: 0xCC,
            };
            let bg = crate::color::RgbC {
                r: 0x11,
                g: 0x22,
                b: 0x33,
            };
            let cur = crate::color::RgbC {
                r: 0xFF,
                g: 0x00,
                b: 0x88,
            };
            assert_eq!(Result::Success, unsafe {
                set(
                    t,
                    Option_::ColorForeground,
                    &fg as *const _ as *const c_void,
                )
            });
            assert_eq!(Result::Success, unsafe {
                set(
                    t,
                    Option_::ColorBackground,
                    &bg as *const _ as *const c_void,
                )
            });
            assert_eq!(Result::Success, unsafe {
                set(t, Option_::ColorCursor, &cur as *const _ as *const c_void)
            });

            // Simulate OSC overrides
            let override_ = crate::color::RgbC {
                r: 0x00,
                g: 0x00,
                b: 0x00,
            };
            unsafe {
                (*zt).colors.foreground.override_ = Some(override_);
                (*zt).colors.background.override_ = Some(override_);
                (*zt).colors.cursor.override_ = Some(override_);
            }

            // Effective returns override
            assert_eq!(Result::Success, unsafe {
                get(
                    t,
                    TerminalData::ColorForeground,
                    &mut rgb as *mut _ as *mut c_void,
                )
            });
            assert_eq!(override_, rgb);
            assert_eq!(Result::Success, unsafe {
                get(
                    t,
                    TerminalData::ColorBackground,
                    &mut rgb as *mut _ as *mut c_void,
                )
            });
            assert_eq!(override_, rgb);
            assert_eq!(Result::Success, unsafe {
                get(
                    t,
                    TerminalData::ColorCursor,
                    &mut rgb as *mut _ as *mut c_void,
                )
            });
            assert_eq!(override_, rgb);

            // Default returns original
            assert_eq!(Result::Success, unsafe {
                get(
                    t,
                    TerminalData::ColorForegroundDefault,
                    &mut rgb as *mut _ as *mut c_void,
                )
            });
            assert_eq!(fg, rgb);
            assert_eq!(Result::Success, unsafe {
                get(
                    t,
                    TerminalData::ColorBackgroundDefault,
                    &mut rgb as *mut _ as *mut c_void,
                )
            });
            assert_eq!(bg, rgb);
            assert_eq!(Result::Success, unsafe {
                get(
                    t,
                    TerminalData::ColorCursorDefault,
                    &mut rgb as *mut _ as *mut c_void,
                )
            });
            assert_eq!(cur, rgb);
            unsafe { free(t) };
        });
    }

    // test "get color default returns no_value when unset"
    #[test]
    fn get_color_default_returns_no_value_when_unset() {
        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);

            let mut rgb = crate::color::RgbC { r: 0, g: 0, b: 0 };
            assert_eq!(Result::NoValue, unsafe {
                get(
                    t,
                    TerminalData::ColorForegroundDefault,
                    &mut rgb as *mut _ as *mut c_void,
                )
            });
            assert_eq!(Result::NoValue, unsafe {
                get(
                    t,
                    TerminalData::ColorBackgroundDefault,
                    &mut rgb as *mut _ as *mut c_void,
                )
            });
            assert_eq!(Result::NoValue, unsafe {
                get(
                    t,
                    TerminalData::ColorCursorDefault,
                    &mut rgb as *mut _ as *mut c_void,
                )
            });
            unsafe { free(t) };
        });
    }

    // test "get color_palette_default vs current"
    #[test]
    fn get_color_palette_default_vs_current() {
        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);

            let zt = unsafe { (*t).terminal };

            // Set a custom default palette
            let mut custom: crate::color::PaletteC =
                unsafe { crate::color::palette_cval(&rustty_vt::color::DEFAULT) };
            custom[0] = crate::color::RgbC {
                r: 0x12,
                g: 0x34,
                b: 0x56,
            };
            assert_eq!(Result::Success, unsafe {
                set(t, Option_::ColorPalette, custom.as_ptr() as *const c_void)
            });

            // Simulate OSC override on index 0
            unsafe {
                (*zt).colors.palette.set(
                    0,
                    crate::color::RgbC {
                        r: 0xFF,
                        g: 0xFF,
                        b: 0xFF,
                    },
                )
            };

            // Current palette returns the override
            let mut palette: crate::color::PaletteC =
                [crate::color::RgbC { r: 0, g: 0, b: 0 }; 256];
            assert_eq!(Result::Success, unsafe {
                get(
                    t,
                    TerminalData::ColorPalette,
                    palette.as_mut_ptr() as *mut c_void,
                )
            });
            assert_eq!(
                crate::color::RgbC {
                    r: 0xFF,
                    g: 0xFF,
                    b: 0xFF
                },
                palette[0]
            );

            // Default palette returns the original
            assert_eq!(Result::Success, unsafe {
                get(
                    t,
                    TerminalData::ColorPaletteDefault,
                    palette.as_mut_ptr() as *mut c_void,
                )
            });
            assert_eq!(custom[0], palette[0]);
            unsafe { free(t) };
        });
    }

    // test "set color sets dirty flag"
    #[test]
    fn set_color_sets_dirty_flag() {
        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);

            let zt = unsafe { (*t).terminal };
            unsafe { (*zt).flags.dirty.palette = false };

            let fg = crate::color::RgbC {
                r: 0xFF,
                g: 0xFF,
                b: 0xFF,
            };
            assert_eq!(Result::Success, unsafe {
                set(
                    t,
                    Option_::ColorForeground,
                    &fg as *const _ as *const c_void,
                )
            });
            assert!(unsafe { (*zt).flags.dirty.palette });
            unsafe { free(t) };
        });
    }

    // test "set glyph protocol disables APC handling and clears glossary"
    #[test]
    #[cfg_attr(
        not(feature = "glyph-protocol"),
        ignore = "SkipZigTest: needs the glyph-protocol vt feature (R14)"
    )]
    fn set_glyph_protocol_disables_apc_handling_and_clears_glossary() {
        // if (comptime !build_options.glyph_protocol) return error.SkipZigTest;
        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);

            let register = b"\x1B_25a1;r;cp=e0a0;AAAAAAAAAAAAAA==\x1B\\";
            vtw(t, register);
            assert!(unsafe { (*(*t).terminal).glyph_glossary.contains(0xE0A0) });

            let disabled = false;
            assert_eq!(Result::Success, unsafe {
                set(
                    t,
                    Option_::GlyphProtocol,
                    &disabled as *const _ as *const c_void,
                )
            });
            assert!(
                !unsafe { (*t).stream.handler.apc_handler.enabled }
                    [rustty_vt::apc::Protocol::Glyph as usize]
            );
            assert!(!unsafe { (*(*t).terminal).glyph_glossary.contains(0xE0A0) });

            vtw(t, register);
            assert!(!unsafe { (*(*t).terminal).glyph_glossary.contains(0xE0A0) });

            let enabled = true;
            assert_eq!(Result::Success, unsafe {
                set(
                    t,
                    Option_::GlyphProtocol,
                    &enabled as *const _ as *const c_void,
                )
            });
            vtw(t, register);
            assert!(unsafe { (*(*t).terminal).glyph_glossary.contains(0xE0A0) });
            unsafe { free(t) };
        });
    }

    // test "get_multi success"
    #[test]
    fn get_multi_success() {
        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);

            let mut cols: u16 = 0;
            let mut rows: u16 = 0;
            let mut written: usize = 0;

            let keys = [TerminalData::Cols, TerminalData::Rows];
            let mut values: [*mut c_void; 2] = [
                &mut cols as *mut _ as *mut c_void,
                &mut rows as *mut _ as *mut c_void,
            ];
            assert_eq!(Result::Success, unsafe {
                get_multi(
                    t,
                    keys.len(),
                    // PORT NOTE (W35-B08): the `keys` array crosses the C
                    // boundary as raw ints; `get_multi` reads each element as a
                    // `c_int` and range-checks it.
                    keys.as_ptr().cast(),
                    values.as_mut_ptr(),
                    &mut written,
                )
            });
            assert_eq!(keys.len(), written);
            assert_eq!(80, cols);
            assert_eq!(24, rows);
            unsafe { free(t) };
        });
    }

    // test "get_multi error sets out_written"
    #[test]
    fn get_multi_error_sets_out_written() {
        with_test_allocator(|alloc| {
            // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);

            let mut cols: u16 = 0;
            let mut written: usize = 99;

            let keys = [TerminalData::Cols, TerminalData::Invalid];
            // SB: build the two out-pointers with `addr_of_mut!` (raw), NOT
            // `&mut cols as *mut _`. Both entries point at the same `cols` (the
            // reference test passes `&cols` twice — raw C pointers); a second
            // `&mut cols` reborrow retags the whole variable and pops the first
            // pointer off the borrow stack, so `get_multi`'s write through
            // `values[0]` would be UB (rust-mut-args-strongly-protected).
            let mut values: [*mut c_void; 2] = [
                core::ptr::addr_of_mut!(cols) as *mut c_void,
                core::ptr::addr_of_mut!(cols) as *mut c_void,
            ];
            assert_eq!(Result::InvalidValue, unsafe {
                get_multi(
                    t,
                    keys.len(),
                    // PORT NOTE (W35-B08): the `keys` array crosses the C
                    // boundary as raw ints; `get_multi` reads each element as a
                    // `c_int` and range-checks it.
                    keys.as_ptr().cast(),
                    values.as_mut_ptr(),
                    &mut written,
                )
            });
            assert_eq!(1, written);
            assert_eq!(80, cols);
            unsafe { free(t) };
        });
    }

    // W35-B08-01 (Rust-only proving test; the reference's guard is a
    // `runtime_safety` block, so it has no unit test for it): a `get_multi`
    // KEY ARRAY is C memory. Before the fix the wrapper loaded each element
    // straight into a `TerminalData`, which materializes an out-of-range
    // discriminant — UB in Rust in EVERY build mode. The reference reaches its
    // `std.enums.fromInt` guard per element (its `get_multi` calls `get`), so
    // the parity disposition is `.invalid_value` + the failing index.
    #[test]
    fn get_multi_out_of_range_raw_key_is_rejected_with_the_failing_index() {
        with_test_allocator(|alloc| {
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);

            let mut cols: u16 = 0;
            let mut rows: u16 = 0;
            let mut written: usize = 99;

            // Element 1 is a raw C integer no `TerminalData` tag can hold.
            let keys: [c_int; 3] = [TerminalData::Cols as c_int, 99, TerminalData::Rows as c_int];
            // SB: raw out-pointers (see `get_multi_error_sets_out_written`).
            let mut values: [*mut c_void; 3] = [
                core::ptr::addr_of_mut!(cols) as *mut c_void,
                core::ptr::addr_of_mut!(rows) as *mut c_void,
                core::ptr::addr_of_mut!(rows) as *mut c_void,
            ];
            assert_eq!(Result::InvalidValue, unsafe {
                get_multi(
                    t,
                    keys.len(),
                    keys.as_ptr(),
                    values.as_mut_ptr(),
                    &mut written,
                )
            });
            // The failing index, exactly as the reference's `get_multi` reports.
            assert_eq!(1, written);
            // Element 0 ran to completion before the guard tripped.
            assert_eq!(80, cols);
            // Element 2 was never reached.
            assert_eq!(0, rows);

            // The same array through the published `extern "C"` entry point.
            written = 99;
            assert_eq!(Result::InvalidValue.to_c_int(), unsafe {
                crate::rustty_terminal_get_multi(
                    t,
                    keys.len(),
                    keys.as_ptr().cast(),
                    values.as_mut_ptr(),
                    &mut written,
                )
            });
            assert_eq!(1, written);

            unsafe { free(t) };
        });
    }

    // W35-B03/B06/B08-02 (Rust-only proving test): the invalid-selector
    // diagnostics are the reference's `log.warn` on the `.terminal_c` /
    // `.selection_c` scopes, which `terminal/c/sys.zig`'s `logFn` routes to the
    // installed C log callback and DROPS when none is installed
    // (`if (global.log == null) return;`). They must never go to stderr.
    mod log_capture {
        use crate::sys::LogLevel;
        use std::ffi::c_void;
        use std::sync::Mutex;

        pub(super) static CAPTURED: Mutex<Vec<(LogLevel, String, String)>> = Mutex::new(Vec::new());

        pub(super) extern "C" fn log_cb(
            _userdata: *mut c_void,
            level: LogLevel,
            scope_ptr: *const u8,
            scope_len: usize,
            message_ptr: *const u8,
            message_len: usize,
        ) {
            // SAFETY: the sink hands us the scope/message byte ranges.
            let scope = unsafe { std::slice::from_raw_parts(scope_ptr, scope_len) };
            // SAFETY: as above.
            let message = unsafe { std::slice::from_raw_parts(message_ptr, message_len) };
            CAPTURED.lock().unwrap().push((
                level,
                String::from_utf8_lossy(scope).into_owned(),
                String::from_utf8_lossy(message).into_owned(),
            ));
        }
    }

    #[test]
    fn terminal_invalid_selector_warns_through_the_scoped_log_sink() {
        // Serialize against the other tests that mutate the process-wide sys
        // GLOBAL (the sys tests and the render-state scoped-log test).
        let _guard = crate::sys::tests::reset_global();
        log_capture::CAPTURED.lock().unwrap().clear();

        // A selector value used by no other test, so the capture can be
        // filtered: these tests run in parallel and other terminal tests also
        // trip invalid-selector warnings while this callback is installed.
        const SEL: c_int = 31338;
        const C_INVALID_VALUE: c_int = -2;

        // No callback installed: silently discarded (the C-ABI contract), and
        // the call still reports `.invalid_value`.
        assert_eq!(C_INVALID_VALUE, unsafe {
            crate::rustty_terminal_set(std::ptr::null_mut(), SEL, std::ptr::null())
        });
        assert_eq!(C_INVALID_VALUE, unsafe {
            crate::rustty_terminal_get(std::ptr::null_mut(), SEL, std::ptr::null_mut())
        });
        assert!(log_capture::CAPTURED.lock().unwrap().is_empty());

        // Install a capturing callback.
        assert_eq!(Result::Success, unsafe {
            crate::sys::set(
                crate::sys::Option_::Log,
                log_capture::log_cb as *const c_void,
            )
        });

        assert_eq!(C_INVALID_VALUE, unsafe {
            crate::rustty_terminal_set(std::ptr::null_mut(), SEL, std::ptr::null())
        });
        assert_eq!(C_INVALID_VALUE, unsafe {
            crate::rustty_terminal_get(std::ptr::null_mut(), SEL, std::ptr::null_mut())
        });
        // The `get_multi` key array reaches the same guard, per element.
        let keys: [c_int; 1] = [SEL];
        let mut values: [*mut c_void; 1] = [std::ptr::null_mut()];
        let mut written: usize = 99;
        assert_eq!(C_INVALID_VALUE, unsafe {
            crate::rustty_terminal_get_multi(
                std::ptr::null_mut(),
                1,
                keys.as_ptr().cast(),
                values.as_mut_ptr(),
                &mut written,
            )
        });
        assert_eq!(0, written);
        // `selection_c`-scoped guards.
        assert_eq!(C_INVALID_VALUE, unsafe {
            crate::rustty_terminal_selection_adjust(std::ptr::null_mut(), std::ptr::null_mut(), SEL)
        });
        assert_eq!(C_INVALID_VALUE, unsafe {
            crate::rustty_terminal_selection_ordered(
                std::ptr::null_mut(),
                std::ptr::null(),
                SEL,
                std::ptr::null_mut(),
            )
        });

        // Clear the callback before asserting, so a failure cannot strand it.
        assert_eq!(Result::Success, unsafe {
            crate::sys::set(crate::sys::Option_::Log, std::ptr::null())
        });

        let all = log_capture::CAPTURED.lock().unwrap().clone();
        // FLAKE FIX (task 15, N28): this loop used to run over EVERY captured
        // entry. `reset_global()` serializes the tests that MUTATE the sys
        // global, but nothing stops a concurrently running test from EMITTING a
        // diagnostic while this callback is installed, so a `debug`-level line
        // from elsewhere in the binary would fail this test for a reason it does
        // not control. Assert over the entries THIS test caused — which is why
        // the unique SEL two lines above exists. Latent here (0 failures in 15
        // runs), live in the two siblings this matches byte for byte:
        // `render::tests::render_invalid_selector_warns_through_the_scoped_log_sink`
        // and `log_root_tests::root_wrapper_invalid_selectors_warn_through_the_c_log_callback`.
        let mine: Vec<&(crate::sys::LogLevel, String, String)> = all
            .iter()
            .filter(|(_, _, m)| m.ends_with("=31338"))
            .collect();
        for (level, _, _) in mine.iter() {
            assert_eq!(crate::sys::LogLevel::Warning, *level);
        }
        let captured: Vec<(&String, &String)> = mine.iter().map(|(_, s, m)| (s, m)).collect();
        let expected = [
            (LOG_SCOPE, "terminal_set invalid option value=31338"),
            (LOG_SCOPE, "terminal_get invalid data value=31338"),
            // `get_multi`'s per-element guard is `get`'s, so is its message.
            (LOG_SCOPE, "terminal_get invalid data value=31338"),
            (
                crate::selection::LOG_SCOPE,
                "terminal_selection_adjust invalid adjustment value=31338",
            ),
            (
                crate::selection::LOG_SCOPE,
                "terminal_selection_ordered invalid desired value=31338",
            ),
        ];
        assert_eq!(expected.len(), captured.len(), "captured: {all:?}");
        for (i, (scope, message)) in captured.iter().enumerate() {
            assert_eq!(expected[i].0, scope.as_str());
            assert_eq!(expected[i].1, message.as_str());
        }
    }

    // -----------------------------------------------------------------------
    // P3-W11 T1 (P3-W6 T4 Steps 3/5) — the kitty temp-file medium is a
    // DIRECTORY across the C ABI (oracle e8525c0fd, b988efcfe, 960c2cca5).
    // -----------------------------------------------------------------------

    /// Read `RUSTTY_TERMINAL_DATA_KITTY_IMAGE_MEDIUM_TEMP_FILE` back as bytes.
    fn temp_dir_data(t: Terminal) -> Vec<u8> {
        let mut out = LibString {
            ptr: core::ptr::null(),
            len: 0,
        };
        assert_eq!(Result::Success, unsafe {
            get(
                t,
                TerminalData::KittyImageMediumTempFile,
                core::ptr::addr_of_mut!(out) as *mut c_void,
            )
        });
        if out.len == 0 {
            return Vec::new();
        }
        unsafe { core::slice::from_raw_parts(out.ptr, out.len) }.to_vec()
    }

    /// Parity S6-F2 (Decision A): `terminal_new` marks the process as the lib
    /// artifact, so mode 5522 is recognized on every OS
    /// (`.disabled = artifact != .lib and os != .macos`), and its DECRQM
    /// report is set/reset, not "not recognized". On macOS the OS half alone
    /// enables the mode, so the artifact assertion is what tells the two
    /// artifacts apart there; on Linux all three assertions do.
    #[test]
    fn new_terminal_marks_the_process_as_the_lib_artifact_for_mode_5522() {
        with_test_allocator(|alloc| {
            // Through the C export, whose first step is
            // `sys::install_log_backend` (`new_t` calls the inner `new`).
            let mut t: Terminal = std::ptr::null_mut();
            let r =
                unsafe { crate::rustty_terminal_new(alloc as *const CAllocator, &mut t, 80, 24) };
            assert_eq!(Result::Success.to_c_int(), r);
            assert_eq!(
                rustty_vt::options::runtime_artifact(),
                rustty_vt::build_options::Artifact::Lib
            );
            assert_eq!(
                rustty_vt::modes::mode_from_int(5522, false),
                Some(rustty_vt::modes::Mode::KittyPasteEvents)
            );
            let report =
                unsafe { &(*zig_terminal(t)).modes }.get_report(rustty_vt::modes::ReportTag {
                    value: 5522,
                    ansi: false,
                });
            assert_eq!(report.state, rustty_vt::modes::State::Reset);
            unsafe { free(t) };
        });
    }

    // Parity S141-F1 / S179-F1: no reference test. `new_` relies on
    // `Terminal.Options`'s field default, and this crate is the `.lib`
    // artifact, whose default is 10MB (`Terminal.zig`'s
    // `.lib => 10 * 1000 * 1000`), not the application's 320MB.
    #[test]
    #[cfg_attr(
        not(feature = "kitty-graphics"),
        ignore = "needs the kitty-graphics vt feature: the storage limit query is gated on it (R14)"
    )]
    fn kitty_image_storage_limit_defaults_to_lib_artifact_value() {
        with_test_allocator(|alloc| {
            let (r, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, r);

            let mut out: u64 = 0;
            assert_eq!(Result::Success, unsafe {
                get(
                    t,
                    TerminalData::KittyImageStorageLimit,
                    &mut out as *mut u64 as *mut c_void,
                )
            });
            assert_eq!(10 * 1000 * 1000, out);

            unsafe { free(t) };
        });
    }

    // W33-B07-32BIT-01: the reference narrows the `uint64_t` storage limit with
    // `@intCast` — a CHECKED conversion that traps in safety-checked builds when
    // the value exceeds `usize`. The port must use `usize::try_from(...)`, not a
    // bare `as`, which would silently truncate. On 64-bit targets the conversion
    // is infallible, so a limit above 2^32 must round-trip byte-exactly (a
    // 32-bit-wide truncation would be visible here); see the companion
    // `#[cfg(target_pointer_width = "32")]` test below for the trapping arm.
    #[test]
    #[cfg(target_pointer_width = "64")]
    #[cfg_attr(
        not(feature = "kitty-graphics"),
        ignore = "SkipZigTest: needs the kitty-graphics vt feature (R14)"
    )]
    fn kitty_image_storage_limit_round_trips_above_32_bits() {
        with_test_allocator(|alloc| {
            let (r, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, r);

            for limit in [0u64, 1 << 20, (1u64 << 32) + 7, u32::MAX as u64 + 1] {
                assert_eq!(Result::Success, unsafe {
                    set(
                        t,
                        Option_::KittyImageStorageLimit,
                        core::ptr::addr_of!(limit) as *const c_void,
                    )
                });

                let mut out: u64 = 0;
                assert_eq!(Result::Success, unsafe {
                    get(
                        t,
                        TerminalData::KittyImageStorageLimit,
                        &mut out as *mut u64 as *mut c_void,
                    )
                });
                assert_eq!(limit, out);
            }

            unsafe { free(t) };
        });
    }

    // Task R13-P (`UPSTREAM-DEFECTS.md` UD63) REVERSED this test, which pinned
    // the trapping arm of `@intCast`. The value is a C-ABI caller's `uint64_t`
    // inside an `extern "C"` setter, not an invariant, so under R13 the port
    // saturates instead of aborting the host. Only reachable where `usize` is
    // narrower than `u64` (e.g. wasm32, a Kitty-graphics-ENABLED target), which
    // cannot be RUN here — `saturating_from_u64_takes_the_maximum` below drives
    // the same helper at u32 width on this host.
    #[test]
    #[cfg(target_pointer_width = "32")]
    fn kitty_image_storage_limit_over_usize_saturates() {
        with_test_allocator(|alloc| {
            let (r, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, r);

            let limit: u64 = u64::from(u32::MAX) + 1;
            let got = unsafe {
                set(
                    t,
                    Option_::KittyImageStorageLimit,
                    core::ptr::addr_of!(limit) as *const c_void,
                )
            };
            assert_eq!(Result::Success, got);

            unsafe { free(t) };
        });
    }

    /// The saturating conversion itself, at a width this host can exercise: on
    /// a 32-bit target `usize` is exactly this narrow, and the limit a C caller
    /// passes above it becomes "no practical limit" instead of a panic.
    #[test]
    fn saturating_from_u64_takes_the_maximum() {
        assert_eq!(0u32, saturating_from_u64(0, u32::MAX));
        assert_eq!(320_000_000u32, saturating_from_u64(320_000_000, u32::MAX));
        assert_eq!(u32::MAX, saturating_from_u64(u64::from(u32::MAX), u32::MAX));
        // The value the reversed test above passes: 2^32 has no u32.
        assert_eq!(
            u32::MAX,
            saturating_from_u64(u64::from(u32::MAX) + 1, u32::MAX)
        );
        assert_eq!(u32::MAX, saturating_from_u64(u64::MAX, u32::MAX));
        // On this host the production width is infallible, and unchanged.
        assert_eq!(usize::MAX, saturating_from_u64(u64::MAX, usize::MAX));
    }

    #[test]
    fn kitty_image_medium_temp_file_is_a_directory_round_trip() {
        with_test_allocator(|alloc| {
            let (r, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, r);

            // Default construction leaves the medium DISABLED, which reads
            // back as the empty string (upstream's `.disabled => ""`).
            assert_eq!(Vec::<u8>::new(), temp_dir_data(t));

            let dir = b"/some/where/tmp";
            let v = LibString::init(dir);
            assert_eq!(Result::Success, unsafe {
                set(
                    t,
                    Option_::KittyImageMediumTempFile,
                    core::ptr::addr_of!(v) as *const c_void,
                )
            });
            assert_eq!(dir.to_vec(), temp_dir_data(t));

            // The string is COPIED into the terminal: mutating the caller's
            // buffer afterwards must not be observable.
            let mut owned = dir.to_vec();
            let v2 = LibString::init(&owned);
            assert_eq!(Result::Success, unsafe {
                set(
                    t,
                    Option_::KittyImageMediumTempFile,
                    core::ptr::addr_of!(v2) as *const c_void,
                )
            });
            owned[1] = b'X';
            assert_eq!(dir.to_vec(), temp_dir_data(t));

            // NULL disables the medium (the `else` arm), and the read-back
            // string becomes empty again.
            assert_eq!(Result::Success, unsafe {
                set(
                    t,
                    Option_::KittyImageMediumTempFile,
                    core::ptr::null::<c_void>(),
                )
            });
            assert_eq!(Vec::<u8>::new(), temp_dir_data(t));

            unsafe { free(t) };
        });
    }

    /// NEGATIVE CONTROL for `960c2cca5` (the copy-length bound).
    ///
    /// Upstream copies into a fixed `[std.fs.max_path_bytes]u8`; the guard
    /// `if (v.len > wrapper.tmp_dir_path.len) return .out_of_memory;` is what
    /// stops an over-long directory, and `960c2cca5` fixed the memcpy it
    /// guards. rustty owns the bytes, so what must be pinned is the guard's
    /// observable contract: `MAX_PATH_BYTES` is accepted, `MAX_PATH_BYTES + 1`
    /// is rejected with `OUT_OF_MEMORY` **and leaves the previous value
    /// untouched** (i.e. no partial copy happened).
    #[test]
    fn kitty_image_medium_temp_file_rejects_an_over_long_directory() {
        with_test_allocator(|alloc| {
            let (r, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, r);

            let ok = vec![b'a'; MAX_PATH_BYTES - 1];
            let v = LibString::init(&ok);
            assert_eq!(Result::Success, unsafe {
                set(
                    t,
                    Option_::KittyImageMediumTempFile,
                    core::ptr::addr_of!(v) as *const c_void,
                )
            });
            assert_eq!(ok, temp_dir_data(t));

            // Exactly at the bound: still accepted.
            let exact = vec![b'b'; MAX_PATH_BYTES];
            let v = LibString::init(&exact);
            assert_eq!(Result::Success, unsafe {
                set(
                    t,
                    Option_::KittyImageMediumTempFile,
                    core::ptr::addr_of!(v) as *const c_void,
                )
            });
            assert_eq!(exact, temp_dir_data(t));

            // One over: rejected, and NOTHING was copied.
            let too_long = vec![b'c'; MAX_PATH_BYTES + 1];
            let v = LibString::init(&too_long);
            assert_eq!(Result::OutOfMemory, unsafe {
                set(
                    t,
                    Option_::KittyImageMediumTempFile,
                    core::ptr::addr_of!(v) as *const c_void,
                )
            });
            assert_eq!(
                exact,
                temp_dir_data(t),
                "a rejected over-long directory must not partially overwrite \
                 the previous value"
            );

            unsafe { free(t) };
        });
    }

    /// DECISIONS B6 (P7-W2 T3, oracle `105d0a545`): the wrapper allocates
    /// exactly ONE copy of the kitty temp-dir path and every screen's
    /// `Limits` BORROWS it — never an independent per-screen copy, and the
    /// one copy is actually freed (through the caller's allocator) when the
    /// path is cleared.
    #[test]
    fn kitty_image_medium_temp_file_screens_share_wrapper_copy() {
        let stats = crate::test_util::CountingCAllocator::new();
        let calloc = stats.c_allocator();
        let (r, t) = new_t(&calloc, 80, 24);
        assert_eq!(Result::Success, r);

        // SAFETY: `t` is a live terminal from `new_t`.
        let zt: *mut ZigTerminal = unsafe { (*t).terminal };
        // The alternate screen is allocated lazily; enter it so both slots
        // are populated — otherwise "every screen" below would be vacuously
        // true for just the primary screen.
        vtw(t, b"\x1B[?1049h");
        let screen_count = unsafe { (*zt).screens.all.iter() }
            .filter(|slot| slot.is_some())
            .count();
        assert!(
            screen_count >= 2,
            "expected at least primary + alternate screens, got {screen_count}"
        );

        let dir = b"/shared/wrapper/copy";
        let v = LibString::init(dir);
        let alloc_calls_before = stats.alloc_calls.get();
        assert_eq!(Result::Success, unsafe {
            set(
                t,
                Option_::KittyImageMediumTempFile,
                core::ptr::addr_of!(v) as *const c_void,
            )
        });
        // The wrapper allocates its ONE copy through the caller allocator —
        // not one allocation per screen (there are `screen_count` screens,
        // all sharing it).
        assert_eq!(
            alloc_calls_before + 1,
            stats.alloc_calls.get(),
            "setting the path must make exactly one allocation, shared by every screen"
        );

        // Every screen's `directory` pointer must be the SAME address as the
        // wrapper's own copy — a shared borrow, not N independent clones.
        // SAFETY: `t` is a live terminal; `tmp_dir_path` was just populated.
        let wrapper_ptr = unsafe { (*t).tmp_dir_path.as_ref() }
            .expect("tmp_dir_path must be populated after set")
            .ptr;
        let mut checked = 0;
        for slot in unsafe { (*zt).screens.all.iter() } {
            if let Some(screen) = *slot {
                match unsafe { &(*screen).kitty_images.image_limits.temporary_file } {
                    TemporaryFileLimit::Enabled { directory } => {
                        assert_eq!(
                            wrapper_ptr as *const u8,
                            directory.as_ptr(),
                            "every screen must borrow the wrapper's single copy"
                        );
                        assert_eq!(dir.as_slice(), directory.as_slice());
                        checked += 1;
                    }
                    TemporaryFileLimit::Disabled => panic!("temp file medium was not enabled"),
                }
            }
        }
        assert_eq!(screen_count, checked);

        // Clearing the path must free the wrapper's one copy through the
        // caller allocator (not leak it, and not free it more than once —
        // Miri/the counting allocator would trip on a double free).
        let free_calls_before = stats.free_calls.get();
        assert_eq!(Result::Success, unsafe {
            set(t, Option_::KittyImageMediumTempFile, core::ptr::null())
        });
        assert!(
            stats.free_calls.get() > free_calls_before,
            "clearing the path must free the wrapper's copy"
        );
        // SAFETY: `t` is still live.
        assert!(
            unsafe { (*t).tmp_dir_path.is_none() },
            "the wrapper must drop its own reference once cleared"
        );
        for slot in unsafe { (*zt).screens.all.iter() } {
            if let Some(screen) = *slot {
                assert_eq!(
                    TemporaryFileLimit::Disabled,
                    unsafe { &(*screen).kitty_images.image_limits.temporary_file }.clone(),
                    "every screen must stop borrowing the freed copy"
                );
            }
        }

        unsafe { free(t) };
    }

    /// The corpus the allocation-failure sweep drives through the C API: text
    /// that scrolls into history (node allocation), titles and pwds (the
    /// `gpa()`-grown lists), OSC 8 hyperlinks (the cursor's heap hyperlink),
    /// the alternate screen (a second `Screen` from `ScreenSet.getInit`), kitty
    /// drag and drop state, wide tab stops after a resize past 512 columns, a
    /// tracked grid ref (a pin and the tracked-pin set), a search (the
    /// terminal's `searches` registry), and a full reset.
    ///
    /// Parity tail S7-4 adds: Kitty graphics (a transmit-and-display, a chunked
    /// transmit, an animation frame and a composition, a display and an
    /// uppercase delete), a glyph protocol registration (the request bytes and
    /// the decoded outline), an OSC 52 write and a Kitty clipboard write
    /// transaction (both through the installed clipboard effect), a
    /// five-representation Kitty clipboard write (the C trampoline's array past
    /// its four stack slots) and a Kitty clipboard read whose reply spills the
    /// reply trampoline's stack buffer, continuation tracking (the tracker's
    /// reservation and growth), a color response larger than its stack buffer,
    /// and XTGETTCAP / DECRQSS.
    fn sweep_corpus(t: Terminal, before_reset: &dyn Fn(Terminal)) -> Vec<Result> {
        // A smaller corpus under Miri (the design's "small-corpus instance");
        // it still reaches every family below.
        let (lines, more_lines) = if cfg!(miri) { (4, 2) } else { (60, 40) };
        let mut results = Vec::new();
        for i in 0..lines {
            vtw(
                t,
                format!("line {i:03} the quick brown fox jumps over the lazy dog\r\n").as_bytes(),
            );
        }
        vtw(t, b"\x1B]0;a window title\x1B\\");
        // A longer title grows a non-empty list: `remap`, then the fallback.
        vtw(
            t,
            b"\x1B]2;a considerably longer window title, past the first capacity of the list: 0123456789012345678901234567890123456789012345678901234567890123456789\x1B\\",
        );
        vtw(t, b"\x1B]7;file:///tmp/sweep\x1B\\");
        // An OSC longer than the parser's fixed buffer: the stream's capture
        // grows through the caller's allocator by `remap`.
        let mut osc = b"\x1B]52;c;".to_vec();
        osc.extend(std::iter::repeat_n(b'A', rustty_vt::osc::MAX_BUF * 3));
        osc.extend_from_slice(b"\x1B\\");
        vtw(t, &osc);
        vtw(
            t,
            b"\x1B]8;id=one;https://example.com/one\x1B\\link\x1B]8;;\x1B\\",
        );
        vtw(t, b"\x1B]8;;https://example.com/implicit\x1B\\open");
        vtw(t, b"\x1B[?1049h");
        vtw(t, b"alternate screen text\r\n\x1B]2;alt title\x1B\\");
        vtw(t, b"\x1B[?1049l");
        // Kitty drag and drop: registration (the state and its MIME list) and
        // an acceptance status; the registration outlives RIS and is
        // destroyed by `free`.
        vtw(t, b"\x1B]72;t=a;text/plain text/uri-list\x1B\\");
        vtw(t, b"\x1B]72;t=m:o=1;text/plain\x1B\\");
        // Kitty graphics.
        vtw(t, b"\x1B_Ga=T,f=24,s=1,v=1,i=1;/wAA\x1B\\");
        vtw(t, b"\x1B_Ga=t,f=24,s=2,v=1,i=2,m=1;/wAA\x1B\\");
        vtw(t, b"\x1B_Gm=0;AAD/\x1B\\");
        vtw(t, b"\x1B_Ga=f,i=1,f=24,s=1,v=1;AAD/\x1B\\");
        vtw(t, b"\x1B_Ga=c,i=1,r=1,c=2,w=1,h=1\x1B\\");
        vtw(t, b"\x1B_Ga=p,i=2\x1B\\");
        vtw(t, b"\x1B_Ga=d,d=I,i=2\x1B\\");
        // A glyph protocol registration: a one-contour, three-point outline.
        vtw(
            t,
            b"\x1B_25a1;r;cp=e0a0;AAEAAAAAAGQAZAACAAABAQEAAABk/84AAAAAAGQ=\x1B\\",
        );
        // Clipboard writes (the sweep installs a clipboard effect).
        vtw(t, b"\x1B]52;c;aGVsbG8=\x1B\\");
        vtw(t, b"\x1B]5522;type=write\x1B\\");
        vtw(
            t,
            b"\x1B]5522;type=wdata:mime=dGV4dC9wbGFpbg==;R2hvc3Q=\x1B\\",
        );
        vtw(t, b"\x1B]5522;type=wdata\x1B\\");
        vtw(t, b"\x1B]5522;type=write\x1B\\");
        for mime in ["dGV4dC9h", "dGV4dC9i", "dGV4dC9j", "dGV4dC9k", "dGV4dC9l"] {
            vtw(
                t,
                format!("\x1B]5522;type=wdata:mime={mime};eA==\x1B\\").as_bytes(),
            );
        }
        vtw(t, b"\x1B]5522;type=wdata\x1B\\");
        vtw(t, b"\x1B]5522;type=read;dGV4dC9wbGFpbg==\x1B\\");
        // Continuation tracking: past `growCapacity(4096)` (6272 bytes on
        // 64-bit targets) the suffix grows beyond the tracker's initial
        // reservation (under Miri only the reservation).
        let (max, unfinished) = if cfg!(miri) {
            (64usize, 40)
        } else {
            (16384, 7000)
        };
        results.push(unsafe {
            set(
                t,
                Option_::ContinuationMaxBytes,
                &max as *const usize as *const c_void,
            )
        });
        let mut csi = b"\x1B[".to_vec();
        csi.extend(std::iter::repeat_n(b'1', unfinished));
        vtw(t, &csi);
        vtw(t, b"m");
        // A color response past its 1024-byte stack buffer.
        let mut queries = b"\x1B]4".to_vec();
        for i in 0..64 {
            queries.extend_from_slice(format!(";{i};?").as_bytes());
        }
        queries.extend_from_slice(b"\x1B\\");
        vtw(t, &queries);
        // DCS: XTGETTCAP ("TN") and DECRQSS (SGR).
        vtw(t, b"\x1BP+q544e\x1B\\");
        vtw(t, b"\x1BP$qm\x1B\\");
        results.push(unsafe { resize(t, 600, 30, 8, 16) });
        vtw(t, b"\x1B[3g\x1B[1;590H\x1BH\r\t\tafter tabs\r\n");
        let mut ref_: crate::grid_ref_tracked::CTrackedGridRef = std::ptr::null_mut();
        results.push(unsafe {
            grid_ref_track(
                t,
                crate::point::PointC::cval(rustty_vt::point::Point::Active(
                    rustty_vt::point::Coordinate { x: 0, y: 0 },
                )),
                &mut ref_,
            )
        });
        // A search over the caller's allocator registers itself in the
        // terminal's `searches` map (grown through the terminal's allocator).
        // SAFETY: `t` is the live wrapper; its allocator copy is read out.
        let search_alloc: Option<CAllocator> = unsafe { (*t).c_alloc };
        let mut search: crate::search::Search = std::ptr::null_mut();
        results.push(unsafe { crate::search::new(search_alloc.as_ref(), &mut search, t) });
        for i in 0..more_lines {
            vtw(t, format!("more {i:03}\r\n").as_bytes());
        }
        results.push(unsafe { resize(t, 80, 24, 8, 16) });
        before_reset(t);
        vtw(t, b"\x1Bc");
        vtw(t, b"\x1B]2;after reset\x1B\\done");
        if !ref_.is_null() {
            unsafe { crate::grid_ref_tracked::tracked_grid_ref_free(ref_) };
        }
        if !search.is_null() {
            unsafe { crate::search::free(search) };
        }
        results
    }

    thread_local! {
        /// Clipboard writes the sweep's effect accepted (this thread's).
        static SWEEP_CLIPBOARD_WRITES: core::cell::Cell<usize> = const { core::cell::Cell::new(0) };
        /// Clipboard reads the sweep's effect served (this thread's).
        static SWEEP_CLIPBOARD_READS: core::cell::Cell<usize> = const { core::cell::Cell::new(0) };
    }

    // Parity tail S7-4 (routing evidence, no reference counterpart): the
    // clipboard trampolines allocate their C arrays through the terminal's
    // `gpa()`. A write of five representations spills past the four stack
    // slots into one 160-byte array on the caller's allocator; four stay on
    // the stack. A read reply of nine contents spills past
    // `stackFallback(256, gpa)` (8 x 32 bytes fit) into exactly one extra
    // allocation. Every spilled byte is returned.
    #[test]
    fn clipboard_trampolines_route_through_the_terminals_allocator() {
        use rustty_vt::alloc::testing::CountingAllocator;
        thread_local! {
            static COUNTING: core::cell::Cell<*const CountingAllocator> =
                const { core::cell::Cell::new(std::ptr::null()) };
            static WRITE_CONTENTS: core::cell::Cell<usize> = const { core::cell::Cell::new(0) };
            static WRITE_CALLS: core::cell::Cell<usize> = const { core::cell::Cell::new(0) };
            static REPLY_CONTENTS: core::cell::Cell<usize> = const { core::cell::Cell::new(0) };
            static REPLY_ALLOCS: core::cell::Cell<usize> = const { core::cell::Cell::new(0) };
        }
        fn counting() -> &'static CountingAllocator {
            // SAFETY: set to the test's live allocator for the test's body.
            unsafe { &*COUNTING.with(|c| c.get()) }
        }
        extern "C" fn write_pty(_: Terminal, _: *mut c_void, _: *const u8, _: usize) {}
        extern "C" fn clipboard_write(_: Terminal, _: *mut c_void, request: *const ClipboardWrite) {
            unsafe {
                WRITE_CONTENTS.with(|c| c.set((*request).contents_len));
                WRITE_CALLS.with(|c| c.set(counting().alloc_calls.get()));
                ((*request).reply)(
                    request,
                    &ClipboardWriteReply {
                        size: std::mem::size_of::<ClipboardWriteReply>(),
                        result: clipboard::WriteStatus::Success,
                        remember: false,
                    },
                );
            }
        }
        extern "C" fn clipboard_read(_: Terminal, _: *mut c_void, request: *const ClipboardRead) {
            let n = REPLY_CONTENTS.with(|c| c.get());
            let contents = [ClipboardContent {
                mime: LibString::init(b"text/plain"),
                data: LibString::init(b"x"),
            }; 9];
            let before = counting().alloc_calls.get();
            unsafe {
                ((*request).reply)(
                    request,
                    &ClipboardReadReply {
                        size: std::mem::size_of::<ClipboardReadReply>(),
                        result: clipboard::ReadStatus::Success,
                        contents: contents.as_ptr(),
                        contents_len: n,
                        available: std::ptr::null(),
                        available_len: 0,
                        remember: false,
                    },
                );
            }
            REPLY_ALLOCS.with(|c| c.set(counting().alloc_calls.get() - before));
        }

        let counting_alloc = CountingAllocator::over_heap();
        COUNTING.with(|c| c.set(&counting_alloc));
        let calloc = counting_alloc.allocator();
        let mut t: Terminal = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe {
            new(&calloc as *const CAllocator, &mut t, 80, 24)
        });
        unsafe {
            assert_eq!(
                Result::Success,
                set(t, Option_::WritePty, write_pty as *const c_void)
            );
            assert_eq!(
                Result::Success,
                set(t, Option_::ClipboardWrite, clipboard_write as *const c_void)
            );
            assert_eq!(
                Result::Success,
                set(t, Option_::ClipboardRead, clipboard_read as *const c_void)
            );
        }
        assert_eq!(32, std::mem::size_of::<ClipboardContent>());

        // Writes: four representations stay on the stack, five spill one
        // allocation more before the callback runs.
        let mut spills = [0usize; 2];
        for (i, (n, mimes)) in [0usize, 1].into_iter().zip([
            (
                4usize,
                &["dGV4dC9h", "dGV4dC9i", "dGV4dC9j", "dGV4dC9k"][..],
            ),
            (
                5,
                &["dGV4dC9h", "dGV4dC9i", "dGV4dC9j", "dGV4dC9k", "dGV4dC9l"][..],
            ),
        ]) {
            vtw(t, b"\x1B]5522;type=write:id=w\x1B\\");
            for m in mimes {
                vtw(
                    t,
                    format!("\x1B]5522;type=wdata:mime={m};eA==\x1B\\").as_bytes(),
                );
            }
            let live = counting_alloc.live_bytes.get();
            let before = counting_alloc.alloc_calls.get();
            vtw(t, b"\x1B]5522;type=wdata\x1B\\");
            spills[i] = WRITE_CALLS.with(|c| c.get()) - before;
            assert_eq!(n, WRITE_CONTENTS.with(|c| c.get()));
            assert!(
                counting_alloc.live_bytes.get() <= live,
                "the array is freed"
            );
        }
        assert_eq!(spills[0] + 1, spills[1], "five representations spill once");

        // Read replies: eight contents fit the 256-byte buffer, nine spill one
        // allocation more.
        let mut allocs = [0usize; 2];
        for (i, n) in [8usize, 9].into_iter().enumerate() {
            REPLY_CONTENTS.with(|c| c.set(n));
            vtw(t, b"\x1B]5522;type=read:id=r;dGV4dC9wbGFpbg==\x1B\\");
            allocs[i] = REPLY_ALLOCS.with(|c| c.get());
        }
        assert_eq!(allocs[0] + 1, allocs[1], "nine contents spill once");

        unsafe { free(t) };
        assert_eq!(0, counting_alloc.live_bytes.get(), "every byte is returned");
        COUNTING.with(|c| c.set(std::ptr::null()));
    }

    // Parity tail S7-4 (routing evidence, no reference counterpart): enabling
    // continuation tracking builds the tracker over the terminal's allocator
    // (`.init(wrapper.terminal.gpa(), max_bytes)`): its best-effort
    // `ensureTotalCapacity(4096)` reservation (`growCapacity(4096)` bytes) and
    // its growth reach the caller's allocator, and disabling or `free` returns
    // every byte.
    #[test]
    fn continuation_tracker_routes_through_the_terminals_allocator() {
        use rustty_vt::alloc::testing::CountingAllocator;
        let counting = CountingAllocator::over_heap();
        let calloc = counting.allocator();
        let mut t: Terminal = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe {
            new(&calloc as *const CAllocator, &mut t, 80, 24)
        });
        let live = counting.live_bytes.get();
        let before = counting.alloc_calls.get();
        let max: usize = 16384;
        assert_eq!(Result::Success, unsafe {
            set(
                t,
                Option_::ContinuationMaxBytes,
                &max as *const usize as *const c_void,
            )
        });
        assert_eq!(before + 1, counting.alloc_calls.get(), "the reservation");
        let first = rustty_vt::alloc::ArrayListUnmanaged::<u8>::grow_capacity(4096);
        #[cfg(any(target_arch = "aarch64", target_arch = "x86_64"))]
        assert_eq!(6272, first);
        assert_eq!(Some((first, 0)), counting.last_alloc.get());

        let mut input = vec![b'1'; 7000];
        input[0..2].copy_from_slice(b"\x1b[");
        vtw(t, &input);
        assert!(
            counting.live_bytes.get() >= live + 7000,
            "the retained suffix is on the caller's allocator"
        );

        let zero: usize = 0;
        assert_eq!(Result::Success, unsafe {
            set(
                t,
                Option_::ContinuationMaxBytes,
                &zero as *const usize as *const c_void,
            )
        });
        assert_eq!(
            live,
            counting.live_bytes.get(),
            "disabling frees the suffix"
        );
        unsafe { free(t) };
        assert_eq!(0, counting.live_bytes.get());
    }

    // Parity tail S7-4: the sweep corpus is not vacuous. On a clean run over a
    // counting C allocator it stores a Kitty image with an animation frame
    // (and deletes the chunked one), registers the glyph, and hands both
    // clipboard writes to the effect; every byte it took is returned by `free`.
    #[test]
    #[cfg_attr(
        miri,
        ignore = "MIRI EXCEPTION (rustty-vt-c, parity tail C-1): intractable under Miri, one pass of the sweep corpus costs about 30 minutes of Miri time (its 600-column resize alone about 27); measured past a 20-minute bound; runs natively and under ASan (see MIRI_EXCEPTIONS.md)"
    )]
    fn sweep_corpus_reaches_the_s7_4_families() {
        use rustty_vt::alloc::testing::CountingAllocator;
        let counting = CountingAllocator::over_heap();
        let calloc = counting.allocator();
        let mut t: Terminal = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe {
            new(&calloc as *const CAllocator, &mut t, 80, 24)
        });
        sweep_install_effects(t);
        SWEEP_CLIPBOARD_WRITES.with(|c| c.set(0));
        SWEEP_CLIPBOARD_READS.with(|c| c.set(0));
        let reached = |t: Terminal| {
            // SAFETY: `t` is the live wrapper; raw reads of its terminal and
            // active screen.
            unsafe {
                let term = (*t).terminal;
                let storage = &(*(*term).screens.active).kitty_images;
                let img = storage.images.get(&1).expect("image 1 is stored");
                assert_eq!(
                    1,
                    img.animation
                        .as_ref()
                        .expect("image 1 animates")
                        .frames
                        .len()
                );
                assert!(storage.images.get(&2).is_none(), "image 2 was deleted");
                assert!(
                    (*term).glyph_glossary.contains(0xE0A0),
                    "the glyph is registered"
                );
                let tracker = (*t)
                    .stream
                    .continuation
                    .as_ref()
                    .expect("continuation tracking is on");
                assert!(!tracker.broken);
                assert_eq!(1, (*t).searches.count(), "the search is registered");
                assert!(
                    tracker.bytes.capacity()
                        > rustty_vt::alloc::ArrayListUnmanaged::<u8>::grow_capacity(4096),
                    "the suffix grew"
                );
            }
        };
        for r in sweep_corpus(t, &reached) {
            assert_eq!(Result::Success, r);
        }
        // OSC 52 twice (the long capture and the short write) and the two
        // Kitty clipboard commits; the Kitty read.
        assert_eq!(4, SWEEP_CLIPBOARD_WRITES.with(|c| c.get()));
        assert_eq!(1, SWEEP_CLIPBOARD_READS.with(|c| c.get()));
        let calls_before_free = counting.alloc_calls.get();
        unsafe { free(t) };
        // (S7-3's corpus made 33 allocations; the families above add theirs.)
        assert!(
            calls_before_free > 33,
            "the corpus allocates: {calls_before_free}"
        );
        assert_eq!(0, counting.live_bytes.get(), "free returns every byte");
    }

    /// The effects the sweep corpus exercises: a pty sink, a clipboard writer
    /// that accepts every write, and a clipboard reader that answers with nine
    /// contents. Setting them allocates nothing.
    fn sweep_install_effects(t: Terminal) {
        extern "C" fn clipboard_read(_: Terminal, _: *mut c_void, request: *const ClipboardRead) {
            SWEEP_CLIPBOARD_READS.with(|c| c.set(c.get() + 1));
            let contents = [ClipboardContent {
                mime: LibString::init(b"text/plain"),
                data: LibString::init(b"x"),
            }; 9];
            // SAFETY: `request` is valid for the callback.
            unsafe {
                ((*request).reply)(
                    request,
                    &ClipboardReadReply {
                        size: std::mem::size_of::<ClipboardReadReply>(),
                        result: clipboard::ReadStatus::Success,
                        contents: contents.as_ptr(),
                        contents_len: contents.len(),
                        available: std::ptr::null(),
                        available_len: 0,
                        remember: false,
                    },
                )
            };
        }
        extern "C" fn write_pty(_: Terminal, _: *mut c_void, _: *const u8, _: usize) {}
        extern "C" fn clipboard_write(_: Terminal, _: *mut c_void, request: *const ClipboardWrite) {
            SWEEP_CLIPBOARD_WRITES.with(|c| c.set(c.get() + 1));
            // SAFETY: `request` is valid for the callback.
            unsafe {
                ((*request).reply)(
                    request,
                    &ClipboardWriteReply {
                        size: std::mem::size_of::<ClipboardWriteReply>(),
                        result: clipboard::WriteStatus::Success,
                        remember: false,
                    },
                )
            };
        }
        assert_eq!(Result::Success, unsafe {
            set(t, Option_::WritePty, write_pty as *const c_void)
        });
        assert_eq!(Result::Success, unsafe {
            set(t, Option_::ClipboardWrite, clipboard_write as *const c_void)
        });
        assert_eq!(Result::Success, unsafe {
            set(t, Option_::ClipboardRead, clipboard_read as *const c_void)
        });
    }

    /// One sweep scenario over `failing`: `new`, the corpus, `free`. Asserts
    /// the result set and that every byte taken from the caller's allocator is
    /// returned. Returns (whether `new` failed, API calls that reported OOM).
    fn sweep_once(
        failing: &rustty_vt::alloc::testing::FailingAllocator,
        what: &str,
    ) -> (bool, usize) {
        let calloc = failing.allocator();
        let mut t: Terminal = std::ptr::null_mut();
        let r = unsafe { new(&calloc as *const CAllocator, &mut t, 80, 24) };
        let mut new_failed = false;
        let mut call_failures = 0usize;
        match r {
            Result::Success => {
                assert!(!t.is_null());
                sweep_install_effects(t);
                for r in sweep_corpus(t, &|_| {}) {
                    assert!(
                        matches!(r, Result::Success | Result::OutOfMemory),
                        "{what}: unexpected result {r:?}"
                    );
                    if r == Result::OutOfMemory {
                        call_failures += 1;
                    }
                }
                unsafe { free(t) };
            }
            Result::OutOfMemory => {
                assert!(
                    t.is_null(),
                    "{what}: a failed new must not return a terminal"
                );
                new_failed = true;
            }
            other => panic!("{what}: new returned {other:?}"),
        }
        assert_eq!(
            failing.allocated_bytes.get(),
            failing.freed_bytes.get(),
            "{what}: every byte taken from the caller's allocator is returned"
        );
        assert_eq!(
            failing.allocations.get(),
            failing.deallocations.get(),
            "{what}"
        );
        (new_failed, call_failures)
    }

    // Parity tail S7-3 (T2, no reference counterpart — the Zig
    // `checkAllAllocationFailures` idea applied to the C API): for every n, a
    // `FailingAllocator` that refuses its n-th allocation drives `new`, a write
    // corpus, `resize`, grid-ref tracking and `free`. Nothing aborts, every
    // result is `SUCCESS` or `OUT_OF_MEMORY` (the stream's failures are
    // log-and-drop), and every byte the terminal took is returned by `free`.
    // The sweep ends at the first n that induces no failure: every allocation
    // the scenario makes has then been failed once.
    //
    // A second sweep (S7-3b, review F3) refuses `remap`/`resize` from their
    // n-th call on (`resize_fail_index`; sticky as in Zig, whose index only
    // advances on success), so every later growth takes the alloc+copy+free
    // fallback, with the same checks. Under Miri the corpus is smaller.
    #[test]
    #[cfg_attr(
        miri,
        ignore = "MIRI EXCEPTION (rustty-vt-c, parity tail C-1): intractable under Miri, one pass of the sweep corpus costs about 30 minutes of Miri time (its 600-column resize alone about 27) and the sweep makes one pass per allocation; measured past a 20-minute bound; runs natively and under ASan (see MIRI_EXCEPTIONS.md)"
    )]
    fn allocation_failure_sweep_through_the_c_api() {
        use rustty_vt::alloc::testing::{Config, FailingAllocator};

        let mut n = 0usize;
        let mut new_failures = 0usize;
        let mut call_failures = 0usize;
        loop {
            let failing = FailingAllocator::over_heap(Config {
                fail_index: n,
                ..Config::default()
            });
            let (new_failed, calls) = sweep_once(&failing, &format!("alloc n={n}"));
            new_failures += usize::from(new_failed);
            call_failures += calls;
            if !failing.has_induced_failure.get() {
                break;
            }
            n += 1;
            assert!(n < 10_000, "the sweep must terminate");
        }
        // The sweep reached into the terminal's internals: more allocations
        // than `new`'s own storage, some of them failing `new` itself and some
        // failing later API calls.
        let min_swept = if cfg!(miri) { 10 } else { 20 };
        assert!(n > min_swept, "only {n} allocations were swept");
        assert!(new_failures > 2, "new failed at {new_failures} indices");
        assert!(call_failures > 0, "no API call reported OUT_OF_MEMORY");

        // The remap/resize sweep. A clean run counts the scenario's successful
        // remap/resize calls (R); refusing from each n in 0..=R reaches every
        // one of them (and n = R refuses none).
        let clean = FailingAllocator::over_heap(Config::default());
        // The clean run fails nothing.
        assert_eq!((false, 0), sweep_once(&clean, "remap clean run"));
        let remaps = clean.resize_index.get();
        assert!(remaps > 0, "the corpus makes no remap/resize call");
        for n in 0..=remaps {
            let failing = FailingAllocator::over_heap(Config {
                resize_fail_index: n,
                ..Config::default()
            });
            let (new_failed, call_failures) = sweep_once(&failing, &format!("remap n={n}"));
            // A refused remap never fails the scenario: the fallback allocates.
            assert!(
                !new_failed,
                "remap n={n}: new must not fail on a refused remap"
            );
            assert_eq!(
                0, call_failures,
                "remap n={n}: no call reports OUT_OF_MEMORY on a refused remap"
            );
            assert!(
                failing.resize_index.get() <= n,
                "remap n={n}: no remap/resize succeeded past the refusal"
            );
        }
    }

    // Parity tail S7-7 (Rust-only routing evidence): the terminal's stream
    // builds its OSC command lists through the caller's allocator
    // (`Stream.init(.{ .allocator = alloc, .. })` hands `alloc` to the OSC
    // parser): an OSC 104 with three indices grows the color list's shelf
    // array and shelf 0, an OSC 21 grows the kitty color list once. The OSC
    // parser keeps each list (the reference's `parser.command` owns it) until
    // the next OSC's `reset` or the terminal's `free`. An OSC 104 with two
    // indices allocates nothing (the preallocated slots).
    #[test]
    fn terminal_stream_osc_lists_route_through_the_callers_allocator() {
        use rustty_vt::alloc::testing::CountingAllocator;
        let counting = CountingAllocator::over_heap();
        let calloc = counting.allocator();
        let mut t: Terminal = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe {
            new(&calloc as *const CAllocator, &mut t, 80, 24)
        });
        let live = counting.live_bytes.get();

        let (a, f) = (counting.alloc_calls.get(), counting.free_calls.get());
        vtw(t, b"\x1B]104;1;2\x1B\\");
        assert_eq!(
            (a, f),
            (counting.alloc_calls.get(), counting.free_calls.get())
        );

        vtw(t, b"\x1B]104;1;2;3\x1B\\");
        assert_eq!(
            a + 2,
            counting.alloc_calls.get(),
            "the shelf array and shelf 0"
        );
        assert_eq!(
            f,
            counting.free_calls.get(),
            "the OSC parser keeps the list"
        );

        vtw(t, b"\x1B]21;foreground=#010203;background=?\x1B\\");
        assert_eq!(a + 3, counting.alloc_calls.get(), "the kitty color list");
        assert_eq!(
            f + 2,
            counting.free_calls.get(),
            "OSC 21's reset freed the color list"
        );
        assert_eq!(
            rustty_vt::alloc::ArrayListUnmanaged::<rustty_vt::kitty::color::Request>::grow_capacity(
                1
            ) * core::mem::size_of::<rustty_vt::kitty::color::Request>(),
            counting.live_bytes.get() - live,
            "only the kitty list is live"
        );

        unsafe { free(t) };
        assert_eq!(0, counting.live_bytes.get());
    }

    // Parity tail S7-8 (N331): `continuation_alloc` builds the continuation in
    // a `Writer.Allocating` over the caller's allocator (`continuationAllocIo(
    // .., lib.alloc.default(alloc_), ..)`) and hands its buffer over
    // (`toOwnedSlice`); `restoreContinuation`'s verification writer is on the
    // terminal's allocator and is freed before returning. The continuation is
    // written a byte at a time (the tracker's `writeByte` loop, which
    // `Writer.Allocating` grows by `growCapacity(1)`).
    #[test]
    fn continuation_writers_route_through_their_allocators() {
        use rustty_vt::alloc::testing::{Config, CountingAllocator, FailingAllocator};
        with_test_allocator(|alloc| {
            let (new_result, t) = new_t(alloc, 80, 24);
            assert_eq!(Result::Success, new_result);
            test_enable_continuation(t);
            vtw(t, b"\x1b[31");

            let counting = CountingAllocator::over_heap();
            let c = counting.allocator();
            let mut out_ptr: *mut u8 = std::ptr::null_mut();
            let mut out_len: usize = 0;
            assert_eq!(Result::Success, unsafe {
                continuation_alloc(t, Some(&c), &mut out_ptr, &mut out_len)
            });
            assert_eq!(b"\x1b[31", unsafe {
                core::slice::from_raw_parts(out_ptr, out_len)
            });
            // One growth (the first `writeByte` rebases: growCapacity(1) =
            // 129 bytes), then `toOwnedSlice` remaps it to the 4 bytes.
            assert_eq!(1, counting.alloc_calls.get());
            assert_eq!(Some((129, 0)), counting.last_alloc.get());
            assert_eq!(1, counting.remap_calls.get());
            assert_eq!(0, counting.free_calls.get());
            assert_eq!(4, counting.live_bytes.get());
            unsafe { crate::allocator::free(Some(&c), out_ptr, out_len) };
            assert_eq!(0, counting.live_bytes.get());

            // A failing caller allocator: OUT_OF_MEMORY, nothing handed out.
            for config in [
                Config {
                    fail_index: 0,
                    ..Config::default()
                },
                Config {
                    fail_index: 1,
                    resize_fail_index: 0,
                },
            ] {
                let failing = FailingAllocator::over_heap(config);
                let fc = failing.allocator();
                let mut out_ptr: *mut u8 = std::ptr::null_mut();
                let mut out_len: usize = 0;
                assert_eq!(Result::OutOfMemory, unsafe {
                    continuation_alloc(t, Some(&fc), &mut out_ptr, &mut out_len)
                });
                assert!(out_ptr.is_null());
                assert!(failing.has_induced_failure.get());
                assert_eq!(failing.allocated_bytes.get(), failing.freed_bytes.get());
            }
            unsafe { free(t) };
        });

        // restoreContinuation: the exported copy is on the terminal's
        // allocator, and a failure there is `error.OutOfMemory`.
        let counting = CountingAllocator::over_heap();
        let c = counting.allocator();
        let (new_result, restored) = new_t(&c, 80, 24);
        assert_eq!(Result::Success, new_result);
        test_enable_continuation(restored);
        let (allocs, frees, live) = (
            counting.alloc_calls.get(),
            counting.free_calls.get(),
            counting.live_bytes.get(),
        );
        unsafe { restore_continuation(restored, b"") }.unwrap();
        // An empty continuation writes nothing: no call.
        assert_eq!(allocs, counting.alloc_calls.get());
        unsafe { restore_continuation(restored, b"\x1b[31") }.unwrap();
        // The replay fits the tracker's reservation; the exported copy is
        // one 129-byte growth, freed on return.
        assert_eq!(allocs + 1, counting.alloc_calls.get());
        assert_eq!(frees + 1, counting.free_calls.get());
        assert_eq!(Some((129, 0)), counting.last_alloc.get());
        assert_eq!(Some((129, 0)), counting.last_free.get());
        assert_eq!(live, counting.live_bytes.get());
        unsafe { free(restored) };
        assert_eq!(0, counting.live_bytes.get());

        let failing = FailingAllocator::over_heap(Config::default());
        let fc = failing.allocator();
        let (new_result, restored) = new_t(&fc, 80, 24);
        assert_eq!(Result::Success, new_result);
        test_enable_continuation(restored);
        vtw(restored, b"\x1b[31");
        // The next allocation (the exported copy) fails.
        failing.fail_index.set(failing.alloc_index.get());
        assert_eq!(Err(RestoreContinuationError::OutOfMemory), unsafe {
            restore_continuation(restored, b"")
        });
        assert!(failing.has_induced_failure.get());
        failing.fail_index.set(usize::MAX);
        unsafe { free(restored) };
        assert_eq!(failing.allocated_bytes.get(), failing.freed_bytes.get());
    }

    // Parity tail S7-8 (T2, no reference counterpart): the allocation-failure
    // sweep over the C API's allocating writers. For every n, a
    // `FailingAllocator` refusing its n-th allocation is both the terminal's
    // allocator and the output allocator of `encode_alloc` (whose continuation
    // copy is on the terminal's) and `continuation_alloc`, and
    // `restoreContinuation` exports on the terminal's. Nothing aborts, every
    // result is SUCCESS or OUT_OF_MEMORY (restore: `InvalidContinuation`, the
    // comparison against an empty continuation, or `OutOfMemory`), and every
    // byte is returned. A second loop refuses remap/resize from their n-th
    // call on: the writers fall back to alloc + copy + free and nothing fails.
    #[test]
    fn allocation_failure_sweep_through_the_allocating_writers() {
        use rustty_vt::alloc::testing::{Config, FailingAllocator};

        fn once(failing: &FailingAllocator, what: &str) -> (bool, usize) {
            let calloc = failing.allocator();
            let mut t: Terminal = std::ptr::null_mut();
            match unsafe { new(&calloc as *const CAllocator, &mut t, 10, 3) } {
                Result::Success => {}
                Result::OutOfMemory => {
                    assert!(t.is_null(), "{what}");
                    assert_eq!(failing.allocated_bytes.get(), failing.freed_bytes.get());
                    return (true, 0);
                }
                other => panic!("{what}: new returned {other:?}"),
            }
            let limit: usize = 1024;
            let r = unsafe {
                set(
                    t,
                    Option_::ContinuationMaxBytes,
                    &limit as *const usize as *const c_void,
                )
            };
            assert!(
                matches!(r, Result::Success | Result::OutOfMemory),
                "{what}: {r:?}"
            );
            vtw(t, b"hello\x1b[31");

            let mut failures = 0usize;
            let mut out_ptr: *mut u8 = std::ptr::null_mut();
            let mut out_len: usize = 0;
            let r = unsafe {
                crate::snapshot::encode_alloc(t, Some(&calloc), &mut out_ptr, &mut out_len)
            };
            match r {
                Result::Success => unsafe {
                    crate::allocator::free(Some(&calloc), out_ptr, out_len)
                },
                // A tracker that could not be built or grown leaves the
                // continuation unavailable: INVALID_VALUE, as in the reference.
                Result::OutOfMemory | Result::InvalidValue => failures += 1,
                other => panic!("{what}: encode_alloc returned {other:?}"),
            }
            let r = unsafe { continuation_alloc(t, Some(&calloc), &mut out_ptr, &mut out_len) };
            match r {
                Result::Success => unsafe {
                    crate::allocator::free(Some(&calloc), out_ptr, out_len)
                },
                Result::OutOfMemory | Result::InvalidValue => failures += 1,
                other => panic!("{what}: continuation_alloc returned {other:?}"),
            }
            match unsafe { restore_continuation(t, b"") } {
                Ok(()) => {}
                Err(RestoreContinuationError::OutOfMemory) => failures += 1,
                Err(
                    RestoreContinuationError::InvalidContinuation
                    | RestoreContinuationError::ContinuationDisabled
                    | RestoreContinuationError::ContinuationUnavailable,
                ) => {}
            }
            unsafe { free(t) };
            assert_eq!(
                failing.allocated_bytes.get(),
                failing.freed_bytes.get(),
                "{what}: every byte is returned"
            );
            assert_eq!(
                failing.allocations.get(),
                failing.deallocations.get(),
                "{what}"
            );
            (false, failures)
        }

        let mut n = 0usize;
        let mut call_failures = 0usize;
        loop {
            let failing = FailingAllocator::over_heap(Config {
                fail_index: n,
                ..Config::default()
            });
            let (_, calls) = once(&failing, &format!("alloc n={n}"));
            call_failures += calls;
            if !failing.has_induced_failure.get() {
                break;
            }
            n += 1;
            assert!(n < 10_000, "the sweep must terminate");
        }
        assert!(call_failures > 0, "no writer reported a failure");

        let clean = FailingAllocator::over_heap(Config::default());
        assert_eq!((false, 0), once(&clean, "remap clean run"));
        let remaps = clean.resize_index.get();
        // The writers' `toOwnedSlice` remaps, at least.
        assert!(remaps >= 3, "only {remaps} remaps");
        for n in 0..=remaps {
            let failing = FailingAllocator::over_heap(Config {
                resize_fail_index: n,
                ..Config::default()
            });
            assert_eq!(
                (false, 0),
                once(&failing, &format!("remap n={n}")),
                "remap n={n}: a refused remap never fails a call"
            );
        }

        // Every remap refused AND the n-th allocation refused: the
        // `toOwnedSlice` fallback allocations fail too (the alloc loop's
        // remaps succeed, so it never reaches them).
        let mut n = 0usize;
        let mut call_failures = 0usize;
        loop {
            let failing = FailingAllocator::over_heap(Config {
                fail_index: n,
                resize_fail_index: 0,
            });
            let (_, calls) = once(&failing, &format!("no-remap alloc n={n}"));
            call_failures += calls;
            if !failing.has_induced_failure.get() {
                break;
            }
            n += 1;
            assert!(n < 10_000, "the sweep must terminate");
        }
        assert!(call_failures > 0, "no writer reported a failure");
    }
}
