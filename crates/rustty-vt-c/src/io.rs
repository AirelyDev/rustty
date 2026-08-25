//! C byte-stream callbacks and adapters for `std.Io`.
//!
//! The public callbacks deliberately expose a smaller contract than Zig's I/O
//! interfaces: reads are synchronous progress/EOF/error, and writes accept an
//! entire slice or fail. These adapters translate that contract while keeping
//! enough state to map failures back to distinct C result codes.
//!
//! PORTS: ghostty src/terminal/c/io.zig
//!
//! PORT STATUS: complete (P4-W5 Bundle S). PORT NOTE (std.Io stand-in): the
//! reference adapts the C callbacks to `std.Io.Reader`/`std.Io.Writer` vtable
//! objects (`@fieldParentPtr` recovery, `rebase`, `drain`). rustty's snapshot
//! subsystem defined its own byte-oriented stand-in for exactly that surface
//! (`rustty_vt::snapshot::io::{Reader, Writer}` — see its module PORT NOTE),
//! so these adapters implement THOSE traits instead of carrying a vtable. The
//! observable C contract is carried verbatim: sticky failure/EOF, offset
//! accounting, the one-byte peek boundary, the 4 KiB transfer/batch buffers,
//! and the invalid-vs-failed callback classification. The decision to port the
//! seam this way (no TinyIo / `std.Io` model) is recorded on the TinyIo row of
//! `.port/phase-4/NON_PORTS.md`.

// const std = @import("std");
// const assert = std.debug.assert;
// const lib = @import("../lib.zig");
use rustty_vt::snapshot::io as vt_io;
use std::ffi::c_void;

/// C: GhosttyReaderFn -> RusttyReaderFn (R2)
// pub const ReaderFn = *const fn (
//     userdata: ?*anyopaque,
//     buffer: [*]u8,
//     capacity: usize,
//     out_read: *usize,
// ) callconv(lib.calling_conv) bool;
pub type ReaderFn = extern "C" fn(*mut c_void, *mut u8, usize, *mut usize) -> bool;

/// C: GhosttyWriterFn -> RusttyWriterFn (R2)
// pub const WriterFn = *const fn (
//     userdata: ?*anyopaque,
//     data: [*]const u8,
//     len: usize,
// ) callconv(lib.calling_conv) bool;
pub type WriterFn = extern "C" fn(*mut c_void, *const u8, usize) -> bool;

/// C: GhosttyReader -> RusttyReader (R2)
// PORT NOTE (cbindgen): the field spells the fn-pointer type out inline
// (rather than `Option<ReaderFn>`) because cbindgen does not monomorphize
// `Option` through a type alias — it emitted an incomplete
// `struct Option_ReaderFn` and include/rustty.h stopped compiling (arm a2).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Reader {
    pub read: Option<extern "C" fn(*mut c_void, *mut u8, usize, *mut usize) -> bool>,
    pub userdata: *mut c_void,
}

impl Reader {
    // pub fn valid(self: Reader) bool { return self.read != null; }
    pub fn valid(&self) -> bool {
        self.read.is_some()
    }
}

impl Default for Reader {
    // `= null` field defaults on the reference extern struct.
    fn default() -> Reader {
        Reader {
            read: None,
            userdata: std::ptr::null_mut(),
        }
    }
}

/// C: GhosttyWriter -> RusttyWriter (R2)
// PORT NOTE (cbindgen): inline fn-pointer type — see `Reader`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Writer {
    pub write: Option<extern "C" fn(*mut c_void, *const u8, usize) -> bool>,
    pub userdata: *mut c_void,
}

impl Writer {
    // pub fn valid(self: Writer) bool { return self.write != null; }
    pub fn valid(&self) -> bool {
        self.write.is_some()
    }
}

impl Default for Writer {
    fn default() -> Writer {
        Writer {
            write: None,
            userdata: std::ptr::null_mut(),
        }
    }
}

/// The reader observables shared between an adapter and the snapshot decoder
/// wrapper.
///
/// PORT NOTE (SB, Rust-only shape): the reference reads `adapter.offset` /
/// `.callback_failed` / `.invalid_read` directly off the adapter stored inside
/// the decoder wrapper. rustty's core `snapshot::Decoder` holds `&mut dyn
/// Reader` for its whole life, so any independent wrapper-side access through
/// the adapter's own storage would invalidate that stored `&mut` under Stacked
/// Borrows (the decoder-aliasing hazard named by the Bundle-S plan). These
/// observables therefore live in a separate heap cell that is only ever
/// accessed through raw place projections — no reference to it is ever
/// created — so the adapter (inside decoder calls) and the wrapper (between
/// them) can both touch it without popping the decoder's borrow. Proven on the
/// vt-c seam tests under Miri.
#[derive(Default)]
pub struct ReaderState {
    /// Bytes successfully obtained from the callback.
    ///
    /// For a borrowed-buffer source this is the fixed reader's `seek`
    /// (the number of source bytes consumed).
    pub offset: usize,

    /// The callback returned false. This distinguishes an I/O failure from
    /// the callback returning true with zero bytes at end-of-file.
    pub callback_failed: bool,

    /// The callback was NULL, returned too many bytes, or overflowed offset.
    pub invalid_read: bool,

    /// The callback returned true with zero bytes. EOF is permanent.
    pub eof: bool,
}

/// Adapts a `RusttyReader` to `rustty_vt::snapshot::io::Reader`.
///
/// The reference adapter starts unbuffered and lazily attaches a one-byte
/// buffer so that operations such as `peekByte` cannot pull a second byte
/// across a snapshot checkpoint or FINISH boundary; bulk reads stream
/// directly into the destination's storage. The port keeps both properties:
/// `refill` pulls exactly one byte into `peek_buffer`, and the bulk
/// `read_slice_all`/`stream_to` overrides invoke the callback directly with
/// the destination's exact capacity.
pub struct ReaderAdapter {
    /// Copy of the C callback pair. The pointed-to userdata remains borrowed.
    source: Reader,

    /// Shared observables cell (see `ReaderState`); raw-access only.
    state: *mut ReaderState,

    /// `std.Io.Reader` needs backing storage for `peekByte`. One byte is
    /// intentional: it satisfies that operation without allowing the adapter
    /// to pull a second byte across a snapshot checkpoint or FINISH boundary.
    peek_buffer: [u8; 1],
    peek_start: usize,
    peek_end: usize,
}

impl ReaderAdapter {
    // pub fn init(source: Reader) ReaderAdapter { ... }
    /// # Safety
    /// `state` must point to a live `ReaderState` that outlives the adapter and
    /// is only ever accessed through raw pointers (never referenced).
    pub unsafe fn init(source: Reader, state: *mut ReaderState) -> ReaderAdapter {
        ReaderAdapter {
            source,
            state,
            peek_buffer: [0],
            peek_start: 0,
            peek_end: 0,
        }
    }

    /// Perform exactly one C callback invocation into caller-selected storage.
    /// This is the single place that interprets the public callback contract.
    fn read_into(&mut self, destination: &mut [u8]) -> Result<usize, vt_io::ReadError> {
        assert!(!destination.is_empty());

        // Failure and EOF are sticky. In particular, a callback that reports
        // EOF is never polled again because the C API defines it as permanent.
        //
        // SAFETY (all `self.state` accesses below): raw place projections into
        // the live shared cell (adapter contract); no reference is created.
        unsafe {
            if (*self.state).callback_failed || (*self.state).invalid_read {
                return Err(vt_io::ReadError::ReadFailed);
            }
            if (*self.state).eof {
                return Err(vt_io::ReadError::EndOfStream);
            }
        }

        // A missing callback is an invalid C value rather than external I/O
        // failure, so record it separately for the public result mapper.
        let read_fn = match self.source.read {
            Some(f) => f,
            None => {
                unsafe { (*self.state).invalid_read = true };
                return Err(vt_io::ReadError::ReadFailed);
            }
        };

        // Initialize this ourselves: a callback returning false is not
        // required to initialize out_read.
        let mut read_len: usize = 0;
        if !read_fn(
            self.source.userdata,
            destination.as_mut_ptr(),
            destination.len(),
            &mut read_len,
        ) {
            unsafe { (*self.state).callback_failed = true };
            return Err(vt_io::ReadError::ReadFailed);
        }

        // Treat a callback that violates the capacity contract as an invalid
        // reader without constructing an out-of-bounds slice.
        if read_len > destination.len() {
            unsafe { (*self.state).invalid_read = true };
            return Err(vt_io::ReadError::ReadFailed);
        }
        // Successful zero-length reads are the C representation of EOF.
        if read_len == 0 {
            unsafe { (*self.state).eof = true };
            return Err(vt_io::ReadError::EndOfStream);
        }

        // Offset counts only bytes actually supplied by successful callbacks.
        // Saturating or wrapping would make SOURCE_OFFSET untrustworthy.
        unsafe {
            match (*self.state).offset.checked_add(read_len) {
                Some(next) => (*self.state).offset = next,
                None => {
                    (*self.state).invalid_read = true;
                    return Err(vt_io::ReadError::ReadFailed);
                }
            }
        }
        Ok(read_len)
    }
}

impl vt_io::Reader for ReaderAdapter {
    fn buffered(&self) -> &[u8] {
        &self.peek_buffer[self.peek_start..self.peek_end]
    }

    fn toss(&mut self, n: usize) {
        debug_assert!(n <= self.peek_end - self.peek_start);
        self.peek_start += n;
    }

    /// Give buffered reader operations contiguous storage on first demand.
    /// (Zig: `rebase` attaches `peek_buffer` lazily.) One byte per pull keeps
    /// buffered operations from reading beyond a requested framing boundary.
    fn refill(&mut self) -> Result<(), vt_io::ReadError> {
        if self.peek_start == self.peek_end {
            self.peek_start = 0;
            self.peek_end = 0;
        }
        debug_assert!(self.peek_end < self.peek_buffer.len());
        let n = {
            let end = self.peek_end;
            let mut byte = [0u8; 1];
            let n = self.read_into(&mut byte)?;
            self.peek_buffer[end] = byte[0];
            n
        };
        self.peek_end += n;
        Ok(())
    }

    fn take_buffered(&mut self, n: usize) -> &[u8] {
        debug_assert!(n <= self.peek_end - self.peek_start);
        let start = self.peek_start;
        self.peek_start += n;
        &self.peek_buffer[start..start + n]
    }

    /// Use the destination's storage directly when possible. This is the
    /// normal path for readSlice operations. (Zig: `stream`'s direct path via
    /// `writer.unusedCapacitySlice()`.)
    fn read_slice_all(&mut self, dest: &mut [u8]) -> Result<(), vt_io::ReadError> {
        let mut filled: usize = 0;
        // Drain any parked peek byte first so ordering is preserved.
        while filled < dest.len() {
            let avail = self.peek_end - self.peek_start;
            if avail == 0 {
                break;
            }
            let n = avail.min(dest.len() - filled);
            dest[filled..filled + n]
                .copy_from_slice(&self.peek_buffer[self.peek_start..self.peek_start + n]);
            self.peek_start += n;
            filled += n;
        }
        while filled < dest.len() {
            let n = self.read_into(&mut dest[filled..])?;
            filled += n;
        }
        Ok(())
    }

    /// Move bytes straight from the callback into `dest`, bounded by the
    /// caller's exact capacity — the reference's `limit`-truncated `stream`.
    fn stream_to(&mut self, dest: &mut [u8]) -> Result<usize, vt_io::ReadError> {
        let avail = self.peek_end - self.peek_start;
        if avail > 0 {
            let n = avail.min(dest.len());
            dest[..n].copy_from_slice(&self.peek_buffer[self.peek_start..self.peek_start + n]);
            self.peek_start += n;
            return Ok(n);
        }
        self.read_into(dest)
    }
}

/// Adapts borrowed, caller-owned bytes to `rustty_vt::snapshot::io::Reader`.
///
/// PORT NOTE: the reference's buffer source is `std.Io.Reader.fixed(bytes)`
/// with its `seek` read directly as the source offset. rustty routes that
/// observable through the same shared `ReaderState` cell as the callback
/// adapter (see the SB note on `ReaderState`), so the decoder wrapper reads
/// one uniform cell for `SOURCE_OFFSET` regardless of source kind.
pub struct BufferSource {
    data: &'static [u8],
    /// Shared observables cell; `offset` mirrors the fixed reader's `seek`.
    state: *mut ReaderState,
}

impl BufferSource {
    /// # Safety
    /// `data` must remain valid and immutable until FINISH is reached or the
    /// decoder is freed (the C contract for `rustty_snapshot_decoder_new_buf`);
    /// `state` as for `ReaderAdapter::init`.
    pub unsafe fn init(data: &'static [u8], state: *mut ReaderState) -> BufferSource {
        BufferSource { data, state }
    }

    fn seek(&self) -> usize {
        // SAFETY: raw read of the live shared cell (constructor contract).
        unsafe { (*self.state).offset }
    }
}

impl vt_io::Reader for BufferSource {
    fn buffered(&self) -> &[u8] {
        &self.data[self.seek()..]
    }

    fn toss(&mut self, n: usize) {
        debug_assert!(n <= self.data.len() - self.seek());
        // SAFETY: raw write of the live shared cell (constructor contract).
        unsafe { (*self.state).offset += n };
    }

    fn refill(&mut self) -> Result<(), vt_io::ReadError> {
        // The fixed reader's whole content is its buffer; there is never more.
        Err(vt_io::ReadError::EndOfStream)
    }

    fn take_buffered(&mut self, n: usize) -> &[u8] {
        let start = self.seek();
        debug_assert!(n <= self.data.len() - start);
        // SAFETY: raw write of the live shared cell (constructor contract).
        unsafe { (*self.state).offset = start + n };
        &self.data[start..start + n]
    }
}

/// Adapts a `RusttyWriter` to the Zig-facing writer surfaces.
///
/// The adapter may be buffered (`init_buffered`) so that the many small
/// writes typical of streaming producers (e.g. the formatter) are batched
/// into far fewer C callback invocations. A buffered adapter's owner must
/// call `flush` before reporting success so that the public contract holds:
/// success means the C callback has already accepted every byte.
///
/// PORT NOTE: the reference's buffer is caller-owned stack storage passed to
/// `initBuffered`; every call site sizes it `recommended_buffer_len`. The port
/// embeds a buffer of exactly that size in the adapter (Rust cannot hold the
/// caller's `&mut` alongside the C pair without infecting every consumer with
/// a lifetime), with `buf_len` selecting the unbuffered (`init`) shape.
pub struct WriterAdapter {
    /// Copy of the C callback pair. The pointed-to userdata remains borrowed.
    destination: Writer,

    /// Bytes accepted by successful callback invocations. Buffered bytes are
    /// counted only once a drain or flush hands them to the callback.
    pub offset: usize,

    /// The callback returned false.
    pub callback_failed: bool,

    /// The callback was NULL or offset accounting overflowed.
    pub invalid_write: bool,

    buf: [u8; WriterAdapter::RECOMMENDED_BUFFER_LEN],
    /// Active buffer capacity: 0 (unbuffered) or `RECOMMENDED_BUFFER_LEN`.
    buf_len: usize,
    /// Buffered byte count (Zig: `interface.end`).
    end: usize,
}

/// `error{WriteFailed}` for the adapter's internal paths.
struct AdapterWriteFailed;

impl WriterAdapter {
    /// The buffer length used by the C API entry points that stream through
    /// a callback. 4 KiB amortizes callback overhead while keeping every
    /// adapter's stack footprint modest, mirroring ReaderAdapter's
    /// transfer_buffer sizing.
    // pub const recommended_buffer_len = 4096;
    pub const RECOMMENDED_BUFFER_LEN: usize = 4096;

    /// An unbuffered adapter: every write reaches the callback immediately
    /// and no flush is required before returning.
    pub fn init(destination: Writer) -> WriterAdapter {
        let mut adapter = WriterAdapter::init_buffered(destination);
        adapter.buf_len = 0;
        adapter
    }

    /// A buffered adapter. The owner must flush before treating the
    /// operation as successful.
    pub fn init_buffered(destination: Writer) -> WriterAdapter {
        WriterAdapter {
            destination,
            offset: 0,
            callback_failed: false,
            invalid_write: false,
            buf: [0; WriterAdapter::RECOMMENDED_BUFFER_LEN],
            buf_len: WriterAdapter::RECOMMENDED_BUFFER_LEN,
            end: 0,
        }
    }

    /// Submit one ordinary slice through the all-or-nothing C write callback.
    ///
    /// A free function over the split field borrows so the buffered paths can
    /// hand `&self.buf[..end]` to it while mutating the accounting fields.
    // fn writeAll(self: *WriterAdapter, data: []const u8) std.Io.Writer.Error!void {
    fn send(
        destination: &Writer,
        offset: &mut usize,
        callback_failed: &mut bool,
        invalid_write: &mut bool,
        data: &[u8],
    ) -> Result<(), AdapterWriteFailed> {
        // Empty writes carry no information and should not call foreign code.
        if data.is_empty() {
            return Ok(());
        }

        // Make failures sticky so a higher-level retry cannot produce a
        // misleading second callback after the output is already partial.
        if *callback_failed || *invalid_write {
            return Err(AdapterWriteFailed);
        }

        // Missing callbacks are invalid arguments; false callback returns are
        // external I/O errors. Preserve that distinction for the C wrapper.
        let write_fn = match destination.write {
            Some(f) => f,
            None => {
                *invalid_write = true;
                return Err(AdapterWriteFailed);
            }
        };
        if !write_fn(destination.userdata, data.as_ptr(), data.len()) {
            *callback_failed = true;
            return Err(AdapterWriteFailed);
        }

        // Count bytes only after the callback accepts the complete slice.
        match offset.checked_add(data.len()) {
            Some(next) => *offset = next,
            None => {
                *invalid_write = true;
                return Err(AdapterWriteFailed);
            }
        }
        Ok(())
    }

    fn send_all(&mut self, data: &[u8]) -> Result<(), AdapterWriteFailed> {
        WriterAdapter::send(
            &self.destination,
            &mut self.offset,
            &mut self.callback_failed,
            &mut self.invalid_write,
            data,
        )
    }

    /// Buffered bytes must reach the destination before any later slice.
    fn drain_buffer(&mut self) -> Result<(), AdapterWriteFailed> {
        let end = self.end;
        self.end = 0;
        WriterAdapter::send(
            &self.destination,
            &mut self.offset,
            &mut self.callback_failed,
            &mut self.invalid_write,
            &self.buf[..end],
        )
    }

    /// Write one slice, batching through the buffer when one is attached.
    /// Slices that fit are parked in the buffer (a later drain or flush
    /// hands them to the callback); anything larger goes to the callback
    /// directly after the buffer is emptied to preserve ordering.
    // fn writeSlice(self: *WriterAdapter, writer_: *std.Io.Writer, slice: []const u8) ... {
    fn write_slice(&mut self, slice: &[u8]) -> Result<(), AdapterWriteFailed> {
        if slice.len() <= self.buf_len - self.end {
            self.buf[self.end..self.end + slice.len()].copy_from_slice(slice);
            self.end += slice.len();
            return Ok(());
        }

        if self.end > 0 {
            self.drain_buffer()?;
        }

        if slice.len() <= self.buf_len {
            self.buf[..slice.len()].copy_from_slice(slice);
            self.end = slice.len();
        } else {
            self.send_all(slice)?;
        }
        Ok(())
    }

    /// Deliver any parked bytes to the callback. (Zig:
    /// `adapter.interface.flush()` draining the interface buffer.)
    pub fn flush_buffered(&mut self) -> Result<(), ()> {
        if self.end > 0 {
            self.drain_buffer().map_err(|_| ())?;
        }
        Ok(())
    }
}

impl vt_io::Writer for WriterAdapter {
    fn write_all(&mut self, bytes: &[u8]) -> Result<(), vt_io::WriteFailed> {
        self.write_slice(bytes).map_err(|_| vt_io::WriteFailed)
    }

    /// Single-byte splats (e.g. runs of blank cells) are memset into the
    /// buffer in bulk rather than repeated one write at a time. (Zig: the
    /// `pattern.len == 1` arm of `drain`.)
    fn splat_byte_all(&mut self, byte: u8, n: usize) -> Result<(), vt_io::WriteFailed> {
        if self.buf_len == 0 {
            // Unbuffered adapters take the generic one-write-per-repeat path.
            for _ in 0..n {
                self.write_slice(&[byte]).map_err(|_| vt_io::WriteFailed)?;
            }
            return Ok(());
        }
        let mut remaining = n;
        while remaining > 0 {
            if self.end == self.buf_len {
                self.flush_buffered().map_err(|_| vt_io::WriteFailed)?;
            }
            let step = remaining.min(self.buf_len - self.end);
            for b in &mut self.buf[self.end..self.end + step] {
                *b = byte;
            }
            self.end += step;
            remaining -= step;
        }
        Ok(())
    }
}

// PORT NOTE: the `stream_terminal` continuation exporter and the formatter
// drivers are generic over `std::io::Write` (the crate-wide `std.Io.Writer`
// stand-in for those tiers — see `writer.rs`); the same adapter serves them.
impl std::io::Write for WriterAdapter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        match self.write_slice(bytes) {
            Ok(()) => Ok(bytes.len()),
            Err(_) => Err(std::io::Error::from(std::io::ErrorKind::WriteZero)),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.flush_buffered()
            .map_err(|_| std::io::Error::from(std::io::ErrorKind::WriteZero))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustty_vt::snapshot::io::{Reader as _, Writer as _};

    // Test scaffolding: the reference constructs adapters on the stack and
    // reads `adapter.offset`/flags directly; the port's observables live in a
    // `ReaderState` cell (see its SB PORT NOTE), so each test owns one boxed
    // cell and reads it back through the raw pointer it handed the adapter.
    fn reader_state() -> *mut ReaderState {
        Box::into_raw(Box::new(ReaderState::default()))
    }

    unsafe fn free_state(state: *mut ReaderState) {
        drop(unsafe { Box::from_raw(state) });
    }

    // test "C reader and writer layouts keep callback first"
    #[test]
    fn c_reader_and_writer_layouts_keep_callback_first() {
        assert_eq!(0, std::mem::offset_of!(Reader, read));
        assert_eq!(
            std::mem::size_of::<Option<ReaderFn>>(),
            std::mem::offset_of!(Reader, userdata)
        );
        assert_eq!(
            std::mem::size_of::<Option<ReaderFn>>() + std::mem::size_of::<*mut c_void>(),
            std::mem::size_of::<Reader>()
        );

        assert_eq!(0, std::mem::offset_of!(Writer, write));
        assert_eq!(
            std::mem::size_of::<Option<WriterFn>>(),
            std::mem::offset_of!(Writer, userdata)
        );
        assert_eq!(
            std::mem::size_of::<Option<WriterFn>>() + std::mem::size_of::<*mut c_void>(),
            std::mem::size_of::<Writer>()
        );
    }

    // test "ReaderAdapter supports short reads and permanent EOF"
    #[test]
    fn reader_adapter_supports_short_reads_and_permanent_eof() {
        struct Context {
            data: &'static [u8],
            offset: usize,
            calls: usize,
        }
        extern "C" fn read(
            userdata: *mut c_void,
            buffer: *mut u8,
            capacity: usize,
            out_read: *mut usize,
        ) -> bool {
            let ctx = unsafe { &mut *(userdata as *mut Context) };
            ctx.calls += 1;
            let remaining = &ctx.data[ctx.offset..];
            let len = remaining.len().min(capacity).min(2);
            unsafe {
                std::ptr::copy_nonoverlapping(remaining.as_ptr(), buffer, len);
                *out_read = len;
            }
            ctx.offset += len;
            true
        }

        let mut context = Context {
            data: b"abcdef",
            offset: 0,
            calls: 0,
        };
        let state = reader_state();
        let mut adapter = unsafe {
            ReaderAdapter::init(
                Reader {
                    read: Some(read),
                    userdata: &mut context as *mut Context as *mut c_void,
                },
                state,
            )
        };

        let mut actual = [0u8; 6];
        adapter.read_slice_all(&mut actual).unwrap();
        assert_eq!(b"abcdef", &actual);
        unsafe {
            assert_eq!(6, (*state).offset);
            assert!(!(*state).eof);
        }

        assert_eq!(Err(vt_io::ReadError::EndOfStream), adapter.take_byte());
        unsafe {
            assert!((*state).eof);
            assert!(!(*state).callback_failed);
        }

        let calls_at_eof = context.calls;
        assert_eq!(Err(vt_io::ReadError::EndOfStream), adapter.take_byte());
        assert_eq!(calls_at_eof, context.calls);
        drop(adapter);
        unsafe { free_state(state) };
    }

    // test "ReaderAdapter peek reads only the requested byte"
    #[test]
    fn reader_adapter_peek_reads_only_the_requested_byte() {
        struct Context {
            data: &'static [u8],
            offset: usize,
            last_capacity: usize,
        }
        extern "C" fn read(
            userdata: *mut c_void,
            buffer: *mut u8,
            capacity: usize,
            out_read: *mut usize,
        ) -> bool {
            let ctx = unsafe { &mut *(userdata as *mut Context) };
            ctx.last_capacity = capacity;
            let remaining = &ctx.data[ctx.offset..];
            let len = remaining.len().min(capacity);
            unsafe {
                std::ptr::copy_nonoverlapping(remaining.as_ptr(), buffer, len);
                *out_read = len;
            }
            ctx.offset += len;
            true
        }

        let mut context = Context {
            data: b"ab",
            offset: 0,
            last_capacity: 0,
        };
        let state = reader_state();
        let mut adapter = unsafe {
            ReaderAdapter::init(
                Reader {
                    read: Some(read),
                    userdata: &mut context as *mut Context as *mut c_void,
                },
                state,
            )
        };

        assert_eq!(Ok(b'a'), adapter.peek_byte());
        assert_eq!(1, context.last_capacity);
        unsafe { assert_eq!(1, (*state).offset) };
        assert_eq!(Ok(b'a'), adapter.take_byte());
        unsafe { assert_eq!(1, (*state).offset) };
        drop(adapter);
        unsafe { free_state(state) };
    }

    // test "ReaderAdapter distinguishes callback failure and invalid length"
    #[test]
    fn reader_adapter_distinguishes_callback_failure_and_invalid_length() {
        extern "C" fn failing_read(_: *mut c_void, _: *mut u8, _: usize, _: *mut usize) -> bool {
            false
        }
        let failing_state = reader_state();
        let mut failing = unsafe {
            ReaderAdapter::init(
                Reader {
                    read: Some(failing_read),
                    userdata: std::ptr::null_mut(),
                },
                failing_state,
            )
        };
        assert_eq!(Err(vt_io::ReadError::ReadFailed), failing.take_byte());
        unsafe {
            assert!((*failing_state).callback_failed);
            assert!(!(*failing_state).eof);
            assert!(!(*failing_state).invalid_read);
        }
        drop(failing);
        unsafe { free_state(failing_state) };

        extern "C" fn oversized_read(
            _: *mut c_void,
            _: *mut u8,
            capacity: usize,
            out_read: *mut usize,
        ) -> bool {
            unsafe { *out_read = capacity + 1 };
            true
        }
        let oversized_state = reader_state();
        let mut oversized = unsafe {
            ReaderAdapter::init(
                Reader {
                    read: Some(oversized_read),
                    userdata: std::ptr::null_mut(),
                },
                oversized_state,
            )
        };
        assert_eq!(Err(vt_io::ReadError::ReadFailed), oversized.take_byte());
        unsafe {
            assert!(!(*oversized_state).callback_failed);
            assert!(!(*oversized_state).eof);
            assert!((*oversized_state).invalid_read);
        }
        drop(oversized);
        unsafe { free_state(oversized_state) };
    }

    // test "ReaderAdapter rejects a null callback"
    #[test]
    fn reader_adapter_rejects_a_null_callback() {
        let state = reader_state();
        let mut adapter = unsafe { ReaderAdapter::init(Reader::default(), state) };
        assert_eq!(Err(vt_io::ReadError::ReadFailed), adapter.take_byte());
        unsafe {
            assert!((*state).invalid_read);
            assert!(!(*state).callback_failed);
        }
        drop(adapter);
        unsafe { free_state(state) };
    }

    // test "WriterAdapter writes vectors and splats"
    //
    // PORT NOTE: the Rust writer surface has no vector form (`writeVecAll`);
    // the vector half becomes two ordinary writes and the splat half uses
    // `splat_byte_all`, preserving the assertions byte-for-byte.
    #[test]
    fn writer_adapter_writes_vectors_and_splats() {
        struct Context {
            data: [u8; 32],
            len: usize,
        }
        extern "C" fn write(userdata: *mut c_void, data: *const u8, len: usize) -> bool {
            let ctx = unsafe { &mut *(userdata as *mut Context) };
            unsafe {
                std::ptr::copy_nonoverlapping(data, ctx.data[ctx.len..].as_mut_ptr(), len);
            }
            ctx.len += len;
            true
        }

        let mut context = Context {
            data: [0; 32],
            len: 0,
        };
        let mut adapter = WriterAdapter::init(Writer {
            write: Some(write),
            userdata: &mut context as *mut Context as *mut c_void,
        });

        adapter.write_all(b"ab").unwrap();
        adapter.write_all(b"cd").unwrap();
        adapter.splat_byte_all(b'!', 2).unwrap();

        assert_eq!(b"abcd!!", &context.data[..context.len]);
        assert_eq!(6, adapter.offset);
        assert!(!adapter.callback_failed);
    }

    // test "WriterAdapter buffered batches small writes"
    #[test]
    fn writer_adapter_buffered_batches_small_writes() {
        struct Context {
            data: [u8; 256],
            len: usize,
            calls: usize,
        }
        extern "C" fn write(userdata: *mut c_void, data: *const u8, len: usize) -> bool {
            let ctx = unsafe { &mut *(userdata as *mut Context) };
            unsafe {
                std::ptr::copy_nonoverlapping(data, ctx.data[ctx.len..].as_mut_ptr(), len);
            }
            ctx.len += len;
            ctx.calls += 1;
            true
        }

        let mut context = Context {
            data: [0; 256],
            len: 0,
            calls: 0,
        };
        let mut adapter = WriterAdapter::init_buffered(Writer {
            write: Some(write),
            userdata: &mut context as *mut Context as *mut c_void,
        });

        // Many small writes fit in the buffer: no callback yet.
        for _ in 0..16 {
            adapter.write_all(b"ab").unwrap();
        }
        assert_eq!(0, context.calls);

        // Flush delivers everything in a single call.
        adapter.flush_buffered().unwrap();
        assert_eq!(1, context.calls);
        assert_eq!("ab".repeat(16).as_bytes(), &context.data[..context.len]);
        assert_eq!(32, adapter.offset);
    }

    // test "WriterAdapter buffered splats and large writes"
    //
    // PORT NOTE: the reference sizes the caller buffer at 16 bytes; the port's
    // buffer is fixed at `RECOMMENDED_BUFFER_LEN` (see the adapter PORT NOTE),
    // so the splat/write lengths scale with it to preserve every boundary the
    // reference exercises (splat larger than the buffer chunked, oversize
    // write flushed and sent directly, chunked-call bound).
    #[test]
    fn writer_adapter_buffered_splats_and_large_writes() {
        const BUF: usize = WriterAdapter::RECOMMENDED_BUFFER_LEN;
        struct Context {
            data: Vec<u8>,
            calls: usize,
        }
        extern "C" fn write(userdata: *mut c_void, data: *const u8, len: usize) -> bool {
            let ctx = unsafe { &mut *(userdata as *mut Context) };
            ctx.data
                .extend_from_slice(unsafe { std::slice::from_raw_parts(data, len) });
            ctx.calls += 1;
            true
        }

        let mut context = Context {
            data: Vec::new(),
            calls: 0,
        };
        let mut adapter = WriterAdapter::init_buffered(Writer {
            write: Some(write),
            userdata: &mut context as *mut Context as *mut c_void,
        });

        // A splat larger than the buffer is chunked, not one call per byte.
        adapter.splat_byte_all(b' ', BUF * 2 + BUF / 2).unwrap();
        // A write larger than the buffer flushes and goes to the callback
        // directly.
        let oversize = vec![b'x'; BUF + 1];
        adapter.write_all(&oversize).unwrap();
        adapter.write_all(b"xy").unwrap();
        adapter.flush_buffered().unwrap();

        let mut expected = vec![b' '; BUF * 2 + BUF / 2];
        expected.extend_from_slice(&oversize);
        expected.extend_from_slice(b"xy");
        assert_eq!(expected, context.data);
        assert_eq!(context.data.len(), adapter.offset);
        // Chunked delivery: far fewer calls than bytes.
        assert!(context.calls <= 6);
    }

    // test "WriterAdapter makes callback failure sticky"
    #[test]
    fn writer_adapter_makes_callback_failure_sticky() {
        struct Context {
            calls: usize,
        }
        extern "C" fn write(userdata: *mut c_void, _: *const u8, _: usize) -> bool {
            let ctx = unsafe { &mut *(userdata as *mut Context) };
            ctx.calls += 1;
            false
        }

        let mut context = Context { calls: 0 };
        let mut adapter = WriterAdapter::init(Writer {
            write: Some(write),
            userdata: &mut context as *mut Context as *mut c_void,
        });
        assert!(adapter.write_all(b"x").is_err());
        assert!(adapter.callback_failed);
        assert_eq!(0, adapter.offset);

        assert!(adapter.write_all(b"y").is_err());
        assert_eq!(1, context.calls);
    }

    // test "WriterAdapter rejects a null callback"
    #[test]
    fn writer_adapter_rejects_a_null_callback() {
        let mut adapter = WriterAdapter::init(Writer::default());
        assert!(adapter.write_all(b"x").is_err());
        assert!(adapter.invalid_write);
        assert!(!adapter.callback_failed);
    }

    // test "ReaderAdapter streams into an unbuffered WriterAdapter"
    //
    // PORT NOTE: the reference drives `reader.interface.streamExact(&writer,
    // n)`; the Rust reader surface's transfer primitive is `stream_to`, so the
    // exact-length transfer is spelled as a bounded pump loop with the same
    // observable result (every byte crosses both adapters; offsets match).
    #[test]
    fn reader_adapter_streams_into_an_unbuffered_writer_adapter() {
        struct ReadContext {
            data: &'static [u8],
            offset: usize,
        }
        extern "C" fn read(
            userdata: *mut c_void,
            buffer: *mut u8,
            capacity: usize,
            out_read: *mut usize,
        ) -> bool {
            let ctx = unsafe { &mut *(userdata as *mut ReadContext) };
            let remaining = &ctx.data[ctx.offset..];
            let len = remaining.len().min(capacity);
            unsafe {
                std::ptr::copy_nonoverlapping(remaining.as_ptr(), buffer, len);
                *out_read = len;
            }
            ctx.offset += len;
            true
        }
        struct WriteContext {
            data: [u8; 8],
            len: usize,
        }
        extern "C" fn write(userdata: *mut c_void, data: *const u8, len: usize) -> bool {
            let ctx = unsafe { &mut *(userdata as *mut WriteContext) };
            unsafe {
                std::ptr::copy_nonoverlapping(data, ctx.data[ctx.len..].as_mut_ptr(), len);
            }
            ctx.len += len;
            true
        }

        let mut read_context = ReadContext {
            data: b"stream",
            offset: 0,
        };
        let state = reader_state();
        let mut reader = unsafe {
            ReaderAdapter::init(
                Reader {
                    read: Some(read),
                    userdata: &mut read_context as *mut ReadContext as *mut c_void,
                },
                state,
            )
        };
        let mut write_context = WriteContext {
            data: [0; 8],
            len: 0,
        };
        let mut writer = WriterAdapter::init(Writer {
            write: Some(write),
            userdata: &mut write_context as *mut WriteContext as *mut c_void,
        });

        // try reader.interface.streamExact(&writer.interface, "stream".len);
        let mut remaining = b"stream".len();
        let mut transfer = [0u8; 8];
        while remaining > 0 {
            let n = reader.stream_to(&mut transfer[..remaining]).unwrap();
            writer.write_all(&transfer[..n]).unwrap();
            remaining -= n;
        }
        assert_eq!(b"stream", &write_context.data[..write_context.len]);
        unsafe { assert_eq!(b"stream".len(), (*state).offset) };
        assert_eq!(b"stream".len(), writer.offset);
        drop(reader);
        unsafe { free_state(state) };
    }
}
