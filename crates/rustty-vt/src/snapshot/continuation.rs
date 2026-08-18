//! Continuation record type.
//!
//! Continuation uses the standard record header. All trailing bytes
//! are payload bytes for the continuation state.
//!
//! A continuation's payload must be the minimal bytes to go from
//! a ground state to a non-ground state in a VT emulator. Any extra
//! bytes are treated as a validation error.
//!
//! PORTS: ghostty src/terminal/snapshot/continuation.zig

// const std = @import("std");
// const Allocator = std.mem.Allocator;
// const record = @import("record.zig");
// const stream_continuation = @import("../stream_continuation.zig");
// const test_fixture = @import("fixture.zig");
use std::borrow::Cow;

use super::io;
use super::record;
use crate::stream_continuation;

/// A borrowed continuation value supplied to encode, or an allocator-owned
/// value stored in a decoded complete snapshot.
///
/// PORT NOTE: the reference `bytes: []const u8` is borrowed at encode and
/// allocator-owned after decode (ownership by context, documented on
/// `decode`). Rust expresses that duality as `Cow`: encode call sites pass
/// `Cow::Borrowed`, `decode` returns `Cow::Owned` (`Value<'static>`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value<'a> {
    /// Neither the VT parser nor UTF-8 decoder has unfinished state.
    Ground,

    /// Canonical replay-safe PTY bytes. Empty is equivalent to `ground`.
    Bytes(Cow<'a, [u8]>),
}

/// `continuation.ValidateError`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValidateError {
    // stream_continuation.ValidateError
    NoPendingState,
    NonCanonicalContinuation,
    ReplayWouldCommit,
    /// The common record's u32 payload length cannot represent these bytes.
    Overflow,
}

impl From<stream_continuation::ValidateError> for ValidateError {
    fn from(err: stream_continuation::ValidateError) -> ValidateError {
        match err {
            stream_continuation::ValidateError::NoPendingState => ValidateError::NoPendingState,
            stream_continuation::ValidateError::NonCanonicalContinuation => {
                ValidateError::NonCanonicalContinuation
            }
            stream_continuation::ValidateError::ReplayWouldCommit => {
                ValidateError::ReplayWouldCommit
            }
        }
    }
}

/// Validate a continuation before any snapshot bytes are emitted.
pub fn validate(value: &Value<'_>) -> Result<(), ValidateError> {
    match value {
        Value::Ground => Ok(()),
        Value::Bytes(bytes) => {
            if u32::try_from(bytes.len()).is_err() {
                return Err(ValidateError::Overflow);
            }
            stream_continuation::validate(bytes)?;
            Ok(())
        }
    }
}

/// `continuation.EncodeError`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EncodeError {
    // ValidateError
    NoPendingState,
    NonCanonicalContinuation,
    ReplayWouldCommit,
    Overflow,
    // record.Writer.FinishError
    WriteFailed,
    PayloadTooLarge,
}

impl From<ValidateError> for EncodeError {
    fn from(err: ValidateError) -> EncodeError {
        match err {
            ValidateError::NoPendingState => EncodeError::NoPendingState,
            ValidateError::NonCanonicalContinuation => EncodeError::NonCanonicalContinuation,
            ValidateError::ReplayWouldCommit => EncodeError::ReplayWouldCommit,
            ValidateError::Overflow => EncodeError::Overflow,
        }
    }
}

impl From<record::FinishError> for EncodeError {
    fn from(err: record::FinishError) -> EncodeError {
        match err {
            record::FinishError::WriteFailed => EncodeError::WriteFailed,
            record::FinishError::PayloadTooLarge => EncodeError::PayloadTooLarge,
        }
    }
}

/// Encode one complete CONTINUATION record.
pub fn encode<W: io::Writer + ?Sized>(
    value: &Value<'_>,
    stream: &mut record::Writer<'_, W>,
) -> Result<(), EncodeError> {
    validate(value)?;

    let payload = stream.begin(record::Tag::Continuation);
    // errdefer stream.cancel(); — `finish` cancels on every exit path, and
    // the only fallible step between `begin` and `finish` is the payload
    // write below, whose failure is cleaned up explicitly.
    match value {
        Value::Ground => {}
        Value::Bytes(bytes) => {
            if let Err(err) = payload.write_all(bytes) {
                stream.cancel();
                return Err(record::FinishError::from(err).into());
            }
        }
    }
    stream.finish().map_err(EncodeError::from)
}

/// `continuation.DecodeError`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecodeError {
    // Allocator.Error
    OutOfMemory,
    // stream_continuation.ValidateError
    NoPendingState,
    NonCanonicalContinuation,
    ReplayWouldCommit,
    // record.Reader.InitError || std.Io.Reader.Error
    ReadFailed,
    EndOfStream,
    InvalidTag,
    // record.Reader.FinishError
    InvalidChecksum,
    PayloadNotExhausted,
    /// The next record is valid but is not CONTINUATION.
    UnexpectedRecordTag,

    /// The declared payload exceeds caller policy.
    ContinuationLimitExceeded,
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

impl From<io::ReadError> for DecodeError {
    fn from(err: io::ReadError) -> DecodeError {
        match err {
            io::ReadError::ReadFailed => DecodeError::ReadFailed,
            io::ReadError::EndOfStream => DecodeError::EndOfStream,
        }
    }
}

impl From<stream_continuation::ValidateError> for DecodeError {
    fn from(err: stream_continuation::ValidateError) -> DecodeError {
        match err {
            stream_continuation::ValidateError::NoPendingState => DecodeError::NoPendingState,
            stream_continuation::ValidateError::NonCanonicalContinuation => {
                DecodeError::NonCanonicalContinuation
            }
            stream_continuation::ValidateError::ReplayWouldCommit => DecodeError::ReplayWouldCommit,
        }
    }
}

/// `alloc.alloc(u8, len)` — a fallible zeroed byte buffer, with the crate's
/// tripwire seam standing in for the reference tests' FailingAllocator.
fn decode_alloc(len: usize) -> Result<Vec<u8>, DecodeError> {
    #[cfg(test)]
    super::tw::check(super::tw::SnapshotFp::Alloc).map_err(|_| DecodeError::OutOfMemory)?;

    let mut bytes: Vec<u8> = Vec::new();
    bytes
        .try_reserve_exact(len)
        .map_err(|_| DecodeError::OutOfMemory)?;
    bytes.resize(len, 0);
    Ok(bytes)
}

/// Decode and validate one CONTINUATION record.
///
/// A zero-length payload returns `ground` without allocating. A nonempty
/// payload returns allocator-owned bytes; although `Value.bytes` is a const
/// slice, the caller must eventually free it with the same allocator.
pub fn decode(
    source: &mut dyn io::Reader,
    max_bytes: usize,
) -> Result<Value<'static>, DecodeError> {
    let mut record_reader = record::Reader::init(source)?;
    if record_reader.header.tag != record::Tag::Continuation {
        return Err(DecodeError::UnexpectedRecordTag);
    }

    // Validate the header payload vs our max size since the header
    // enforces it.
    let len: usize = record_reader.header.payload_len as usize;
    if len > max_bytes {
        return Err(DecodeError::ContinuationLimitExceeded);
    }

    if len == 0 {
        // Empty is the ground representation, but finish is still required:
        // its CRC covers the tag and zero length even though there is no body.
        record_reader.finish()?;
        return Ok(Value::Ground);
    }

    // Consume the bytes
    let mut bytes = decode_alloc(len)?;
    // errdefer alloc.free(bytes); — the owned Vec drops on every error path.
    record_reader.payload_reader().read_slice_all(&mut bytes)?;
    record_reader.finish()?;

    // Framing is valid, so it is now safe to classify the bytes as replay
    // state. Validation requires a minimal, unfinished, side-effect-free tail.
    stream_continuation::validate(&bytes)?;

    // Ownership transfers to the returned Value only after all validation.
    Ok(Value::Bytes(Cow::Owned(bytes)))
}

#[cfg(test)]
mod tests {
    use super::super::fixture;
    use super::*;

    fn test_ground_fixture() -> Vec<u8> {
        fixture::parse(include_str!("testdata/continuation-ground-v1.hex"))
    }
    fn test_utf8_fixture() -> Vec<u8> {
        fixture::parse(include_str!("testdata/continuation-utf8-v1.hex"))
    }
    fn test_esc_fixture() -> Vec<u8> {
        fixture::parse(include_str!("testdata/continuation-esc-v1.hex"))
    }
    fn test_csi_fixture() -> Vec<u8> {
        fixture::parse(include_str!("testdata/continuation-csi-v1.hex"))
    }
    fn test_osc_fixture() -> Vec<u8> {
        fixture::parse(include_str!("testdata/continuation-osc-v1.hex"))
    }
    fn test_dcs_fixture() -> Vec<u8> {
        fixture::parse(include_str!("testdata/continuation-dcs-v1.hex"))
    }
    fn test_apc_fixture() -> Vec<u8> {
        fixture::parse(include_str!("testdata/continuation-apc-v1.hex"))
    }

    // test "continuation golden records"
    #[test]
    fn continuation_golden_records() {
        struct Golden {
            path: &'static str,
            candidate: &'static str,
            value: Value<'static>,
            expected: Vec<u8>,
        }
        let values = [
            Golden {
                path: "src/snapshot/testdata/continuation-ground-v1.hex",
                candidate: "snapshot_fixture-continuation-ground-v1.hex",
                value: Value::Ground,
                expected: test_ground_fixture(),
            },
            Golden {
                path: "src/snapshot/testdata/continuation-utf8-v1.hex",
                candidate: "snapshot_fixture-continuation-utf8-v1.hex",
                value: Value::Bytes(Cow::Borrowed(b"\xF0\x9F\x98")),
                expected: test_utf8_fixture(),
            },
            Golden {
                path: "src/snapshot/testdata/continuation-esc-v1.hex",
                candidate: "snapshot_fixture-continuation-esc-v1.hex",
                value: Value::Bytes(Cow::Borrowed(b"\x1b")),
                expected: test_esc_fixture(),
            },
            Golden {
                path: "src/snapshot/testdata/continuation-csi-v1.hex",
                candidate: "snapshot_fixture-continuation-csi-v1.hex",
                value: Value::Bytes(Cow::Borrowed(b"\x1b[31")),
                expected: test_csi_fixture(),
            },
            Golden {
                path: "src/snapshot/testdata/continuation-osc-v1.hex",
                candidate: "snapshot_fixture-continuation-osc-v1.hex",
                value: Value::Bytes(Cow::Borrowed(b"\x1b]2;title")),
                expected: test_osc_fixture(),
            },
            Golden {
                path: "src/snapshot/testdata/continuation-dcs-v1.hex",
                candidate: "snapshot_fixture-continuation-dcs-v1.hex",
                value: Value::Bytes(Cow::Borrowed(b"\x1bPqdata")),
                expected: test_dcs_fixture(),
            },
            Golden {
                path: "src/snapshot/testdata/continuation-apc-v1.hex",
                candidate: "snapshot_fixture-continuation-apc-v1.hex",
                value: Value::Bytes(Cow::Borrowed(b"\x1b_Gdata")),
                expected: test_apc_fixture(),
            },
        ];

        for golden in &values {
            let mut encoded = io::AllocatingWriter::new();
            let mut stream = record::Writer::init(&mut encoded);
            encode(&golden.value, &mut stream).unwrap();
            fixture::expect_equal(
                fixture::Kind::Bytes,
                golden.path,
                golden.candidate,
                &golden.expected,
                stream.destination().written(),
            );
            stream.deinit();
        }
    }

    // test "continuation validates supported unfinished states"
    #[test]
    fn continuation_validates_supported_unfinished_states() {
        let values: [&[u8]; 6] = [
            b"\x1b",
            b"\x1b[31",
            b"\x1b]2;title",
            b"\x1bPqdata",
            b"\x1b_Gdata",
            b"\xF0\x9F\x98",
        ];
        for bytes in values {
            validate(&Value::Bytes(Cow::Borrowed(bytes))).unwrap();
        }
    }

    // test "empty continuation bytes encode and decode as ground"
    #[test]
    fn empty_continuation_bytes_encode_and_decode_as_ground() {
        validate(&Value::Bytes(Cow::Borrowed(b""))).unwrap();

        let mut encoded = io::AllocatingWriter::new();
        let mut stream = record::Writer::init(&mut encoded);
        encode(&Value::Bytes(Cow::Borrowed(b"")), &mut stream).unwrap();
        assert_eq!(
            test_ground_fixture().as_slice(),
            stream.destination().written(),
        );
        stream.deinit();

        let mut source = io::FixedReader::new(encoded.written());
        let decoded = decode(&mut source, 0).unwrap();
        assert!(matches!(decoded, Value::Ground));
    }

    // test "continuation rejects invalid semantic shapes"
    #[test]
    fn continuation_rejects_invalid_semantic_shapes() {
        assert_eq!(
            Err(ValidateError::NoPendingState),
            validate(&Value::Bytes(Cow::Borrowed(b"\x1b[31m"))),
        );
        assert_eq!(
            Err(ValidateError::ReplayWouldCommit),
            validate(&Value::Bytes(Cow::Borrowed(b"\x1b[31\x07"))),
        );
        assert_eq!(
            Err(ValidateError::NonCanonicalContinuation),
            validate(&Value::Bytes(Cow::Borrowed(b"prefix\x1b[31"))),
        );
        assert_eq!(
            Err(ValidateError::NonCanonicalContinuation),
            validate(&Value::Bytes(Cow::Borrowed(b"\x1b[31\x1b[4"))),
        );
    }

    // test "continuation record round trip and cap"
    #[test]
    fn continuation_record_round_trip_and_cap() {
        let values: [Value<'static>; 3] = [
            Value::Ground,
            Value::Bytes(Cow::Borrowed(b"\x1b[31")),
            Value::Bytes(Cow::Borrowed(b"\xF0\x9F")),
        ];

        for value in &values {
            let mut encoded = io::AllocatingWriter::new();
            let mut stream = record::Writer::init(&mut encoded);
            encode(value, &mut stream).unwrap();
            stream.deinit();

            let mut source = io::FixedReader::new(encoded.written());
            let decoded = decode(&mut source, 1024).unwrap();
            match value {
                Value::Ground => assert!(matches!(decoded, Value::Ground)),
                Value::Bytes(expected) => match &decoded {
                    Value::Bytes(bytes) => assert_eq!(expected.as_ref(), bytes.as_ref()),
                    Value::Ground => panic!("expected bytes"),
                },
            }
        }

        let mut encoded = io::AllocatingWriter::new();
        let mut stream = record::Writer::init(&mut encoded);
        encode(&Value::Bytes(Cow::Borrowed(b"\x1b[31")), &mut stream).unwrap();
        stream.deinit();

        let mut capped_source = io::FixedReader::new(encoded.written());
        // var failing = testing.FailingAllocator.init(testing.allocator, .{
        //     .fail_index = 0,
        // });
        // PORT NOTE: FailingAllocator → the SNAPSHOT_TW tripwire seam. The
        // reference arms an allocator that fails on its FIRST allocation to
        // prove the limit check precedes allocation; here the armed tripwire
        // must end UNTRIPPED for exactly the same reason.
        super::super::tw::error_always(super::super::tw::SnapshotFp::Alloc);
        assert_eq!(
            Err(DecodeError::ContinuationLimitExceeded),
            decode(&mut capped_source, 3),
        );
        assert_eq!(
            Err(rustty_util::tripwire::UntrippedError),
            super::super::tw::end(rustty_util::tripwire::ResetMode::Reset),
        );
        assert_eq!(record::Header::LEN, capped_source.seek);
    }

    // test "continuation record rejects truncation, checksum, and tag"
    #[test]
    fn continuation_record_rejects_truncation_checksum_and_tag() {
        let test_csi_fixture = test_csi_fixture();
        for len in 0..test_csi_fixture.len() {
            let mut source = io::FixedReader::new(&test_csi_fixture[..len]);
            assert_eq!(Err(DecodeError::EndOfStream), decode(&mut source, 1024));
        }

        let mut invalid_checksum = test_csi_fixture.clone();
        invalid_checksum[6] ^= 1;
        let mut checksum_source = io::FixedReader::new(&invalid_checksum);
        assert_eq!(
            Err(DecodeError::InvalidChecksum),
            decode(&mut checksum_source, 1024),
        );

        let mut wrong_tag = test_ground_fixture();
        wrong_tag[0..2].copy_from_slice(&(record::Tag::Ready as u16).to_le_bytes());
        let mut tag_source = io::FixedReader::new(&wrong_tag);
        assert_eq!(
            Err(DecodeError::UnexpectedRecordTag),
            decode(&mut tag_source, 1024),
        );
    }
}
