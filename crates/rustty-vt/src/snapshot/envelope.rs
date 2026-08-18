//! Snapshot envelope.
//!
//! Every snapshot binary blob begins with exactly one envelope at byte zero.
//! It is followed by a set of records. Each record provides their own
//! tag, payload length, CRC, etc.
//!
//! The envelope identifies the bytes as a terminal snapshot and selects the
//! single version governing the entire blob. A decoder validates both fields
//! before reading any records.
//!
//! The envelope is exactly ten bytes. All integers are unsigned and
//! little-endian.
//!
//! | Offset | Size | Field                |
//! | -----: | ---: | :------------------- |
//! |      0 |    8 | Magic (`GHOSTSNP`)   |
//! |      8 |    2 | Version (`u16`)      |
//!
//! PORTS: ghostty src/terminal/snapshot/envelope.zig

// const std = @import("std");
// const test_fixture = @import("fixture.zig");
use super::io;
use super::io::{Reader, Writer};

/// Identifies a Ghostty terminal snapshot and rejects unrelated input before
/// any record decoding begins. All eight bytes are part of the wire value.
///
/// PORT NOTE (R2): `GHOSTSNP` is a WIRE constant, not prose — renaming it
/// would break every golden fixture and interop with upstream snapshots.
pub const MAGIC: &[u8; 8] = b"GHOSTSNP";

/// The complete compatibility boundary for snapshot layout and behavior.
/// Version 1 readers require this value to match exactly.
pub const VERSION: u16 = 1;

/// Number of bytes in the fixed envelope: magic followed by version.
///
/// PORT NOTE: the reference computes this at comptime by running the encoder
/// (`computeLen`) and `comptime`-asserts the result is 10. The port pins the
/// constant and the `encoded_len_matches_encoder` test carries both the
/// encoder synchronization and the assert ("if it changes we should think
/// carefully").
pub const ENCODED_LEN: usize = 10;

/// `envelope.DecodeError`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecodeError {
    // std.Io.Reader.Error
    ReadFailed,
    EndOfStream,
    InvalidMagic,
    UnsupportedVersion,
}

impl From<io::ReadError> for DecodeError {
    fn from(err: io::ReadError) -> DecodeError {
        match err {
            io::ReadError::ReadFailed => DecodeError::ReadFailed,
            io::ReadError::EndOfStream => DecodeError::EndOfStream,
        }
    }
}

/// Encode the envelope.
pub fn encode(writer: &mut dyn Writer) -> Result<(), io::WriteFailed> {
    writer.write_all(MAGIC)?;
    io::write_int::<u16>(writer, VERSION)
}

/// Decode and validate the envelope.
pub fn decode(reader: &mut dyn Reader) -> Result<(), DecodeError> {
    let mut actual_magic = [0u8; MAGIC.len()];
    reader.read_slice_all(&mut actual_magic)?;
    if MAGIC != &actual_magic {
        return Err(DecodeError::InvalidMagic);
    }

    let actual_version = io::read_int::<u16>(reader)?;
    if actual_version != VERSION {
        return Err(DecodeError::UnsupportedVersion);
    }
    Ok(())
}

/// The reference `computeLen`, retained as the encoder-synchronization check
/// behind `ENCODED_LEN` (see its PORT NOTE).
#[cfg(test)]
fn compute_len() -> usize {
    let mut buf = [0u8; 128];
    let mut writer = io::FixedWriter::new(&mut buf);
    encode(&mut writer).unwrap();
    writer.end
}

#[cfg(test)]
mod tests {
    use super::super::fixture;
    use super::*;

    // const test_golden_fixture = test_fixture.parse(@embedFile("testdata/envelope-v1.hex"));
    fn test_golden_fixture() -> Vec<u8> {
        fixture::parse(include_str!("testdata/envelope-v1.hex"))
    }

    // comptime { std.debug.assert(encoded_len == 10); }
    #[test]
    fn encoded_len_matches_encoder() {
        assert_eq!(ENCODED_LEN, compute_len());
        assert_eq!(ENCODED_LEN, 10);
    }

    // test "golden encoding"
    #[test]
    fn golden_encoding() {
        let test_golden_fixture = test_golden_fixture();

        let mut buf = [0u8; ENCODED_LEN];
        let mut writer = io::FixedWriter::new(&mut buf);
        encode(&mut writer).unwrap();

        fixture::expect_equal(
            fixture::Kind::Bytes,
            "src/snapshot/testdata/envelope-v1.hex",
            "snapshot_fixture-envelope-v1.hex",
            &test_golden_fixture,
            writer.buffered(),
        );

        let mut reader = io::FixedReader::new(&test_golden_fixture);
        decode(&mut reader).unwrap();
    }

    // test "reject invalid magic and version"
    #[test]
    fn reject_invalid_magic_and_version() {
        let mut invalid_magic = io::FixedReader::new(b"GHOSTSNX\x01\x00");
        assert_eq!(Err(DecodeError::InvalidMagic), decode(&mut invalid_magic));

        let mut invalid_version = io::FixedReader::new(b"GHOSTSNP\x00\x00");
        assert_eq!(
            Err(DecodeError::UnsupportedVersion),
            decode(&mut invalid_version),
        );
    }

    // test "reject every truncation"
    #[test]
    fn reject_every_truncation() {
        let test_golden_fixture = test_golden_fixture();
        for len in 0..ENCODED_LEN {
            let mut reader = io::FixedReader::new(&test_golden_fixture[..len]);
            assert_eq!(Err(DecodeError::EndOfStream), decode(&mut reader));
        }
    }
}
