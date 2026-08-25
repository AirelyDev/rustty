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
//! PORT STATUS: complete (phase-4 wsnap-2 + wsnap-3) — the complete
//! production surface (Header, encode, Decoder, decode, decodePage) and all
//! four upstream tests are live ("HISTORY encodes newest first and restores
//! complete history" landed with `snapshot/screen.rs`, which it drives
//! through `screen.encode` / `screen.decode`).

// const std = @import("std");
// const Allocator = std.mem.Allocator;
// const test_fixture = @import("fixture.zig");
// const io = @import("io.zig");
// const page = @import("page.zig");
// const record = @import("record.zig");
// const screen = @import("screen.zig"); — test-only import (see the test)
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
    //
    // PORT NOTE (Stacked Borrows): `allocation` holds a raw alias into
    // `terminal_screen.pages` until `finalize`/`deinit`. Everything below
    // therefore works through ONE raw screen pointer taken here — forming
    // another `&mut TerminalScreen` (e.g. as a `decode_page_inner` argument)
    // while the alias is live re-protects the screen and invalidates the
    // alias before `finalize` uses it (the crate's raw-ptr-args discipline;
    // Miri-proven by the snapshot codec lane).
    let screen_ptr: *mut TerminalScreen = terminal_screen;
    let mut decoder = page::Decoder::init(&mut *source).map_err(DecodePageError::from)?;
    // SAFETY: `screen_ptr` is live for this call; the raw field projection
    // forms no reference.
    let pages_ptr: *mut TerminalPageList = unsafe { core::ptr::addr_of_mut!((*screen_ptr).pages) };
    // SAFETY: `pages_ptr` points at the live PageList owned by
    // `terminal_screen`; until `finalize`/`deinit` below, the list is only
    // touched through the allocation itself (the allocate_page contract).
    let mut allocation: PageAllocation =
        unsafe { TerminalPageList::allocate_page(pages_ptr, decoder.capacity()) }
            .map_err(|_| DecodePageError::OutOfMemory)?;
    // defer allocation.deinit();
    let result = decode_page_inner(&mut decoder, &mut allocation, screen_ptr);
    allocation.deinit();
    result
}

fn decode_page_inner(
    decoder: &mut page::Decoder<'_>,
    allocation: &mut PageAllocation,
    // PORT NOTE: raw, not `&mut` — see the Stacked Borrows note in
    // `decode_page` (`allocation` aliases this screen's PageList).
    terminal_screen: *mut TerminalScreen,
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
        // SAFETY: `terminal_screen` is live (decode_page's argument); a raw
        // field write that forms no reference over the finalized list.
        unsafe { (*terminal_screen).semantic_prompt.seen = true };
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
    #[test]
    fn history_encodes_newest_first_and_restores_complete_history() {
        use crate::pagelist::Node;
        use crate::point;
        use crate::snapshot::screen;

        // Choose an active height which spans two native pages, then grow
        // until exactly two additional complete pages precede the active
        // boundary.
        let page_rows = {
            let mut probe = TerminalScreen::init(
                Allocator,
                ScreenOptions {
                    cols: 80,
                    rows: 1,
                    max_scrollback_bytes: Some(0),
                    ..ScreenOptions::default()
                },
            )
            .unwrap();
            // SAFETY: the top-left node is live within the probe screen.
            let rows =
                unsafe { Node::capacity(probe.pages.get_top_left(point::Tag::Screen).node).rows };
            probe.deinit();
            rows
        };
        let screen_rows = page_rows + 1;

        let mut source_screen =
            TerminalScreen::init(Allocator, screen_options(80, screen_rows)).unwrap();
        // defer source_screen.deinit();
        source_screen.cursor_absolute(0, screen_rows - 1);
        while source_screen.pages.total_pages() < 4 {
            source_screen.test_write_string(b"\n").unwrap();
        }

        let active_top = source_screen.pages.get_top_left(point::Tag::Active);
        // SAFETY (this block and the marker writes below): live nodes and
        // resident pages of `source_screen`'s PageList; the writes touch
        // distinct cells.
        let (newest_history, oldest_history) = unsafe {
            let newest_history = (*active_top.node).prev;
            let oldest_history = (*newest_history).prev;
            assert_eq!(
                source_screen.pages.get_top_left(point::Tag::Screen).node,
                oldest_history,
            );
            assert!((*oldest_history).prev.is_null());
            (newest_history, oldest_history)
        };

        // Mark the two complete historical pages so the wire sequence and
        // final native order are visible. The oldest prompt also verifies
        // derived state is updated only when that page is restored.
        unsafe {
            let oldest_cell = (*Node::page(oldest_history)).get_row_and_cell(0, 0).cell;
            *oldest_cell = crate::page::Cell::init('A' as u32);
            (*oldest_cell).set_semantic_content(crate::page::SemanticContent::Prompt);
            *(*Node::page(newest_history)).get_row_and_cell(0, 0).cell =
                crate::page::Cell::init('B' as u32);
        }

        // Encode SCREEN before history compression, then compress eligible
        // history and require HISTORY encoding to preserve each source
        // storage state.
        let mut destination = io::AllocatingWriter::new();
        let mut stream = record::Writer::init(&mut destination);
        // defer stream.deinit();
        screen::encode(&source_screen, TerminalScreenKey::Primary, &mut stream).unwrap();
        let history_offset = stream.destination().written().len();

        let _ = source_screen
            .pages
            .compress(crate::pagelist::compress::CompressionMode::Full);
        // SAFETY: live nodes (see above); `storage` reads the representation.
        let oldest_storage = unsafe { Node::storage(oldest_history) };
        let newest_storage = unsafe { Node::storage(newest_history) };
        encode(&source_screen, TerminalScreenKey::Primary, &mut stream).unwrap();
        // SAFETY: as above.
        unsafe {
            assert_eq!(oldest_storage, Node::storage(oldest_history));
            assert_eq!(newest_storage, Node::storage(newest_history));
        }

        let written = stream.destination().written().to_vec();
        stream.deinit();

        // Inspect the manifest and PAGE records independently. Newest history
        // is sent first even though native PageList order is oldest-to-newest.
        let mut history_source = io::FixedReader::new(&written[history_offset..]);
        let mut history_record = record::Reader::init(&mut history_source).unwrap();
        assert_eq!(record::Tag::History, history_record.header.tag);
        let header = Header::decode(history_record.payload_reader()).unwrap();
        history_record.finish().unwrap();
        assert_eq!(TerminalScreenKey::Primary, header.key);
        assert_eq!(2u32, header.page_count);

        let mut decoded_newest = page::decode(&mut history_source).unwrap();
        // defer decoded_newest.deinit();
        // SAFETY (here and below): live cells of the standalone decoded pages.
        assert_eq!('B' as u32, unsafe {
            (*decoded_newest.get_row_and_cell(0, 0).cell).codepoint()
        });
        let mut decoded_oldest = page::decode(&mut history_source).unwrap();
        // defer decoded_oldest.deinit();
        assert_eq!('A' as u32, unsafe {
            (*decoded_oldest.get_row_and_cell(0, 0).cell).codepoint()
        });
        assert_eq!(Err(io::ReadError::EndOfStream), history_source.take_byte());
        decoded_oldest.deinit();
        decoded_newest.deinit();

        // Restore SCREEN first, then prepend HISTORY directly into that
        // PageList.
        let mut restore_source = io::FixedReader::new(&written);
        let mut decoded_screen = screen::decode(
            &mut restore_source,
            Allocator,
            ScreenOptions {
                cols: 80,
                rows: screen_rows,
                max_scrollback_bytes: None,
                ..ScreenOptions::default()
            },
        )
        .unwrap();
        // defer decoded_screen.deinit();
        assert_eq!(TerminalScreenKey::Primary, decoded_screen.key);
        assert_eq!(
            (source_screen.pages.total_rows - screen_rows as usize) as u64,
            decoded_screen.history_rows,
        );
        let restored = &mut decoded_screen.screen;
        assert!(!restored.semantic_prompt.seen);
        decode(&mut restore_source, TerminalScreenKey::Primary, restored).unwrap();

        assert_eq!(
            source_screen.pages.total_pages(),
            restored.pages.total_pages(),
        );
        let restored_oldest = restored.pages.get_top_left(point::Tag::Screen).node;
        // SAFETY: live nodes/pages of the restored screen; shared reads only.
        unsafe {
            assert_eq!(
                'A' as u32,
                (*(*Node::page(restored_oldest)).get_row_and_cell(0, 0).cell).codepoint(),
            );
            assert_eq!(
                'B' as u32,
                (*(*Node::page((*restored_oldest).next))
                    .get_row_and_cell(0, 0)
                    .cell)
                    .codepoint(),
            );
        }
        assert!(restored.semantic_prompt.seen);
        assert_eq!(Err(io::ReadError::EndOfStream), restore_source.take_byte());
        decoded_screen.deinit();

        // Reuse a writable copy of the sequence for failure-path fixtures.
        let mut encoded = written.clone();
        // defer testing.allocator.free(encoded);
        let first_page_offset = history_offset + record::Header::LEN + Header::LEN;
        let first_payload_len = u32::from_le_bytes(
            encoded[first_page_offset + 2..first_page_offset + 6]
                .try_into()
                .unwrap(),
        );
        let second_page_offset =
            first_page_offset + record::Header::LEN + first_payload_len as usize;

        // Truncate the first PAGE only after its header has exposed a
        // capacity. Its detached allocation is discarded and the live SCREEN
        // list remains completely unchanged.
        let mut truncated_source = io::FixedReader::new(&encoded[0..second_page_offset - 1]);
        let mut decoded_truncated = screen::decode(
            &mut truncated_source,
            Allocator,
            ScreenOptions {
                cols: 80,
                rows: screen_rows,
                max_scrollback_bytes: None,
                ..ScreenOptions::default()
            },
        )
        .unwrap();
        // defer decoded_truncated.deinit();
        {
            let truncated = &mut decoded_truncated.screen;
            let truncated_screen_first = truncated.pages.get_top_left(point::Tag::Screen).node;
            let truncated_screen_page_count = truncated.pages.total_pages();
            assert_eq!(
                Err(DecodeError::EndOfStream),
                decode(&mut truncated_source, TerminalScreenKey::Primary, truncated),
            );
            assert_eq!(
                truncated_screen_page_count,
                truncated.pages.total_pages(),
            );
            assert_eq!(
                truncated_screen_first,
                truncated.pages.get_top_left(point::Tag::Screen).node,
            );
            truncated.pages.assert_integrity();
            truncated.assert_integrity();
        }
        decoded_truncated.deinit();

        // Corrupt only the older PAGE tag. A failure in a later PAGE keeps
        // only the successfully prepended newer pages, contiguous with SCREEN.
        encoded[second_page_offset..second_page_offset + 2]
            .copy_from_slice(&(record::Tag::Screen as u16).to_le_bytes());

        let mut partial_source = io::FixedReader::new(&encoded);
        let mut decoded_partial = screen::decode(
            &mut partial_source,
            Allocator,
            ScreenOptions {
                cols: 80,
                rows: screen_rows,
                max_scrollback_bytes: None,
                ..ScreenOptions::default()
            },
        )
        .unwrap();
        // defer decoded_partial.deinit();
        {
            let partial = &mut decoded_partial.screen;
            let screen_page_count = partial.pages.total_pages();
            assert_eq!(
                Err(DecodeError::UnexpectedRecordTag),
                decode(&mut partial_source, TerminalScreenKey::Primary, partial),
            );
            assert_eq!(screen_page_count + 1, partial.pages.total_pages());
            // SAFETY: live top-left page of the partially restored screen.
            assert_eq!('B' as u32, unsafe {
                (*(*Node::page(partial.pages.get_top_left(point::Tag::Screen).node))
                    .get_row_and_cell(0, 0)
                    .cell)
                    .codepoint()
            });
            assert!(!partial.semantic_prompt.seen);
            partial.pages.assert_integrity();
            partial.assert_integrity();
        }
        decoded_partial.deinit();

        source_screen.deinit();
    }

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
