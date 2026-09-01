//! Snapshot record framing.
//!
//! Records begin immediately after the single snapshot envelope and continue
//! back-to-back until FINISH. Each record has one fixed-size header followed
//! by exactly `payload_len` bytes. The payload length limits the record reader
//! so malformed payloads cannot consume bytes from the following record.
//!
//! The CRC covers the first six header bytes (tag and payload length)
//! followed by the payload. The CRC field itself is not included for obvious
//! reasons. All integers are unsigned and little-endian.
//!
//! | Offset | Size          | Field                  |
//! | -----: | ------------: | :--------------------- |
//! |      0 |             2 | Tag (`u16`)            |
//! |      2 |             4 | Payload length (`u32`) |
//! |      6 |             4 | CRC32C (`u32`)         |
//! |     10 | `payload_len` | Payload                |
//!
//! Supported tags are in `Tag`.
//!
//! PORTS: ghostty src/terminal/snapshot/record.zig

// const std = @import("std");
// const assert = std.debug.assert;
// const Allocator = std.mem.Allocator;
// const test_fixture = @import("fixture.zig");
use super::io;

/// CRC32C as specified by the snapshot format. This is the parameter set
/// Zig's standard library names after its iSCSI use, backed by dedicated
/// CRC32C instructions where the target has them.
// pub const Crc32c = @import("../../crc32c.zig").Crc32c;
pub use rustty_lib::crc32c::Crc32c;

/// Identifies the layout and meaning of a record payload.
/// The current snapshot version rejects every value not listed here.
#[repr(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tag {
    /// Terminal-wide live state and configuration.
    Terminal = 1,

    /// One live screen and its page sequence.
    Screen = 2,

    /// One complete logical terminal page.
    Page = 3,

    /// One screen's complete history manifest and page sequence.
    History = 4,

    /// Marker separating the renderable terminal state from history.
    Ready = 5,

    /// Marker terminating the complete snapshot blob.
    Finish = 6,

    /// Canonical unfinished standard TerminalStream input.
    Continuation = 7,
}

impl Tag {
    /// `std.enums.fromInt(Tag, raw)`
    pub fn from_int(raw: u16) -> Option<Tag> {
        Some(match raw {
            1 => Tag::Terminal,
            2 => Tag::Screen,
            3 => Tag::Page,
            4 => Tag::History,
            5 => Tag::Ready,
            6 => Tag::Finish,
            7 => Tag::Continuation,
            _ => return None,
        })
    }

    /// `@tagName(tag)` — the reference (lowercase) tag name.
    pub fn name(self) -> &'static str {
        match self {
            Tag::Terminal => "terminal",
            Tag::Screen => "screen",
            Tag::Page => "page",
            Tag::History => "history",
            Tag::Ready => "ready",
            Tag::Finish => "finish",
            Tag::Continuation => "continuation",
        }
    }
}

/// Errors possible while decoding a record header. (`Header.DecodeError`.)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeaderDecodeError {
    // std.Io.Reader.Error
    ReadFailed,
    EndOfStream,
    InvalidTag,
}

impl From<io::ReadError> for HeaderDecodeError {
    fn from(err: io::ReadError) -> HeaderDecodeError {
        match err {
            io::ReadError::ReadFailed => HeaderDecodeError::ReadFailed,
            io::ReadError::EndOfStream => HeaderDecodeError::EndOfStream,
        }
    }
}

/// The fixed framing that precedes every record payload.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Header {
    /// Determines how the payload is decoded.
    pub tag: Tag,

    /// Number of payload bytes immediately following this header.
    pub payload_len: u32,

    /// CRC32C over the encoded tag, payload length, and payload.
    pub crc32c: u32,
}

impl Header {
    /// Number of bytes written by `encode`.
    ///
    /// PORT NOTE: the reference calculates this at comptime using the encoder
    /// itself (`computeLen`) and `comptime`-asserts it is 10 ("this size is
    /// part of the wire format"). The port pins the constant; the
    /// `header_len_matches_encoder` test carries both the synchronization and
    /// the assert.
    pub const LEN: usize = 10;

    /// Encode the fixed record header.
    pub fn encode(self, writer: &mut dyn io::Writer) -> Result<(), io::WriteFailed> {
        encode_checksum_prefix(self.tag, self.payload_len, writer)?;
        io::write_int::<u32>(writer, self.crc32c)
    }

    /// Decode a fixed record header and reject unknown tags.
    /// Payload length and CRC validation occur while decoding the payload.
    pub fn decode(reader: &mut dyn io::Reader) -> Result<Header, HeaderDecodeError> {
        let tag_raw = io::read_int::<u16>(reader)?;
        let tag = Tag::from_int(tag_raw).ok_or(HeaderDecodeError::InvalidTag)?;

        Ok(Header {
            tag,
            payload_len: io::read_int::<u32>(reader)?,
            crc32c: io::read_int::<u32>(reader)?,
        })
    }

    /// Computes the required header length (the reference comptime
    /// `computeLen`; see `LEN`).
    #[cfg(test)]
    fn compute_len() -> usize {
        let mut buf = [0u8; 128];
        let mut writer = io::FixedWriter::new(&mut buf);
        let header = Header {
            tag: Tag::Terminal,
            payload_len: 0,
            crc32c: 0,
        };
        header.encode(&mut writer).unwrap();
        writer.end
    }
}

/// A writer that calculates a record checksum without retaining payload bytes.
/// The checksum is initialized with the encoded tag and payload length.
///
/// PORT NOTE: the reference wraps `std.Io.Writer.Hashing(Crc32c)`; the port
/// holds the hasher directly and implements the `io::Writer` vtable itself
/// (same observable behavior; `final` needs no flush because nothing buffers).
pub struct Checksum {
    pub hasher: Crc32c,
}

impl Checksum {
    /// Begin a checksum with the encoded tag and payload length already added.
    pub fn init(tag: Tag, payload_len: u32) -> Checksum {
        let mut result = Checksum {
            hasher: Crc32c::init(),
        };
        encode_checksum_prefix(tag, payload_len, &mut result).unwrap();
        result
    }

    /// Return the writer through which the complete payload must be encoded.
    pub fn writer(&mut self) -> &mut dyn io::Writer {
        self
    }

    /// Finish and return the checksum stored in the record header.
    pub fn r#final(&mut self) -> u32 {
        // self.hashing.writer.flush() catch unreachable; — nothing buffers.
        self.hasher.r#final()
    }
}

impl io::Writer for Checksum {
    fn write_all(&mut self, bytes: &[u8]) -> Result<(), io::WriteFailed> {
        self.hasher.update(bytes);
        Ok(())
    }
}

/// `record.Writer.FinishError`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FinishError {
    // std.Io.Writer.Error
    WriteFailed,
    /// The payload cannot be represented by the record's `u32` length.
    PayloadTooLarge,
}

impl From<io::WriteFailed> for FinishError {
    fn from(_: io::WriteFailed) -> FinishError {
        FinishError::WriteFailed
    }
}

/// Streams complete records while retaining only one payload at a time.
///
/// The scratch allocation is retained between records so a stream's peak
/// memory is the largest record payload rather than the complete snapshot.
///
/// PORT NOTE: the reference takes an `Allocator` for the scratch buffer; the
/// port uses the global allocator (the crate-wide treatment) behind a
/// stack-fallback scratch (see [`RECORD_SCRATCH_STACK_BYTES`]). The destination
/// is generic (`W`) rather than `*std.Io.Writer` so tests can read a concrete
/// `AllocatingWriter` destination back through `destination()` while this
/// writer borrows it (the reference reads the destination directly; Rust
/// borrow discipline routes those reads through the borrow holder).
pub struct Writer<'d, W: io::Writer + ?Sized> {
    destination: &'d mut W,
    scratch: io::StackFallbackWriter<RECORD_SCRATCH_STACK_BYTES>,
    active_tag: Option<Tag>,
}

/// The record scratch's stack-resident size.
///
/// PORT NOTE (`ee8095d37`, `snapshot.zig` `encode`): "A measured 512-byte
/// buffer handles our smallest records on the stack before larger records
/// fall back to the heap." The reference wraps the caller's allocator in
/// `std.heap.stackFallback(512, alloc)` at the `snapshot.encode` call site
/// only; the port's `Writer` has no allocator parameter to wrap, so the
/// fallback is baked into every `Writer`'s scratch here (the other `Writer`
/// constructors are tests and the C-API tier, where the only difference is
/// where the first 512 bytes of a payload live).
pub const RECORD_SCRATCH_STACK_BYTES: usize = 512;

impl<'d, W: io::Writer + ?Sized> Writer<'d, W> {
    pub fn init(destination: &'d mut W) -> Writer<'d, W> {
        Writer {
            destination,
            scratch: io::StackFallbackWriter::new(),
            active_tag: None,
        }
    }

    pub fn deinit(self) {
        assert!(self.active_tag.is_none());
        assert!(self.scratch.end() == 0);
        // self.* = undefined; — dropped.
    }

    /// Return the destination writer for unframed snapshot bytes.
    pub fn writer(&mut self) -> &mut W {
        assert!(self.active_tag.is_none());
        self.destination
    }

    /// Read access to the destination (see the struct PORT NOTE).
    pub fn destination(&self) -> &W {
        self.destination
    }

    /// Begin one record and return its reusable payload writer.
    pub fn begin(&mut self, tag: Tag) -> &mut dyn io::Writer {
        assert!(self.active_tag.is_none());
        assert!(self.scratch.end() == 0);
        self.active_tag = Some(tag);
        &mut self.scratch
    }

    /// Finish and emit the active record's header followed by its payload.
    ///
    /// Payload validation failures occur before this function and emit no part
    /// of the record. A destination failure here may have emitted a prefix.
    pub fn finish(&mut self) -> Result<(), FinishError> {
        assert!(self.active_tag.is_some());
        let tag = self.active_tag.unwrap();
        // defer self.cancel();
        let result = self.finish_inner(tag);
        self.cancel();
        result
    }

    fn finish_inner(&mut self, tag: Tag) -> Result<(), FinishError> {
        let payload_len = u32::try_from(self.scratch.written().len())
            .map_err(|_| FinishError::PayloadTooLarge)?;

        // The CRC prefix contains the payload length, so checksum the retained
        // payload only after its final length is known.
        let mut checksum = Checksum::init(tag, payload_len);
        checksum.writer().write_all(self.scratch.written()).unwrap();

        let header = Header {
            tag,
            payload_len,
            crc32c: checksum.r#final(),
        };
        let mut header_bytes = [0u8; Header::LEN];
        let mut header_writer = io::FixedWriter::new(&mut header_bytes);
        header.encode(&mut header_writer).unwrap();

        self.destination.write_all(&header_bytes)?;
        self.destination.write_all(self.scratch.written())?;
        Ok(())
    }

    /// Discard the active record without emitting any bytes.
    ///
    /// This is idempotent so an outer error cleanup may call it after `finish`
    /// has already cleared state following a destination failure.
    pub fn cancel(&mut self) {
        self.scratch.shrink_retaining_capacity(0);
        self.active_tag = None;
    }
}

/// `record.Reader.InitError`.
pub type InitError = HeaderDecodeError;

/// Errors detected after a payload decoder returns. (`record.Reader.FinishError`.)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReaderFinishError {
    /// The payload bytes do not match the CRC in the record header.
    InvalidChecksum,

    /// The decoder did not consume exactly `payload_len` bytes.
    PayloadNotExhausted,
}

/// When the source already has the complete payload buffered, for
/// example an in-memory snapshot, the payload is borrowed straight
/// from the source buffer instead of streaming through the limited
/// and hashing adapters. The checksum is then verified with one bulk
/// update in `finish`, and the source is not advanced until `finish`.
///
/// PORT NOTE: the reference stores a `Reader.fixed` view aliasing the source
/// buffer plus the source pointer; the port stores only the window cursor and
/// re-derives the byte view from `source.buffered()` on demand, so the alias
/// is a plain reborrow instead of a stored raw pointer (no `*` alias to prove;
/// the reference's "self must remain at a stable address" contract becomes
/// the borrow checker's).
#[derive(Clone, Copy)]
struct Borrowed {
    /// The borrowed payload window is `source.buffered()[seek..len]`.
    len: usize,
    seek: usize,
}

/// Reads one complete record.
///
/// `init` decodes the header, `payload_reader` returns a reader limited to the
/// declared payload, and `finish` verifies exact exhaustion and the CRC32C.
///
/// PORT NOTE (flattened adapters): the reference nests
/// `std.Io.Reader.Limited` (1-byte buffer) feeding `std.Io.Reader.Hashed`
/// (256-byte buffer), which requires the record reader to sit at a stable
/// address (`@fieldParentPtr`-style self-reference). Rust cannot express that
/// self-borrow, so the streaming path is flattened into this type: one
/// 256-byte buffer (`hashing_buffer`), one `remaining` payload limit, and one
/// hasher updated as bytes enter the buffer. Observable behavior — payload
/// limiting, EndOfStream on truncation, exhaustion and CRC checks in `finish`
/// — is identical; the carried upstream tests cover both paths.
pub struct Reader<'s> {
    pub header: Header,

    source: &'s mut dyn io::Reader,
    borrowed: Option<Borrowed>,

    // Flattened streaming state (see the PORT NOTE above). Fixed-header
    // decoding performs several small reads. 256 bytes batches them while
    // CRC32C is calculated without making Reader large on the stack. (Zig:
    // `hashing_buffer`.)
    hasher: Crc32c,
    remaining: u64,
    hashing_buffer: [u8; 256],
    hstart: usize,
    hend: usize,
}

impl<'s> Reader<'s> {
    /// Decode a record header and initialize its payload reader.
    ///
    /// Decode the payload only through the reader returned by
    /// `payload_reader`; do not read `source` elsewhere until `finish`
    /// returns.
    // PORT NOTE: the reference initializes in place (`self: *Reader` over
    // undefined memory) because of the stable-address adapters; the flattened
    // port is a plain constructor.
    pub fn init(source: &'s mut dyn io::Reader) -> Result<Reader<'s>, InitError> {
        let header = Header::decode(source)?;

        // The complete payload is already sitting in the source buffer:
        // borrow it in place. Payload decoders read from a fixed reader
        // over the borrowed bytes and `finish` checksums them in one pass.
        if source.buffered_len() >= header.payload_len as usize {
            return Ok(Reader {
                header,
                source,
                borrowed: Some(Borrowed {
                    len: header.payload_len as usize,
                    seek: 0,
                }),
                hasher: Crc32c::init(),
                remaining: 0,
                hashing_buffer: [0; 256],
                hstart: 0,
                hend: 0,
            });
        }

        let checksum = Checksum::init(header.tag, header.payload_len);
        Ok(Reader {
            header,
            source,
            borrowed: None,
            hasher: checksum.hasher,
            remaining: header.payload_len as u64,
            hashing_buffer: [0; 256],
            hstart: 0,
            hend: 0,
        })
    }

    /// Return the length-limited, checksum-updating payload reader.
    pub fn payload_reader(&mut self) -> &mut dyn io::Reader {
        self
    }

    /// Require exact payload exhaustion and validate its CRC32C.
    pub fn finish(&mut self) -> Result<(), ReaderFinishError> {
        if let Some(borrowed) = self.borrowed {
            if borrowed.len - borrowed.seek != 0 {
                return Err(ReaderFinishError::PayloadNotExhausted);
            }

            let mut checksum = Checksum::init(self.header.tag, self.header.payload_len);
            checksum
                .writer()
                .write_all(&self.source.buffered()[..borrowed.len])
                .unwrap();
            if checksum.r#final() != self.header.crc32c {
                return Err(ReaderFinishError::InvalidChecksum);
            }

            // The borrowed bytes validated, so consume them from the
            // source only now, leaving it positioned at the next record.
            self.source.toss(self.header.payload_len as usize);
            return Ok(());
        }

        if self.hend - self.hstart != 0 || self.remaining != 0 {
            return Err(ReaderFinishError::PayloadNotExhausted);
        }

        if self.hasher.r#final() != self.header.crc32c {
            return Err(ReaderFinishError::InvalidChecksum);
        }
        Ok(())
    }
}

impl io::Reader for Reader<'_> {
    fn buffered(&self) -> &[u8] {
        match &self.borrowed {
            Some(b) => &self.source.buffered()[b.seek..b.len],
            None => &self.hashing_buffer[self.hstart..self.hend],
        }
    }

    fn toss(&mut self, n: usize) {
        match self.borrowed.as_mut() {
            Some(b) => {
                debug_assert!(n <= b.len - b.seek);
                b.seek += n;
            }
            None => {
                debug_assert!(n <= self.hend - self.hstart);
                self.hstart += n;
            }
        }
    }

    fn refill(&mut self) -> Result<(), io::ReadError> {
        if self.borrowed.is_some() {
            // The borrowed window is the complete payload; there is never
            // more. (Zig: the fixed payload view's ending stream.)
            return Err(io::ReadError::EndOfStream);
        }

        if self.hstart == self.hend {
            self.hstart = 0;
            self.hend = 0;
        } else if self.hend == self.hashing_buffer.len() {
            self.hashing_buffer.copy_within(self.hstart..self.hend, 0);
            self.hend -= self.hstart;
            self.hstart = 0;
        }
        debug_assert!(self.hend < self.hashing_buffer.len());

        if self.remaining == 0 {
            // The payload limit is exhausted: reads past it fail without
            // consuming bytes from the following record. (Zig: Limited.)
            return Err(io::ReadError::EndOfStream);
        }
        let space = self.hashing_buffer.len() - self.hend;
        let cap = space.min(usize::try_from(self.remaining).unwrap_or(usize::MAX));

        let n = self
            .source
            .stream_to(&mut self.hashing_buffer[self.hend..self.hend + cap])?;
        // Bytes are hashed exactly once, as they enter the payload buffer.
        // (Zig: Hashed.)
        self.hasher
            .update(&self.hashing_buffer[self.hend..self.hend + n]);
        self.remaining -= n as u64;
        self.hend += n;
        Ok(())
    }

    fn take_buffered(&mut self, n: usize) -> &[u8] {
        if let Some(b) = self.borrowed.as_mut() {
            debug_assert!(n <= b.len - b.seek);
            let start = b.seek;
            b.seek += n;
            return &self.source.buffered()[start..start + n];
        }
        debug_assert!(n <= self.hend - self.hstart);
        let start = self.hstart;
        self.hstart += n;
        &self.hashing_buffer[start..start + n]
    }
}

/// Encode the portion of a record header covered by CRC32C.
fn encode_checksum_prefix(
    tag: Tag,
    payload_len: u32,
    writer: &mut dyn io::Writer,
) -> Result<(), io::WriteFailed> {
    io::write_int::<u16>(writer, tag as u16)?;
    io::write_int::<u32>(writer, payload_len)
}

#[cfg(test)]
mod tests {
    use super::super::fixture;
    use super::*;
    use crate::snapshot::io::{Reader as _, Writer as _};

    // const test_page_header_fixture = test_fixture.parse(
    //     @embedFile("testdata/record-page-header-v1.hex"),
    // );
    fn test_page_header_fixture() -> Vec<u8> {
        fixture::parse(include_str!("testdata/record-page-header-v1.hex"))
    }

    // comptime { assert(len == 10); }
    #[test]
    fn header_len_matches_encoder() {
        assert_eq!(Header::LEN, Header::compute_len());
        assert_eq!(Header::LEN, 10);
    }

    // test "golden PAGE record header and checksum"
    #[test]
    fn golden_page_record_header_and_checksum() {
        let test_page_header_fixture = test_page_header_fixture();

        let page_header: &[u8] = b"\x50\x00\x18\x00\x00\x00\x00\x00\
              \x00\x00\x00\x00\x00\x00\x00\x00\
              \x00\x00\x00\x00\x00\x00\x00\x00";

        let mut checksum = Checksum::init(Tag::Page, page_header.len() as u32);
        checksum.writer().write_all(page_header).unwrap();
        assert_eq!(0x7178441bu32, checksum.r#final());

        let header = Header {
            tag: Tag::Page,
            payload_len: page_header.len() as u32,
            crc32c: 0x7178441b,
        };
        let mut buf = [0u8; Header::LEN];
        let mut writer = io::FixedWriter::new(&mut buf);
        header.encode(&mut writer).unwrap();

        fixture::expect_equal(
            fixture::Kind::Bytes,
            "src/snapshot/testdata/record-page-header-v1.hex",
            "snapshot_fixture-record-page-header-v1.hex",
            &test_page_header_fixture,
            writer.buffered(),
        );

        let mut reader = io::FixedReader::new(&test_page_header_fixture);
        assert_eq!(header, Header::decode(&mut reader).unwrap());
    }

    // test "reject invalid tags"
    #[test]
    fn reject_invalid_tags() {
        for tag in [0u16, 8, u16::MAX] {
            let mut fixture_bytes = [0u8; Header::LEN];
            fixture_bytes[0..2].copy_from_slice(&tag.to_le_bytes());
            let mut reader = io::FixedReader::new(&fixture_bytes);
            assert_eq!(
                Err(HeaderDecodeError::InvalidTag),
                Header::decode(&mut reader),
            );
        }
    }

    // test "reject every header truncation"
    #[test]
    fn reject_every_header_truncation() {
        let test_page_header_fixture = test_page_header_fixture();
        for len in 0..Header::LEN {
            let mut reader = io::FixedReader::new(&test_page_header_fixture[..len]);
            assert_eq!(
                Err(HeaderDecodeError::EndOfStream),
                Header::decode(&mut reader),
            );
        }
    }

    // test "record reader verifies exhaustion and checksum"
    #[test]
    fn record_reader_verifies_exhaustion_and_checksum() {
        let payload: &[u8] = b"snapshot payload";
        let mut checksum = Checksum::init(Tag::Page, payload.len() as u32);
        checksum.writer().write_all(payload).unwrap();
        let header = Header {
            tag: Tag::Page,
            payload_len: payload.len() as u32,
            crc32c: checksum.r#final(),
        };

        let mut encoded = [0u8; Header::LEN + 16 + 4];
        let mut writer = io::FixedWriter::new(&mut encoded);
        header.encode(&mut writer).unwrap();
        writer.write_all(payload).unwrap();
        writer.write_all(b"next").unwrap();

        let buffered = writer.buffered().to_vec();
        let mut source = io::FixedReader::new(&buffered);
        let mut record_reader = Reader::init(&mut source).unwrap();
        let mut decoded = [0u8; 16];
        record_reader
            .payload_reader()
            .read_slice_all(&mut decoded)
            .unwrap();
        record_reader.finish().unwrap();
        assert_eq!(payload, &decoded);
        assert_eq!(b"next", source.take(4).unwrap());
    }

    // test "record reader rejects remaining bytes and invalid checksum"
    #[test]
    fn record_reader_rejects_remaining_bytes_and_invalid_checksum() {
        let payload: &[u8] = b"payload";
        let header = Header {
            tag: Tag::Page,
            payload_len: payload.len() as u32,
            crc32c: 0,
        };

        let mut encoded = [0u8; Header::LEN + 7];
        let mut writer = io::FixedWriter::new(&mut encoded);
        header.encode(&mut writer).unwrap();
        writer.write_all(payload).unwrap();
        let buffered = writer.buffered().to_vec();

        {
            let mut source = io::FixedReader::new(&buffered);
            let mut record_reader = Reader::init(&mut source).unwrap();
            let _ = record_reader.payload_reader().take_byte().unwrap();
            assert_eq!(
                Err(ReaderFinishError::PayloadNotExhausted),
                record_reader.finish(),
            );
        }

        {
            let mut source = io::FixedReader::new(&buffered);
            let mut record_reader = Reader::init(&mut source).unwrap();
            record_reader
                .payload_reader()
                .discard_all(payload.len())
                .unwrap();
            assert_eq!(
                Err(ReaderFinishError::InvalidChecksum),
                record_reader.finish(),
            );
        }
    }

    // test "payload limit does not consume the next record"
    #[test]
    fn payload_limit_does_not_consume_the_next_record() {
        let payload: &[u8] = b"ab";
        let mut checksum = Checksum::init(Tag::Page, payload.len() as u32);
        checksum.writer().write_all(payload).unwrap();
        let header = Header {
            tag: Tag::Page,
            payload_len: payload.len() as u32,
            crc32c: checksum.r#final(),
        };

        let mut encoded = [0u8; Header::LEN + 2 + 4];
        let mut writer = io::FixedWriter::new(&mut encoded);
        header.encode(&mut writer).unwrap();
        writer.write_all(payload).unwrap();
        writer.write_all(b"next").unwrap();
        let buffered = writer.buffered().to_vec();

        let mut source = io::FixedReader::new(&buffered);
        let mut record_reader = Reader::init(&mut source).unwrap();
        let mut too_long = [0u8; 3];
        assert_eq!(
            Err(io::ReadError::EndOfStream),
            record_reader.payload_reader().read_slice_all(&mut too_long),
        );
        record_reader.finish().unwrap();
        assert_eq!(b"next", source.take(4).unwrap());
    }

    // test "record writer buffers a payload and appends framing"
    #[test]
    fn record_writer_buffers_a_payload_and_appends_framing() {
        let mut destination = io::AllocatingWriter::new();
        destination.write_all(b"prefix").unwrap();
        let mut stream = Writer::init(&mut destination);

        let payload = stream.begin(Tag::Page);
        payload.write_all(b"payload").unwrap();
        stream.finish().unwrap();
        stream.deinit();

        let encoded = destination.written();
        assert_eq!(b"prefix", &encoded[0..6]);

        let mut source = io::FixedReader::new(&encoded[6..]);
        let mut record_reader = Reader::init(&mut source).unwrap();
        assert_eq!(Tag::Page, record_reader.header.tag);
        assert_eq!(7u32, record_reader.header.payload_len);
        assert_eq!(b"payload", record_reader.payload_reader().take(7).unwrap());
        record_reader.finish().unwrap();
    }

    // test "record writer cancel preserves preceding bytes"
    #[test]
    fn record_writer_cancel_preserves_preceding_bytes() {
        let mut destination = io::AllocatingWriter::new();
        destination.write_all(b"prefix").unwrap();
        let mut stream = Writer::init(&mut destination);

        let partial = stream.begin(Tag::Page);
        partial.write_all(b"partial").unwrap();
        let scratch_capacity = stream.scratch.capacity();
        stream.cancel();

        assert_eq!(b"prefix", stream.destination().written());
        assert_eq!(scratch_capacity, stream.scratch.capacity());

        // Cancel retains the allocation but leaves it ready for the next record.
        let next = stream.begin(Tag::Page);
        next.write_all(b"next").unwrap();
        stream.finish().unwrap();
        stream.deinit();

        let written = destination.written().to_vec();
        let mut source = io::FixedReader::new(&written[6..]);
        let mut record_reader = Reader::init(&mut source).unwrap();
        assert_eq!(b"next", record_reader.payload_reader().take(4).unwrap());
        record_reader.finish().unwrap();
    }

    // test "record writer clears active state after destination failure"
    #[test]
    fn record_writer_clears_active_state_after_destination_failure() {
        // The header fits but the payload does not, leaving a permitted partial
        // record in the destination while the reusable writer becomes idle again.
        let mut destination_bytes = [0u8; Header::LEN];
        let mut destination = io::FixedWriter::new(&mut destination_bytes);
        let mut stream = Writer::init(&mut destination);

        let payload = stream.begin(Tag::Page);
        payload.write_byte(0).unwrap();
        assert_eq!(Err(FinishError::WriteFailed), stream.finish());
        assert_eq!(None, stream.active_tag);
        assert_eq!(0usize, stream.scratch.end());
        stream.deinit();
    }
}
