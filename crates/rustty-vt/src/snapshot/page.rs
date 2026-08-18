//! PAGE record payload encoding.
//!
//! One PAGE record represents a set of rows/columns in the terminal.
//! In librustty, this happens to map internally to a very specific
//! data structure called a "page" (hence the name), but consumers of
//! the format don't need to reproduce that.
//!
//! The important thing is that one page is fully self-contained: it has
//! a dimension (cols x rows), a set of styles, hyperlinks, cells, etc.
//! and depends on no external state to decode that with some exceptions
//! like assets such as images.
//!
//! A page could have a dimension that doesn't match the terminal
//! dimensions, e.g. for lazy resize/reflow. Callers must be prepared for
//! that.
//!
//! ## Binary Format
//!
//! Every PAGE payload begins with a fixed header, followed by a
//! payload of styles, hyperlinks, and rows and columns.
//!
//! All integers are unsigned and little-endian.
//!
//! ### Header
//!
//! | Offset | Size | Field                              |
//! | -----: | ---: | :--------------------------------- |
//! |      0 |    2 | Logical columns (`u16`)            |
//! |      2 |    2 | Logical rows (`u16`)               |
//! |      4 |    2 | Non-default style count (`u16`)    |
//! |      6 |    2 | Unique hyperlink count (`u16`)     |
//! |      8 |    2 | Style capacity hint (`u16`)        |
//! |     10 |    2 | Hyperlink capacity bytes (`u16`)   |
//! |     12 |    4 | Grapheme capacity bytes (`u32`)    |
//! |     16 |    4 | String capacity bytes (`u32`)      |
//!
//! The first two fields (columns and rows) denote the dimensionality
//! of the page. The payload is guaranteed to have this dimensionality;
//! every row has exactly the columns specified.
//!
//! Next, the style count and hyperlink count denote the number of
//! styles and hyperlinks respectively that are sent with the page.
//! The default style and absence of a hyperlink are implicit at native
//! ID zero and are not included in these counts. Each encoded table entry
//! carries its nonzero native page ID.
//!
//! The final four fields are allocation hints copied from the source page.
//! These represent upper limits on what this page might contain. A decoder
//! can optionally choose to use this for preallocation or it can ignore
//! and decode and allocate dynamically.
//!
//! ### Payload
//!
//! ```text
//! +---------------------------+
//! | Header                    |
//! | 20 bytes                  |
//! +---------------------------+
//! | Style table               |
//! | style_count entries       |
//! +---------------------------+
//! | Hyperlink table           |
//! | hyperlink_count entries   |
//! +---------------------------+
//! | Grid                      |
//! | rows and grapheme section |
//! +---------------------------+
//!
//! Style entry     = encoded ID + style record
//! Hyperlink entry = encoded ID + hyperlink record
//! Grid            = row 0 ... row (rows - 1) + grapheme suffix section
//! Row             = row header + encoded cells
//! ```
//!
//! Following the header, the payload contains exactly `style_count` style
//! records and `hyperlink_count` hyperlink records, followed by exactly
//! `rows` row records and one grapheme suffix section. There is no padding
//! between records.
//!
//! Each style begins with an ID (`u16`) followed by the typical style
//! binary representation (in style.zig). The ID is what cells will use
//! to reference the style. Decoders must maintain some kind of mapping
//! in order to associate these properly.
//!
//! Each hyperlink also begins with an ID (`u16`) with the same semantics
//! as the style ID. The hyperlink and style IDs are separate namespaces,
//! so they may collide. Following the ID, the hyperlink binary format
//! is encoded directly.
//!
//! Both style and hyperlink IDs are not guaranteed to be in any specific
//! order and may contain gaps (e.g. ID 1 and 3 but not 2).
//!
//! Rows and cells use the grid encoding documented in `grid.zig`.
//!
//! PORTS: ghostty src/terminal/snapshot/page.zig
//!
//! PORT STATUS: partial (phase-4 wsnap-1) — the wire `Header`, `discard`, and
//! the `page_hyperlink` helper are live. Everything that reads or writes the
//! grid section is BLOCKED on `snapshot/grid.rs` (assessment B cluster C3, the
//! next-stage agent) and lands with it, END STATE unchanged:
//!
//!   * `EncodeError` / `DecodeError` / `PayloadEncodeError` /
//!     `PayloadDecodeError` (they union `grid.EncodeError` / `grid.DecodeError`)
//!   * `encode` / `encodePayload` (grid.encode; sparse style + hyperlink
//!     table iteration is already expressible — see `page_hyperlink`)
//!   * `Decoder` (init/capacity/decode; the `max_staged_payload = 8MiB`
//!     staging split of 3e5d12835/e37865bed, and the
//!     `build_options.slow_runtime_safety`-gated `verifyIntegrity` of
//!     9f6656347)
//!   * `decode` / `decodePayload` / `decodePayloadBody` (grid.StyleRemap /
//!     grid.HyperlinkRemap; the skip-remap-tables-for-style-less-pages fast
//!     path of 47a518262; the TRANSACTIONAL release of every style /
//!     hyperlink table reference on both the success and error paths —
//!     9a5279db6 / b5290e74c — whose release-on-error tests below are the
//!     regression net and MUST be carried with it)
//!   * deferred upstream tests (assertion-for-assertion with grid.rs):
//!     "framed PAGE encode and decode a sparse native page",
//!     "framed PAGE golden empty record",
//!     "framed PAGE rejects incomplete wide cells transactionally",
//!     "framed PAGE rejects a different record tag",
//!     "framed PAGE preserves following bytes",
//!     "decode sparse page rejects every truncation",
//!     "decode accepts unordered sparse style IDs and ignores zero",
//!     "decode accepts unordered sparse hyperlink IDs",
//!     "decode defaults missing sparse cell references",
//!     "decode normalizes invalid grid semantics",
//!     "decode validates dimensions",
//!     "decode propagates a style table read failure",
//!     "decode normalizes duplicate default and invalid style entries",
//!     "decode releases unused style table references",
//!     "decode reuses duplicate hyperlinks",
//!     "decode ignores empty hyperlink strings"
//!
//! Their goldens (`page-v1.hex`, `page-empty-record-v1.hex`) are already
//! checked in under `testdata/`.

// const std = @import("std");
// const build_options = @import("terminal_options");
// const Allocator = std.mem.Allocator;
// const test_fixture = @import("fixture.zig");
// const grid = @import("grid.zig"); — NEXT STAGE (see PORT STATUS above)
// const hyperlink = @import("hyperlink.zig");
// const io = @import("io.zig");
// const record = @import("record.zig");
// const style = @import("style.zig");
// const terminal_hyperlink = @import("../hyperlink.zig");
// const terminal_page = @import("../page.zig");
// const terminal_style = @import("../style.zig");
use super::io;
use super::record;
use crate::hyperlink as terminal_hyperlink;
use crate::page as terminal_page;

// Frequent constants we use
use terminal_hyperlink::PageEntry as TerminalHyperlinkPageEntry;
use terminal_page::Capacity as TerminalPageCapacity;
use terminal_page::Page as TerminalPage;

/// Errors possible while discarding one PAGE record without decoding it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiscardError {
    // record.Reader.InitError || std.Io.Reader.Error
    ReadFailed,
    EndOfStream,
    InvalidTag,
    // record.Reader.FinishError
    InvalidChecksum,
    PayloadNotExhausted,
    /// The next record is valid but is not a PAGE.
    UnexpectedRecordTag,
}

impl From<record::InitError> for DiscardError {
    fn from(err: record::InitError) -> DiscardError {
        match err {
            record::HeaderDecodeError::ReadFailed => DiscardError::ReadFailed,
            record::HeaderDecodeError::EndOfStream => DiscardError::EndOfStream,
            record::HeaderDecodeError::InvalidTag => DiscardError::InvalidTag,
        }
    }
}

impl From<record::ReaderFinishError> for DiscardError {
    fn from(err: record::ReaderFinishError) -> DiscardError {
        match err {
            record::ReaderFinishError::InvalidChecksum => DiscardError::InvalidChecksum,
            record::ReaderFinishError::PayloadNotExhausted => DiscardError::PayloadNotExhausted,
        }
    }
}

impl From<io::ReadError> for DiscardError {
    fn from(err: io::ReadError) -> DiscardError {
        match err {
            io::ReadError::ReadFailed => DiscardError::ReadFailed,
            io::ReadError::EndOfStream => DiscardError::EndOfStream,
        }
    }
}

/// Consume exactly one complete PAGE record, validating its framing and
/// CRC32C while discarding the payload without structural validation.
///
/// This lets a decoder stay aligned with the record sequence while dropping
/// page content it can no longer apply.
pub fn discard(source: &mut dyn io::Reader) -> Result<(), DiscardError> {
    let mut record_reader = record::Reader::init(source)?;
    if record_reader.header.tag != record::Tag::Page {
        return Err(DiscardError::UnexpectedRecordTag);
    }
    let payload_len = record_reader.header.payload_len as usize;
    record_reader.payload_reader().discard_all(payload_len)?;
    record_reader.finish()?;
    Ok(())
}

/// `Header.CapacityError`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CapacityError {
    InvalidDimensions,
}

/// The fixed logical dimensions, table counts, and allocation hints at the
/// start of PAGE.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Header {
    /// Number of logical cells in every encoded row.
    pub columns: u16,

    /// Number of logical rows encoded after the tables.
    pub rows: u16,

    /// Number of non-default style entries following the header.
    pub style_count: u16,

    /// Number of unique hyperlink entries following the style table.
    pub hyperlink_count: u16,

    /// Suggested capacity for the native non-default style set.
    pub style_capacity: u16,

    /// Suggested native hyperlink storage capacity in bytes.
    pub hyperlink_capacity_bytes: u16,

    /// Suggested native grapheme storage capacity in bytes.
    pub grapheme_capacity_bytes: u32,

    /// Suggested native string storage capacity in bytes.
    pub string_capacity_bytes: u32,
}

impl Header {
    /// Number of bytes written by `encode`.
    ///
    /// PORT NOTE: the reference calculates this at comptime using the encoder
    /// itself (`computeLen`) and `comptime`-asserts it is 20 ("this size is
    /// part of the wire format"). The port pins the constant; the
    /// `header_len_matches_encoder` test carries both the synchronization and
    /// the assert.
    pub const LEN: usize = 20;

    /// Encode the fixed PAGE payload header.
    pub fn encode(self, writer: &mut dyn io::Writer) -> Result<(), io::WriteFailed> {
        io::write_int::<u16>(writer, self.columns)?;
        io::write_int::<u16>(writer, self.rows)?;
        io::write_int::<u16>(writer, self.style_count)?;
        io::write_int::<u16>(writer, self.hyperlink_count)?;
        io::write_int::<u16>(writer, self.style_capacity)?;
        io::write_int::<u16>(writer, self.hyperlink_capacity_bytes)?;
        io::write_int::<u32>(writer, self.grapheme_capacity_bytes)?;
        io::write_int::<u32>(writer, self.string_capacity_bytes)
    }

    /// Decode the fixed PAGE payload header.
    ///
    /// This reads field values only. The complete PAGE decoder is responsible
    /// for applying configured limits before using any capacity hint.
    pub fn decode(reader: &mut dyn io::Reader) -> Result<Header, io::ReadError> {
        Ok(Header {
            columns: io::read_int::<u16>(reader)?,
            rows: io::read_int::<u16>(reader)?,
            style_count: io::read_int::<u16>(reader)?,
            hyperlink_count: io::read_int::<u16>(reader)?,
            style_capacity: io::read_int::<u16>(reader)?,
            hyperlink_capacity_bytes: io::read_int::<u16>(reader)?,
            grapheme_capacity_bytes: io::read_int::<u32>(reader)?,
            string_capacity_bytes: io::read_int::<u32>(reader)?,
        })
    }

    /// Initialize a header from the current contents of a native page.
    ///
    /// This copies the page's existing allocation capacities. It does not
    /// scan cells, allocate, or encode any bytes.
    // Used by `encodePayload` and the deferred grid-coupled tests (see the
    // module PORT STATUS).
    #[allow(dead_code)]
    fn init(page: &TerminalPage) -> Header {
        Header {
            columns: page.size.cols,
            rows: page.size.rows,
            style_count: u16::try_from(page.styles.count()).unwrap(),
            hyperlink_count: u16::try_from(page.hyperlink_set.count()).unwrap(),
            style_capacity: page.capacity.styles,
            hyperlink_capacity_bytes: page.capacity.hyperlink_bytes,
            grapheme_capacity_bytes: page.capacity.grapheme_bytes,
            string_capacity_bytes: page.capacity.string_bytes,
        }
    }

    /// Validate native allocation requirements and produce the page capacity.
    // Used by the deferred `Decoder` (see the module PORT STATUS).
    #[allow(dead_code)]
    fn page_capacity(self) -> Result<TerminalPageCapacity, CapacityError> {
        if self.columns == 0 || self.rows == 0 {
            return Err(CapacityError::InvalidDimensions);
        }

        Ok(TerminalPageCapacity {
            cols: self.columns,
            rows: self.rows,
            styles: self.style_capacity,
            hyperlink_bytes: self.hyperlink_capacity_bytes,
            grapheme_bytes: self.grapheme_capacity_bytes,
            string_bytes: self.string_capacity_bytes,
        })
    }

    /// The reference comptime `computeLen` (see `LEN`).
    #[cfg(test)]
    fn compute_len() -> usize {
        let mut buf = [0u8; 128];
        let mut writer = io::FixedWriter::new(&mut buf);
        let header = Header {
            columns: 0,
            rows: 0,
            style_count: 0,
            hyperlink_count: 0,
            style_capacity: 0,
            hyperlink_capacity_bytes: 0,
            grapheme_capacity_bytes: 0,
            string_capacity_bytes: 0,
        };
        header.encode(&mut writer).unwrap();
        writer.end
    }
}

/// Reconstruct a borrowed `Hyperlink` view of a page-owned table entry.
///
/// PORT NOTE: the reference returns slices aliasing page memory; the crate's
/// `Hyperlink` owns its byte strings (`Vec<u8>`, the Zig-byteslice mapping),
/// so this copies them out.
// Used by `encodePayload` and the deferred grid-coupled tests (see the module
// PORT STATUS).
#[allow(dead_code)]
fn page_hyperlink(
    page: &TerminalPage,
    entry: &TerminalHyperlinkPageEntry,
) -> terminal_hyperlink::Hyperlink {
    let base = page.memory_ptr();
    terminal_hyperlink::Hyperlink {
        id: match entry.id.tag() {
            terminal_hyperlink::PageEntryIdTag::Implicit => {
                terminal_hyperlink::HyperlinkId::Implicit(entry.id.implicit())
            }
            terminal_hyperlink::PageEntryIdTag::Explicit => {
                // SAFETY: the entry belongs to `page`'s hyperlink set, so its
                // offsets resolve within the mapping based at `base`.
                terminal_hyperlink::HyperlinkId::Explicit(unsafe {
                    entry.id.explicit().slice(base).to_vec()
                })
            }
        },
        // SAFETY: as above.
        uri: unsafe { entry.uri.slice(base).to_vec() },
    }
}

#[cfg(test)]
mod tests {
    use super::super::fixture;
    use super::*;
    use crate::snapshot::io::{Reader as _, Writer as _};

    // const test_header_fixture = test_fixture.parse(@embedFile("testdata/page-header-v1.hex"));
    fn test_header_fixture() -> Vec<u8> {
        fixture::parse(include_str!("testdata/page-header-v1.hex"))
    }

    // comptime { std.debug.assert(len == 20); }
    #[test]
    fn header_len_matches_encoder() {
        assert_eq!(Header::LEN, Header::compute_len());
        assert_eq!(Header::LEN, 20);
    }

    // test "PAGE header golden encoding and decoding"
    #[test]
    fn page_header_golden_encoding_and_decoding() {
        let test_header_fixture = test_header_fixture();

        let header = Header {
            columns: 0x0102,
            rows: 0x0304,
            style_count: 0x0506,
            hyperlink_count: 0x0708,
            style_capacity: 0x090a,
            hyperlink_capacity_bytes: 0x0b0c,
            grapheme_capacity_bytes: 0x0d0e0f10,
            string_capacity_bytes: 0x11121314,
        };
        let mut buf = [0u8; Header::LEN];
        let mut writer = io::FixedWriter::new(&mut buf);
        header.encode(&mut writer).unwrap();
        fixture::expect_equal(
            fixture::Kind::Bytes,
            "src/snapshot/testdata/page-header-v1.hex",
            "snapshot_fixture-page-header-v1.hex",
            &test_header_fixture,
            writer.buffered(),
        );

        let mut source = io::FixedReader::new(&test_header_fixture);
        let mut read_buf = [0u8; 1];
        let mut limited = io::LimitedReader::new(&mut source, io::Limit::Unlimited, &mut read_buf);
        assert_eq!(header, Header::decode(&mut limited).unwrap());
    }

    // test "reject every truncation"
    #[test]
    fn reject_every_truncation() {
        let test_header_fixture = test_header_fixture();
        for len in 0..Header::LEN {
            let mut reader = io::FixedReader::new(&test_header_fixture[..len]);
            assert_eq!(
                Err(io::ReadError::EndOfStream),
                Header::decode(&mut reader),
            );
        }
    }

    // test "discard consumes exactly one PAGE record"
    #[test]
    fn discard_consumes_exactly_one_page_record() {
        // Discard validates framing only, so an arbitrary payload keeps this test
        // focused on record consumption rather than page structure.
        let mut encoded = io::AllocatingWriter::new();
        let mut stream = record::Writer::init(&mut encoded);
        {
            let payload = stream.begin(record::Tag::Page);
            payload.write_all(b"undecodable page payload").unwrap();
            stream.finish().unwrap();
        }
        let record_len = stream.destination().written().len();
        stream.writer().write_all(b"next").unwrap();
        stream.deinit();

        let mut source = io::FixedReader::new(encoded.written());
        discard(&mut source).unwrap();
        assert_eq!(b"next", source.take(4).unwrap());

        // Only PAGE records may be discarded; the tag remains strict.
        let mut wrong_tag = io::AllocatingWriter::new();
        let mut wrong_tag_stream = record::Writer::init(&mut wrong_tag);
        {
            let payload = wrong_tag_stream.begin(record::Tag::History);
            payload.write_all(b"payload").unwrap();
            wrong_tag_stream.finish().unwrap();
        }
        wrong_tag_stream.deinit();
        let mut wrong_tag_source = io::FixedReader::new(wrong_tag.written());
        assert_eq!(
            Err(DiscardError::UnexpectedRecordTag),
            discard(&mut wrong_tag_source),
        );

        // Discarded bytes are still covered by the record CRC.
        let mut corrupted = encoded.written()[..record_len].to_vec();
        let last = corrupted.len() - 1;
        corrupted[last] ^= 1;
        let mut corrupted_source = io::FixedReader::new(&corrupted);
        assert_eq!(
            Err(DiscardError::InvalidChecksum),
            discard(&mut corrupted_source),
        );

        // A truncated payload is detected before `finish`.
        let mut truncated_source = io::FixedReader::new(&encoded.written()[..record_len - 1]);
        assert_eq!(Err(DiscardError::EndOfStream), discard(&mut truncated_source));
    }
}
