//! HISTORY record payload encoding.
//!
//! One HISTORY record describes the complete history for a previously encoded
//! screen. It is followed immediately by the number of complete PAGE records
//! declared by `page_count`. Those pages contain the history older than the
//! first page in the corresponding SCREEN sequence and are ordered from newest
//! to oldest.
//!
//! Every encoded SCREEN has one corresponding HISTORY record. A screen with no
//! older pages uses a zero page count and has no following PAGE records.
//!
//! The first SCREEN page may begin above the active area. Those incidental
//! history rows are already present at READY and are not repeated after
//! HISTORY. The corresponding SCREEN header declares the complete logical
//! history extent, including both that resident overlap and the following PAGE
//! records.
//!
//! All integers are unsigned and little-endian.
//!
//! ## Binary Format
//!
//! A HISTORY record is followed immediately by its declared PAGE records:
//!
//! ```text
//! +----------------------+
//! | HISTORY record       |
//! +----------------------+
//! | PAGE record 0        | newest
//! +----------------------+
//! | ...                  |
//! +----------------------+
//! | PAGE record (n - 1)  | oldest
//! +----------------------+
//!
//! n = page_count
//! ```
//!
//! Newest-to-oldest order lets a reader start showing most-recent
//! history as soon as possible which is more useful to a user.
//!
//! The PAGE sequence may be empty when all history is already included in the
//! first SCREEN page or when the screen has no history.
//!
//! ### Header
//!
//! The HISTORY payload consists only of this fixed header:
//!
//! ```text
//!  0 +--------------------------------+
//!    | Screen key (u16)               |
//!  2 +--------------------------------+
//!    | Following page count (u32)     |
//!  6 +--------------------------------+
//! ```
//!
//! A decoder uses `page_count`, rather than another record tag, to find the end
//! of the page sequence.
//!
//! PORTS: ghostty src/terminal/snapshot/history.zig
//!
//! PORT STATUS: partial (phase-4 wsnap-2) — the complete production surface
//! (Header, encode, Decoder, decode, decodePage) is live. ONE upstream test,
//! "HISTORY encodes newest first and restores complete history", drives the
//! sequence through `snapshot/screen.rs` (`screen.encode` / `screen.decode`),
//! which is the remaining next-stage module (assessment B cluster C4); that
//! test lands verbatim with it. The other three upstream tests are carried
//! below.

// const std = @import("std");
// const Allocator = std.mem.Allocator;
// const test_fixture = @import("fixture.zig");
// const io = @import("io.zig");
// const page = @import("page.zig");
// const record = @import("record.zig");
// const screen = @import("screen.zig"); — NEXT STAGE (see PORT STATUS above)
// const TerminalPage = @import("../page.zig").Page;
// const TerminalPageList = @import("../PageList.zig");
// const TerminalScreen = @import("../Screen.zig");
// const TerminalScreenKey = @import("../ScreenSet.zig").Key;
use super::io;
use super::page;
use super::record;
use crate::page::Page as TerminalPage;
use crate::pagelist::builder::{FinalizeError, Location, PageAllocation};
use crate::pagelist::PageList as TerminalPageList;
use crate::screen::Screen as TerminalScreen;
use crate::screen_set::Key as TerminalScreenKey;

/// The complete fixed payload of one HISTORY record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Header {
    /// Identifies the previously encoded screen that owns this history.
    pub key: TerminalScreenKey,

    /// Number of complete PAGE records immediately following this record.
    pub page_count: u32,
}

/// `Header.DecodeError`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeaderDecodeError {
    // std.Io.Reader.Error
    ReadFailed,
    EndOfStream,
    InvalidKey,
}

impl From<io::ReadError> for HeaderDecodeError {
    fn from(err: io::ReadError) -> HeaderDecodeError {
        match err {
            io::ReadError::ReadFailed => HeaderDecodeError::ReadFailed,
            io::ReadError::EndOfStream => HeaderDecodeError::EndOfStream,
        }
    }
}

impl Header {
    /// Number of encoded bytes in the fixed payload, calculated by its encoder.
    ///
    /// PORT NOTE: the reference computes this at comptime with the encoder
    /// itself and `comptime`-asserts it is 6 ("this size is part of the wire
    /// format"). The port pins the constant; the `header_len_matches_encoder`
    /// test carries both the synchronization and the assert.
    pub const LEN: usize = 6;

    /// Encode the fixed HISTORY payload.
    pub fn encode(self, writer: &mut dyn io::Writer) -> Result<(), io::WriteFailed> {
        io::write_int::<u16>(writer, self.key as u16)?;
        io::write_int::<u32>(writer, self.page_count)
    }

    /// Decode and validate the fixed HISTORY payload.
    pub fn decode(reader: &mut dyn io::Reader) -> Result<Header, HeaderDecodeError> {
        let raw = io::read_int::<u16>(reader)?;
        // std.math.cast(Tag, raw) orelse error.InvalidKey, then
        // std.enums.fromInt(TerminalScreenKey, value) orelse error.InvalidKey.
        let key = match raw {
            0 => TerminalScreenKey::Primary,
            1 => TerminalScreenKey::Alternate,
            _ => return Err(HeaderDecodeError::InvalidKey),
        };
        Ok(Header {
            key,
            page_count: io::read_int::<u32>(reader)?,
        })
    }

    /// The reference comptime `computeLen` (see `LEN`).
    #[cfg(test)]
    fn compute_len() -> usize {
        let mut buf = [0u8; 128];
        let mut writer = io::FixedWriter::new(&mut buf);
        let value = Header {
            key: TerminalScreenKey::Primary,
            page_count: 0,
        };
        value.encode(&mut writer).unwrap();
        writer.end
    }
}

/// Errors possible while encoding one HISTORY and its complete PAGE sequence.
// pub const EncodeError = Allocator.Error || page.EncodeError ||
//     record.Writer.FinishError || error{...};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EncodeError {
    // Allocator.Error
    OutOfMemory,
    // page.EncodeError (incl. record.Writer.FinishError)
    WriteFailed,
    InvalidUri,
    InvalidExplicitId,
    StringTooLong,
    InvalidWideCell,
    TooManyGraphemes,
    PayloadTooLarge,

    /// The complete historical prefix has more pages than the header fits.
    PageCountOverflow,
}

impl From<page::EncodeError> for EncodeError {
    fn from(err: page::EncodeError) -> EncodeError {
        match err {
            page::EncodeError::WriteFailed => EncodeError::WriteFailed,
            page::EncodeError::InvalidUri => EncodeError::InvalidUri,
            page::EncodeError::InvalidExplicitId => EncodeError::InvalidExplicitId,
            page::EncodeError::StringTooLong => EncodeError::StringTooLong,
            page::EncodeError::InvalidWideCell => EncodeError::InvalidWideCell,
            page::EncodeError::TooManyGraphemes => EncodeError::TooManyGraphemes,
            page::EncodeError::PayloadTooLarge => EncodeError::PayloadTooLarge,
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

impl From<io::WriteFailed> for EncodeError {
    fn from(_: io::WriteFailed) -> EncodeError {
        EncodeError::WriteFailed
    }
}

/// Encode one screen's HISTORY and its complete historical pages.
///
/// Pages are encoded newest-to-oldest. Compressed source pages are inspected
/// without changing their storage state. Completed records may already be
/// emitted if a later page fails.
pub fn encode<W: io::Writer + ?Sized>(
    terminal_screen: &TerminalScreen,
    key: TerminalScreenKey,
    destination: &mut record::Writer<'_, W>,
) -> Result<(), EncodeError> {
    use crate::point;

    // SCREEN begins at the page containing the active area's first row. Its
    // leading rows are already resident; every previous complete page belongs
    // to this HISTORY sequence.
    let active_node = terminal_screen.pages.get_top_left(point::Tag::Active).node;
    // SAFETY: the pin's node is live within this screen's PageList; `prev`
    // links stay valid while the list is not mutated (encode only reads).
    let first: *mut crate::pagelist::Node = unsafe { (*active_node).prev };
    let page_count: usize = {
        let mut page_count: usize = 0;
        let mut node = first;
        while !node.is_null() {
            page_count += 1;
            // SAFETY: as above.
            node = unsafe { (*node).prev };
        }
        page_count
    };

    let header = Header {
        key,
        page_count: u32::try_from(page_count).map_err(|_| EncodeError::PageCountOverflow)?,
    };

    // HISTORY declares exactly how many PAGE records follow.
    {
        let payload = destination.begin(record::Tag::History);
        // errdefer destination.cancel();
        if let Err(err) = header.encode(payload) {
            destination.cancel();
            return Err(err.into());
        }
        if let Err(err) = destination.finish() {
            destination.cancel();
            return Err(err.into());
        }
    }

    // Walk backward so each page can be prepended by the decoder immediately.
    // PreservedPage borrows resident pages and clones compressed pages without
    // changing the source node's representation.
    let mut node = first;
    while !node.is_null() {
        // SAFETY: `node` is live (see above); encode does not mutate the list.
        let mut preserved = unsafe { crate::pagelist::Node::page_preserving_state(node) }
            .map_err(|_| EncodeError::OutOfMemory)?;
        // defer preserved.deinit();
        // SAFETY: `preserved.page()` is a live page for the preserved scope.
        let result = page::encode(unsafe { &*preserved.page() }, destination);
        preserved.deinit();
        result?;
        // SAFETY: as above.
        node = unsafe { (*node).prev };
    }
    Ok(())
}

/// Errors possible while restoring one HISTORY and its PAGE sequence.
// pub const DecodeError = Decoder.InitError || Decoder.RestoreError || error{...};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecodeError {
    // Decoder.InitError
    ReadFailed,
    EndOfStream,
    InvalidTag,
    InvalidKey,
    InvalidChecksum,
    PayloadNotExhausted,
    UnexpectedRecordTag,
    // Decoder.RestoreError (DecodePageError + ExistingHistory)
    OutOfMemory,
    InvalidDimensions,
    InvalidRowCellCount,
    InvalidKind,
    InvalidDestinationCapacity,
    Integrity(crate::page::IntegrityError),
    InvalidPageDimensions,
    RowCountOverflow,
    PageSizeOverflow,
    MaxSizeExceeded,
    MaxLinesExceeded,
    ExistingHistory,

    /// The HISTORY key does not match the caller-selected screen.
    UnexpectedScreenKey,
}

impl From<InitError> for DecodeError {
    fn from(err: InitError) -> DecodeError {
        match err {
            InitError::ReadFailed => DecodeError::ReadFailed,
            InitError::EndOfStream => DecodeError::EndOfStream,
            InitError::InvalidTag => DecodeError::InvalidTag,
            InitError::InvalidKey => DecodeError::InvalidKey,
            InitError::InvalidChecksum => DecodeError::InvalidChecksum,
            InitError::PayloadNotExhausted => DecodeError::PayloadNotExhausted,
            InitError::UnexpectedRecordTag => DecodeError::UnexpectedRecordTag,
        }
    }
}

impl From<DecodePageError> for DecodeError {
    fn from(err: DecodePageError) -> DecodeError {
        match err {
            DecodePageError::OutOfMemory => DecodeError::OutOfMemory,
            DecodePageError::ReadFailed => DecodeError::ReadFailed,
            DecodePageError::EndOfStream => DecodeError::EndOfStream,
            DecodePageError::InvalidTag => DecodeError::InvalidTag,
            DecodePageError::InvalidChecksum => DecodeError::InvalidChecksum,
            DecodePageError::PayloadNotExhausted => DecodeError::PayloadNotExhausted,
            DecodePageError::UnexpectedRecordTag => DecodeError::UnexpectedRecordTag,
            DecodePageError::InvalidDimensions => DecodeError::InvalidDimensions,
            DecodePageError::InvalidRowCellCount => DecodeError::InvalidRowCellCount,
            DecodePageError::InvalidKind => DecodeError::InvalidKind,
            DecodePageError::InvalidDestinationCapacity => DecodeError::InvalidDestinationCapacity,
            DecodePageError::Integrity(err) => DecodeError::Integrity(err),
            DecodePageError::InvalidPageDimensions => DecodeError::InvalidPageDimensions,
            DecodePageError::RowCountOverflow => DecodeError::RowCountOverflow,
            DecodePageError::PageSizeOverflow => DecodeError::PageSizeOverflow,
            DecodePageError::MaxSizeExceeded => DecodeError::MaxSizeExceeded,
            DecodePageError::MaxLinesExceeded => DecodeError::MaxLinesExceeded,
        }
    }
}

/// Errors possible while restoring one history PAGE into a native Screen.
// pub const DecodePageError = Allocator.Error || page.DecodeError ||
//     TerminalPageList.PageAllocation.FinalizeError;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecodePageError {
    // Allocator.Error
    OutOfMemory,
    // page.DecodeError
    ReadFailed,
    EndOfStream,
    InvalidTag,
    InvalidChecksum,
    PayloadNotExhausted,
    UnexpectedRecordTag,
    InvalidDimensions,
    InvalidRowCellCount,
    InvalidKind,
    InvalidDestinationCapacity,
    Integrity(crate::page::IntegrityError),
    // PageAllocation.FinalizeError
    InvalidPageDimensions,
    RowCountOverflow,
    PageSizeOverflow,
    MaxSizeExceeded,
    MaxLinesExceeded,
}

impl From<page::DecodeError> for DecodePageError {
    fn from(err: page::DecodeError) -> DecodePageError {
        match err {
            page::DecodeError::ReadFailed => DecodePageError::ReadFailed,
            page::DecodeError::EndOfStream => DecodePageError::EndOfStream,
            page::DecodeError::InvalidDimensions => DecodePageError::InvalidDimensions,
            page::DecodeError::InvalidRowCellCount => DecodePageError::InvalidRowCellCount,
            page::DecodeError::InvalidKind => DecodePageError::InvalidKind,
            page::DecodeError::OutOfMemory => DecodePageError::OutOfMemory,
            page::DecodeError::InvalidDestinationCapacity => {
                DecodePageError::InvalidDestinationCapacity
            }
            page::DecodeError::InvalidTag => DecodePageError::InvalidTag,
            page::DecodeError::InvalidChecksum => DecodePageError::InvalidChecksum,
            page::DecodeError::PayloadNotExhausted => DecodePageError::PayloadNotExhausted,
            page::DecodeError::Integrity(err) => DecodePageError::Integrity(err),
            page::DecodeError::UnexpectedRecordTag => DecodePageError::UnexpectedRecordTag,
        }
    }
}

impl From<FinalizeError> for DecodePageError {
    fn from(err: FinalizeError) -> DecodePageError {
        match err {
            FinalizeError::InvalidPageDimensions => DecodePageError::InvalidPageDimensions,
            FinalizeError::RowCountOverflow => DecodePageError::RowCountOverflow,
            FinalizeError::PageSizeOverflow => DecodePageError::PageSizeOverflow,
            FinalizeError::MaxSizeExceeded => DecodePageError::MaxSizeExceeded,
            FinalizeError::MaxLinesExceeded => DecodePageError::MaxLinesExceeded,
        }
    }
}

/// Restore the next history PAGE record and prepend it to the native Screen.
///
/// Returns the number of rows added above the screen's existing content.
/// The page is decoded directly into a detached PageList-pooled allocation
/// and prepended only after its record validates, so a failure leaves the
/// screen unchanged. A limit failure from `finalize` occurs after the record
/// bytes were fully consumed, leaving `source` aligned on the next record.
pub fn decode_page(
    source: &mut dyn io::Reader,
    terminal_screen: &mut TerminalScreen,
) -> Result<usize, DecodePageError> {
    // PAGE exposes its exact capacity before decoding the payload, allowing
    // the destination PageList to allocate the final backing memory once.
    let mut decoder = page::Decoder::init(&mut *source).map_err(DecodePageError::from)?;
    let pages_ptr: *mut TerminalPageList = &mut terminal_screen.pages;
    // SAFETY: `pages_ptr` points at the live PageList owned by
    // `terminal_screen`; until `finalize`/`deinit` below, the list is only
    // touched through the allocation itself (the allocate_page contract).
    let mut allocation: PageAllocation =
        unsafe { TerminalPageList::allocate_page(pages_ptr, decoder.capacity()) }
            .map_err(|_| DecodePageError::OutOfMemory)?;
    // defer allocation.deinit();
    let result = decode_page_inner(&mut decoder, &mut allocation, terminal_screen);
    allocation.deinit();
    result
}

fn decode_page_inner(
    decoder: &mut page::Decoder<'_>,
    allocation: &mut PageAllocation,
    terminal_screen: &mut TerminalScreen,
) -> Result<usize, DecodePageError> {
    // SAFETY: `allocation.page()` is the freshly allocated detached page; the
    // exclusive borrow lives only for this decode call.
    decoder.decode(unsafe { &mut *allocation.page() })?;

    // SAFETY: as above (shared reads of the decoded page).
    let (rows, contains_prompt) = unsafe {
        let decoded_page: *mut TerminalPage = allocation.page();
        (
            (*decoded_page).size.rows as usize,
            has_semantic_prompt(&*decoded_page),
        )
    };
    allocation.finalize(Location::Prepend)?;
    if contains_prompt {
        terminal_screen.semantic_prompt.seen = true;
    }
    Ok(rows)
}

/// `Decoder.InitError`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InitError {
    // Header.DecodeError
    ReadFailed,
    EndOfStream,
    InvalidKey,
    // record.Reader.InitError
    InvalidTag,
    // record.Reader.FinishError
    InvalidChecksum,
    PayloadNotExhausted,

    /// The next record is valid but is not a HISTORY.
    UnexpectedRecordTag,
}

impl From<HeaderDecodeError> for InitError {
    fn from(err: HeaderDecodeError) -> InitError {
        match err {
            HeaderDecodeError::ReadFailed => InitError::ReadFailed,
            HeaderDecodeError::EndOfStream => InitError::EndOfStream,
            HeaderDecodeError::InvalidKey => InitError::InvalidKey,
        }
    }
}

impl From<record::InitError> for InitError {
    fn from(err: record::InitError) -> InitError {
        match err {
            record::HeaderDecodeError::ReadFailed => InitError::ReadFailed,
            record::HeaderDecodeError::EndOfStream => InitError::EndOfStream,
            record::HeaderDecodeError::InvalidTag => InitError::InvalidTag,
        }
    }
}

impl From<record::ReaderFinishError> for InitError {
    fn from(err: record::ReaderFinishError) -> InitError {
        match err {
            record::ReaderFinishError::InvalidChecksum => InitError::InvalidChecksum,
            record::ReaderFinishError::PayloadNotExhausted => InitError::PayloadNotExhausted,
        }
    }
}

/// A decoded HISTORY manifest ready to restore its following PAGE records.
///
/// Keeping manifest decoding separate lets the full snapshot wrapper route a
/// HISTORY sequence by its encoded key before any pages mutate a Screen.
pub struct Decoder<'s> {
    pub source: &'s mut dyn io::Reader,
    pub header: Header,
}

impl<'s> Decoder<'s> {
    /// Decode and finish the self-contained HISTORY manifest.
    // PORT NOTE: plain constructor (see `page::Decoder::init`).
    pub fn init(source: &'s mut dyn io::Reader) -> Result<Decoder<'s>, InitError> {
        let header = {
            let mut record_reader = record::Reader::init(&mut *source)?;
            if record_reader.header.tag != record::Tag::History {
                return Err(InitError::UnexpectedRecordTag);
            }
            let header = Header::decode(record_reader.payload_reader())?;
            record_reader.finish()?;
            header
        };
        Ok(Decoder { source, header })
    }

    /// Restore the declared PAGE records into the selected native Screen.
    // pub const RestoreError = DecodePageError || error{ExistingHistory};
    // (flattened into `DecodeError`; `decode` returns the page error type
    // plus `ExistingHistory` through it.)
    pub fn decode(&mut self, terminal_screen: &mut TerminalScreen) -> Result<(), DecodeError> {
        use crate::point;

        // A freshly restored SCREEN may carry overlap inside its first page,
        // but cannot already contain a complete historical page before that
        // boundary.
        let active_top = terminal_screen.pages.get_top_left(point::Tag::Active);
        if terminal_screen.pages.get_top_left(point::Tag::Screen).node != active_top.node {
            return Err(DecodeError::ExistingHistory);
        }

        // Native row totals remain derived from the actual PAGE dimensions.
        for _ in 0..self.header.page_count {
            let _ = decode_page(&mut *self.source, terminal_screen)?;
        }

        terminal_screen.pages.assert_integrity();
        terminal_screen.assert_integrity();
        Ok(())
    }
}

/// Restore one HISTORY and its declared PAGE records into a native Screen.
///
/// Each PAGE is decoded directly into a detached PageList-pooled allocation and
/// prepended only after its record and native integrity are validated. If a
/// later PAGE fails, earlier successful pages remain as a contiguous recent
/// history prefix.
pub fn decode(
    source: &mut dyn io::Reader,
    expected_key: TerminalScreenKey,
    terminal_screen: &mut TerminalScreen,
) -> Result<(), DecodeError> {
    let mut decoder = Decoder::init(source)?;
    if decoder.header.key != expected_key {
        return Err(DecodeError::UnexpectedScreenKey);
    }
    decoder.decode(terminal_screen)
}

fn has_semantic_prompt(terminal_page: &TerminalPage) -> bool {
    for y in 0..terminal_page.size.rows as usize {
        let row_ptr = terminal_page.get_row(y);
        // SAFETY: live page row/cells; shared reads only.
        unsafe {
            if (*row_ptr).semantic_prompt() != crate::page::SemanticPrompt::None {
                return true;
            }

            let cells: &[crate::page::Cell] = &*terminal_page.get_cells(row_ptr);
            for cell in cells {
                if cell.semantic_content() == crate::page::SemanticContent::Prompt {
                    return true;
                }
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::super::fixture;
    use super::*;
    use crate::screen::Options as ScreenOptions;
    use crate::snapshot::io::Reader as _;
    use allocator_api2::alloc::Global as Allocator;

    // const test_header_fixture = test_fixture.parse(@embedFile("testdata/history-header-v1.hex"));
    fn test_header_fixture() -> Vec<u8> {
        fixture::parse(include_str!("testdata/history-header-v1.hex"))
    }

    fn screen_options(cols: u16, rows: u16) -> ScreenOptions {
        ScreenOptions {
            cols,
            rows,
            max_scrollback_bytes: None,
            ..ScreenOptions::default()
        }
    }

    // comptime { assert(len == 6); }
    #[test]
    fn header_len_matches_encoder() {
        assert_eq!(Header::LEN, Header::compute_len());
        assert_eq!(Header::LEN, 6);
    }

    // test "HISTORY header golden encoding and decoding"
    #[test]
    fn history_header_golden_encoding_and_decoding() {
        let test_header_fixture = test_header_fixture();

        let expected = Header {
            key: TerminalScreenKey::Alternate,
            page_count: 0x01020304,
        };
        let mut encoded = [0u8; Header::LEN];
        let mut writer = io::FixedWriter::new(&mut encoded);
        expected.encode(&mut writer).unwrap();
        fixture::expect_equal(
            fixture::Kind::Bytes,
            "src/snapshot/testdata/history-header-v1.hex",
            "snapshot_fixture-history-header-v1.hex",
            &test_header_fixture,
            writer.buffered(),
        );
        assert_eq!(Header::LEN, test_header_fixture.len());

        let mut reader = io::FixedReader::new(&test_header_fixture);
        assert_eq!(expected, Header::decode(&mut reader).unwrap());
        for fixture_len in 0..Header::LEN {
            let mut truncated = io::FixedReader::new(&test_header_fixture[0..fixture_len]);
            assert_eq!(
                Err(HeaderDecodeError::EndOfStream),
                Header::decode(&mut truncated),
            );
        }

        let mut invalid_key = test_header_fixture.clone();
        invalid_key[0] = 2;
        let mut invalid_key_reader = io::FixedReader::new(&invalid_key);
        assert_eq!(
            Err(HeaderDecodeError::InvalidKey),
            Header::decode(&mut invalid_key_reader),
        );
    }

    // test "HISTORY encodes newest first and restores complete history"
    // PORT NOTE (wsnap-2 boundary): deferred with `snapshot/screen.rs`
    // (assessment B cluster C4) — the test drives this module's encode/decode
    // through `screen.encode` / `screen.decode`; see the module PORT STATUS.

    // test "HISTORY encodes and restores an empty sequence"
    #[test]
    fn history_encodes_and_restores_an_empty_sequence() {
        let mut terminal_screen =
            TerminalScreen::init(Allocator, screen_options(2, 2)).unwrap();

        let mut destination = io::AllocatingWriter::new();
        let mut stream = record::Writer::init(&mut destination);
        encode(&terminal_screen, TerminalScreenKey::Primary, &mut stream).unwrap();

        let mut inspect_source = io::FixedReader::new(stream.destination().written());
        let mut history_record = record::Reader::init(&mut inspect_source).unwrap();
        let header = Header::decode(history_record.payload_reader()).unwrap();
        history_record.finish().unwrap();
        assert_eq!(0u32, header.page_count);
        assert_eq!(
            Err(io::ReadError::EndOfStream),
            inspect_source.take_byte(),
        );

        let mut decode_source = io::FixedReader::new(stream.destination().written());
        let initial_first = terminal_screen
            .pages
            .get_top_left(crate::point::Tag::Screen)
            .node;
        decode(
            &mut decode_source,
            TerminalScreenKey::Primary,
            &mut terminal_screen,
        )
        .unwrap();
        assert_eq!(
            initial_first,
            terminal_screen
                .pages
                .get_top_left(crate::point::Tag::Screen)
                .node,
        );
        assert_eq!(Err(io::ReadError::EndOfStream), decode_source.take_byte());

        stream.deinit();
        terminal_screen.deinit();
    }

    // test "HISTORY rejects invalid routing and incomplete sequences"
    #[test]
    fn history_rejects_invalid_routing_and_incomplete_sequences() {
        let mut terminal_screen =
            TerminalScreen::init(Allocator, screen_options(2, 2)).unwrap();

        // Routing and sequence length still determine which native screen is
        // mutated and where the following record begins, so they remain strict.
        struct Case {
            header: Header,
            expected: DecodeError,
        }
        let cases = [
            Case {
                header: Header {
                    key: TerminalScreenKey::Alternate,
                    page_count: 0,
                },
                expected: DecodeError::UnexpectedScreenKey,
            },
            Case {
                header: Header {
                    key: TerminalScreenKey::Primary,
                    page_count: 1,
                },
                expected: DecodeError::EndOfStream,
            },
        ];
        for case in cases {
            let mut destination = io::AllocatingWriter::new();
            let mut stream = record::Writer::init(&mut destination);
            {
                let payload = stream.begin(record::Tag::History);
                // errdefer stream.cancel();
                case.header.encode(payload).unwrap();
                stream.finish().unwrap();
            }

            let mut source = io::FixedReader::new(stream.destination().written());
            assert_eq!(
                Err(case.expected),
                decode(
                    &mut source,
                    TerminalScreenKey::Primary,
                    &mut terminal_screen,
                ),
            );
            terminal_screen.pages.assert_integrity();
            terminal_screen.assert_integrity();

            stream.deinit();
        }

        terminal_screen.deinit();
    }
}
