//! C ABI wrappers for snapshot encoding and incremental restoration.
//!
//! PORTS: ghostty src/terminal/c/snapshot.zig
//!
//! PORT STATUS: complete (P4-W5 Bundle S — d7bb4b863 end state with the
//! 9be6c2ea2 retain-continuation option; digest-free per 219173ab3; buffered
//! writer per 846d24e12). The reference `terminal_c.Io` transfer plumbing is
//! N/A (see terminal.rs's `Io` PORT NOTE / the NON_PORTS TinyIo row).

// const std = @import("std");
// const testing = std.testing;
// const lib = @import("../lib.zig");
// const CAllocator = lib.alloc.Allocator;
// const io_c = @import("io.zig");
// const Result = @import("result.zig").Result;
// const snapshot_core = @import("../snapshot/main.zig");
// const apc = @import("../apc.zig");
use crate::io as io_c;
use crate::result::Result;
use crate::terminal as terminal_c;
use rustty_lib::allocator::Allocator as CAllocator;
use rustty_vt::snapshot as snapshot_core;
use rustty_vt::snapshot::io as vt_io;
use std::ffi::c_void;
use std::os::raw::c_int;

/// Snapshot readers accept the largest default APC payload. Deriving this from
/// the APC protocols keeps snapshot validation in sync as protocols change.
/// Opt-in decoded-terminal retention reuses this configured validation bound
/// as its runtime tracking limit.
// const default_max_continuation_bytes = apc.Protocol.maxDefaultBytes();
// PORT NOTE: `maxDefaultBytes` is comptime in Zig; the Rust port's is a plain
// fn, so this is a fn too (evaluated at each use, same value).
fn default_max_continuation_bytes() -> usize {
    rustty_vt::apc::Protocol::max_default_bytes()
}
// const terminal_c = @import("terminal.zig");

/// C: GhosttySnapshotDecoderOption -> RusttySnapshotDecoderOption (R2)
///
/// Options configure validation and returned-terminal policy before snapshot
/// decoding begins.
// pub const DecoderOption = enum(c_int) { ..., _ };
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecoderOption {
    MaxContinuationBytes = 0,
    RetainContinuation = 1,
}

// D11 closed-enum c_int narrowing (the `_` non-exhaustive arm of the reference
// enum): the extern "C" wrappers take a raw `c_int` and narrow here.
impl DecoderOption {
    pub fn from_c_int(v: c_int) -> Option<DecoderOption> {
        match v {
            0 => Some(DecoderOption::MaxContinuationBytes),
            1 => Some(DecoderOption::RetainContinuation),
            _ => None,
        }
    }
}
const _: () = assert!(std::mem::size_of::<DecoderOption>() == std::mem::size_of::<c_int>());

/// C: GhosttySnapshotDecoderData -> RusttySnapshotDecoderData (R2)
///
/// Data keys select typed values exposed by the decoder getter APIs.
// pub const DecoderData = enum(c_int) { ..., _ };
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecoderData {
    Invalid = 0,
    MaxContinuationBytes = 1,
    SourceOffset = 2,
    HistoryRowsPrimary = 3,
    HistoryRowsAlternate = 4,
    ProgressScreen = 5,
    ProgressRows = 6,
    ProgressRemaining = 7,
    RetainContinuation = 8,
    // /// Return the Zig type stored through the C API output pointer for a key.
    // fn OutType(comptime self: DecoderData) type {
    //     return switch (self) {
    //         .invalid => void,
    //         .max_continuation_bytes, .source_offset => usize,
    //         .history_rows_primary, .history_rows_alternate => u64,
    //         .progress_screen => terminal_c.TerminalScreen,
    //         .progress_rows => usize,
    //         .progress_remaining => u32,
    //         .retain_continuation => bool,
    //         _ => void,
    //     };
    // }
    //
    // PORT NOTE: `OutType` is a comptime type-returning fn; rustty erases the
    // output to `*mut c_void` at the FFI boundary and writes it back as the
    // per-arm type inside `decoder_get_typed` (the terminal.rs convention).
}

impl DecoderData {
    pub fn from_c_int(v: c_int) -> Option<DecoderData> {
        // Contiguous 0..=8; `Invalid = 0` is IN range and keeps its
        // `.invalid_value` disposition inside `decoder_get`.
        if (0..=8).contains(&v) {
            // SAFETY: `v` is a valid `DecoderData` discriminant (checked).
            Some(unsafe { std::mem::transmute::<c_int, DecoderData>(v) })
        } else {
            None
        }
    }
}
const _: () = assert!(std::mem::size_of::<DecoderData>() == std::mem::size_of::<c_int>());

/// A callback or borrowed-buffer source with uniform reader access.
// const Source = union(enum) { callback: io_c.ReaderAdapter, buffer: std.Io.Reader, ... };
//
// PORT NOTE: the reference's `reader()`/`offset()` accessors dissolve — the
// enum implements the reader trait directly (uniform access), and the offset
// observable is routed through the shared `ReaderState` cell by both variants
// (see `io_c::ReaderState`'s SB PORT NOTE); `DecoderWrapper` reads that cell.
pub enum Source {
    Callback(io_c::ReaderAdapter),
    Buffer(io_c::BufferSource),
}

impl vt_io::Reader for Source {
    fn buffered(&self) -> &[u8] {
        match self {
            Source::Callback(value) => value.buffered(),
            Source::Buffer(value) => value.buffered(),
        }
    }

    fn toss(&mut self, n: usize) {
        match self {
            Source::Callback(value) => value.toss(n),
            Source::Buffer(value) => value.toss(n),
        }
    }

    fn refill(&mut self) -> std::result::Result<(), vt_io::ReadError> {
        match self {
            Source::Callback(value) => value.refill(),
            Source::Buffer(value) => value.refill(),
        }
    }

    fn take_buffered(&mut self, n: usize) -> &[u8] {
        match self {
            Source::Callback(value) => value.take_buffered(n),
            Source::Buffer(value) => value.take_buffered(n),
        }
    }

    fn read_slice_all(&mut self, dest: &mut [u8]) -> std::result::Result<(), vt_io::ReadError> {
        match self {
            Source::Callback(value) => value.read_slice_all(dest),
            Source::Buffer(value) => value.read_slice_all(dest),
        }
    }

    fn stream_to(&mut self, dest: &mut [u8]) -> std::result::Result<usize, vt_io::ReadError> {
        match self {
            Source::Callback(value) => value.stream_to(dest),
            Source::Buffer(value) => value.stream_to(dest),
        }
    }
}

/// READY metadata that remains queryable through FINISH or a later error.
// const Metadata = struct { history_rows: std.EnumMap(terminal_c.TerminalScreen, u64) };
#[derive(Clone, Copy)]
struct Metadata {
    history_rows: snapshot_core::HistoryRows,
}

/// Decoder lifecycle and the data available in each phase.
// const State = union(enum) { configuring, history: ..., finished: Metadata, failed: ?Metadata };
enum State {
    Configuring,

    History {
        terminal: terminal_c::Terminal,
        metadata: Metadata,
        progress: Option<snapshot_core::Progress>,
    },

    Finished(Metadata),

    /// READY failures have no metadata. Failures from `next` retain the
    /// READY metadata even though decoding cannot continue.
    Failed(Option<Metadata>),
}

/// Owns the source adapter and all state needed by an opaque C decoder handle.
// const DecoderWrapper = struct { alloc, source, decoder, state, max_continuation_bytes, retain_continuation };
//
// PORT NOTE (SB, decoder-aliasing discipline): the reference stores the source
// INLINE and hands the core decoder a pointer into its own heap allocation.
// rustty's core `snapshot::Decoder<'s>` holds `&'s mut dyn Reader` for its
// whole life, so the source lives in its OWN heap allocation (`*mut Source`,
// created before the decoder and freed after it in `decoder_free`) — a later
// `&mut DecoderWrapper` therefore never overlaps the decoder's stored borrow.
// The offset/failure observables the wrapper must read BETWEEN decoder calls
// live in the raw-only `ReaderState` cell (see io.rs). LIFETIMES rows:
// `.port/audits/GHOSTTY_LIFETIMES.tsv` (snapshot.rs entries).
pub struct DecoderWrapper {
    /// The caller's C allocator selection (null = default), re-resolved per
    /// use. Stored as the caller's ORIGINAL pointer — not a copy — because the
    /// reference's resolved `std.mem.Allocator` value likewise chains to the
    /// embedder's allocator struct, and `fromDecoded` hands it to a terminal
    /// that OUTLIVES this decoder (a copy inside this wrapper would dangle
    /// once `decoder_free` runs before `terminal_free`). The C contract keeps
    /// the struct alive for everything allocated through it.
    alloc: *const CAllocator,
    /// Heap-stable source; the core decoder borrows it until `decoder_free`.
    source: *mut Source,
    /// Shared source observables; raw-access only (see `io_c::ReaderState`).
    reader_state: *mut io_c::ReaderState,
    decoder: snapshot_core::Decoder<'static>,
    state: State,
    max_continuation_bytes: usize,
    retain_continuation: bool,
}

/// C: GhosttySnapshotDecoder, an opaque nullable decoder handle.
// pub const Decoder = ?*DecoderWrapper;
pub type Decoder = *mut DecoderWrapper;

/// Create a decoder backed by a synchronous C reader callback.
// pub fn decoder_new(alloc_, out_, reader) ... Result {
/// # Safety
/// `alloc_`, if provided, must reference a valid `CAllocator`; `out_`, if
/// non-null, must be writable; `reader`'s callback/userdata must satisfy the
/// documented C contract for the decoder's lifetime.
pub unsafe fn decoder_new(
    alloc_: Option<&CAllocator>,
    out_: *mut Decoder,
    reader: io_c::Reader,
) -> Result {
    // Validate C-owned output and callback pointers before allocating anything.
    // const out = out_ orelse return .invalid_value;
    if out_.is_null() {
        return Result::InvalidValue;
    }
    // out.* = null;
    // SAFETY: `out_` is non-null and writable (caller contract).
    unsafe { *out_ = std::ptr::null_mut() };
    if reader.read.is_none() {
        return Result::InvalidValue;
    }

    // Store the adapter inline so its std.Io.Reader has a stable address.
    // SAFETY: forwarded caller contracts; the state cell is wired by
    // `decoder_new_source` before the adapter is used.
    unsafe {
        decoder_new_source(alloc_, out_, |state| {
            Source::Callback(io_c::ReaderAdapter::init(reader, state))
        })
    }
}

/// Create a decoder backed by immutable, caller-owned bytes.
// pub fn decoder_new_buf(alloc_, out_, ptr, len) ... Result {
/// # Safety
/// As `decoder_new`; `ptr`, if non-null, must point to `len` bytes that remain
/// valid and immutable until FINISH is reached or the decoder is freed.
pub unsafe fn decoder_new_buf(
    alloc_: Option<&CAllocator>,
    out_: *mut Decoder,
    ptr: *const u8,
    len: usize,
) -> Result {
    // Normalize the C pointer and length into a borrowed Zig slice.
    // const out = out_ orelse return .invalid_value;
    if out_.is_null() {
        return Result::InvalidValue;
    }
    // SAFETY: `out_` is non-null and writable (caller contract).
    unsafe { *out_ = std::ptr::null_mut() };
    let bytes: &'static [u8] = if !ptr.is_null() {
        // SAFETY: `ptr[0..len]` stays valid and immutable until FINISH/free
        // (caller contract) — the `'static` is that C lifetime contract.
        unsafe { core::slice::from_raw_parts(ptr, len) }
    } else if len == 0 {
        &[]
    } else {
        return Result::InvalidValue;
    };

    // The fixed reader records its current seek position as the source offset.
    // SAFETY: forwarded caller contracts.
    unsafe {
        decoder_new_source(alloc_, out_, |state| {
            Source::Buffer(io_c::BufferSource::init(bytes, state))
        })
    }
}

/// Allocate and initialize a decoder around either supported source type.
// fn decoderNewSource(alloc_, out, source) Result {
/// # Safety
/// Caller contracts forwarded from the public constructors.
unsafe fn decoder_new_source(
    alloc_: Option<&CAllocator>,
    out: *mut Decoder,
    make_source: impl FnOnce(*mut io_c::ReaderState) -> Source,
) -> Result {
    // Allocate first so the source and its embedded reader never move again.
    // const alloc = lib.alloc.default(alloc_);
    // const wrapper = alloc.create(DecoderWrapper) catch return .out_of_memory;
    // errdefer alloc.destroy(wrapper);
    // SAFETY: `alloc_` validity is the caller's contract; the storage is
    // written below before any read (`alloc_uninit` shape).
    let wrapper_ptr: *mut DecoderWrapper = unsafe { crate::cwrap::alloc_uninit(alloc_) };
    if wrapper_ptr.is_null() {
        return Result::OutOfMemory;
    }

    // PORT NOTE: the source and its observables cell are separate heap
    // allocations (global allocator — internal state, like Vec growth) so the
    // core decoder's stored `&mut` never overlaps wrapper accesses; see the
    // `DecoderWrapper` SB PORT NOTE.
    let reader_state: *mut io_c::ReaderState = Box::into_raw(Box::default());
    let source: *mut Source = Box::into_raw(Box::new(make_source(reader_state)));

    // Initialize the core decoder only after its reader has a stable address.
    // wrapper.decoder = .init(wrapper.source.reader());
    // SAFETY: `source` is heap-stable until `decoder_free` reclaims it; the
    // decoder's `&'static mut` borrow is that C lifetime contract.
    let source_ref: &'static mut Source = unsafe { &mut *source };
    let decoder = snapshot_core::Decoder::init(source_ref);

    // wrapper.* = undefined; wrapper.alloc = alloc; ... out.* = wrapper;
    // SAFETY: `wrapper_ptr` is valid uninitialized storage from `alloc_uninit`.
    unsafe {
        wrapper_ptr.write(DecoderWrapper {
            alloc: alloc_.map_or(std::ptr::null(), |r| r as *const CAllocator),
            source,
            reader_state,
            decoder,
            state: State::Configuring,
            max_continuation_bytes: default_max_continuation_bytes(),
            retain_continuation: false,
        });
        *out = wrapper_ptr;
    }
    Result::Success
}

/// Destroy a decoder without taking ownership of any returned terminal.
// pub fn decoder_free(decoder_: Decoder) ... void {
/// # Safety
/// `decoder_`, if non-null, must be a live decoder handle, not yet freed.
pub unsafe fn decoder_free(decoder_: Decoder) {
    // const decoder = decoder_ orelse return;
    if decoder_.is_null() {
        return;
    }
    // const alloc = decoder.alloc;
    // alloc.destroy(decoder);
    // SAFETY: `decoder_` was produced by `decoder_new_source` via
    // `alloc_uninit` with `alloc`; move the value out, return the storage,
    // then release the internal allocations — the source strictly AFTER the
    // decoder value (which borrows it) is gone.
    let wrapper: DecoderWrapper = unsafe { std::ptr::read(decoder_) };
    // SAFETY: `alloc`, if non-null, is the caller's still-live allocator (C
    // contract; see the field note).
    unsafe { crate::cwrap::free_uninit(wrapper.alloc.as_ref(), decoder_) };
    let DecoderWrapper {
        source,
        reader_state,
        decoder,
        state,
        ..
    } = wrapper;
    drop(decoder);
    drop(state);
    // SAFETY: both came from `Box::into_raw` in `decoder_new_source`; nothing
    // refers to them anymore.
    unsafe {
        drop(Box::from_raw(source));
        drop(Box::from_raw(reader_state));
    }
}

/// Set a decoder option while the decoder is still configuring.
// pub fn decoder_set(decoder_, option, value_) ... Result {
/// # Safety
/// `decoder_`, if non-null, must be a live decoder handle; `value_`, if
/// non-null, must point to the type documented for `option`.
pub unsafe fn decoder_set(
    decoder_: Decoder,
    option: DecoderOption,
    value_: *const c_void,
) -> Result {
    // Options freeze as soon as any decoding attempt starts.
    // const decoder = decoder_ orelse return .invalid_value;
    if decoder_.is_null() {
        return Result::InvalidValue;
    }
    // SAFETY: `decoder_` is a live handle (caller contract).
    match unsafe { &(*decoder_).state } {
        State::Configuring => {}
        _ => return Result::InvalidValue,
    }

    // Interpret the erased C pointer according to the selected option.
    // const value = value_ orelse return .invalid_value;
    if value_.is_null() {
        return Result::InvalidValue;
    }
    match option {
        // SAFETY (both arms): `value_` points to the documented type (caller
        // contract); `decoder_` is live.
        DecoderOption::MaxContinuationBytes => unsafe {
            (*decoder_).max_continuation_bytes = *(value_ as *const usize)
        },
        DecoderOption::RetainContinuation => unsafe {
            (*decoder_).retain_continuation = *(value_ as *const bool)
        },
    }
    Result::Success
}

/// Read one typed decoder value through the type-erased C API.
// pub fn decoder_get(decoder_, data, out) ... Result {
/// # Safety
/// `decoder_`, if non-null, must be a live decoder handle; `out`, if non-null,
/// must point to writable storage of the type documented for `data`.
pub unsafe fn decoder_get(decoder_: Decoder, data: DecoderData, out: *mut c_void) -> Result {
    // Enumerate known keys so each branch recovers the correct output type.
    // (`inline .invalid, ... => |comptime_data| decoderGetTyped(...)`; the `_`
    // open arm is rejected by the extern "C" wrapper's `from_c_int` guard.)
    // SAFETY: forwarded caller contracts.
    unsafe { decoder_get_typed(decoder_, data, out) }
}

/// Implement a getter after the key has selected its concrete output type.
// fn decoderGetTyped(decoder_, comptime data, out: *data.OutType()) Result {
/// # Safety
/// As `decoder_get`.
unsafe fn decoder_get_typed(decoder_: Decoder, data: DecoderData, out: *mut c_void) -> Result {
    // const decoder = decoder_ orelse return .invalid_value;
    if decoder_.is_null() {
        return Result::InvalidValue;
    }
    match data {
        // Decoder-wide configuration and source state.
        DecoderData::Invalid => return Result::InvalidValue,
        DecoderData::MaxContinuationBytes => {
            // SAFETY: `decoder_` is live; `out` points to a writable `usize`.
            unsafe {
                if decoder_failed(decoder_) {
                    return Result::NoValue;
                }
                *(out as *mut usize) = (*decoder_).max_continuation_bytes;
            }
        }
        DecoderData::SourceOffset => {
            // out.* = decoder.source.offset();
            // SAFETY: `decoder_` is live; the offset observable is the shared
            // raw-only cell (both source kinds maintain it — io.rs).
            unsafe {
                if decoder_failed(decoder_) {
                    return Result::NoValue;
                }
                *(out as *mut usize) = (*(*decoder_).reader_state).offset;
            }
        }
        DecoderData::RetainContinuation => {
            // SAFETY: `decoder_` is live; `out` points to a writable `bool`.
            unsafe {
                if decoder_failed(decoder_) {
                    return Result::NoValue;
                }
                *(out as *mut bool) = (*decoder_).retain_continuation;
            }
        }

        // READY metadata remains available after history decoding completes.
        DecoderData::HistoryRowsPrimary => {
            // SAFETY: `decoder_` is live; `out` points to a writable `u64`.
            unsafe {
                let metadata = match decoder_metadata(decoder_) {
                    Some(metadata) => metadata,
                    None => return Result::NoValue,
                };
                let value = match metadata
                    .history_rows
                    .get(rustty_vt::screen_set::Key::Primary)
                {
                    Some(value) => value,
                    None => return Result::NoValue,
                };
                *(out as *mut u64) = value;
            }
        }
        DecoderData::HistoryRowsAlternate => {
            // SAFETY: as `HistoryRowsPrimary`.
            unsafe {
                let metadata = match decoder_metadata(decoder_) {
                    Some(metadata) => metadata,
                    None => return Result::NoValue,
                };
                let value = match metadata
                    .history_rows
                    .get(rustty_vt::screen_set::Key::Alternate)
                {
                    Some(value) => value,
                    None => return Result::NoValue,
                };
                *(out as *mut u64) = value;
            }
        }

        // Progress describes only the most recent successful `next` call.
        DecoderData::ProgressScreen => {
            // SAFETY: `decoder_` is live; `out` points to a writable
            // `RusttyTerminalScreen`.
            unsafe {
                let progress = match decoder_progress(decoder_) {
                    Some(progress) => progress,
                    None => return Result::NoValue,
                };
                *(out as *mut terminal_c::TerminalScreen) = progress.key;
            }
        }
        DecoderData::ProgressRows => {
            // SAFETY: `out` points to a writable `usize`.
            unsafe {
                let progress = match decoder_progress(decoder_) {
                    Some(progress) => progress,
                    None => return Result::NoValue,
                };
                *(out as *mut usize) = progress.rows;
            }
        }
        DecoderData::ProgressRemaining => {
            // SAFETY: `out` points to a writable `u32`.
            unsafe {
                let progress = match decoder_progress(decoder_) {
                    Some(progress) => progress,
                    None => return Result::NoValue,
                };
                *(out as *mut u32) = progress.remaining;
            }
        }
    }
    Result::Success
}

/// Return whether a consuming operation has permanently failed.
// fn decoderFailed(decoder: *const DecoderWrapper) bool {
/// # Safety
/// `decoder` must be a live decoder handle.
unsafe fn decoder_failed(decoder: *const DecoderWrapper) -> bool {
    // SAFETY: `decoder` is live (caller contract); shared read.
    matches!(unsafe { &(*decoder).state }, State::Failed(_))
}

/// Return READY metadata in every state where it remains meaningful.
// fn decoderMetadata(decoder: *const DecoderWrapper) ?DecoderWrapper.Metadata {
/// # Safety
/// `decoder` must be a live decoder handle.
unsafe fn decoder_metadata(decoder: *const DecoderWrapper) -> Option<Metadata> {
    // SAFETY: `decoder` is live (caller contract); shared read.
    match unsafe { &(*decoder).state } {
        State::History { metadata, .. } => Some(*metadata),
        State::Finished(metadata) => Some(*metadata),
        State::Failed(metadata) => *metadata,
        State::Configuring => None,
    }
}

/// Return page progress only during incremental history decoding.
// fn decoderProgress(decoder: *const DecoderWrapper) ?snapshot_core.Decoder.Progress {
/// # Safety
/// `decoder` must be a live decoder handle.
unsafe fn decoder_progress(decoder: *const DecoderWrapper) -> Option<snapshot_core::Progress> {
    // SAFETY: `decoder` is live (caller contract); shared read.
    match unsafe { &(*decoder).state } {
        State::History { progress, .. } => *progress,
        _ => None,
    }
}

/// Query several typed decoder values in order, stopping at the first error.
// pub fn decoder_get_multi(decoder_, count, keys_, values_, out_written) ... Result {
/// # Safety
/// `decoder_`, if non-null, must be a live decoder handle; `keys_`/`values_`,
/// if non-null, must point to `count` readable elements; each values element
/// must satisfy `decoder_get`'s contract for its key; `out_written`, if
/// non-null, must be writable.
pub unsafe fn decoder_get_multi(
    decoder_: Decoder,
    count: usize,
    keys_: *const DecoderData,
    values_: *mut *mut c_void,
    out_written: *mut usize,
) -> Result {
    // Invalid arrays and first-key failures both report zero completed writes.
    if !out_written.is_null() {
        // SAFETY: `out_written` is non-null and writable (caller contract).
        unsafe { *out_written = 0 };
    }
    // const keys = keys_ orelse return .invalid_value;
    // const values = values_ orelse return .invalid_value;
    if keys_.is_null() || values_.is_null() {
        return Result::InvalidValue;
    }

    // Reuse the single-key path so availability rules remain identical.
    for i in 0..count {
        // SAFETY: `keys_[i]`/`values_[i]` are readable (caller contract).
        // The keys array crossed the ABI as raw `c_int`s: narrow per element
        // (the get_multi convention) rather than trusting the storage.
        let raw = unsafe { *(keys_.add(i) as *const c_int) };
        let key = match DecoderData::from_c_int(raw) {
            Some(key) => key,
            None => {
                if !out_written.is_null() {
                    unsafe { *out_written = i };
                }
                return Result::InvalidValue;
            }
        };
        let value = unsafe { *values_.add(i) };
        // SAFETY: forwarded caller contracts.
        let result = unsafe { decoder_get(decoder_, key, value) };
        if result != Result::Success {
            if !out_written.is_null() {
                unsafe { *out_written = i };
            }
            return result;
        }
    }
    if !out_written.is_null() {
        unsafe { *out_written = count };
    }
    Result::Success
}

/// Decode through READY and return a renderable terminal for incremental use.
// pub fn decoder_ready(decoder_, out_) ... Result {
/// # Safety
/// `decoder_`, if non-null, must be a live decoder handle; `out_`, if
/// non-null, must be writable.
pub unsafe fn decoder_ready(decoder_: Decoder, out_: *mut terminal_c::Terminal) -> Result {
    // Validate the output and require an untouched decoder lifecycle.
    // const out = out_ orelse return .invalid_value;
    if out_.is_null() {
        return Result::InvalidValue;
    }
    // out.* = null;
    // SAFETY: `out_` is non-null and writable (caller contract).
    unsafe { *out_ = std::ptr::null_mut() };
    // const decoder = decoder_ orelse return .invalid_value;
    if decoder_.is_null() {
        return Result::InvalidValue;
    }
    // SAFETY: `decoder_` is a live handle (caller contract).
    match unsafe { &(*decoder_).state } {
        State::Configuring => {}
        _ => return Result::InvalidValue,
    }
    // Decode READY and transfer the decoded state into a C-owned terminal.
    // SAFETY: forwarded caller contracts.
    let ready = match unsafe { decoder_ready_terminal(decoder_) } {
        Ok(ready) => ready,
        Err(err) => {
            unsafe { (*decoder_).state = State::Failed(None) };
            return unsafe { decoder_map_error(decoder_, err) };
        }
    };

    // Keep only the terminal handle and queryable metadata for page decoding.
    unsafe {
        (*decoder_).state = State::History {
            terminal: ready.terminal,
            metadata: ready.metadata,
            progress: None,
        };
        *out_ = ready.terminal;
    }
    Result::Success
}

/// Decode and apply exactly one history page, or validate FINISH.
// pub fn decoder_next(decoder_: Decoder) ... Result {
/// # Safety
/// `decoder_`, if non-null, must be a live decoder handle whose READY
/// terminal (if any) is still alive.
pub unsafe fn decoder_next(decoder_: Decoder) -> Result {
    // FINISH is idempotent; every other non-history state rejects `next`.
    // const decoder = decoder_ orelse return .invalid_value;
    if decoder_.is_null() {
        return Result::InvalidValue;
    }
    // SAFETY: `decoder_` is a live handle (caller contract).
    let (terminal, metadata) = match unsafe { &(*decoder_).state } {
        State::Finished(_) => return Result::NoValue,
        State::History {
            terminal, metadata, ..
        } => (*terminal, *metadata),
        _ => return Result::InvalidValue,
    };

    // Resolve the terminal retained by READY and clear stale page progress.
    // const terminal = history.terminal orelse return .invalid_value;
    if terminal.is_null() {
        return Result::InvalidValue;
    }
    // const native = terminal_c.zigTerminal(terminal).?;
    // SAFETY: `terminal` is the live READY terminal (caller contract).
    let native = unsafe { terminal_c::zig_terminal(terminal) };
    // history.progress = null;
    unsafe {
        if let State::History { progress, .. } = &mut (*decoder_).state {
            *progress = None;
        }
    }

    // Consume one validated record and preserve READY metadata on failure.
    // const progress = decoder.decoder.next(decoder.alloc, native) catch |err| { ... };
    // SAFETY: transient `&mut` of the decoder field / native terminal off the
    // raw handles; both die with this statement (the
    // rust-mut-args-strongly-protected shape).
    // SAFETY: `decoder_` is a live handle (caller contract).
    let gpa = unsafe { decoder_gpa(decoder_) };
    let progress = match unsafe { (*decoder_).decoder.next(gpa, &mut *native) } {
        Ok(progress) => progress,
        Err(err) => {
            unsafe { (*decoder_).state = State::Failed(Some(metadata)) };
            return unsafe { decoder_map_error(decoder_, ReadyTerminalError::Decode(err.into())) };
        }
    };

    // Publish page progress, or transition permanently to validated FINISH.
    if let Some(value) = progress {
        unsafe {
            if let State::History { progress, .. } = &mut (*decoder_).state {
                *progress = Some(value);
            }
        }
        return Result::Success;
    }

    unsafe { (*decoder_).state = State::Finished(metadata) };
    Result::NoValue
}

/// Decode and validate the complete snapshot transactionally.
// pub fn decoder_decode(decoder_, out_) ... Result {
/// # Safety
/// As `decoder_ready`.
pub unsafe fn decoder_decode(decoder_: Decoder, out_: *mut terminal_c::Terminal) -> Result {
    // Validate the output and require an untouched decoder lifecycle.
    if out_.is_null() {
        return Result::InvalidValue;
    }
    // SAFETY: `out_` is non-null and writable (caller contract).
    unsafe { *out_ = std::ptr::null_mut() };
    if decoder_.is_null() {
        return Result::InvalidValue;
    }
    // SAFETY: `decoder_` is a live handle (caller contract).
    match unsafe { &(*decoder_).state } {
        State::Configuring => {}
        _ => return Result::InvalidValue,
    }
    // Build the terminal from the renderable READY prefix.
    // SAFETY: forwarded caller contracts.
    let ready = match unsafe { decoder_ready_terminal(decoder_) } {
        Ok(ready) => ready,
        Err(err) => {
            unsafe { (*decoder_).state = State::Failed(None) };
            return unsafe { decoder_map_error(decoder_, err) };
        }
    };
    // const native = terminal_c.zigTerminal(ready.terminal).?;
    // SAFETY: `ready.terminal` is the live terminal created above.
    let native = unsafe { terminal_c::zig_terminal(ready.terminal) };

    // Apply every history page and require a valid FINISH record.
    loop {
        // SAFETY: transient `&mut`s off the raw handles (see `decoder_next`).
        // SAFETY: `decoder_` is a live handle (caller contract).
        let gpa = unsafe { decoder_gpa(decoder_) };
        let progress = match unsafe { (*decoder_).decoder.next(gpa, &mut *native) } {
            Ok(progress) => progress,
            Err(err) => {
                unsafe {
                    terminal_c::free(ready.terminal);
                    (*decoder_).state = State::Failed(Some(ready.metadata));
                }
                return unsafe {
                    decoder_map_error(decoder_, ReadyTerminalError::Decode(err.into()))
                };
            }
        };
        if progress.is_none() {
            break;
        }
    }

    // Publish the terminal only after the entire snapshot succeeds.
    unsafe {
        (*decoder_).state = State::Finished(ready.metadata);
        *out_ = ready.terminal;
    }
    Result::Success
}

/// Values produced while moving a core READY result into a C terminal.
// const ReadyTerminal = struct { terminal: terminal_c.Terminal, metadata: DecoderWrapper.Metadata };
struct ReadyTerminal {
    terminal: terminal_c::Terminal,
    metadata: Metadata,
}

/// The error domain of `decoder_ready_terminal` (the reference's `anyerror`
/// union of decode + `fromDecoded` failures; `Io.init` is N/A).
enum ReadyTerminalError {
    Decode(snapshot_core::DecodeError),
    FromDecoded(terminal_c::FromDecodedError),
}

/// The core allocator parameter (`decoder.alloc`): the caller's allocator as
/// the runtime handle (`lib.alloc.default(alloc_)`), or the Rust heap for NULL
/// (parity tail S7-4). The decoded terminal is built over it, so it becomes
/// that terminal's `gpa()`, and every later `next` applies history through it.
///
/// # Safety
/// `decoder` is a live decoder handle whose stored allocator, if non-null, is
/// the caller's still-live allocator (the C contract, N260).
unsafe fn decoder_gpa(decoder: *const DecoderWrapper) -> rustty_vt::alloc::Allocator {
    // SAFETY: the caller's contract.
    unsafe { rustty_vt::alloc::default((*decoder).alloc.as_ref()) }
}

/// Decode READY, create terminal-owned I/O, and construct the C terminal.
// fn decoderReadyTerminal(decoder: *DecoderWrapper) anyerror!ReadyTerminal {
/// # Safety
/// `decoder` must be a live decoder handle in the configuring state.
unsafe fn decoder_ready_terminal(
    decoder: *mut DecoderWrapper,
) -> std::result::Result<ReadyTerminal, ReadyTerminalError> {
    // Terminal I/O is intentionally allocated only when decoding begins.
    // const io = try terminal_c.Io.init(decoder.alloc);
    //   ^ N/A: see terminal.rs's `Io` PORT NOTE.

    // Decode READY while the fresh I/O implementation is still locally owned.
    // var decoded = decoder.decoder.ready(decoder.alloc, io.io(), .{
    //     .max_continuation_bytes = decoder.max_continuation_bytes,
    // }) catch |err| { io.deinit(decoder.alloc); return err; };
    // defer decoded.deinit(decoder.alloc);
    // SAFETY: `decoder` is live (caller contract); the transient `&mut` of the
    // decoder field dies with this statement.
    let options = snapshot_core::DecodeOptions {
        max_continuation_bytes: unsafe { (*decoder).max_continuation_bytes },
    };
    // SAFETY: `decoder` is live (caller contract).
    let gpa = unsafe { decoder_gpa(decoder) };
    let mut decoded = match unsafe { (*decoder).decoder.ready(gpa, &options) } {
        Ok(decoded) => decoded,
        Err(err) => return Err(ReadyTerminalError::Decode(err.into())),
    };

    // Copy small query metadata before `fromDecoded` consumes the core result.
    let metadata = Metadata {
        history_rows: decoded.history_rows,
    };

    // Move terminal state and I/O into their final C-owned allocation.
    // const terminal = try terminal_c.fromDecoded(decoder.alloc, io, &decoded, ...);
    // SAFETY: the stored allocator pointer, if non-null, is the caller's
    // still-live allocator (C contract; see the `alloc` field note).
    let result = unsafe {
        terminal_c::from_decoded(
            (*decoder).alloc.as_ref(),
            &mut decoded,
            if (*decoder).retain_continuation {
                (*decoder).max_continuation_bytes
            } else {
                0
            },
        )
    };
    // defer decoded.deinit(decoder.alloc); — runs on both outcomes.
    decoded.deinit(gpa);
    let terminal = result.map_err(ReadyTerminalError::FromDecoded)?;
    Ok(ReadyTerminal { terminal, metadata })
}

/// Map decoder and source-adapter failures to public C result codes.
// fn decoderMapError(decoder: *DecoderWrapper, err: anyerror) Result {
/// # Safety
/// `decoder` must be a live decoder handle.
unsafe fn decoder_map_error(decoder: *mut DecoderWrapper, err: ReadyTerminalError) -> Result {
    // Callback protocol failures are more specific than the core read error.
    // switch (decoder.source) { .callback => |adapter| { ... }, .buffer => {} }
    // PORT NOTE: the flags live in the shared raw-only cell and a buffer
    // source never sets them, so the unconditional read is arm-equivalent.
    // SAFETY: `decoder` is live (caller contract); raw reads of the cell.
    unsafe {
        let state = (*decoder).reader_state;
        if (*state).invalid_read {
            return Result::InvalidValue;
        }
        if (*state).callback_failed {
            return Result::IoError;
        }
    }

    // A successful zero-byte read is clean EOF by contract. Reaching it before
    // the required marker therefore means truncated snapshot data, not an
    // external I/O failure, and deliberately maps to invalid_value below.
    // return switch (err) {
    //     error.OutOfMemory => .out_of_memory,
    //     error.ContinuationLimitExceeded => .limit_exceeded,
    //     error.ContinuationDisabled, error.ContinuationUnavailable,
    //     error.DecoderNotReady, error.DecoderFailed => .invalid_value,
    //     error.InvalidContinuation => .invalid_value,
    //     else => .invalid_value,
    // };
    match err {
        ReadyTerminalError::Decode(snapshot_core::DecodeError::OutOfMemory)
        | ReadyTerminalError::FromDecoded(terminal_c::FromDecodedError::OutOfMemory) => {
            Result::OutOfMemory
        }
        ReadyTerminalError::Decode(snapshot_core::DecodeError::ContinuationLimitExceeded) => {
            Result::LimitExceeded
        }
        ReadyTerminalError::Decode(
            snapshot_core::DecodeError::DecoderNotReady | snapshot_core::DecodeError::DecoderFailed,
        ) => Result::InvalidValue,
        ReadyTerminalError::FromDecoded(terminal_c::FromDecodedError::InvalidContinuation) => {
            Result::InvalidValue
        }
        _ => Result::InvalidValue,
    }
}

/// Export the allocator-owned continuation required by snapshot encoding.
// fn continuationOwned(terminal: terminal_c.Terminal) struct { result: Result, bytes: []u8 } {
/// # Safety
/// `terminal`, if non-null, must be a live terminal handle.
unsafe fn continuation_owned(
    terminal: terminal_c::Terminal,
) -> (Result, rustty_vt::alloc::AVec<u8>) {
    // The terminal allocator also owns the temporary continuation copy.
    // const native = terminal_c.zigTerminal(terminal) orelse return .{ ... };
    // SAFETY: `terminal` validity is the caller's contract.
    let native = unsafe { terminal_c::zig_terminal(terminal) };
    if native.is_null() {
        return (
            Result::InvalidValue,
            rustty_vt::alloc::AVec::new_in(rustty_vt::alloc::VtAllocator::GLOBAL),
        );
    }
    // const alloc = native.gpa();
    // SAFETY: `native` is the live terminal; raw reads of the active
    // screen's allocator (`Terminal.gpa()`), no `&mut` retag.
    let alloc = unsafe { (*(*native).screens.active).alloc };

    // Snapshotting can prove an untracked ground stream needs no replay bytes;
    // only non-ground streams require tracking to have retained their prefix.
    // SAFETY: forwarded caller contract.
    match unsafe { terminal_c::continuation_alloc_io(terminal, alloc, true) } {
        Ok(bytes) => (Result::Success, bytes),
        Err(err) => (
            match err {
                terminal_c::ContinuationAllocError::InvalidValue => Result::InvalidValue,
                terminal_c::ContinuationAllocError::OutOfMemory => Result::OutOfMemory,
                terminal_c::ContinuationAllocError::ContinuationDisabled
                | terminal_c::ContinuationAllocError::ContinuationUnavailable => {
                    Result::InvalidValue
                }
            },
            rustty_vt::alloc::AVec::new_in(alloc),
        ),
    }
}

/// Convert exported bytes to the core snapshot continuation representation.
// fn continuationValue(bytes: []const u8) snapshot_core.Continuation {
fn continuation_value(bytes: &[u8]) -> snapshot_core::Continuation<'_> {
    if bytes.is_empty() {
        snapshot_core::Continuation::Ground
    } else {
        snapshot_core::Continuation::Bytes(std::borrow::Cow::Borrowed(bytes))
    }
}

/// Encode a terminal snapshot to a synchronous C writer callback.
// pub fn encode(terminal, writer) ... Result {
/// # Safety
/// `terminal`, if non-null, must be a live terminal handle with no concurrent
/// mutation; `writer`'s callback must not call terminal APIs with the same
/// terminal handle.
pub unsafe fn encode(terminal: terminal_c::Terminal, writer: io_c::Writer) -> Result {
    // Validate both handles and preflight the terminal continuation.
    // const native = terminal_c.zigTerminal(terminal) orelse return .invalid_value;
    // SAFETY: `terminal` validity is the caller's contract.
    let native = unsafe { terminal_c::zig_terminal(terminal) };
    if native.is_null() {
        return Result::InvalidValue;
    }
    if writer.write.is_none() {
        return Result::InvalidValue;
    }
    let (result, bytes) = unsafe { continuation_owned(terminal) };
    if result != Result::Success {
        return result;
    }
    // defer native.gpa().free(continuation.bytes); — the Vec frees on drop.

    // Stream the snapshot through the callback adapter, batching the
    // encoder's writes into few callback invocations.
    // var buffer: [io_c.WriterAdapter.recommended_buffer_len]u8 = undefined;
    // var adapter: io_c.WriterAdapter = .initBuffered(writer, &buffer);
    let mut adapter = io_c::WriterAdapter::init_buffered(writer);
    // SAFETY: shared read of the live native terminal for the encode duration
    // (the caller serializes all terminal access — header contract).
    if let Err(err) = snapshot_core::encode(
        // native.gpa()
        unsafe { (*(*native).screens.active).alloc },
        &mut adapter,
        unsafe { &*native },
        snapshot_core::EncodeOptions {
            continuation: continuation_value(&bytes),
        },
    ) {
        return map_encode_error(err, Some(&adapter));
    }
    if adapter.flush_buffered().is_err() {
        return map_encode_error(snapshot_core::EncodeError::WriteFailed, Some(&adapter));
    }
    Result::Success
}

/// Encode a terminal snapshot into a caller-provided fixed buffer.
// pub fn encode_buf(terminal, buf, buf_len, out_written_) ... Result {
/// # Safety
/// As `encode`; `buf`, if non-null, must point to `buf_len` writable bytes;
/// `out_written_`, if non-null, must be writable.
pub unsafe fn encode_buf(
    terminal: terminal_c::Terminal,
    buf: *mut u8,
    buf_len: usize,
    out_written_: *mut usize,
) -> Result {
    // Validate output storage and preflight the terminal continuation.
    // const out_written = out_written_ orelse return .invalid_value;
    if out_written_.is_null() {
        return Result::InvalidValue;
    }
    // SAFETY: `out_written_` is non-null and writable (caller contract).
    unsafe { *out_written_ = 0 };
    if buf.is_null() && buf_len != 0 {
        return Result::InvalidValue;
    }
    // SAFETY: `terminal` validity is the caller's contract.
    let native = unsafe { terminal_c::zig_terminal(terminal) };
    if native.is_null() {
        return Result::InvalidValue;
    }
    let (result, bytes) = unsafe { continuation_owned(terminal) };
    if result != Result::Success {
        return result;
    }

    // A null size query becomes a zero-capacity fixed writer and follows the
    // same WriteFailed counting path as every other undersized destination.
    // const destination: []u8 = if (buf) |ptr| ptr[0..buf_len] else &.{};
    let mut empty: [u8; 0] = [];
    let destination: &mut [u8] = if buf.is_null() {
        &mut empty
    } else {
        // SAFETY: `buf[0..buf_len]` is writable (caller contract).
        unsafe { core::slice::from_raw_parts_mut(buf, buf_len) }
    };
    // var writer: std.Io.Writer = .fixed(destination);
    let mut writer = vt_io::FixedWriter::new(destination);
    // SAFETY: shared read of the live native terminal (see `encode`).
    if let Err(err) = snapshot_core::encode(
        // native.gpa()
        unsafe { (*(*native).screens.active).alloc },
        &mut writer,
        unsafe { &*native },
        snapshot_core::EncodeOptions {
            continuation: continuation_value(&bytes),
        },
    ) {
        match err {
            snapshot_core::EncodeError::WriteFailed => {
                // The fixed writer may already contain a valid snapshot prefix.
                // Repeat only on this failure path to report the exact capacity.
                // var counter: std.Io.Writer.Discarding = .init(&.{});
                let mut counter = CountingVtWriter::default();
                // SAFETY: as above.
                if let Err(count_err) = snapshot_core::encode(
                    // native.gpa()
                    unsafe { (*(*native).screens.active).alloc },
                    &mut counter,
                    unsafe { &*native },
                    snapshot_core::EncodeOptions {
                        continuation: continuation_value(&bytes),
                    },
                ) {
                    return map_encode_error(count_err, None);
                }
                // out_written.* = std.math.cast(usize, counter.count) orelse
                //     return .limit_exceeded;
                //   ^ `count` is already a usize here; the cast cannot narrow.
                unsafe { *out_written_ = counter.count };
                return Result::OutOfSpace;
            }
            _ => return map_encode_error(err, None),
        }
    }
    // out_written.* = writer.end;
    unsafe { *out_written_ = writer.end };
    Result::Success
}

/// Encode a terminal snapshot into a newly allocated C-owned buffer.
// pub fn encode_alloc(terminal, alloc_, out_ptr_, out_len_) ... Result {
/// # Safety
/// As `encode`; `alloc_`, if provided, must reference a valid `CAllocator`;
/// `out_ptr_`/`out_len_`, if non-null, must be writable.
pub unsafe fn encode_alloc(
    terminal: terminal_c::Terminal,
    alloc_: Option<&CAllocator>,
    out_ptr_: *mut *mut u8,
    out_len_: *mut usize,
) -> Result {
    // Initialize outputs before validating terminal state or allocating bytes.
    if out_ptr_.is_null() || out_len_.is_null() {
        return Result::InvalidValue;
    }
    // SAFETY: both outputs are non-null and writable (caller contract).
    unsafe {
        *out_ptr_ = std::ptr::null_mut();
        *out_len_ = 0;
    }

    // Preflight continuation state before building an allocating writer.
    // SAFETY: `terminal` validity is the caller's contract.
    let native = unsafe { terminal_c::zig_terminal(terminal) };
    if native.is_null() {
        return Result::InvalidValue;
    }
    let (result, bytes) = unsafe { continuation_owned(terminal) };
    if result != Result::Success {
        return result;
    }

    // Encode once, then transfer ownership of the writer's completed slice.
    // const alloc = lib.alloc.default(alloc_);
    // SAFETY: a non-NULL `alloc_` is a valid `RusttyAllocator` for the call.
    let alloc = unsafe { rustty_vt::alloc::default(alloc_) };
    // var writer: std.Io.Writer.Allocating = .init(alloc); defer writer.deinit();
    // PORT NOTE (parity tail S7-8): the writer grows through the caller's
    // allocator (Zig's `growCapacity`, `remap` first), and a failed growth is
    // `error.WriteFailed` (OUT_OF_MEMORY through `map_encode_error`).
    let mut writer = rustty_vt::alloc::AllocatingWriter::init(alloc);
    // SAFETY: shared read of the live native terminal (see `encode`).
    if let Err(err) = snapshot_core::encode(
        // native.gpa()
        unsafe { (*(*native).screens.active).alloc },
        &mut writer,
        unsafe { &*native },
        snapshot_core::EncodeOptions {
            continuation: continuation_value(&bytes),
        },
    ) {
        return map_encode_error(err, None);
    }
    // const bytes = writer.toOwnedSlice() catch return .out_of_memory;
    // PORT NOTE (parity tail S7-8): the writer's own buffer, shrunk to the
    // written length through the caller's allocator (`toOwnedSlice`).
    let (dst, len) = match writer.to_owned_slice() {
        Ok(buf) => crate::allocator::into_c_bytes(buf),
        Err(_) => return Result::OutOfMemory,
    };
    // out_ptr.* = bytes.ptr; out_len.* = bytes.len;
    unsafe {
        *out_ptr_ = dst;
        *out_len_ = len;
    }
    Result::Success
}

/// `std.Io.Writer.Discarding` over the snapshot writer trait (counts bytes,
/// never fails). The `writer.rs` `CountingWriter` serves the `std::io::Write`
/// tiers; this one serves `rustty_vt::snapshot::io::Writer`.
#[derive(Default)]
struct CountingVtWriter {
    count: usize,
}

impl vt_io::Writer for CountingVtWriter {
    fn write_all(&mut self, bytes: &[u8]) -> std::result::Result<(), vt_io::WriteFailed> {
        self.count += bytes.len();
        Ok(())
    }
}

/// Map core encoder and callback-adapter failures to public result codes.
// fn mapEncodeError(err: anyerror, adapter: ?*const io_c.WriterAdapter) Result {
fn map_encode_error(
    err: snapshot_core::EncodeError,
    adapter: Option<&io_c::WriterAdapter>,
) -> Result {
    // Callback validation happens before encoding, so invalid_write can only
    // mean output accounting overflow; callback_failed is external I/O.
    if let Some(value) = adapter {
        if value.invalid_write {
            return Result::LimitExceeded;
        }
        if value.callback_failed {
            return Result::IoError;
        }
    }

    // Fixed and allocating writers surface only core allocation or size errors.
    match err {
        snapshot_core::EncodeError::OutOfMemory | snapshot_core::EncodeError::WriteFailed => {
            Result::OutOfMemory
        }
        snapshot_core::EncodeError::Overflow
        | snapshot_core::EncodeError::PayloadTooLarge
        | snapshot_core::EncodeError::PageCountOverflow
        | snapshot_core::EncodeError::ScrollbackLimitOverflow
        | snapshot_core::EncodeError::StringTooLong => Result::LimitExceeded,
        _ => Result::InvalidValue,
    }
}

/// Enable continuation tracking for tests that encode C terminal snapshots.
// fn testEnableContinuation(terminal: terminal_c.Terminal) !void {
#[cfg(test)]
fn test_enable_continuation(terminal: terminal_c::Terminal) {
    let limit: usize = default_max_continuation_bytes();
    assert_eq!(Result::Success, unsafe {
        terminal_c::set(
            terminal,
            terminal_c::Option_::ContinuationMaxBytes,
            &limit as *const usize as *const c_void,
        )
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal::{Option_, TerminalData};
    use crate::test_util::with_test_allocator;
    use rustty_lib::allocator::{Alignment, ZigSlice};

    // Shared scaffold (the terminal.rs test conventions): `new_t` returns the
    // `new` Result UNASSERTED; `vtw` writes a byte slice; `plain_string`
    // renders the terminal text (`t.?.terminal.plainString(testing.allocator)`).
    fn new_t(alloc: &CAllocator, cols: u16, rows: u16) -> (Result, terminal_c::Terminal) {
        let mut t: terminal_c::Terminal = std::ptr::null_mut();
        let r = unsafe { terminal_c::new(alloc as *const CAllocator, &mut t, cols, rows) };
        (r, t)
    }

    fn vtw(t: terminal_c::Terminal, bytes: &[u8]) {
        unsafe { terminal_c::vt_write(t, bytes.as_ptr(), bytes.len()) };
    }

    fn plain_string(t: terminal_c::Terminal) -> Vec<u8> {
        // SAFETY: `t` is a live terminal from `new_t`/the decoder.
        let zt = unsafe { &mut *terminal_c::zig_terminal(t) };
        zt.plain_string(rustty_vt::alloc::Global).unwrap()
    }

    // `lib.alloc.default(&lib.alloc.test_allocator).free(encoded)`
    fn free_alloc(alloc: &CAllocator, ptr: *mut u8, len: usize) {
        rustty_lib::allocator::default(Some(alloc)).free(
            ZigSlice::new(ptr, len),
            Alignment::from_byte_units(1),
            0,
        );
    }

    fn set_usize(t: terminal_c::Terminal, option: Option_, value: usize) -> Result {
        unsafe { terminal_c::set(t, option, &value as *const usize as *const c_void) }
    }

    fn get_usize(t: terminal_c::Terminal, data: TerminalData, out: &mut usize) -> Result {
        unsafe { terminal_c::get(t, data, out as *mut usize as *mut c_void) }
    }

    // test "decoder option and empty source"
    #[test]
    fn decoder_option_and_empty_source() {
        with_test_allocator(|alloc| {
            let mut decoder: Decoder = std::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                decoder_new_buf(Some(alloc), &mut decoder, std::ptr::null(), 0)
            });

            let mut limit: usize = 0;
            assert_eq!(Result::Success, unsafe {
                decoder_get(
                    decoder,
                    DecoderData::MaxContinuationBytes,
                    &mut limit as *mut usize as *mut c_void,
                )
            });
            assert_eq!(default_max_continuation_bytes(), limit);

            limit = 1234;
            assert_eq!(Result::Success, unsafe {
                decoder_set(
                    decoder,
                    DecoderOption::MaxContinuationBytes,
                    &limit as *const usize as *const c_void,
                )
            });
            limit = 0;
            assert_eq!(Result::Success, unsafe {
                decoder_get(
                    decoder,
                    DecoderData::MaxContinuationBytes,
                    &mut limit as *mut usize as *mut c_void,
                )
            });
            assert_eq!(1234usize, limit);

            let mut retain = true;
            assert_eq!(Result::Success, unsafe {
                decoder_get(
                    decoder,
                    DecoderData::RetainContinuation,
                    &mut retain as *mut bool as *mut c_void,
                )
            });
            assert!(!retain);
            retain = true;
            assert_eq!(Result::Success, unsafe {
                decoder_set(
                    decoder,
                    DecoderOption::RetainContinuation,
                    &retain as *const bool as *const c_void,
                )
            });
            retain = false;
            assert_eq!(Result::Success, unsafe {
                decoder_get(
                    decoder,
                    DecoderData::RetainContinuation,
                    &mut retain as *mut bool as *mut c_void,
                )
            });
            assert!(retain);

            let mut written: usize = 99;
            assert_eq!(Result::InvalidValue, unsafe {
                decoder_get_multi(
                    decoder,
                    1,
                    std::ptr::null(),
                    std::ptr::null_mut(),
                    &mut written,
                )
            });
            assert_eq!(0usize, written);

            let mut terminal: terminal_c::Terminal = std::ptr::null_mut();
            assert_eq!(Result::InvalidValue, unsafe {
                decoder_decode(decoder, &mut terminal)
            });
            assert!(terminal.is_null());
            unsafe { decoder_free(decoder) };
        });
    }

    // rustty-only (R13, UPSTREAM-DEFECTS.md UD64): the C ABI is the snapshot
    // decoder's only production caller, so an embedder decoding snapshot bytes
    // from a file, a peer or the network is the whole reachability of this
    // defect. A single PAGE header field is enough: before the fix this
    // aborted the process at `assert(layout.total_size <= max_page_size)`,
    // inside an `extern "C"` entry point where the panic cannot unwind. It is
    // now an ordinary malformed-input result.
    #[test]
    fn decoder_rejects_a_page_capacity_that_would_abort_the_library() {
        with_test_allocator(|alloc| {
            let (new_result, source) = new_t(alloc, 20, 4);
            assert_eq!(Result::Success, new_result);
            test_enable_continuation(source);
            vtw(source, b"hello");

            let mut encoded_ptr: *mut u8 = std::ptr::null_mut();
            let mut encoded_len: usize = 0;
            assert_eq!(Result::Success, unsafe {
                encode_alloc(source, Some(alloc), &mut encoded_ptr, &mut encoded_len)
            });
            // SAFETY: the allocation returned above; copied out and freed here.
            let mut bytes: Vec<u8> =
                unsafe { core::slice::from_raw_parts(encoded_ptr, encoded_len) }.to_vec();
            free_alloc(alloc, encoded_ptr, encoded_len);

            // A control first: these bytes decode cleanly as they stand, so
            // the rejection below is caused by the patch and nothing else.
            {
                let mut decoder: Decoder = std::ptr::null_mut();
                assert_eq!(Result::Success, unsafe {
                    decoder_new_buf(Some(alloc), &mut decoder, bytes.as_ptr(), bytes.len())
                });
                let mut restored: terminal_c::Terminal = std::ptr::null_mut();
                assert_eq!(Result::Success, unsafe {
                    decoder_decode(decoder, &mut restored)
                });
                unsafe {
                    terminal_c::free(restored);
                    decoder_free(decoder);
                }
            }

            // Walk the record framing (10-byte envelope, then 10-byte record
            // headers) and patch the first PAGE record's
            // `string_capacity_bytes`, at bytes 16..20 of its payload.
            let mut offset: usize = 10;
            loop {
                assert!(offset + 10 <= bytes.len(), "no PAGE record found");
                let tag = u16::from_le_bytes(bytes[offset..offset + 2].try_into().unwrap());
                let payload_len =
                    u32::from_le_bytes(bytes[offset + 2..offset + 6].try_into().unwrap()) as usize;
                let payload = offset + 10;
                if tag == 3 {
                    bytes[payload + 16..payload + 20].copy_from_slice(&u32::MAX.to_le_bytes());
                    break;
                }
                offset = payload + payload_len;
            }

            // The record's CRC is now wrong too, which does not matter and is
            // the point: the capacity is consumed before the payload checksum
            // is verified, so a bad CRC is no barrier to reaching it.
            let mut decoder: Decoder = std::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                decoder_new_buf(Some(alloc), &mut decoder, bytes.as_ptr(), bytes.len())
            });
            let mut restored: terminal_c::Terminal = std::ptr::null_mut();
            assert_eq!(Result::InvalidValue, unsafe {
                decoder_decode(decoder, &mut restored)
            });
            assert!(restored.is_null());

            unsafe {
                decoder_free(decoder);
                terminal_c::free(source);
            }
        });
    }

    // rustty-only (R13, UPSTREAM-DEFECTS.md UD66): the TERMINAL string-length
    // bound on the `decoder_new` callback route. There the record payload
    // streams through the decoder's hashing buffer, so the bytes the record
    // can still supply are what the payload limit permits PLUS what is
    // already buffered. A bound that forgot the buffered part rejects an
    // honest snapshot whenever a string's first bytes arrived together with
    // its length, which depends on how the callback happens to chunk its
    // reads. So the same honest snapshot (a 300-byte pwd and a title) is
    // decoded with the callback handing over 1 to 8 bytes at a time, as a
    // pipe or socket reader might, and every chunking must restore it.
    #[test]
    fn callback_decoder_restores_strings_whatever_the_read_size() {
        use rustty_lib::String as LibString;

        struct ReadContext<'a> {
            source: &'a [u8],
            offset: usize,
            chunk: usize,
        }
        extern "C" fn read(
            userdata: *mut c_void,
            destination: *mut u8,
            capacity: usize,
            out_read: *mut usize,
        ) -> bool {
            let ctx = unsafe { &mut *(userdata as *mut ReadContext<'_>) };
            let remaining = &ctx.source[ctx.offset..];
            let len = remaining.len().min(capacity).min(ctx.chunk);
            unsafe {
                std::ptr::copy_nonoverlapping(remaining.as_ptr(), destination, len);
                *out_read = len;
            }
            ctx.offset += len;
            true
        }

        /// Read one string datum of `t` as an owned byte vector.
        fn string_of(t: terminal_c::Terminal, key: TerminalData) -> Vec<u8> {
            let mut value = LibString {
                ptr: std::ptr::null(),
                len: 0,
            };
            assert_eq!(Result::Success, unsafe {
                terminal_c::get(t, key, &mut value as *mut _ as *mut c_void)
            });
            if value.len == 0 {
                return Vec::new();
            }
            unsafe { core::slice::from_raw_parts(value.ptr, value.len) }.to_vec()
        }

        with_test_allocator(|alloc| {
            let (new_result, source) = new_t(alloc, 20, 4);
            assert_eq!(Result::Success, new_result);
            let pwd = [b'p'; 300];
            let title = b"snapshot title";
            for (option, bytes) in [
                (Option_::Pwd, pwd.as_slice()),
                (Option_::Title, title.as_slice()),
            ] {
                let value = LibString {
                    ptr: bytes.as_ptr(),
                    len: bytes.len(),
                };
                assert_eq!(Result::Success, unsafe {
                    terminal_c::set(source, option, &value as *const _ as *const c_void)
                });
            }

            let mut encoded_ptr: *mut u8 = std::ptr::null_mut();
            let mut encoded_len: usize = 0;
            assert_eq!(Result::Success, unsafe {
                encode_alloc(source, Some(alloc), &mut encoded_ptr, &mut encoded_len)
            });
            // SAFETY: the allocation returned above; copied out and freed here.
            let bytes: Vec<u8> =
                unsafe { core::slice::from_raw_parts(encoded_ptr, encoded_len) }.to_vec();
            free_alloc(alloc, encoded_ptr, encoded_len);

            for chunk in 1..=8 {
                let mut read_context = ReadContext {
                    source: &bytes,
                    offset: 0,
                    chunk,
                };
                let mut decoder: Decoder = std::ptr::null_mut();
                assert_eq!(Result::Success, unsafe {
                    decoder_new(
                        Some(alloc),
                        &mut decoder,
                        io_c::Reader {
                            read: Some(read),
                            userdata: &mut read_context as *mut ReadContext<'_> as *mut c_void,
                        },
                    )
                });
                let mut restored: terminal_c::Terminal = std::ptr::null_mut();
                assert_eq!(
                    Result::Success,
                    unsafe { decoder_decode(decoder, &mut restored) },
                    "{chunk}-byte reads",
                );
                assert_eq!(bytes.len(), read_context.offset, "{chunk}-byte reads");
                assert_eq!(
                    pwd.as_slice(),
                    string_of(restored, TerminalData::Pwd),
                    "{chunk}-byte reads",
                );
                assert_eq!(
                    title.as_slice(),
                    string_of(restored, TerminalData::Title),
                    "{chunk}-byte reads",
                );
                unsafe {
                    terminal_c::free(restored);
                    decoder_free(decoder);
                }
            }

            unsafe { terminal_c::free(source) };
        });
    }

    // test "snapshot C API full round trip restores continuation"
    #[test]
    fn snapshot_c_api_full_round_trip_restores_continuation() {
        with_test_allocator(|alloc| {
            let (new_result, source) = new_t(alloc, 20, 4);
            assert_eq!(Result::Success, new_result);
            test_enable_continuation(source);
            vtw(source, b"hello\x1b[31");

            let mut encoded_ptr: *mut u8 = std::ptr::null_mut();
            let mut encoded_len: usize = 0;
            assert_eq!(Result::Success, unsafe {
                encode_alloc(source, Some(alloc), &mut encoded_ptr, &mut encoded_len)
            });
            // SAFETY: the allocation returned above, freed at the end.
            let encoded: &[u8] = unsafe { core::slice::from_raw_parts(encoded_ptr, encoded_len) };

            let mut decoder: Decoder = std::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                decoder_new_buf(Some(alloc), &mut decoder, encoded.as_ptr(), encoded.len())
            });

            let mut restored: terminal_c::Terminal = std::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                decoder_decode(decoder, &mut restored)
            });

            let source_text = plain_string(source);
            let restored_text = plain_string(restored);
            assert_eq!(source_text, restored_text);

            let mut restored_limit: usize = usize::MAX;
            assert_eq!(
                Result::Success,
                get_usize(
                    restored,
                    TerminalData::ContinuationMaxBytes,
                    &mut restored_limit
                )
            );
            assert_eq!(0usize, restored_limit);

            // Parser state is restored, but its tracking policy is not. The terminal
            // cannot be snapshotted again until the restored sequence reaches ground.
            let mut restored_required: usize = 0;
            assert_eq!(Result::InvalidValue, unsafe {
                encode_buf(restored, std::ptr::null_mut(), 0, &mut restored_required)
            });
            vtw(restored, b"m");
            assert_eq!(Result::OutOfSpace, unsafe {
                encode_buf(restored, std::ptr::null_mut(), 0, &mut restored_required)
            });
            assert!(restored_required > 0);

            let mut offset: usize = 0;
            assert_eq!(Result::Success, unsafe {
                decoder_get(
                    decoder,
                    DecoderData::SourceOffset,
                    &mut offset as *mut usize as *mut c_void,
                )
            });
            assert_eq!(encoded.len(), offset);
            assert_eq!(Result::NoValue, unsafe { decoder_next(decoder) });

            let mut retained_decoder: Decoder = std::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                decoder_new_buf(
                    Some(alloc),
                    &mut retained_decoder,
                    encoded.as_ptr(),
                    encoded.len(),
                )
            });
            let retained_limit: usize = 1024;
            assert_eq!(Result::Success, unsafe {
                decoder_set(
                    retained_decoder,
                    DecoderOption::MaxContinuationBytes,
                    &retained_limit as *const usize as *const c_void,
                )
            });
            let retain = true;
            assert_eq!(Result::Success, unsafe {
                decoder_set(
                    retained_decoder,
                    DecoderOption::RetainContinuation,
                    &retain as *const bool as *const c_void,
                )
            });

            // The returned tracker owns the exact decoded continuation, so the
            // decoder can be freed before callers export those terminal-owned bytes.
            let mut retained: terminal_c::Terminal = std::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                decoder_decode(retained_decoder, &mut retained)
            });
            unsafe { decoder_free(retained_decoder) };

            let mut retained_terminal_limit: usize = 0;
            assert_eq!(
                Result::Success,
                get_usize(
                    retained,
                    TerminalData::ContinuationMaxBytes,
                    &mut retained_terminal_limit,
                )
            );
            assert_eq!(retained_limit, retained_terminal_limit);
            let mut continuation_buf_ = [0u8; 16];
            let mut continuation_len: usize = 0;
            assert_eq!(Result::Success, unsafe {
                terminal_c::continuation_buf(
                    retained,
                    continuation_buf_.as_mut_ptr(),
                    continuation_buf_.len(),
                    &mut continuation_len,
                )
            });
            assert_eq!(b"\x1b[31", &continuation_buf_[..continuation_len]);

            // Disabling tracking releases the exported prefix without changing the
            // unfinished parser state. The final byte still completes the SGR.
            assert_eq!(
                Result::Success,
                set_usize(retained, Option_::ContinuationMaxBytes, 0)
            );
            assert_eq!(Result::InvalidValue, unsafe {
                terminal_c::continuation_buf(
                    retained,
                    continuation_buf_.as_mut_ptr(),
                    continuation_buf_.len(),
                    &mut continuation_len,
                )
            });
            vtw(retained, b"m");
            let mut retained_ground = false;
            assert_eq!(Result::Success, unsafe {
                terminal_c::get(
                    retained,
                    TerminalData::VtGround,
                    &mut retained_ground as *mut bool as *mut c_void,
                )
            });
            assert!(retained_ground);

            let mut limited: Decoder = std::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                decoder_new_buf(Some(alloc), &mut limited, encoded.as_ptr(), encoded.len())
            });
            let limit: usize = 3;
            assert_eq!(Result::Success, unsafe {
                decoder_set(
                    limited,
                    DecoderOption::MaxContinuationBytes,
                    &limit as *const usize as *const c_void,
                )
            });
            let mut rejected: terminal_c::Terminal = std::ptr::null_mut();
            assert_eq!(Result::LimitExceeded, unsafe {
                decoder_decode(limited, &mut rejected)
            });
            assert!(rejected.is_null());
            assert_eq!(Result::InvalidValue, unsafe {
                decoder_set(
                    limited,
                    DecoderOption::MaxContinuationBytes,
                    &limit as *const usize as *const c_void,
                )
            });
            let mut failed_offset: usize = 0;
            assert_eq!(Result::NoValue, unsafe {
                decoder_get(
                    limited,
                    DecoderData::SourceOffset,
                    &mut failed_offset as *mut usize as *mut c_void,
                )
            });
            unsafe { decoder_free(limited) };

            assert_eq!(
                Result::Success,
                set_usize(source, Option_::ContinuationMaxBytes, 0)
            );
            let mut required: usize = 0;
            assert_eq!(Result::InvalidValue, unsafe {
                encode_buf(source, std::ptr::null_mut(), 0, &mut required)
            });

            unsafe {
                terminal_c::free(retained);
                terminal_c::free(restored);
                decoder_free(decoder);
            }
            free_alloc(alloc, encoded_ptr, encoded_len);
            unsafe { terminal_c::free(source) };
        });
    }

    // test "snapshot decoder retains every supported continuation class"
    #[test]
    fn snapshot_decoder_retains_every_supported_continuation_class() {
        struct Case {
            name: &'static str,
            prefix: &'static [u8],
            suffix: &'static [u8],
        }
        let cases = [
            Case {
                name: "ground",
                prefix: b"",
                suffix: b"X",
            },
            Case {
                name: "CSI",
                prefix: b"\x1b[31",
                suffix: b"mX",
            },
            Case {
                name: "OSC",
                prefix: b"\x1b]2;title",
                suffix: b"\x07",
            },
            Case {
                name: "DCS",
                prefix: b"\x1bP$qm",
                suffix: b"\x1b\\",
            },
            Case {
                name: "APC",
                prefix: b"\x1b_Ga=q;payload",
                suffix: b"\x1b\\",
            },
            Case {
                name: "UTF-8",
                prefix: &[0xF0, 0x9F],
                suffix: &[0x98, 0x84],
            },
        ];
        let limit: usize = 4096;
        let retain = true;

        with_test_allocator(|alloc| {
            for case in &cases {
                // errdefer std.log.err("retained continuation case failed: {s}", ...);
                // (rustty: the assertion messages carry the case name.)
                let ctx = case.name;

                let (new_result, source) = new_t(alloc, 20, 4);
                assert_eq!(Result::Success, new_result, "case {ctx}");
                assert_eq!(
                    Result::Success,
                    set_usize(source, Option_::ContinuationMaxBytes, limit),
                    "case {ctx}"
                );
                vtw(source, case.prefix);

                let mut snapshot_ptr: *mut u8 = std::ptr::null_mut();
                let mut snapshot_len: usize = 0;
                assert_eq!(
                    Result::Success,
                    unsafe {
                        encode_alloc(source, Some(alloc), &mut snapshot_ptr, &mut snapshot_len)
                    },
                    "case {ctx}"
                );
                let snapshot: &[u8] =
                    unsafe { core::slice::from_raw_parts(snapshot_ptr, snapshot_len) };

                let mut decoder: Decoder = std::ptr::null_mut();
                assert_eq!(
                    Result::Success,
                    unsafe {
                        decoder_new_buf(
                            Some(alloc),
                            &mut decoder,
                            snapshot.as_ptr(),
                            snapshot.len(),
                        )
                    },
                    "case {ctx}"
                );
                assert_eq!(
                    Result::Success,
                    unsafe {
                        decoder_set(
                            decoder,
                            DecoderOption::MaxContinuationBytes,
                            &limit as *const usize as *const c_void,
                        )
                    },
                    "case {ctx}"
                );
                assert_eq!(
                    Result::Success,
                    unsafe {
                        decoder_set(
                            decoder,
                            DecoderOption::RetainContinuation,
                            &retain as *const bool as *const c_void,
                        )
                    },
                    "case {ctx}"
                );

                let mut restored: terminal_c::Terminal = std::ptr::null_mut();
                assert_eq!(
                    Result::Success,
                    unsafe { decoder_decode(decoder, &mut restored) },
                    "case {ctx}"
                );
                unsafe { decoder_free(decoder) };

                let mut restored_limit: usize = 0;
                assert_eq!(
                    Result::Success,
                    get_usize(
                        restored,
                        TerminalData::ContinuationMaxBytes,
                        &mut restored_limit
                    ),
                    "case {ctx}"
                );
                assert_eq!(limit, restored_limit, "case {ctx}");

                let mut continuation = [0u8; 4096];
                let mut continuation_len: usize = 0;
                assert_eq!(
                    Result::Success,
                    unsafe {
                        terminal_c::continuation_buf(
                            restored,
                            continuation.as_mut_ptr(),
                            continuation.len(),
                            &mut continuation_len,
                        )
                    },
                    "case {ctx}"
                );
                assert_eq!(case.prefix, &continuation[..continuation_len], "case {ctx}");

                // Releasing the retained bytes must not reset the parser. Both the
                // source and replica should reach the same fully serializable state
                // after consuming the suffix that followed the snapshot cut.
                assert_eq!(
                    Result::Success,
                    set_usize(restored, Option_::ContinuationMaxBytes, 0),
                    "case {ctx}"
                );
                assert_eq!(
                    Result::InvalidValue,
                    unsafe {
                        terminal_c::continuation_buf(
                            restored,
                            continuation.as_mut_ptr(),
                            continuation.len(),
                            &mut continuation_len,
                        )
                    },
                    "case {ctx}"
                );
                vtw(source, case.suffix);
                vtw(restored, case.suffix);

                let mut source_ground = false;
                let mut restored_ground = false;
                assert_eq!(
                    Result::Success,
                    unsafe {
                        terminal_c::get(
                            source,
                            TerminalData::VtGround,
                            &mut source_ground as *mut bool as *mut c_void,
                        )
                    },
                    "case {ctx}"
                );
                assert_eq!(
                    Result::Success,
                    unsafe {
                        terminal_c::get(
                            restored,
                            TerminalData::VtGround,
                            &mut restored_ground as *mut bool as *mut c_void,
                        )
                    },
                    "case {ctx}"
                );
                assert!(source_ground, "case {ctx}");
                assert!(restored_ground, "case {ctx}");

                let mut source_final_ptr: *mut u8 = std::ptr::null_mut();
                let mut source_final_len: usize = 0;
                assert_eq!(
                    Result::Success,
                    unsafe {
                        encode_alloc(
                            source,
                            Some(alloc),
                            &mut source_final_ptr,
                            &mut source_final_len,
                        )
                    },
                    "case {ctx}"
                );
                let source_final: &[u8] =
                    unsafe { core::slice::from_raw_parts(source_final_ptr, source_final_len) };

                let mut restored_final_ptr: *mut u8 = std::ptr::null_mut();
                let mut restored_final_len: usize = 0;
                assert_eq!(
                    Result::Success,
                    unsafe {
                        encode_alloc(
                            restored,
                            Some(alloc),
                            &mut restored_final_ptr,
                            &mut restored_final_len,
                        )
                    },
                    "case {ctx}"
                );
                let restored_final: &[u8] =
                    unsafe { core::slice::from_raw_parts(restored_final_ptr, restored_final_len) };
                assert_eq!(source_final, restored_final, "case {ctx}");

                free_alloc(alloc, restored_final_ptr, restored_final_len);
                free_alloc(alloc, source_final_ptr, source_final_len);
                unsafe { terminal_c::free(restored) };
                free_alloc(alloc, snapshot_ptr, snapshot_len);
                unsafe { terminal_c::free(source) };
            }
        });
    }

    // test "snapshot encoding accepts an untracked ground terminal"
    #[test]
    fn snapshot_encoding_accepts_an_untracked_ground_terminal() {
        with_test_allocator(|alloc| {
            let (new_result, source) = new_t(alloc, 10, 3);
            assert_eq!(Result::Success, new_result);

            let mut limit: usize = usize::MAX;
            assert_eq!(
                Result::Success,
                get_usize(source, TerminalData::ContinuationMaxBytes, &mut limit)
            );
            assert_eq!(0usize, limit);

            vtw(source, b"ground");
            let mut required: usize = 0;
            assert_eq!(Result::OutOfSpace, unsafe {
                encode_buf(source, std::ptr::null_mut(), 0, &mut required)
            });
            assert!(required > 0);

            // Once untracked input leaves ground, the missing prefix is unknowable.
            vtw(source, b"\x1b[31");
            assert_eq!(Result::InvalidValue, unsafe {
                encode_buf(source, std::ptr::null_mut(), 0, &mut required)
            });
            unsafe { terminal_c::free(source) };
        });
    }

    // test "snapshot callbacks stop at FINISH and map I/O failures"
    #[test]
    fn snapshot_callbacks_stop_at_finish_and_map_io_failures() {
        with_test_allocator(|alloc| {
            let (new_result, source) = new_t(alloc, 10, 3);
            assert_eq!(Result::Success, new_result);
            test_enable_continuation(source);
            vtw(source, b"callback");

            let mut required: usize = 0;
            assert_eq!(Result::OutOfSpace, unsafe {
                encode_buf(source, std::ptr::null_mut(), 0, &mut required)
            });
            let mut buffer_encoded = vec![0u8; required];
            let mut buffer_written: usize = 0;
            assert_eq!(Result::Success, unsafe {
                encode_buf(
                    source,
                    buffer_encoded.as_mut_ptr(),
                    buffer_encoded.len(),
                    &mut buffer_written,
                )
            });
            assert_eq!(required, buffer_written);

            // A non-null undersized buffer takes the fixed-first path. It may contain
            // the prefix written before capacity exhaustion while still reporting the
            // exact complete size.
            let mut short = [0xAAu8; 10];
            let mut short_required: usize = 0;
            assert_eq!(Result::OutOfSpace, unsafe {
                encode_buf(source, short.as_mut_ptr(), short.len(), &mut short_required)
            });
            assert_eq!(required, short_required);
            assert_eq!(b"GHOSTSNP", &short[..8]);

            let mut encoded = vec![0u8; required];

            struct WriteContext<'a> {
                destination: &'a mut [u8],
                offset: usize,
            }
            extern "C" fn write(userdata: *mut c_void, data: *const u8, len: usize) -> bool {
                let ctx = unsafe { &mut *(userdata as *mut WriteContext<'_>) };
                if len > ctx.destination.len() - ctx.offset {
                    return false;
                }
                unsafe {
                    std::ptr::copy_nonoverlapping(
                        data,
                        ctx.destination[ctx.offset..].as_mut_ptr(),
                        len,
                    )
                };
                ctx.offset += len;
                true
            }
            let mut write_context = WriteContext {
                destination: &mut encoded,
                offset: 0,
            };
            assert_eq!(Result::Success, unsafe {
                encode(
                    source,
                    io_c::Writer {
                        write: Some(write),
                        userdata: &mut write_context as *mut WriteContext<'_> as *mut c_void,
                    },
                )
            });
            assert_eq!(required, write_context.offset);
            assert_eq!(buffer_encoded, encoded);

            // var failing = testing.FailingAllocator.init(testing.allocator, .{
            //     .fail_index = 0,
            // });
            let failing = rustty_vt::alloc::testing::FailingAllocator::over_heap(
                rustty_vt::alloc::testing::Config {
                    fail_index: 0,
                    ..rustty_vt::alloc::testing::Config::default()
                },
            );
            // const failing_zig = failing.allocator();
            // const failing_c: CAllocator = .fromZig(&failing_zig);
            // (rustty: the port's FailingAllocator is built on the C ABI, so
            // `allocator()` is the C allocator itself; parity tail S7-8.)
            let failing_c = failing.allocator();
            let mut failed_ptr: *mut u8 = std::ptr::null_mut();
            let mut failed_len: usize = 99;
            assert_eq!(Result::OutOfMemory, unsafe {
                encode_alloc(source, Some(&failing_c), &mut failed_ptr, &mut failed_len)
            });
            assert!(failed_ptr.is_null());
            assert_eq!(0usize, failed_len);

            extern "C" fn fail_write(_: *mut c_void, _: *const u8, _: usize) -> bool {
                false
            }
            assert_eq!(Result::IoError, unsafe {
                encode(
                    source,
                    io_c::Writer {
                        write: Some(fail_write),
                        userdata: std::ptr::null_mut(),
                    },
                )
            });

            let mut combined = encoded.clone();
            combined.extend_from_slice(b"tail");

            struct ReadContext<'a> {
                source: &'a [u8],
                offset: usize,
            }
            extern "C" fn read(
                userdata: *mut c_void,
                destination: *mut u8,
                capacity: usize,
                out_read: *mut usize,
            ) -> bool {
                let ctx = unsafe { &mut *(userdata as *mut ReadContext<'_>) };
                let remaining = &ctx.source[ctx.offset..];
                let len = remaining.len().min(capacity).min(3);
                unsafe {
                    std::ptr::copy_nonoverlapping(remaining.as_ptr(), destination, len);
                    *out_read = len;
                }
                ctx.offset += len;
                true
            }
            let mut read_context = ReadContext {
                source: &combined,
                offset: 0,
            };
            let mut decoder: Decoder = std::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                decoder_new(
                    Some(alloc),
                    &mut decoder,
                    io_c::Reader {
                        read: Some(read),
                        userdata: &mut read_context as *mut ReadContext<'_> as *mut c_void,
                    },
                )
            });
            let mut restored: terminal_c::Terminal = std::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                decoder_decode(decoder, &mut restored)
            });
            assert_eq!(encoded.len(), read_context.offset);
            assert_eq!(b"tail", &read_context.source[read_context.offset..]);

            extern "C" fn fail_read(_: *mut c_void, _: *mut u8, _: usize, _: *mut usize) -> bool {
                false
            }
            let mut failed_decoder: Decoder = std::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                decoder_new(
                    Some(alloc),
                    &mut failed_decoder,
                    io_c::Reader {
                        read: Some(fail_read),
                        userdata: std::ptr::null_mut(),
                    },
                )
            });
            let mut failed_terminal: terminal_c::Terminal = std::ptr::null_mut();
            assert_eq!(Result::IoError, unsafe {
                decoder_decode(failed_decoder, &mut failed_terminal)
            });
            unsafe { decoder_free(failed_decoder) };

            extern "C" fn eof_read(
                _: *mut c_void,
                _: *mut u8,
                _: usize,
                out_read: *mut usize,
            ) -> bool {
                unsafe { *out_read = 0 };
                true
            }
            let mut eof_decoder: Decoder = std::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                decoder_new(
                    Some(alloc),
                    &mut eof_decoder,
                    io_c::Reader {
                        read: Some(eof_read),
                        userdata: std::ptr::null_mut(),
                    },
                )
            });
            // Successful EOF before READY/FINISH is truncated snapshot data. It is
            // intentionally INVALID_VALUE; only a false callback is IO_ERROR.
            assert_eq!(Result::InvalidValue, unsafe {
                decoder_decode(eof_decoder, &mut failed_terminal)
            });
            unsafe { decoder_free(eof_decoder) };

            extern "C" fn invalid_read(
                _: *mut c_void,
                _: *mut u8,
                capacity: usize,
                out_read: *mut usize,
            ) -> bool {
                unsafe { *out_read = capacity + 1 };
                true
            }
            let mut invalid_decoder: Decoder = std::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                decoder_new(
                    Some(alloc),
                    &mut invalid_decoder,
                    io_c::Reader {
                        read: Some(invalid_read),
                        userdata: std::ptr::null_mut(),
                    },
                )
            });
            assert_eq!(Result::InvalidValue, unsafe {
                decoder_decode(invalid_decoder, &mut failed_terminal)
            });
            unsafe { decoder_free(invalid_decoder) };

            unsafe {
                terminal_c::free(restored);
                decoder_free(decoder);
                terminal_c::free(source);
            }
        });
    }

    // test "snapshot incremental decoder exposes READY and page progress"
    #[test]
    fn snapshot_incremental_decoder_exposes_ready_and_page_progress() {
        with_test_allocator(|alloc| {
            let (new_result, source) = new_t(alloc, 215, 2);
            assert_eq!(Result::Success, new_result);
            test_enable_continuation(source);
            // Keep multiple complete pages. The ordinary C-terminal byte limit keeps
            // at least one page but can evict older ones before snapshot encoding.
            assert_eq!(Result::Success, unsafe {
                terminal_c::set(source, Option_::ScrollbackMaxBytes, std::ptr::null())
            });
            // const first_page_rows = terminal_c.zigTerminal(source).?.screens
            //     .get(.primary).?.pages.pages.first.?.capacity().rows;
            let first_page_rows: usize = unsafe {
                let zt = terminal_c::zig_terminal(source);
                let primary = (*zt)
                    .screens
                    .get(rustty_vt::screen_set::Key::Primary)
                    .unwrap();
                let first = (*primary).pages.pages.first;
                assert!(!first.is_null());
                rustty_vt::pagelist::Node::capacity(first).rows as usize
            };
            for _ in 0..first_page_rows * 3 {
                vtw(source, b"x\r\n");
            }
            let ready_continuation = b"\x1b]2;ready";
            vtw(source, ready_continuation);

            let mut encoded_ptr: *mut u8 = std::ptr::null_mut();
            let mut encoded_len: usize = 0;
            assert_eq!(Result::Success, unsafe {
                encode_alloc(source, Some(alloc), &mut encoded_ptr, &mut encoded_len)
            });
            let encoded: &[u8] = unsafe { core::slice::from_raw_parts(encoded_ptr, encoded_len) };

            let mut decoder: Decoder = std::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                decoder_new_buf(Some(alloc), &mut decoder, encoded.as_ptr(), encoded.len())
            });
            let retained_limit: usize = 1024;
            assert_eq!(Result::Success, unsafe {
                decoder_set(
                    decoder,
                    DecoderOption::MaxContinuationBytes,
                    &retained_limit as *const usize as *const c_void,
                )
            });
            let retain = true;
            assert_eq!(Result::Success, unsafe {
                decoder_set(
                    decoder,
                    DecoderOption::RetainContinuation,
                    &retain as *const bool as *const c_void,
                )
            });

            let mut restored: terminal_c::Terminal = std::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                decoder_ready(decoder, &mut restored)
            });

            // READY applies the same retention policy as one-shot decode and makes
            // the unfinished snapshot continuation immediately exportable.
            let mut restored_limit: usize = 0;
            assert_eq!(
                Result::Success,
                get_usize(
                    restored,
                    TerminalData::ContinuationMaxBytes,
                    &mut restored_limit
                )
            );
            assert_eq!(retained_limit, restored_limit);
            let mut ready_buf = [0u8; 32];
            let mut ready_len: usize = 0;
            assert_eq!(Result::Success, unsafe {
                terminal_c::continuation_buf(
                    restored,
                    ready_buf.as_mut_ptr(),
                    ready_buf.len(),
                    &mut ready_len,
                )
            });
            assert_eq!(&ready_continuation[..], &ready_buf[..ready_len]);

            let mut history_rows: u64 = 0;
            assert_eq!(Result::Success, unsafe {
                decoder_get(
                    decoder,
                    DecoderData::HistoryRowsPrimary,
                    &mut history_rows as *mut u64 as *mut c_void,
                )
            });
            assert!(history_rows > 0);

            let mut page_count: usize = 0;
            loop {
                let result = unsafe { decoder_next(decoder) };
                if result == Result::NoValue {
                    break;
                }
                assert_eq!(Result::Success, result);
                page_count += 1;

                let mut screen: terminal_c::TerminalScreen = terminal_c::TerminalScreen::Primary;
                let mut rows: usize = 0;
                let mut remaining: u32 = 0;
                let keys = [
                    DecoderData::ProgressScreen,
                    DecoderData::ProgressRows,
                    DecoderData::ProgressRemaining,
                ];
                let values: [*mut c_void; 3] = [
                    &mut screen as *mut terminal_c::TerminalScreen as *mut c_void,
                    &mut rows as *mut usize as *mut c_void,
                    &mut remaining as *mut u32 as *mut c_void,
                ];
                let mut written: usize = 0;
                assert_eq!(Result::Success, unsafe {
                    decoder_get_multi(
                        decoder,
                        keys.len(),
                        keys.as_ptr(),
                        values.as_ptr() as *mut *mut c_void,
                        &mut written,
                    )
                });
                assert_eq!(keys.len(), written);
                assert_eq!(terminal_c::TerminalScreen::Primary, screen);
                assert!(rows > 0);
            }
            assert!(page_count > 0);
            let mut rows: usize = 0;
            assert_eq!(Result::NoValue, unsafe {
                decoder_get(
                    decoder,
                    DecoderData::ProgressRows,
                    &mut rows as *mut usize as *mut c_void,
                )
            });
            assert_eq!(Result::NoValue, unsafe { decoder_next(decoder) });

            let mut source_total: usize = 0;
            let mut restored_total: usize = 0;
            assert_eq!(
                Result::Success,
                get_usize(source, TerminalData::TotalRows, &mut source_total)
            );
            assert_eq!(
                Result::Success,
                get_usize(restored, TerminalData::TotalRows, &mut restored_total)
            );
            assert_eq!(source_total, restored_total);

            let mut dropped_decoder: Decoder = std::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                decoder_new_buf(
                    Some(alloc),
                    &mut dropped_decoder,
                    encoded.as_ptr(),
                    encoded.len(),
                )
            });
            let mut dropped: terminal_c::Terminal = std::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                decoder_ready(dropped_decoder, &mut dropped)
            });
            assert_eq!(Result::Success, unsafe {
                terminal_c::resize(dropped, 214, 2, 0, 0)
            });
            assert_eq!(Result::Success, unsafe { decoder_next(dropped_decoder) });
            let mut dropped_rows: usize = 1;
            assert_eq!(Result::Success, unsafe {
                decoder_get(
                    dropped_decoder,
                    DecoderData::ProgressRows,
                    &mut dropped_rows as *mut usize as *mut c_void,
                )
            });
            assert_eq!(0usize, dropped_rows);
            while unsafe { decoder_next(dropped_decoder) } == Result::Success {}
            assert_eq!(Result::NoValue, unsafe { decoder_next(dropped_decoder) });

            unsafe {
                terminal_c::free(dropped);
                decoder_free(dropped_decoder);
                terminal_c::free(restored);
                decoder_free(decoder);
            }
            free_alloc(alloc, encoded_ptr, encoded_len);
            unsafe { terminal_c::free(source) };
        });
    }

    /// Encoded snapshot bytes of a small terminal with scrollback, a title and
    /// a hyperlink (built over the test allocator).
    fn sample_snapshot() -> Vec<u8> {
        sample_snapshot_with(None)
    }

    /// `sample_snapshot`, optionally ending inside an unfinished CSI of
    /// `continuation` parameter bytes (continuation tracking enabled on the
    /// source).
    fn sample_snapshot_with(continuation: Option<usize>) -> Vec<u8> {
        with_test_allocator(|alloc| {
            let (new_result, source) = new_t(alloc, 20, 4);
            assert_eq!(Result::Success, new_result);
            if continuation.is_some() {
                test_enable_continuation(source);
            }
            let lines = if cfg!(miri) { 6 } else { 30 };
            for i in 0..lines {
                vtw(source, format!("line {i:02}\r\n").as_bytes());
            }
            vtw(source, b"\x1B]2;a title\x1B\\");
            vtw(
                source,
                b"\x1B]8;id=x;https://example.com\x1B\\link\x1B]8;;\x1B\\",
            );
            if let Some(n) = continuation {
                let mut csi = b"\x1B[".to_vec();
                csi.extend(std::iter::repeat_n(b'1', n));
                vtw(source, &csi);
            }
            let mut encoded_ptr: *mut u8 = std::ptr::null_mut();
            let mut encoded_len: usize = 0;
            assert_eq!(Result::Success, unsafe {
                encode_alloc(source, Some(alloc), &mut encoded_ptr, &mut encoded_len)
            });
            // SAFETY: the allocation returned above; copied out and freed here.
            let bytes = unsafe { core::slice::from_raw_parts(encoded_ptr, encoded_len) }.to_vec();
            free_alloc(alloc, encoded_ptr, encoded_len);
            unsafe { terminal_c::free(source) };
            bytes
        })
    }

    // Parity tail S7-4 (routing evidence): the decoder builds the terminal over
    // the caller's allocator (`decoder.ready(decoder.alloc, ..)`, the
    // terminal's `gpa()` from then on), so a decoded terminal's internals are
    // on the caller's allocator like a `new`-created one's: it holds at least
    // what a fresh terminal of the same size holds, and `free` returns every
    // byte.
    #[test]
    fn decoded_terminal_internals_route_through_the_callers_allocator() {
        use rustty_vt::alloc::testing::CountingAllocator;
        let bytes = sample_snapshot();

        let fresh = CountingAllocator::over_heap();
        let fresh_c = fresh.allocator();
        let (new_result, t) = new_t(&fresh_c, 20, 4);
        assert_eq!(Result::Success, new_result);
        let fresh_live = fresh.live_bytes.get();
        unsafe { terminal_c::free(t) };

        let counting = CountingAllocator::over_heap();
        let c = counting.allocator();
        let mut decoder: Decoder = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe {
            decoder_new_buf(Some(&c), &mut decoder, bytes.as_ptr(), bytes.len())
        });
        let decoder_live = counting.live_bytes.get();
        let mut restored: terminal_c::Terminal = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe {
            decoder_decode(decoder, &mut restored)
        });
        unsafe { decoder_free(decoder) };
        assert!(
            counting.live_bytes.get() >= fresh_live,
            "the decoded terminal ({} bytes, decoder {}) holds at least a fresh one's {}",
            counting.live_bytes.get(),
            decoder_live,
            fresh_live
        );
        // SAFETY: the decoded terminal is live.
        let text = plain_string(restored);
        assert!(text.ends_with(b"link"));
        unsafe { terminal_c::free(restored) };
        assert_eq!(0, counting.live_bytes.get(), "free returns every byte");
    }

    // Parity tail S7-4 (routing evidence): with retention enabled the decoded
    // terminal's continuation tracker is built by `wrap` over the caller's
    // allocator (`Stream.init(.{ .allocator = alloc, .continuation_max_bytes =
    // .. })`), like the rest of the terminal.
    #[test]
    fn decoded_terminal_continuation_tracker_is_on_the_callers_allocator() {
        use rustty_vt::alloc::testing::CountingAllocator;
        let bytes = sample_snapshot();
        let counting = CountingAllocator::over_heap();
        let c = counting.allocator();
        let mut decoder: Decoder = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe {
            decoder_new_buf(Some(&c), &mut decoder, bytes.as_ptr(), bytes.len())
        });
        let retain = true;
        assert_eq!(Result::Success, unsafe {
            decoder_set(
                decoder,
                DecoderOption::RetainContinuation,
                &retain as *const bool as *const c_void,
            )
        });
        let mut restored: terminal_c::Terminal = std::ptr::null_mut();
        assert_eq!(Result::Success, unsafe {
            decoder_decode(decoder, &mut restored)
        });
        unsafe { decoder_free(decoder) };
        // SAFETY: the decoded terminal is live; a shared read of its stream.
        unsafe {
            let tracker = (*restored)
                .stream
                .continuation
                .as_ref()
                .expect("retention keeps a tracker");
            assert_eq!(
                rustty_vt::alloc::default(Some(&c)),
                tracker.allocator(),
                "the tracker grows through the caller's allocator"
            );
        }
        unsafe { terminal_c::free(restored) };
        assert_eq!(0, counting.live_bytes.get(), "free returns every byte");
    }

    // Parity tail S7-4 (T2, no reference counterpart): for every n, a
    // `FailingAllocator` refusing its n-th allocation drives the snapshot
    // decoder (`decoder_new_buf`, `decoder_decode`: the decoded terminal and
    // its history) and `free`. Nothing aborts, every result is `SUCCESS` or
    // `OUT_OF_MEMORY`, and every byte is returned. A second loop refuses
    // `remap`/`resize` from their n-th call on. The snapshot ends inside a
    // 7000-byte unfinished CSI, so the decoded terminal's verification tracker
    // grows past its `growCapacity(4096)` reservation while the continuation
    // is replayed: the decode's remap.
    #[test]
    fn allocation_failure_sweep_through_snapshot_decode() {
        use rustty_vt::alloc::testing::{Config, FailingAllocator};
        let bytes = sample_snapshot_with(Some(7000));

        let run = |failing: &FailingAllocator, what: &str| -> bool {
            let c = failing.allocator();
            let mut decoder: Decoder = std::ptr::null_mut();
            let mut failed = false;
            match unsafe { decoder_new_buf(Some(&c), &mut decoder, bytes.as_ptr(), bytes.len()) } {
                Result::Success => {
                    let mut restored: terminal_c::Terminal = std::ptr::null_mut();
                    match unsafe { decoder_decode(decoder, &mut restored) } {
                        Result::Success => unsafe { terminal_c::free(restored) },
                        Result::OutOfMemory => {
                            assert!(restored.is_null(), "{what}");
                            failed = true;
                        }
                        other => panic!("{what}: decode returned {other:?}"),
                    }
                    unsafe { decoder_free(decoder) };
                }
                Result::OutOfMemory => {
                    assert!(decoder.is_null(), "{what}");
                    failed = true;
                }
                other => panic!("{what}: decoder_new_buf returned {other:?}"),
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
            failed
        };

        let mut n = 0usize;
        let mut failures = 0usize;
        loop {
            let failing = FailingAllocator::over_heap(Config {
                fail_index: n,
                ..Config::default()
            });
            failures += usize::from(run(&failing, &format!("alloc n={n}")));
            if !failing.has_induced_failure.get() {
                break;
            }
            n += 1;
            assert!(n < 10_000, "the sweep must terminate");
        }
        assert!(n > 10, "only {n} allocations were swept");
        assert!(failures > 5, "only {failures} indices failed the decode");

        let clean = FailingAllocator::over_heap(Config::default());
        assert!(!run(&clean, "remap clean run"));
        let remaps = clean.resize_index.get();
        assert!(remaps > 0, "the decode makes no remap/resize call");
        for n in 0..=remaps {
            let failing = FailingAllocator::over_heap(Config {
                resize_fail_index: n,
                ..Config::default()
            });
            assert!(
                !run(&failing, &format!("remap n={n}")),
                "a refused remap never fails the decode"
            );
        }
    }

    // Parity tail S7-8 (N331): `encode_alloc`'s `Writer.Allocating` grows
    // through the caller's allocator and `toOwnedSlice` hands that buffer
    // over; the temporary continuation copy (`continuationOwned`) is on the
    // TERMINAL's allocator (`native.gpa()`) and is freed before returning.
    #[test]
    fn encode_alloc_writer_routes_through_the_callers_allocator() {
        use rustty_vt::alloc::testing::CountingAllocator;
        let terminal_counting = CountingAllocator::over_heap();
        let tc = terminal_counting.allocator();
        let (new_result, source) = new_t(&tc, 10, 3);
        assert_eq!(Result::Success, new_result);
        test_enable_continuation(source);
        vtw(source, b"hello\x1b[31");

        let mut required: usize = 0;
        assert_eq!(Result::OutOfSpace, unsafe {
            encode_buf(source, std::ptr::null_mut(), 0, &mut required)
        });
        let mut expected = vec![0u8; required];
        let mut written: usize = 0;
        assert_eq!(Result::Success, unsafe {
            encode_buf(source, expected.as_mut_ptr(), expected.len(), &mut written)
        });

        let terminal_allocs = terminal_counting.alloc_calls.get();
        let terminal_frees = terminal_counting.free_calls.get();
        let terminal_remaps = terminal_counting.remap_calls.get();
        let terminal_live = terminal_counting.live_bytes.get();

        let counting = CountingAllocator::over_heap();
        let c = counting.allocator();
        let mut out_ptr: *mut u8 = std::ptr::null_mut();
        let mut out_len: usize = 0;
        assert_eq!(Result::Success, unsafe {
            encode_alloc(source, Some(&c), &mut out_ptr, &mut out_len)
        });
        assert_eq!(expected.as_slice(), unsafe {
            std::slice::from_raw_parts(out_ptr, out_len)
        });
        // The caller's allocator: the envelope's 8-byte write reserves
        // growCapacity(8 + 8 + 1) = 153 bytes, one remap grows it, and
        // `toOwnedSlice` remaps it to the 1095 encoded bytes.
        assert_eq!(1095, out_len);
        assert_eq!(1, counting.alloc_calls.get());
        assert_eq!(Some((153, 0)), counting.last_alloc.get());
        assert_eq!(2, counting.remap_calls.get());
        assert_eq!(0, counting.free_calls.get());
        // The terminal's allocator (`native.gpa()`): the continuation copy
        // (`\\x1b[31`, written with `writeByte`: growCapacity(1) = 129,
        // then the `toOwnedSlice` remap to 4) and the record scratch's spill
        // past its 512 stack bytes (S7-8 N332: 513 bytes, then one remap
        // growth), both freed before returning, the continuation copy last.
        assert_eq!(2, terminal_counting.alloc_calls.get() - terminal_allocs);
        assert_eq!(Some((513, 0)), terminal_counting.last_alloc.get());
        assert_eq!(2, terminal_counting.remap_calls.get() - terminal_remaps);
        assert_eq!(2, terminal_counting.free_calls.get() - terminal_frees);
        assert_eq!(Some((4, 0)), terminal_counting.last_free.get());
        assert_eq!(terminal_live, terminal_counting.live_bytes.get());
        // The caller owns exactly the encoded bytes.
        assert_eq!(out_len, counting.live_bytes.get());
        unsafe { crate::allocator::free(Some(&c), out_ptr, out_len) };
        assert_eq!(0, counting.live_bytes.get());
        unsafe { terminal_c::free(source) };
        assert_eq!(0, terminal_counting.live_bytes.get());
    }
}
