//! PORTS: ghostty src/terminal/c/terminal.zig — the `vt_write`,
//! `vt_write_until_ground`, continuation export (`continuation_*`) and
//! compression (`compression_activity`, `compress`) cluster.

use super::*;

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
    // N452: raw-self (`Stream::next_slice_raw`), so an effect callback that
    // re-enters this terminal finds no protected `&mut Stream`.
    unsafe { Stream::next_slice_raw(std::ptr::addr_of_mut!((*terminal_).stream), input) };
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
    // N452: raw-self, as in `vt_write`.
    if let Some(consumed) = unsafe {
        Stream::next_slice_until_ground_raw(std::ptr::addr_of_mut!((*terminal_).stream), input)
    } {
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
    // N452: by place (`write_continuation_raw`); this may run from an effect
    // callback while the handler dispatch holds a protected `&mut Handler`.
    unsafe { Stream::write_continuation_raw(std::ptr::addr_of!((*terminal_).stream), writer) }
        .map_err(|err| match err {
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
        && unsafe { Stream::ground_raw(std::ptr::addr_of!((*terminal_).stream)) }
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
pub(super) unsafe fn continuation_max_bytes(wrapper: *const TerminalWrapper) -> usize {
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
pub(super) unsafe fn set_continuation_max_bytes(wrapper: *mut TerminalWrapper, max_bytes: usize) {
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
        if !Stream::ground_raw(std::ptr::addr_of!((*wrapper).stream)) {
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
