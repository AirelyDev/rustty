//! READY and FINISH snapshot marker records.
//!
//! READY separates the renderable terminal state from the history sequences.
//! FINISH terminates one snapshot; bytes after it belong to the containing
//! transport. Both markers have empty payloads, independently framed and
//! protected by the same CRC32C as every other record.
//!
//! ## Binary Format
//!
//! READY and FINISH contain only their record headers. Their declared payload
//! length must be zero.
//!
//! PORTS: ghostty src/terminal/snapshot/checkpoint.zig (END STATE: the BLAKE3
//! digests were removed by 219173ab3 — the markers are header-only).

// const std = @import("std");
// const test_fixture = @import("fixture.zig");
use super::io;
use super::record;

/// Selects one of the two marker positions in a snapshot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Ready,
    Finish,
}

impl Kind {
    fn tag(self) -> record::Tag {
        match self {
            Kind::Ready => record::Tag::Ready,
            Kind::Finish => record::Tag::Finish,
        }
    }
}

// pub const EncodeError = record.Writer.FinishError;
pub type EncodeError = record::FinishError;

/// Append one empty marker record.
pub fn encode<W: io::Writer + ?Sized>(
    kind: Kind,
    stream: &mut record::Writer<'_, W>,
) -> Result<(), EncodeError> {
    let _ = stream.begin(kind.tag());
    // errdefer stream.cancel(); — `finish` cancels on every exit path.
    stream.finish()
}

/// `checkpoint.DecodeError`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecodeError {
    // record.Reader.InitError
    ReadFailed,
    EndOfStream,
    InvalidTag,
    // record.Reader.FinishError
    InvalidChecksum,
    PayloadNotExhausted,
    /// The next record is valid but is not the expected marker.
    UnexpectedRecordTag,
}

impl From<record::InitError> for DecodeError {
    fn from(err: record::InitError) -> DecodeError {
        match err {
            record::HeaderDecodeError::ReadFailed => DecodeError::ReadFailed,
            record::HeaderDecodeError::EndOfStream => DecodeError::EndOfStream,
            record::HeaderDecodeError::InvalidTag => DecodeError::InvalidTag,
        }
    }
}

impl From<record::ReaderFinishError> for DecodeError {
    fn from(err: record::ReaderFinishError) -> DecodeError {
        match err {
            record::ReaderFinishError::InvalidChecksum => DecodeError::InvalidChecksum,
            record::ReaderFinishError::PayloadNotExhausted => DecodeError::PayloadNotExhausted,
        }
    }
}

/// Decode one marker and require its payload to be empty.
pub fn decode(kind: Kind, source: &mut dyn io::Reader) -> Result<(), DecodeError> {
    let mut record_reader = record::Reader::init(source)?;
    if record_reader.header.tag != kind.tag() {
        return Err(DecodeError::UnexpectedRecordTag);
    }
    record_reader.finish()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::fixture;
    use super::*;
    use crate::snapshot::io::{Reader as _, Writer as _};

    // const test_ready_fixture = test_fixture.parse(
    //     @embedFile("testdata/checkpoint-ready-v1.hex"),
    // );
    fn test_ready_fixture() -> Vec<u8> {
        fixture::parse(include_str!("testdata/checkpoint-ready-v1.hex"))
    }

    // test "READY golden encoding"
    #[test]
    fn ready_golden_encoding() {
        let test_ready_fixture = test_ready_fixture();
        let prefix: &[u8] = b"abc";

        let mut snapshot = io::AllocatingWriter::new();
        let mut stream = record::Writer::init(&mut snapshot);
        stream.writer().write_all(prefix).unwrap();
        encode(Kind::Ready, &mut stream).unwrap();

        fixture::expect_equal(
            fixture::Kind::Bytes,
            "src/snapshot/testdata/checkpoint-ready-v1.hex",
            "snapshot_fixture-checkpoint-ready-v1.hex",
            &test_ready_fixture,
            stream.destination().written(),
        );
        stream.deinit();

        // Decode the checked-in record rather than the generated candidate.
        let mut source = io::FixedReader::new(&test_ready_fixture);
        source.discard_all(prefix.len()).unwrap();
        decode(Kind::Ready, &mut source).unwrap();
    }

    // test "READY and FINISH are empty marker records"
    #[test]
    fn ready_and_finish_are_empty_marker_records() {
        let mut snapshot = io::AllocatingWriter::new();
        let mut stream = record::Writer::init(&mut snapshot);
        stream.writer().write_all(b"prefix").unwrap();

        let ready_offset = stream.destination().written().len();
        encode(Kind::Ready, &mut stream).unwrap();
        stream.writer().write_all(b"history").unwrap();
        let finish_offset = stream.destination().written().len();
        encode(Kind::Finish, &mut stream).unwrap();
        stream.deinit();

        assert_eq!(
            record::Header::LEN,
            finish_offset - ready_offset - "history".len(),
        );
        assert_eq!(record::Header::LEN, snapshot.written().len() - finish_offset);

        let mut source = io::FixedReader::new(snapshot.written());
        source.discard_all(ready_offset).unwrap();
        decode(Kind::Ready, &mut source).unwrap();
        source.discard_all("history".len()).unwrap();
        decode(Kind::Finish, &mut source).unwrap();
    }

    // test "checkpoint rejects wrong tags and nonempty payloads"
    #[test]
    fn checkpoint_rejects_wrong_tags_and_nonempty_payloads() {
        let mut snapshot = io::AllocatingWriter::new();
        let mut stream = record::Writer::init(&mut snapshot);
        encode(Kind::Ready, &mut stream).unwrap();
        stream.deinit();

        let mut wrong_tag_source = io::FixedReader::new(snapshot.written());
        assert_eq!(
            Err(DecodeError::UnexpectedRecordTag),
            decode(Kind::Finish, &mut wrong_tag_source),
        );

        let mut invalid = io::AllocatingWriter::new();
        let mut invalid_stream = record::Writer::init(&mut invalid);
        let invalid_payload = invalid_stream.begin(record::Tag::Ready);
        invalid_payload.write_byte(0).unwrap();
        invalid_stream.finish().unwrap();
        invalid_stream.deinit();

        let mut invalid_source = io::FixedReader::new(invalid.written());
        assert_eq!(
            Err(DecodeError::PayloadNotExhausted),
            decode(Kind::Ready, &mut invalid_source),
        );
    }

    // test "FINISH leaves continuation bytes unread"
    #[test]
    fn finish_leaves_continuation_bytes_unread() {
        let mut finished = io::AllocatingWriter::new();
        let mut finished_stream = record::Writer::init(&mut finished);
        encode(Kind::Finish, &mut finished_stream).unwrap();
        finished_stream.deinit();
        finished.write_byte(0).unwrap();

        let mut source = io::FixedReader::new(finished.written());
        decode(Kind::Finish, &mut source).unwrap();
        assert_eq!(0u8, source.take_byte().unwrap());
    }
}
