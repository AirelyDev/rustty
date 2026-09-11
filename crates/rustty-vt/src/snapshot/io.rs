//! Wire-format I/O helpers.
//!
//! These are sometimes very thin layers over standard `std.Io.Writer`
//! but its to help ensure consistency in our behavior and avoid some
//! pitfalls.
//!
//! PORTS: ghostty src/terminal/snapshot/io.zig (`write_int` / `read_int` and
//! their test).
//!
//! PORT NOTE (std.Io stand-in): the reference passes `*std.Io.Writer` /
//! `*std.Io.Reader` — Zig standard-library vtable stream types — throughout
//! the snapshot subsystem. Rust has no equivalent, and the crate's existing
//! `fixed_writer.rs` stand-in is `core::fmt::Write` (str-only), unusable for a
//! binary wire format. This module therefore also carries the faithful
//! byte-oriented stand-in for the `std.Io` surface the snapshot subsystem
//! uses: the `Writer`/`Reader` traits (the vtable), `FixedWriter` /
//! `AllocatingWriter` (`std.Io.Writer.fixed` / `.Allocating`), and
//! `FixedReader` / `LimitedReader` (`std.Io.Reader.fixed` / `.Limited`).
//! `std.Io.Reader.Hashed` / `std.Io.Writer.Hashing` have no generic stand-in:
//! their only snapshot consumers are `record.Checksum` (which implements
//! `Writer` directly over `Crc32c`) and `record.Reader`'s streaming payload
//! path (flattened into that type — see the PORT NOTE there).
//!
//! PORT NOTE (bulk-read bypass): Zig's `Reader` streams bulk reads directly
//! into the destination, bypassing its buffer. The stand-in always moves bytes
//! through the buffer (`stream_to`). Behavior is identical; only the copy
//! count differs.

// const std = @import("std");
// const testing = std.testing;

/// `std.Io.Writer.Error` — `error{WriteFailed}`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WriteFailed;

/// `std.Io.Reader.Error` — `error{ ReadFailed, EndOfStream }`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReadError {
    ReadFailed,
    EndOfStream,
}

/// The `std.Io.Writer` surface the snapshot subsystem uses.
pub trait Writer {
    /// `writer.writeAll(bytes)`
    fn write_all(&mut self, bytes: &[u8]) -> Result<(), WriteFailed>;

    /// `writer.writeByte(byte)`
    fn write_byte(&mut self, byte: u8) -> Result<(), WriteFailed> {
        self.write_all(&[byte])
    }

    /// `writer.splatByteAll(byte, n)`
    fn splat_byte_all(&mut self, byte: u8, n: usize) -> Result<(), WriteFailed> {
        for _ in 0..n {
            self.write_byte(byte)?;
        }
        Ok(())
    }
}

/// The `std.Io.Reader` surface the snapshot subsystem uses.
///
/// Like the reference type this is a buffered reader: implementations expose
/// their currently buffered bytes (`buffered`), consume them (`toss`), and
/// pull more from their source (`refill`, the vtable `stream` indirection).
/// Everything else is provided combinators over those primitives.
pub trait Reader {
    /// `reader.buffered()` — the buffered, not-yet-consumed bytes.
    fn buffered(&self) -> &[u8];

    /// `reader.toss(n)` — consume `n` buffered bytes. `n` must not exceed
    /// `buffered().len()`.
    fn toss(&mut self, n: usize);

    /// Pull at least one more byte into the buffer, or fail with
    /// `EndOfStream` / `ReadFailed`. (Zig: the `vtable.stream` indirection
    /// behind every non-buffered read.)
    fn refill(&mut self) -> Result<(), ReadError>;

    /// Consume `n` buffered bytes and return them. The caller must have
    /// ensured `n` bytes are buffered (the provided `take` does). This is the
    /// impl-specific tail of Zig's `take`, split out so the borrow of the
    /// returned slice can outlive the seek advance.
    fn take_buffered(&mut self, n: usize) -> &[u8];

    /// `reader.bufferedLen()`
    fn buffered_len(&self) -> usize {
        self.buffered().len()
    }

    /// How many bytes this reader can still supply, when it knows.
    ///
    /// `None` means "no bound available", which is the honest answer for a
    /// source that streams from a callback. A decoder must therefore treat
    /// `None` as "cannot check" and not as "nothing remains".
    ///
    /// PORT NOTE (R13, UPSTREAM-DEFECTS.md UD64): the reference has no such
    /// accessor because each decode site reaches its own bound directly —
    /// `payload.bufferedLen()` on the borrowed path and `limited.remaining`
    /// on the streaming one (`src/terminal/snapshot/record.zig`). The port's
    /// `record.Reader` flattens both paths into one type behind
    /// `&mut dyn Reader`, so the same two quantities are projected through
    /// this one method instead. Only `record::Reader` overrides it; that is
    /// the only reader whose bound is a record payload.
    fn payload_remaining(&self) -> Option<u64> {
        None
    }

    /// `reader.readSliceAll(dest)` — fill `dest` exactly or fail with
    /// `error.EndOfStream` (bytes may have been consumed).
    fn read_slice_all(&mut self, dest: &mut [u8]) -> Result<(), ReadError> {
        let mut filled: usize = 0;
        while filled < dest.len() {
            let avail = self.buffered();
            if avail.is_empty() {
                self.refill()?;
                continue;
            }
            let n = avail.len().min(dest.len() - filled);
            dest[filled..filled + n].copy_from_slice(&avail[..n]);
            self.toss(n);
            filled += n;
        }
        Ok(())
    }

    /// `reader.take(n)` — consume and return `n` contiguous bytes. As in the
    /// reference, the reader's buffer must be able to hold `n` bytes.
    fn take(&mut self, n: usize) -> Result<&[u8], ReadError> {
        while self.buffered_len() < n {
            self.refill()?;
        }
        Ok(self.take_buffered(n))
    }

    /// `reader.takeByte()`
    fn take_byte(&mut self) -> Result<u8, ReadError> {
        Ok(self.take(1)?[0])
    }

    /// `reader.peekByte()`
    fn peek_byte(&mut self) -> Result<u8, ReadError> {
        while self.buffered_len() < 1 {
            self.refill()?;
        }
        Ok(self.buffered()[0])
    }

    /// `reader.discardAll(n)`
    fn discard_all(&mut self, n: usize) -> Result<(), ReadError> {
        let mut left = n;
        while left > 0 {
            let avail = self.buffered_len();
            if avail == 0 {
                self.refill()?;
                continue;
            }
            let step = avail.min(left);
            self.toss(step);
            left -= step;
        }
        Ok(())
    }

    /// `reader.discardRemaining()` — discard until end-of-stream, returning
    /// how many bytes were discarded. End-of-stream is the termination
    /// condition, not an error; only `ReadFailed` fails.
    fn discard_remaining(&mut self) -> Result<usize, ReadError> {
        let mut discarded: usize = 0;
        loop {
            let avail = self.buffered_len();
            if avail > 0 {
                self.toss(avail);
                discarded += avail;
                continue;
            }
            match self.refill() {
                Ok(()) => continue,
                Err(ReadError::EndOfStream) => return Ok(discarded),
                Err(err @ ReadError::ReadFailed) => return Err(err),
            }
        }
    }

    /// Move up to `dest.len()` buffered-or-refilled bytes into `dest`,
    /// returning how many were moved. (Zig: `Reader.stream`, the primitive
    /// adapters pull through.)
    fn stream_to(&mut self, dest: &mut [u8]) -> Result<usize, ReadError> {
        if self.buffered().is_empty() {
            self.refill()?;
        }
        let avail = self.buffered();
        let n = avail.len().min(dest.len());
        dest[..n].copy_from_slice(&avail[..n]);
        self.toss(n);
        Ok(n)
    }
}

/// `std.Io.Writer.fixed(&buf)` — a writer over a caller-supplied byte buffer
/// that fails with `error.WriteFailed` on overflow.
///
/// (Distinct from the crate-root `fixed_writer::FixedWriter`, which is the
/// `core::fmt::Write` stand-in for text encoders; this one is binary.)
pub struct FixedWriter<'a> {
    buf: &'a mut [u8],
    /// Number of bytes written so far. (Zig: `std.Io.Writer.end`.)
    pub end: usize,
}

impl<'a> FixedWriter<'a> {
    pub fn new(buf: &'a mut [u8]) -> FixedWriter<'a> {
        FixedWriter { buf, end: 0 }
    }

    /// `writer.buffered()`
    // PORT NOTE: consumed by tests today; the benchmark/C-API tiers are its
    // non-test consumers (see mod.rs).
    #[allow(dead_code)]
    pub fn buffered(&self) -> &[u8] {
        &self.buf[..self.end]
    }
}

impl Writer for FixedWriter<'_> {
    fn write_all(&mut self, bytes: &[u8]) -> Result<(), WriteFailed> {
        let next = match self.end.checked_add(bytes.len()) {
            Some(v) => v,
            None => return Err(WriteFailed),
        };
        if next > self.buf.len() {
            return Err(WriteFailed);
        }
        self.buf[self.end..next].copy_from_slice(bytes);
        self.end = next;
        Ok(())
    }
}

/// `std.Io.Writer.Allocating` — a growable in-memory destination.
///
/// PORT NOTE: the reference takes an allocator and can fail with
/// `error.WriteFailed` on OOM; the port uses the global allocator (the
/// crate-wide treatment of plain `Allocator` parameters), so writes are
/// infallible.
pub struct AllocatingWriter {
    data: Vec<u8>,
}

/// Purely additive: the reference has no `Default` analog (its `.init(alloc)`
/// takes an allocator), but `new()` with no arguments is what clippy wants a
/// `Default` beside, and a caller reaching for `Default` should get the same
/// empty writer `new()` builds.
impl Default for AllocatingWriter {
    fn default() -> AllocatingWriter {
        AllocatingWriter::new()
    }
}

impl AllocatingWriter {
    /// `.init(alloc)`
    pub fn new() -> AllocatingWriter {
        AllocatingWriter { data: Vec::new() }
    }

    /// `allocating.written()`
    pub fn written(&self) -> &[u8] {
        &self.data
    }

    /// `allocating.writer.end`
    pub fn end(&self) -> usize {
        self.data.len()
    }

    /// `allocating.shrinkRetainingCapacity(n)`
    pub fn shrink_retaining_capacity(&mut self, n: usize) {
        self.data.truncate(n);
    }

    /// `allocating.writer.buffer.len` — the retained allocation size.
    // PORT NOTE: consumed by tests today (see the FixedWriter::buffered note).
    #[allow(dead_code)]
    pub fn capacity(&self) -> usize {
        self.data.capacity()
    }

    // deinit: the Vec frees on drop.
}

impl Writer for AllocatingWriter {
    fn write_all(&mut self, bytes: &[u8]) -> Result<(), WriteFailed> {
        self.data.extend_from_slice(bytes);
        Ok(())
    }
}

/// `std.Io.Writer.Allocating` over `std.heap.stackFallback(N, alloc)`: a
/// growable byte writer whose first `N` bytes live inline (on the stack of
/// whoever owns it) and which falls back to the heap only when a payload
/// outgrows that buffer.
///
/// PORT NOTE (`ee8095d37`): the reference builds `record.Writer`'s scratch on
/// `std.heap.stackFallback(512, alloc).get()` — an allocator whose first
/// allocation is served from a 512-byte stack buffer and whose later (larger)
/// allocations come from `alloc`. Rust has no swappable allocator behind
/// `Vec`, so the observable property — no heap allocation for records that
/// fit in 512 bytes, heap growth beyond — is reproduced by this writer with
/// an inline `[u8; N]` plus a spill `Vec`. Once spilled, the payload stays on
/// the heap for the life of the writer (the Zig stack-fallback allocator also
/// never moves a grown buffer back to the stack; and `cancel`'s
/// `shrinkRetainingCapacity(0)` retains the heap capacity exactly as the
/// `AllocatingWriter` scratch did).
pub struct StackFallbackWriter<const N: usize> {
    inline: [u8; N],
    len: usize,
    /// `Some` once the payload has spilled to the heap; the inline buffer is
    /// then unused.
    heap: Option<Vec<u8>>,
}

impl<const N: usize> Default for StackFallbackWriter<N> {
    fn default() -> StackFallbackWriter<N> {
        StackFallbackWriter::new()
    }
}

impl<const N: usize> StackFallbackWriter<N> {
    /// `.init(stack_alloc.get())`
    pub fn new() -> StackFallbackWriter<N> {
        StackFallbackWriter {
            inline: [0u8; N],
            len: 0,
            heap: None,
        }
    }

    /// `allocating.written()`
    pub fn written(&self) -> &[u8] {
        match &self.heap {
            Some(heap) => heap,
            None => &self.inline[..self.len],
        }
    }

    /// `allocating.writer.end`
    pub fn end(&self) -> usize {
        match &self.heap {
            Some(heap) => heap.len(),
            None => self.len,
        }
    }

    /// `allocating.shrinkRetainingCapacity(n)`
    pub fn shrink_retaining_capacity(&mut self, n: usize) {
        match &mut self.heap {
            Some(heap) => heap.truncate(n),
            None => self.len = self.len.min(n),
        }
    }

    /// `allocating.writer.buffer.len` — the retained allocation size: the
    /// inline buffer until the first spill, the heap capacity after.
    #[allow(dead_code)]
    pub fn capacity(&self) -> usize {
        match &self.heap {
            Some(heap) => heap.capacity(),
            None => N,
        }
    }

    /// True while no heap allocation has been made (test observability of
    /// the stack-fallback property).
    #[allow(dead_code)]
    pub fn on_stack(&self) -> bool {
        self.heap.is_none()
    }
}

impl<const N: usize> Writer for StackFallbackWriter<N> {
    fn write_all(&mut self, bytes: &[u8]) -> Result<(), WriteFailed> {
        if let Some(heap) = &mut self.heap {
            heap.extend_from_slice(bytes);
            return Ok(());
        }
        let new_len = self.len + bytes.len();
        if new_len <= N {
            self.inline[self.len..new_len].copy_from_slice(bytes);
            self.len = new_len;
            return Ok(());
        }
        // Spill: the stack-fallback allocator's first heap allocation copies
        // the buffered prefix and continues on the heap.
        let mut heap = Vec::with_capacity(new_len);
        heap.extend_from_slice(&self.inline[..self.len]);
        heap.extend_from_slice(bytes);
        self.heap = Some(heap);
        self.len = 0;
        Ok(())
    }
}

/// `std.Io.Reader.fixed(bytes)` — a reader whose buffer is the complete data.
// PORT NOTE: constructed by tests today; the decode drivers take
// `&mut dyn Reader`, so the benchmark/C-API tiers are its non-test
// constructors (see the FixedWriter::buffered note).
#[allow(dead_code)]
pub struct FixedReader<'a> {
    data: &'a [u8],
    /// Bytes consumed so far. (Zig: `std.Io.Reader.seek`.)
    pub seek: usize,
}

impl<'a> FixedReader<'a> {
    #[allow(dead_code)]
    pub fn new(data: &'a [u8]) -> FixedReader<'a> {
        FixedReader { data, seek: 0 }
    }
}

impl Reader for FixedReader<'_> {
    fn buffered(&self) -> &[u8] {
        &self.data[self.seek..]
    }

    fn toss(&mut self, n: usize) {
        debug_assert!(n <= self.data.len() - self.seek);
        self.seek += n;
    }

    fn refill(&mut self) -> Result<(), ReadError> {
        // The fixed reader's whole content is its buffer; there is never more.
        Err(ReadError::EndOfStream)
    }

    fn take_buffered(&mut self, n: usize) -> &[u8] {
        debug_assert!(n <= self.data.len() - self.seek);
        let start = self.seek;
        self.seek += n;
        &self.data[start..start + n]
    }
}

/// `std.Io.Limit` for `LimitedReader`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)] // `Limited` is part of the std.Io surface; tests use `Unlimited`.
pub enum Limit {
    Unlimited,
    Limited(u64),
}

/// `source.limited(limit, &buf)` — `std.Io.Reader.Limited`: a reader with its
/// own (typically tiny) buffer that pulls from an inner reader, never beyond
/// `limit`. The snapshot tests use it with a one-byte buffer to force the
/// streaming (non-borrowed) paths of every decoder.
// PORT NOTE: constructed by tests today (see the FixedReader note).
#[allow(dead_code)]
pub struct LimitedReader<'i, 'b> {
    inner: &'i mut dyn Reader,
    buf: &'b mut [u8],
    start: usize,
    end: usize,
    /// Bytes still permitted to be pulled from `inner`. (Zig: `remaining`.)
    pub remaining: Limit,
}

impl<'i, 'b> LimitedReader<'i, 'b> {
    #[allow(dead_code)]
    pub fn new(
        inner: &'i mut dyn Reader,
        limit: Limit,
        buf: &'b mut [u8],
    ) -> LimitedReader<'i, 'b> {
        LimitedReader {
            inner,
            buf,
            start: 0,
            end: 0,
            remaining: limit,
        }
    }
}

impl Reader for LimitedReader<'_, '_> {
    fn buffered(&self) -> &[u8] {
        &self.buf[self.start..self.end]
    }

    fn toss(&mut self, n: usize) {
        debug_assert!(n <= self.end - self.start);
        self.start += n;
    }

    fn refill(&mut self) -> Result<(), ReadError> {
        if self.start == self.end {
            self.start = 0;
            self.end = 0;
        } else if self.end == self.buf.len() {
            // Make room while preserving unconsumed bytes (the provided
            // `take` may need more buffered bytes than currently fit at the
            // tail). Zig instead asserts the buffer capacity in `take`.
            self.buf.copy_within(self.start..self.end, 0);
            self.end -= self.start;
            self.start = 0;
        }
        debug_assert!(
            self.end < self.buf.len(),
            "reader buffer too small for the requested read",
        );

        let space = self.buf.len() - self.end;
        let cap = match self.remaining {
            Limit::Unlimited => space,
            Limit::Limited(r) => {
                if r == 0 {
                    return Err(ReadError::EndOfStream);
                }
                space.min(usize::try_from(r).unwrap_or(usize::MAX))
            }
        };

        let n = self
            .inner
            .stream_to(&mut self.buf[self.end..self.end + cap])?;
        self.end += n;
        if let Limit::Limited(r) = self.remaining {
            self.remaining = Limit::Limited(r - n as u64);
        }
        Ok(())
    }

    fn take_buffered(&mut self, n: usize) -> &[u8] {
        debug_assert!(n <= self.end - self.start);
        let start = self.start;
        self.start += n;
        &self.buf[start..start + n]
    }
}

/// A little-endian wire integer (the `comptime T` accepted by `writeInt` /
/// `readInt`).
pub trait LeInt: Copy {
    /// `@sizeOf(T)`
    const SIZE: usize;
    fn write_le(self, buf: &mut [u8]);
    fn read_le(buf: &[u8]) -> Self;
}

macro_rules! le_int {
    ($t:ty) => {
        impl LeInt for $t {
            const SIZE: usize = core::mem::size_of::<$t>();
            fn write_le(self, buf: &mut [u8]) {
                buf.copy_from_slice(&self.to_le_bytes());
            }
            fn read_le(buf: &[u8]) -> Self {
                <$t>::from_le_bytes(buf.try_into().unwrap())
            }
        }
    };
}

le_int!(u16);
le_int!(u32);
le_int!(u64);

/// writeInt ensures we write in little-endian since that is our
/// standard form.
pub fn write_int<T: LeInt>(writer: &mut dyn Writer, value: T) -> Result<(), WriteFailed> {
    // try writer.writeInt(T, value, .little);
    let mut buf = [0u8; 8];
    value.write_le(&mut buf[..T::SIZE]);
    writer.write_all(&buf[..T::SIZE])
}

/// Read a little-endian integer.
///
/// This avoids `takeInt` because at the time of writing this the underlying
/// implementation uses `takeArray` which asserts that the reader buffer
/// is as large as T. Our approach works with caller-selected buffers that
/// may be smaller.
pub fn read_int<T: LeInt>(reader: &mut dyn Reader) -> Result<T, ReadError> {
    let mut buf = [0u8; 8];
    reader.read_slice_all(&mut buf[..T::SIZE])?;
    Ok(T::read_le(&buf[..T::SIZE]))
}

#[cfg(test)]
mod tests {
    use super::*;

    // test "integer round trip with a one-byte reader buffer"
    #[test]
    fn integer_round_trip_with_a_one_byte_reader_buffer() {
        let mut encoded = [0u8; 6];
        let mut writer = FixedWriter::new(&mut encoded);
        write_int::<u16>(&mut writer, 0x1234).unwrap();
        write_int::<u32>(&mut writer, 0x56789abc).unwrap();

        // Keep the reader buffer smaller than either integer to verify readInt
        // does not inherit std.Io.Reader.takeInt's buffer-size requirement.
        let mut source = FixedReader::new(&encoded);
        let mut buf = [0u8; 1];
        let mut limited = LimitedReader::new(&mut source, Limit::Unlimited, &mut buf);

        assert_eq!(0x1234u16, read_int::<u16>(&mut limited).unwrap());
        assert_eq!(0x56789abcu32, read_int::<u32>(&mut limited).unwrap());
    }
}
