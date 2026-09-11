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

// const std = @import("std");
// const build_options = @import("terminal_options");
// const Allocator = std.mem.Allocator;
// const test_fixture = @import("fixture.zig");
// const grid = @import("grid.zig");
// const hyperlink = @import("hyperlink.zig");
// const io = @import("io.zig");
// const record = @import("record.zig");
// const style = @import("style.zig");
// const terminal_hyperlink = @import("../hyperlink.zig");
// const terminal_page = @import("../page.zig");
// const terminal_style = @import("../style.zig");
use super::grid;
use super::hyperlink;
use super::io;
use super::io::Reader as _;
use super::record;
use super::style;
use crate::hyperlink as terminal_hyperlink;
use crate::options;
use crate::page as terminal_page;
use crate::style as terminal_style;

// Frequent constants we use
#[cfg(test)]
use terminal_hyperlink::Hyperlink as TerminalHyperlink;
use terminal_hyperlink::Id as TerminalHyperlinkId;
use terminal_hyperlink::PageEntry as TerminalHyperlinkPageEntry;
use terminal_page::Capacity as TerminalPageCapacity;
use terminal_page::Page as TerminalPage;
use terminal_style::Id as TerminalStyleId;
use terminal_style::Style as TerminalStyle;

/// const PayloadEncodeError = hyperlink.EncodeError || grid.EncodeError;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PayloadEncodeError {
    // std.Io.Writer.Error
    WriteFailed,
    // hyperlink.EncodeError
    InvalidUri,
    InvalidExplicitId,
    StringTooLong,
    // grid.EncodeError
    InvalidWideCell,
    TooManyGraphemes,
}

impl From<io::WriteFailed> for PayloadEncodeError {
    fn from(_: io::WriteFailed) -> PayloadEncodeError {
        PayloadEncodeError::WriteFailed
    }
}

impl From<hyperlink::EncodeError> for PayloadEncodeError {
    fn from(err: hyperlink::EncodeError) -> PayloadEncodeError {
        match err {
            hyperlink::EncodeError::WriteFailed => PayloadEncodeError::WriteFailed,
            hyperlink::EncodeError::InvalidUri => PayloadEncodeError::InvalidUri,
            hyperlink::EncodeError::InvalidExplicitId => PayloadEncodeError::InvalidExplicitId,
            hyperlink::EncodeError::StringTooLong => PayloadEncodeError::StringTooLong,
        }
    }
}

impl From<grid::EncodeError> for PayloadEncodeError {
    fn from(err: grid::EncodeError) -> PayloadEncodeError {
        match err {
            grid::EncodeError::WriteFailed => PayloadEncodeError::WriteFailed,
            grid::EncodeError::InvalidWideCell => PayloadEncodeError::InvalidWideCell,
            grid::EncodeError::TooManyGraphemes => PayloadEncodeError::TooManyGraphemes,
        }
    }
}

/// const PayloadDecodeError = std.Io.Reader.Error || Header.CapacityError ||
///     grid.DecodeError || error{...};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PayloadDecodeError {
    // std.Io.Reader.Error
    ReadFailed,
    EndOfStream,
    // Header.CapacityError
    InvalidDimensions,
    // grid.DecodeError
    InvalidRowCellCount,

    /// The hyperlink kind is not defined by snapshot version 1.
    InvalidKind,

    /// Native page backing memory could not be allocated.
    OutOfMemory,

    /// The caller-provided page does not have the advertised capacity.
    InvalidDestinationCapacity,
}

impl From<io::ReadError> for PayloadDecodeError {
    fn from(err: io::ReadError) -> PayloadDecodeError {
        match err {
            io::ReadError::ReadFailed => PayloadDecodeError::ReadFailed,
            io::ReadError::EndOfStream => PayloadDecodeError::EndOfStream,
        }
    }
}

impl From<CapacityError> for PayloadDecodeError {
    fn from(err: CapacityError) -> PayloadDecodeError {
        match err {
            CapacityError::InvalidDimensions => PayloadDecodeError::InvalidDimensions,
        }
    }
}

impl From<grid::DecodeError> for PayloadDecodeError {
    fn from(err: grid::DecodeError) -> PayloadDecodeError {
        match err {
            grid::DecodeError::ReadFailed => PayloadDecodeError::ReadFailed,
            grid::DecodeError::EndOfStream => PayloadDecodeError::EndOfStream,
            grid::DecodeError::InvalidRowCellCount => PayloadDecodeError::InvalidRowCellCount,
        }
    }
}

impl From<hyperlink::DecodePageError> for PayloadDecodeError {
    fn from(err: hyperlink::DecodePageError) -> PayloadDecodeError {
        match err {
            hyperlink::DecodePageError::ReadFailed => PayloadDecodeError::ReadFailed,
            hyperlink::DecodePageError::EndOfStream => PayloadDecodeError::EndOfStream,
            hyperlink::DecodePageError::InvalidKind => PayloadDecodeError::InvalidKind,
        }
    }
}

/// Errors possible while encoding a complete PAGE record.
// pub const EncodeError = PayloadEncodeError || record.Writer.FinishError;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EncodeError {
    // PayloadEncodeError
    WriteFailed,
    InvalidUri,
    InvalidExplicitId,
    StringTooLong,
    InvalidWideCell,
    TooManyGraphemes,
    // record.Writer.FinishError
    PayloadTooLarge,
}

impl From<PayloadEncodeError> for EncodeError {
    fn from(err: PayloadEncodeError) -> EncodeError {
        match err {
            PayloadEncodeError::WriteFailed => EncodeError::WriteFailed,
            PayloadEncodeError::InvalidUri => EncodeError::InvalidUri,
            PayloadEncodeError::InvalidExplicitId => EncodeError::InvalidExplicitId,
            PayloadEncodeError::StringTooLong => EncodeError::StringTooLong,
            PayloadEncodeError::InvalidWideCell => EncodeError::InvalidWideCell,
            PayloadEncodeError::TooManyGraphemes => EncodeError::TooManyGraphemes,
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

/// Encode one complete PAGE record from a native page.
///
/// The payload is built in the stream's reusable record buffer. If payload
/// encoding fails, no bytes from this record are emitted.
pub fn encode<W: io::Writer + ?Sized>(
    page: &TerminalPage,
    destination: &mut record::Writer<'_, W>,
) -> Result<(), EncodeError> {
    let payload = destination.begin(record::Tag::Page);
    // errdefer destination.cancel();
    if let Err(err) = encode_payload(page, payload) {
        destination.cancel();
        return Err(err.into());
    }
    if let Err(err) = destination.finish() {
        destination.cancel();
        return Err(err.into());
    }
    Ok(())
}

/// Errors possible while decoding and validating a complete PAGE record.
// pub const DecodeError = PayloadDecodeError || record.Reader.InitError ||
//     record.Reader.FinishError || TerminalPage.IntegrityError || error{...};
//
// PORT NOTE: the reference merges `TerminalPage.IntegrityError`'s members
// into the set; the port carries them behind one wrapping variant (no test
// compares an individual integrity member through this set).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecodeError {
    // PayloadDecodeError
    ReadFailed,
    EndOfStream,
    InvalidDimensions,
    InvalidRowCellCount,
    InvalidKind,
    OutOfMemory,
    InvalidDestinationCapacity,
    // record.Reader.InitError
    InvalidTag,
    // record.Reader.FinishError
    InvalidChecksum,
    PayloadNotExhausted,
    // TerminalPage.IntegrityError
    Integrity(terminal_page::IntegrityError),

    /// The next record is valid but is not a PAGE.
    UnexpectedRecordTag,
}

impl From<PayloadDecodeError> for DecodeError {
    fn from(err: PayloadDecodeError) -> DecodeError {
        match err {
            PayloadDecodeError::ReadFailed => DecodeError::ReadFailed,
            PayloadDecodeError::EndOfStream => DecodeError::EndOfStream,
            PayloadDecodeError::InvalidDimensions => DecodeError::InvalidDimensions,
            PayloadDecodeError::InvalidRowCellCount => DecodeError::InvalidRowCellCount,
            PayloadDecodeError::InvalidKind => DecodeError::InvalidKind,
            PayloadDecodeError::OutOfMemory => DecodeError::OutOfMemory,
            PayloadDecodeError::InvalidDestinationCapacity => {
                DecodeError::InvalidDestinationCapacity
            }
        }
    }
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

/// Decode and validate one complete PAGE record into a fresh native page.
///
/// The record tag, payload boundary, CRC32C, payload contents, and final page
/// integrity are all validated before the page is returned. `alloc` is used
/// only for temporary decode remaps and integrity-check storage.
pub fn decode(reader: &mut dyn io::Reader) -> Result<TerminalPage, DecodeError> {
    let mut decoder = Decoder::init(reader)?;

    let mut page = TerminalPage::init(decoder.capacity()).map_err(|_| DecodeError::OutOfMemory)?;
    // errdefer page.deinit();
    if let Err(err) = decoder.decode(&mut page) {
        page.deinit();
        return Err(err);
    }
    Ok(page)
}

/// PAGE record decoder where the caller is responsible for owning
/// the Page memory. This is particularly useful paired with
/// PageList.Builder so you can build up a proper PageList with
/// valid memory ownership.
pub struct Decoder<'s> {
    record_reader: record::Reader<'s>,
    header: Header,
}

impl<'s> Decoder<'s> {
    /// Begin decoding one PAGE record and expose its required capacity.
    /// After this, call `capacity` to get the capacity to allocate
    /// the page properly, then `decode` into it.
    // PORT NOTE: the reference initializes in place (`self: *Decoder`); the
    // flattened `record::Reader` port makes this a plain constructor.
    pub fn init(reader: &'s mut dyn io::Reader) -> Result<Decoder<'s>, DecodeError> {
        let mut record_reader = record::Reader::init(reader)?;
        if record_reader.header.tag != record::Tag::Page {
            return Err(DecodeError::UnexpectedRecordTag);
        }

        let header = Header::decode(record_reader.payload_reader())?;
        let _ = header
            .page_capacity()
            .map_err(PayloadDecodeError::from)
            .map_err(DecodeError::from)?;
        Ok(Decoder {
            record_reader,
            header,
        })
    }

    /// Return the exact native capacity advertised by the PAGE header.
    pub fn capacity(&self) -> TerminalPageCapacity {
        self.header.page_capacity().unwrap()
    }

    /// Largest remaining PAGE payload staged into one contiguous buffer.
    /// This comfortably covers every payload a standard-capacity page can
    /// produce while keeping the allocation an untrusted length can force
    /// far below the record framing's four-byte length limit.
    const MAX_STAGED_PAYLOAD: usize = 8 * 1024 * 1024;

    /// Decode the remaining payload into caller-owned native page storage.
    ///
    /// The destination must be freshly initialized with `capacity`. `alloc`
    /// is used only for temporary staging, ID remaps, and integrity-check
    /// storage.
    pub fn decode(&mut self, destination: &mut TerminalPage) -> Result<(), DecodeError> {
        // if (!std.meta.eql(destination.capacity, self.capacity()))
        if destination.capacity != self.capacity() {
            return Err(DecodeError::InvalidDestinationCapacity);
        }

        destination.size = terminal_page::PageSize {
            cols: self.header.columns,
            rows: self.header.rows,
        };

        // `init` consumed exactly the fixed header from the declared
        // payload, so this is the byte count of the tables and grid.
        let remaining = self.record_reader.header.payload_len as usize - Header::LEN;

        if self.record_reader.payload_reader().buffered_len() >= remaining {
            // The complete payload is already buffered, e.g. borrowed from
            // an in-memory snapshot. Parse it in place with no staging
            // copy; `finish` still enforces the CRC and exact exhaustion.
            decode_payload_body(
                self.record_reader.payload_reader(),
                destination,
                self.header,
            )
            .map_err(DecodeError::from)?;
        } else if remaining <= Self::MAX_STAGED_PAYLOAD {
            // Stage the payload with one bulk read. The whole payload passes
            // through the checksum hasher as one update and the payload
            // decoders then parse a flat buffer, which keeps per-row work free
            // of stream adapters. The CRC and exact-length checks in `finish`
            // are unaffected.
            let mut staged = vec![0u8; remaining];
            self.record_reader
                .payload_reader()
                .read_slice_all(&mut staged)?;

            let mut staged_reader = io::FixedReader::new(&staged);
            decode_payload_body(&mut staged_reader, destination, self.header)
                .map_err(DecodeError::from)?;
            if staged_reader.buffered_len() != 0 {
                return Err(DecodeError::PayloadNotExhausted);
            }
        } else {
            // A payload this large is either hostile or a page far beyond
            // native capacities. Decode it through the streaming payload
            // reader so its declared length cannot force an allocation.
            decode_payload_body(
                self.record_reader.payload_reader(),
                destination,
                self.header,
            )
            .map_err(DecodeError::from)?;
        }
        self.record_reader.finish()?;

        // The decoder normalizes every semantic value, so a complete decode
        // upholds native page invariants by construction. Verifying them
        // again is a defense against decoder bugs and follows the native
        // page policy: full integrity verification only when slow runtime
        // safety is enabled.
        if options::SLOW_RUNTIME_SAFETY {
            destination
                .verify_integrity()
                .map_err(DecodeError::Integrity)?;
        }
        Ok(())
    }
}

/// Encode a PAGE payload directly from a native page.
fn encode_payload(
    page: &TerminalPage,
    writer: &mut dyn io::Writer,
) -> Result<(), PayloadEncodeError> {
    // Write header
    Header::init(page).encode(writer)?;

    // Sparse styles
    // SAFETY: iterating the page's own style set against its own base.
    let mut style_it = unsafe { page.styles.iterator(page.memory_ptr()) };
    while let Some(entry) = style_it.next() {
        io::write_int::<u16>(writer, entry.id)?;
        // SAFETY: `value_ptr` points into the live set storage.
        style::encode(unsafe { *entry.value_ptr }, writer)?;
    }

    // Sparse hyperlinks
    // SAFETY: as above for the hyperlink set.
    let mut hyperlink_it = unsafe { page.hyperlink_set.iterator(page.memory_ptr()) };
    while let Some(entry) = hyperlink_it.next() {
        io::write_int::<u16>(writer, entry.id)?;
        // SAFETY: `value_ptr` points into the live set storage.
        hyperlink::encode(&page_hyperlink(page, unsafe { &*entry.value_ptr }), writer)?;
    }

    // Rows and cells
    grid::encode(page, writer)?;
    Ok(())
}

/// Decode a PAGE payload directly into a fresh native page.
///
/// `alloc` is used only for the temporary native-ID remap tables. Styles,
/// hyperlinks, strings, graphemes, rows, and cells are stored in the page.
// PORT NOTE: as in the reference, only this file's tests consume the unframed
// payload decoder today (`decode`/`Decoder` are the framed entry points).
#[allow(dead_code)]
fn decode_payload(reader: &mut dyn io::Reader) -> Result<TerminalPage, PayloadDecodeError> {
    let header = Header::decode(reader)?;
    let capacity = header.page_capacity()?;
    let mut page = TerminalPage::init(capacity).map_err(|_| PayloadDecodeError::OutOfMemory)?;
    // errdefer page.deinit();
    if let Err(err) = decode_payload_body(reader, &mut page, header) {
        page.deinit();
        return Err(err);
    }
    Ok(page)
}

/// Decode PAGE tables and grid after the header into initialized native page
/// storage with the exact advertised capacity and dimensions.
fn decode_payload_body(
    reader: &mut dyn io::Reader,
    page: &mut TerminalPage,
    header: Header,
) -> Result<(), PayloadDecodeError> {
    page.pause_integrity_checks(true);
    // defer page.pauseIntegrityChecks(false);
    let result = decode_payload_body_inner(reader, page, header);
    page.pause_integrity_checks(false);
    result
}

fn decode_payload_body_inner(
    reader: &mut dyn io::Reader,
    page: &mut TerminalPage,
    header: Header,
) -> Result<(), PayloadDecodeError> {
    // Pages without styles or hyperlinks, the common case for plain
    // scrollback, skip the remap tables entirely: every encoded cell ID
    // resolves to the default through the empty remap.
    let mut style_remap: grid::StyleRemap = if header.style_count > 0 {
        grid::StyleRemap::init()
    } else {
        grid::StyleRemap::empty()
    };
    // defer style_remap.deinit(alloc); — dropped.

    let mut hyperlink_remap: grid::HyperlinkRemap = if header.hyperlink_count > 0 {
        grid::HyperlinkRemap::init()
    } else {
        grid::HyperlinkRemap::empty()
    };
    // defer hyperlink_remap.deinit(alloc); — dropped.

    // Styles. The complete fixed-size entry is parsed from the buffered
    // payload when possible so each entry costs no reader calls.
    let style_entry_len = core::mem::size_of::<TerminalStyleId>() + style::LEN;
    for _ in 0..header.style_count {
        let (native_id, value): (TerminalStyleId, Option<TerminalStyle>) = {
            if reader.buffered_len() >= style_entry_len {
                let bytes = &reader.buffered()[0..style_entry_len];
                let entry = (
                    u16::from_le_bytes(bytes[0..2].try_into().unwrap()),
                    style::parse_or_null(bytes[2..2 + style::LEN].try_into().unwrap()),
                );
                // defer reader.toss(style_entry_len);
                reader.toss(style_entry_len);
                entry
            } else {
                (io::read_int::<u16>(reader)?, style::decode_or_null(reader)?)
            }
        };

        // Zero is reserved for the implicit default. For a duplicate encoded
        // ID, the first entry wins and this complete entry is simply ignored.
        if native_id == 0 || style_remap.contains(native_id) {
            continue;
        }

        // Invalid/default styles map to the native default. `add` returns
        // the existing entry for a repeated concrete value, taking one
        // reference either way, while capacity failure degrades only this
        // style. The references are surrendered through the remap below
        // once every cell reference is installed.
        let decoded_id: TerminalStyleId = match value {
            Some(valid) => {
                if valid.default_style() {
                    0
                } else {
                    // SAFETY: adding to this page's own style set.
                    unsafe { page.styles.add(page.memory_ptr(), valid) }.unwrap_or(0)
                }
            }
            None => 0,
        };
        style_remap.put(native_id, decoded_id);
    }

    // Hyperlinks
    for _ in 0..header.hyperlink_count {
        let native_id = io::read_int::<u16>(reader)?;
        let decoded_id = hyperlink::decode_page(page, reader)?;

        // As with styles, zero is reserved and the first duplicate encoded ID
        // wins. Release any native entry reference created for an ignored ID.
        if native_id == 0 || hyperlink_remap.contains(native_id) {
            if decoded_id != 0 {
                release_page_hyperlink(page, decoded_id);
            }
            continue;
        }

        // Zero records an ignored table entry. Grid decoding treats every
        // reference to it as no hyperlink.
        hyperlink_remap.put(native_id, decoded_id);
    }

    // Rows and cells
    grid::decode(page, reader, &style_remap, &hyperlink_remap)?;

    // Every accepted table entry took one reference through `add` so grid
    // decoding can safely attach its style to any number of cells. Unlike
    // organically built pages, those references do not themselves represent
    // cells. Release through the encoded-ID remap, so duplicate values
    // which deduplicated to the same native ID each surrender their own
    // reference; unused entries then become dead and disappear from
    // canonical re-encoding.
    {
        let base = page.memory_ptr();
        for encoded_id in style_remap.seen_iter() {
            let id = style_remap.entries[encoded_id];
            if id != 0 {
                // SAFETY: `id` was assigned by this page's style set above.
                unsafe { page.styles.release(base, id) };
            }
        }
    }

    // Hyperlink insertion likewise creates one temporary reference for every
    // accepted wire table entry. Release through the encoded-ID remap rather
    // than the native set so duplicate values which deduplicated to the same
    // native ID each surrender their own reference.
    for encoded_id in hyperlink_remap.seen_iter() {
        let id = hyperlink_remap.entries[encoded_id];
        if id != 0 {
            release_page_hyperlink(page, id);
        }
    }
    Ok(())
}

/// `page.hyperlink_set.release(page.memory, id)` — with the page context the
/// crate's `RefCountedSet` needs so a reference count reaching zero can free
/// the entry's page-owned strings (the `set_hyperlink_inner` pattern).
fn release_page_hyperlink(page: &mut TerminalPage, id: TerminalHyperlinkId) {
    let page_ptr: *mut TerminalPage = page;
    page.hyperlink_set.context.set_page(page_ptr);
    let base = page.memory_ptr();
    // SAFETY: `id` was assigned by this page's hyperlink set and holds at
    // least the reference being surrendered here.
    unsafe { page.hyperlink_set.release(base, id) };
}

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

/// Upper bound on what `Page::layout` adds on top of the capacity's own
/// regions: bitmap-allocator metadata, the grapheme and hyperlink hash maps,
/// per-region alignment, and the final rounding to `page_size_min`. The
/// multiple covers everything that scales with a declared capacity and the
/// fixed term covers everything that does not (the rounding, the map headers,
/// and the set structures). Both are deliberately generous and are PINNED by
/// `the_page_size_bound_is_an_upper_bound_on_the_real_layout` below, which
/// sweeps the extremes of every capacity field.
const LAYOUT_OVERHEAD_MULTIPLE: u64 = 8;
const LAYOUT_OVERHEAD_FIXED: u64 = 1 << 20;

/// An upper bound on the style set's per-item size, asserted by the same test.
const STYLE_ITEM_SIZE_BOUND: u64 = 64;

/// The bytes a declared capacity asks for in its own right, before any
/// allocator or map overhead. Computed in `u64` so it is identical on every
/// target, including the 32-bit ones (`wasm32`) where `Page::layout`'s own
/// `usize` arithmetic would wrap.
fn declared_region_bytes(cap: TerminalPageCapacity) -> u64 {
    let cols = cap.cols as u64;
    let rows = cap.rows as u64;
    rows * (core::mem::size_of::<terminal_page::Row>() as u64)
        + cols * rows * (core::mem::size_of::<terminal_page::Cell>() as u64)
        + (cap.styles as u64) * STYLE_ITEM_SIZE_BOUND
        + (cap.hyperlink_bytes as u64)
        + (cap.grapheme_bytes as u64)
        + (cap.string_bytes as u64)
}

/// Reject a declared PAGE capacity whose native page could not be allocated.
///
/// PORT NOTE: reference defect (R13, UPSTREAM-DEFECTS.md UD64).
/// `Header.pageCapacity` (`snapshot/page.zig:560-578`) says it validates
/// "native allocation requirements" but checks only that `columns` and `rows`
/// are nonzero. The four capacity fields then flow, unchecked, into
/// `createPageExt`, where `assert(layout.total_size <= size.max_page_size)`
/// (`PageList.zig:4566`) is the first thing that looks at them — an assertion
/// whose own comment says callers are expected to handle the limit
/// themselves. A single PAGE header with `string_capacity_bytes = 0xFFFFFFFF`
/// therefore aborts the process (in the port, in every profile, inside an
/// `extern "C"` entry point where the panic cannot unwind). The CRC is no
/// barrier: the capacity is used before the payload checksum is verified.
///
/// R7/R13 boundary: that assertion is an INVARIANT for the native callers,
/// whose capacities are internal and provably bounded (`initialCapacity`
/// proves it at comptime; `increaseCapacity` returns an error above the
/// limit), so it stays a checked assertion there. What must not happen is
/// decoded input reaching it. The check belongs here, at the decoder
/// boundary, and reuses `pageCapacity`'s own existing error.
///
/// Both routes into `createPageExt` — the SCREEN pages via
/// `Builder::allocate_page` and the HISTORY pages via
/// `PageList::allocate_page` — obtain their capacity from `Decoder::capacity`,
/// so this one site covers both.
fn check_native_page_size(cap: TerminalPageCapacity) -> Result<(), CapacityError> {
    // Bound the layout before computing it. This is what makes the exact
    // computation below safe on a 32-bit target, where `Page::layout` sums
    // `usize` and a declared capacity can overflow it (a panic in the port's
    // checked profiles, and in `release-fast` a wrapped, small `total_size`
    // that passes the assertion and leaves the page buffer undersized for the
    // grid decode that follows).
    let bound = declared_region_bytes(cap)
        .saturating_mul(LAYOUT_OVERHEAD_MULTIPLE)
        .saturating_add(LAYOUT_OVERHEAD_FIXED);
    if bound > crate::size::MAX_PAGE_SIZE as u64 {
        return Err(CapacityError::InvalidDimensions);
    }

    // The bound holds, so the real layout fits a `usize` on every target and
    // its exact total can be compared with the limit the native assertion
    // enforces.
    if TerminalPage::layout(cap).total_size as u64 > crate::size::MAX_PAGE_SIZE as u64 {
        return Err(CapacityError::InvalidDimensions);
    }
    Ok(())
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
    fn page_capacity(self) -> Result<TerminalPageCapacity, CapacityError> {
        if self.columns == 0 || self.rows == 0 {
            return Err(CapacityError::InvalidDimensions);
        }

        let cap = TerminalPageCapacity {
            cols: self.columns,
            rows: self.rows,
            styles: self.style_capacity,
            hyperlink_bytes: self.hyperlink_capacity_bytes,
            grapheme_bytes: self.grapheme_capacity_bytes,
            string_bytes: self.string_capacity_bytes,
        };
        check_native_page_size(cap)?;
        Ok(cap)
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
    use crate::snapshot::io::Writer as _;

    // const test_page_fixture = test_fixture.parse(@embedFile("testdata/page-v1.hex"));
    fn test_page_fixture() -> Vec<u8> {
        fixture::parse(include_str!("testdata/page-v1.hex"))
    }

    // const test_header_fixture = test_fixture.parse(@embedFile("testdata/page-header-v1.hex"));
    fn test_header_fixture() -> Vec<u8> {
        fixture::parse(include_str!("testdata/page-header-v1.hex"))
    }

    // const test_empty_framed_page_fixture = test_fixture.parse(@embedFile("testdata/page-empty-record-v1.hex"));
    fn test_empty_framed_page_fixture() -> Vec<u8> {
        fixture::parse(include_str!("testdata/page-empty-record-v1.hex"))
    }

    /// `std.Io.Writer.Discarding` — counts written bytes without storing them.
    struct DiscardingWriter {
        count: u64,
    }

    impl DiscardingWriter {
        // `.init(&.{})`
        fn init() -> DiscardingWriter {
            DiscardingWriter { count: 0 }
        }
    }

    impl io::Writer for DiscardingWriter {
        fn write_all(&mut self, bytes: &[u8]) -> Result<(), io::WriteFailed> {
            self.count += bytes.len() as u64;
            Ok(())
        }
    }

    // test "framed PAGE encode and decode a sparse native page"
    #[test]
    fn framed_page_encode_and_decode_a_sparse_native_page() {
        use crate::style::{Color, Flags};
        let test_page_fixture = test_page_fixture();

        let capacity = TerminalPageCapacity {
            cols: 3,
            rows: 2,
            styles: 8,
            hyperlink_bytes: 512,
            grapheme_bytes: 128,
            string_bytes: 256,
        };
        let mut page = TerminalPage::init(capacity).unwrap();

        // Create holes in both source tables so the encoded IDs exercise sparse
        // table remapping rather than coincidentally matching decoded IDs.
        let base = page.memory_ptr();
        let (style_a, dead_style, style_b) = unsafe {
            let style_a = page
                .styles
                .add(
                    base,
                    TerminalStyle {
                        flags: Flags {
                            bold: true,
                            ..Default::default()
                        },
                        ..Default::default()
                    },
                )
                .unwrap();
            let dead_style = page
                .styles
                .add(
                    base,
                    TerminalStyle {
                        flags: Flags {
                            italic: true,
                            ..Default::default()
                        },
                        ..Default::default()
                    },
                )
                .unwrap();
            let style_b = page
                .styles
                .add(
                    base,
                    TerminalStyle {
                        bg_color: Color::Palette(42),
                        ..Default::default()
                    },
                )
                .unwrap();
            page.styles.release(base, dead_style);
            (style_a, dead_style, style_b)
        };
        let _ = dead_style;

        let first = page.get_row_and_cell(0, 0);
        unsafe {
            (*first.cell).set_style_id(style_a);
            (*first.row).set_styled(true);
        }

        let second = page.get_row_and_cell(1, 0);
        unsafe {
            (*second.cell).set_style_id(style_b);
            (*second.row).set_styled(true);
        }

        let third = page.get_row_and_cell(2, 0);
        unsafe {
            page.styles.use_id(base, style_a);
            (*third.cell).set_style_id(style_a);
            (*third.row).set_styled(true);
        }

        let hyperlink_a = page
            .insert_hyperlink(TerminalHyperlink {
                id: terminal_hyperlink::HyperlinkId::Explicit(b"a".to_vec()),
                uri: b"alpha".to_vec(),
            })
            .unwrap();
        let dead_hyperlink = page
            .insert_hyperlink(TerminalHyperlink {
                id: terminal_hyperlink::HyperlinkId::Explicit(b"dead-id".to_vec()),
                uri: b"dead-uri".to_vec(),
            })
            .unwrap();
        let hyperlink_b = page
            .insert_hyperlink(TerminalHyperlink {
                id: terminal_hyperlink::HyperlinkId::Implicit(0x01020304),
                uri: b"beta".to_vec(),
            })
            .unwrap();
        release_page_hyperlink(&mut page, dead_hyperlink);

        unsafe { page.hyperlink_set.use_id(page.memory_ptr(), hyperlink_a) };
        page.set_hyperlink(first.row, first.cell, hyperlink_a)
            .unwrap();
        page.set_hyperlink(second.row, second.cell, hyperlink_b)
            .unwrap();
        page.set_hyperlink(third.row, third.cell, hyperlink_a)
            .unwrap();

        // Cover graphemes, wide-cell pairs, colors, protection, and semantic
        // metadata in one compact two-row page.
        let grapheme = page.get_row_and_cell(0, 1);
        unsafe { *grapheme.cell = terminal_page::Cell::init('x' as u32) };
        page.set_graphemes(grapheme.row, grapheme.cell, &[0x0301, 0x0302])
            .unwrap();

        unsafe {
            (*first.cell).set_content_codepoint('A' as u32);
            (*first.cell).set_wide(terminal_page::Wide::Wide);
            (*first.cell).set_protected(true);
            (*first.cell).set_semantic_content(terminal_page::SemanticContent::Prompt);
            (*first.row).set_semantic_prompt(terminal_page::SemanticPrompt::Prompt);

            (*second.cell).set_wide(terminal_page::Wide::SpacerTail);
            (*second.cell).set_semantic_content(terminal_page::SemanticContent::Input);

            (*third.cell).set_content_tag(terminal_page::ContentTag::BgColorPalette);
            (*third.cell).set_content_color_palette(7);
        }

        let rgb = page.get_row_and_cell(1, 1);
        unsafe {
            (*rgb.cell).set_content_tag(terminal_page::ContentTag::BgColorRgb);
            (*rgb.cell).set_content_color_rgb(terminal_page::CellRgb {
                r: 0xaa,
                g: 0xbb,
                b: 0xcc,
            });
            (*rgb.cell).set_protected(true);
        }

        let spacer_head = page.get_row_and_cell(2, 1);
        unsafe {
            (*spacer_head.cell).set_wide(terminal_page::Wide::SpacerHead);
            (*spacer_head.row).set_wrap(true);
            (*spacer_head.row).set_wrap_continuation(true);
            (*spacer_head.row)
                .set_semantic_prompt(terminal_page::SemanticPrompt::PromptContinuation);
        }

        let header = Header {
            columns: 3,
            rows: 2,
            style_count: 2,
            hyperlink_count: 2,
            style_capacity: 8,
            hyperlink_capacity_bytes: 512,
            grapheme_capacity_bytes: 128,
            string_capacity_bytes: 256,
        };
        assert_eq!(header, Header::init(&page));

        // Lock the payload bytes and independently verify the counting writer sees
        // the same encoded length.
        let mut counter = DiscardingWriter::init();
        encode_payload(&page, &mut counter).unwrap();

        let mut encoded = [0u8; 512];
        let mut writer = io::FixedWriter::new(&mut encoded);
        encode_payload(&page, &mut writer).unwrap();

        fixture::expect_equal(
            fixture::Kind::Page,
            "src/snapshot/testdata/page-v1.hex",
            "snapshot_fixture-page-v1.hex",
            &test_page_fixture,
            writer.buffered(),
        );
        assert_eq!(test_page_fixture.len() as u64, counter.count);

        // Decode the checked-in reference rather than the just-generated bytes.
        // A one-byte backing buffer exercises streaming reads across every field.
        let mut source = io::FixedReader::new(&test_page_fixture);
        let mut read_buf = [0u8; 1];
        let mut limited = io::LimitedReader::new(&mut source, io::Limit::Unlimited, &mut read_buf);
        let mut decoded = decode_payload(&mut limited).unwrap();

        assert_eq!(header, Header::init(&decoded));
        decoded.verify_integrity().unwrap();

        // Decoding compacts the sparse source IDs while preserving table values
        // and all cell references through the remap tables.
        let decoded_base = decoded.memory_ptr();
        unsafe {
            let mut style_it = decoded.styles.iterator(decoded_base);
            let decoded_style_a = style_it.next().unwrap();
            assert_eq!(1 as TerminalStyleId, decoded_style_a.id);
            assert!(TerminalStyle {
                flags: Flags {
                    bold: true,
                    ..Default::default()
                },
                ..Default::default()
            }
            .eql(&*decoded_style_a.value_ptr));
            let decoded_style_b = style_it.next().unwrap();
            assert_eq!(2 as TerminalStyleId, decoded_style_b.id);
            assert!(TerminalStyle {
                bg_color: Color::Palette(42),
                ..Default::default()
            }
            .eql(&*decoded_style_b.value_ptr));
            assert!(style_it.next().is_none());
            assert_eq!(
                2u16,
                decoded.styles.ref_count(decoded_base, decoded_style_a.id),
            );
            assert_eq!(
                1u16,
                decoded.styles.ref_count(decoded_base, decoded_style_b.id),
            );

            let mut hyperlink_it = decoded.hyperlink_set.iterator(decoded_base);
            let decoded_link_a = hyperlink_it.next().unwrap();
            assert_eq!(1 as TerminalHyperlinkId, decoded_link_a.id);
            let decoded_hyperlink_a = page_hyperlink(&decoded, &*decoded_link_a.value_ptr);
            match &decoded_hyperlink_a.id {
                terminal_hyperlink::HyperlinkId::Explicit(id) => {
                    assert_eq!(b"a", id.as_slice())
                }
                _ => panic!("expected explicit id"),
            }
            assert_eq!(b"alpha", decoded_hyperlink_a.uri.as_slice());
            let decoded_link_b = hyperlink_it.next().unwrap();
            assert_eq!(2 as TerminalHyperlinkId, decoded_link_b.id);
            let decoded_hyperlink_b = page_hyperlink(&decoded, &*decoded_link_b.value_ptr);
            match &decoded_hyperlink_b.id {
                terminal_hyperlink::HyperlinkId::Implicit(id) => {
                    assert_eq!(0x01020304u32, *id)
                }
                _ => panic!("expected implicit id"),
            }
            assert_eq!(b"beta", decoded_hyperlink_b.uri.as_slice());
            assert!(hyperlink_it.next().is_none());
        }

        let decoded_first = decoded.get_row_and_cell(0, 0);
        unsafe {
            assert_eq!(1 as TerminalStyleId, (*decoded_first.cell).style_id());
        }
        assert_eq!(
            1 as TerminalHyperlinkId,
            decoded.lookup_hyperlink(decoded_first.cell).unwrap(),
        );

        let decoded_second = decoded.get_row_and_cell(1, 0);
        unsafe {
            assert_eq!(2 as TerminalStyleId, (*decoded_second.cell).style_id());
        }
        assert_eq!(
            2 as TerminalHyperlinkId,
            decoded.lookup_hyperlink(decoded_second.cell).unwrap(),
        );

        // The first re-encode reflects compacted IDs; subsequent round trips must
        // stabilize byte-for-byte.
        let mut reencoded = [0u8; 512];
        let mut rewriter = io::FixedWriter::new(&mut reencoded);
        encode_payload(&decoded, &mut rewriter).unwrap();
        assert!(writer.buffered() != rewriter.buffered());

        let mut reencoded_reader = io::FixedReader::new(rewriter.buffered());
        let mut decoded_again = decode_payload(&mut reencoded_reader).unwrap();
        decoded_again.verify_integrity().unwrap();

        let mut reencoded_again = [0u8; 512];
        let mut rewriter_again = io::FixedWriter::new(&mut reencoded_again);
        encode_payload(&decoded_again, &mut rewriter_again).unwrap();
        assert_eq!(rewriter.buffered(), rewriter_again.buffered());

        page.deinit();
        decoded.deinit();
        decoded_again.deinit();
    }

    // test "framed PAGE golden empty record"
    #[test]
    fn framed_page_golden_empty_record() {
        let test_empty_framed_page_fixture = test_empty_framed_page_fixture();

        let capacity = TerminalPageCapacity {
            cols: 1,
            rows: 1,
            styles: 0,
            hyperlink_bytes: 0,
            grapheme_bytes: 0,
            string_bytes: 0,
        };
        let mut page = TerminalPage::init(capacity).unwrap();

        let mut destination = io::AllocatingWriter::new();
        let mut stream = record::Writer::init(&mut destination);
        encode(&page, &mut stream).unwrap();
        fixture::expect_equal(
            fixture::Kind::Bytes,
            "src/snapshot/testdata/page-empty-record-v1.hex",
            "snapshot_fixture-page-empty-record-v1.hex",
            &test_empty_framed_page_fixture,
            stream.destination().written(),
        );
        stream.deinit();

        let mut source = io::FixedReader::new(&test_empty_framed_page_fixture);
        let mut decoded = decode(&mut source).unwrap();
        assert_eq!(Header::init(&page), Header::init(&decoded));

        page.deinit();
        decoded.deinit();
    }

    // test "framed PAGE rejects incomplete wide cells transactionally"
    #[test]
    fn framed_page_rejects_incomplete_wide_cells_transactionally() {
        // One column puts the wide cell at the row end. Two columns put an
        // ordinary narrow cell after it. Neither case supplies a spacer tail.
        for columns in [1u16, 2] {
            let mut page = TerminalPage::init(TerminalPageCapacity::new(columns, 1)).unwrap();
            let first = page.get_row_and_cell(0, 0).cell;
            unsafe {
                *first = terminal_page::Cell::init('A' as u32);
                (*first).set_wide(terminal_page::Wide::Wide);
            }
            if columns > 1 {
                unsafe {
                    *page.get_row_and_cell(1, 0).cell = terminal_page::Cell::init('B' as u32);
                }
            }

            let mut destination = io::AllocatingWriter::new();
            destination.write_all(b"prefix").unwrap();
            let mut stream = record::Writer::init(&mut destination);
            assert_eq!(
                Err(EncodeError::InvalidWideCell),
                encode(&page, &mut stream),
            );
            assert_eq!(b"prefix", stream.destination().written());
            stream.deinit();

            page.deinit();
        }
    }

    // test "framed PAGE rejects a different record tag"
    #[test]
    fn framed_page_rejects_a_different_record_tag() {
        let mut wrong_tag = test_empty_framed_page_fixture();
        wrong_tag[0..2].copy_from_slice(&(record::Tag::Screen as u16).to_le_bytes());
        let mut reader = io::FixedReader::new(&wrong_tag);
        assert_eq!(
            Err(DecodeError::UnexpectedRecordTag),
            decode(&mut reader).map(|mut page| page.deinit()),
        );
    }

    // test "framed PAGE preserves following bytes"
    #[test]
    fn framed_page_preserves_following_bytes() {
        let mut fixture_bytes = test_empty_framed_page_fixture();
        fixture_bytes.extend_from_slice(b"next");
        let mut source = io::FixedReader::new(&fixture_bytes);
        let mut decoded = decode(&mut source).unwrap();
        assert_eq!(b"next", source.take(4).unwrap());
        decoded.deinit();
    }

    // test "decode sparse page rejects every truncation"
    #[test]
    fn decode_sparse_page_rejects_every_truncation() {
        let test_page_fixture = test_page_fixture();
        for len in 0..test_page_fixture.len() {
            let mut reader = io::FixedReader::new(&test_page_fixture[..len]);
            assert_eq!(
                Err(PayloadDecodeError::EndOfStream),
                decode_payload(&mut reader).map(|mut page| page.deinit()),
            );
        }
    }

    // test "decode accepts unordered sparse style IDs and ignores zero"
    #[test]
    fn decode_accepts_unordered_sparse_style_ids_and_ignores_zero() {
        use crate::style::Flags;

        let header = Header {
            columns: 2,
            rows: 1,
            style_count: 2,
            hyperlink_count: 0,
            style_capacity: 8,
            hyperlink_capacity_bytes: 0,
            grapheme_capacity_bytes: 0,
            string_capacity_bytes: 0,
        };

        let mut descending = [0u8; Header::LEN + 2 * (2 + style::LEN) + 3 + 2 * 8 + 4];
        let mut descending_writer = io::FixedWriter::new(&mut descending);
        header.encode(&mut descending_writer).unwrap();
        io::write_int::<u16>(&mut descending_writer, 3).unwrap();
        style::encode(
            TerminalStyle {
                flags: Flags {
                    bold: true,
                    ..Default::default()
                },
                ..Default::default()
            },
            &mut descending_writer,
        )
        .unwrap();
        io::write_int::<u16>(&mut descending_writer, 2).unwrap();
        style::encode(
            TerminalStyle {
                flags: Flags {
                    italic: true,
                    ..Default::default()
                },
                ..Default::default()
            },
            &mut descending_writer,
        )
        .unwrap();
        descending_writer
            .write_byte(
                grid::Row {
                    cell_width: grid::EncodedWidth::Eight,
                    ..Default::default()
                }
                .bits(),
            )
            .unwrap();
        io::write_int::<u16>(&mut descending_writer, 2).unwrap(); // cell count
        io::write_int::<u64>(
            &mut descending_writer,
            grid::Cell {
                content: 'A' as u32,
                style_id: 3,
                ..Default::default()
            }
            .bits(),
        )
        .unwrap();
        io::write_int::<u64>(
            &mut descending_writer,
            grid::Cell {
                content: 'B' as u32,
                style_id: 2,
                ..Default::default()
            }
            .bits(),
        )
        .unwrap();
        io::write_int::<u32>(&mut descending_writer, 0).unwrap(); // grapheme section

        let descending_buffered = descending_writer.buffered().to_vec();
        let mut descending_reader = io::FixedReader::new(&descending_buffered);
        let mut decoded_descending = decode_payload(&mut descending_reader).unwrap();
        assert_eq!(2usize, decoded_descending.styles.count());
        unsafe {
            assert_eq!(
                1 as TerminalStyleId,
                (*decoded_descending.get_row_and_cell(0, 0).cell).style_id(),
            );
            assert_eq!(
                2 as TerminalStyleId,
                (*decoded_descending.get_row_and_cell(1, 0).cell).style_id(),
            );
        }

        let one_header = Header {
            columns: 1,
            rows: 1,
            style_count: 1,
            hyperlink_count: 0,
            style_capacity: 8,
            hyperlink_capacity_bytes: 0,
            grapheme_capacity_bytes: 0,
            string_capacity_bytes: 0,
        };

        let mut zero = [0u8; Header::LEN + 2 + style::LEN + 7];
        let mut zero_writer = io::FixedWriter::new(&mut zero);
        one_header.encode(&mut zero_writer).unwrap();
        io::write_int::<u16>(&mut zero_writer, 0).unwrap();
        style::encode(
            TerminalStyle {
                flags: Flags {
                    bold: true,
                    ..Default::default()
                },
                ..Default::default()
            },
            &mut zero_writer,
        )
        .unwrap();

        let mut zero_grid = TerminalPage::init(TerminalPageCapacity::new(1, 1)).unwrap();
        grid::encode(&zero_grid, &mut zero_writer).unwrap();

        let zero_buffered = zero_writer.buffered().to_vec();
        let mut zero_reader = io::FixedReader::new(&zero_buffered);
        let mut decoded_zero = decode_payload(&mut zero_reader).unwrap();
        assert_eq!(0usize, decoded_zero.styles.count());

        decoded_descending.deinit();
        zero_grid.deinit();
        decoded_zero.deinit();
    }

    // test "decode accepts unordered sparse hyperlink IDs"
    #[test]
    fn decode_accepts_unordered_sparse_hyperlink_ids() {
        let header = Header {
            columns: 2,
            rows: 1,
            style_count: 0,
            hyperlink_count: 2,
            style_capacity: 0,
            hyperlink_capacity_bytes: 512,
            grapheme_capacity_bytes: 0,
            string_capacity_bytes: 6,
        };

        let first = TerminalHyperlink {
            id: terminal_hyperlink::HyperlinkId::Implicit(1),
            uri: b"one".to_vec(),
        };
        let second = TerminalHyperlink {
            id: terminal_hyperlink::HyperlinkId::Implicit(2),
            uri: b"two".to_vec(),
        };

        let mut encoded = [0u8; Header::LEN + 2 * 14 + 3 + 2 * 8 + 4];
        let mut writer = io::FixedWriter::new(&mut encoded);
        header.encode(&mut writer).unwrap();
        io::write_int::<u16>(&mut writer, 3).unwrap();
        hyperlink::encode(&first, &mut writer).unwrap();
        io::write_int::<u16>(&mut writer, 2).unwrap();
        hyperlink::encode(&second, &mut writer).unwrap();
        writer
            .write_byte(
                grid::Row {
                    cell_width: grid::EncodedWidth::Eight,
                    ..Default::default()
                }
                .bits(),
            )
            .unwrap();
        io::write_int::<u16>(&mut writer, 2).unwrap(); // cell count
        io::write_int::<u64>(
            &mut writer,
            grid::Cell {
                content: 'A' as u32,
                hyperlink: true,
                hyperlink_id: 3,
                ..Default::default()
            }
            .bits(),
        )
        .unwrap();
        io::write_int::<u64>(
            &mut writer,
            grid::Cell {
                content: 'B' as u32,
                hyperlink: true,
                hyperlink_id: 2,
                ..Default::default()
            }
            .bits(),
        )
        .unwrap();
        io::write_int::<u32>(&mut writer, 0).unwrap(); // grapheme section

        let buffered = writer.buffered().to_vec();
        let mut reader = io::FixedReader::new(&buffered);
        let mut decoded = decode_payload(&mut reader).unwrap();
        assert_eq!(2usize, decoded.hyperlink_set.count());
        let first_id = decoded
            .lookup_hyperlink(decoded.get_row_and_cell(0, 0).cell)
            .unwrap();
        let second_id = decoded
            .lookup_hyperlink(decoded.get_row_and_cell(1, 0).cell)
            .unwrap();
        let base = decoded.memory_ptr();
        unsafe {
            assert_eq!(1u16, decoded.hyperlink_set.ref_count(base, first_id));
            assert_eq!(1u16, decoded.hyperlink_set.ref_count(base, second_id));
            assert_eq!(
                b"one",
                (*decoded.hyperlink_set.get(base, first_id)).uri.slice(base),
            );
            assert_eq!(
                b"two",
                (*decoded.hyperlink_set.get(base, second_id))
                    .uri
                    .slice(base),
            );
        }

        decoded.deinit();
    }

    // test "decode defaults missing sparse cell references"
    #[test]
    fn decode_defaults_missing_sparse_cell_references() {
        // An unknown style ID falls back to the default style.
        let style_header = Header {
            columns: 1,
            rows: 1,
            style_count: 0,
            hyperlink_count: 0,
            style_capacity: 8,
            hyperlink_capacity_bytes: 0,
            grapheme_capacity_bytes: 0,
            string_capacity_bytes: 0,
        };

        let mut style_encoded = [0u8; Header::LEN + 3 + 8 + 4];
        let mut style_writer = io::FixedWriter::new(&mut style_encoded);
        style_header.encode(&mut style_writer).unwrap();
        style_writer.write_byte(0x30).unwrap(); // row flags, full cell width
        io::write_int::<u16>(&mut style_writer, 1).unwrap(); // cell count
        io::write_int::<u64>(
            &mut style_writer,
            grid::Cell {
                style_id: 1,
                ..Default::default()
            }
            .bits(),
        )
        .unwrap();
        io::write_int::<u32>(&mut style_writer, 0).unwrap(); // grapheme section

        let style_buffered = style_writer.buffered().to_vec();
        let mut style_reader = io::FixedReader::new(&style_buffered);
        let mut style_page = decode_payload(&mut style_reader).unwrap();
        unsafe {
            assert_eq!(
                0 as TerminalStyleId,
                (*style_page.get_row_and_cell(0, 0).cell).style_id(),
            );
        }

        // An unknown hyperlink ID likewise falls back to no hyperlink.
        let hyperlink_header = Header {
            columns: 1,
            rows: 1,
            style_count: 0,
            hyperlink_count: 0,
            style_capacity: 0,
            hyperlink_capacity_bytes: 512,
            grapheme_capacity_bytes: 0,
            string_capacity_bytes: 0,
        };

        let mut hyperlink_encoded = [0u8; Header::LEN + 3 + 8 + 4];
        let mut hyperlink_writer = io::FixedWriter::new(&mut hyperlink_encoded);
        hyperlink_header.encode(&mut hyperlink_writer).unwrap();
        hyperlink_writer.write_byte(0x30).unwrap(); // row flags, full cell width
        io::write_int::<u16>(&mut hyperlink_writer, 1).unwrap(); // cell count
        io::write_int::<u64>(
            &mut hyperlink_writer,
            grid::Cell {
                hyperlink: true,
                hyperlink_id: 1,
                ..Default::default()
            }
            .bits(),
        )
        .unwrap();
        io::write_int::<u32>(&mut hyperlink_writer, 0).unwrap(); // grapheme section

        let hyperlink_buffered = hyperlink_writer.buffered().to_vec();
        let mut hyperlink_reader = io::FixedReader::new(&hyperlink_buffered);
        let mut hyperlink_page = decode_payload(&mut hyperlink_reader).unwrap();
        let hyperlink_cell = hyperlink_page.get_row_and_cell(0, 0).cell;
        unsafe {
            assert!(!(*hyperlink_cell).hyperlink());
        }
        assert_eq!(None, hyperlink_page.lookup_hyperlink(hyperlink_cell));

        style_page.deinit();
        hyperlink_page.deinit();
    }

    // test "decode normalizes invalid grid semantics"
    #[test]
    fn decode_normalizes_invalid_grid_semantics() {
        let header = Header {
            columns: 3,
            rows: 1,
            style_count: 0,
            hyperlink_count: 0,
            style_capacity: 0,
            hyperlink_capacity_bytes: 0,
            grapheme_capacity_bytes: 0,
            string_capacity_bytes: 0,
        };

        let mut encoded = [0u8; Header::LEN + 3 + 3 * 8 + 4 + 10];
        let mut writer = io::FixedWriter::new(&mut encoded);
        header.encode(&mut writer).unwrap();

        // Preserve wrap while degrading the unknown semantic-prompt value to none
        // and ignoring all reserved row bits.
        writer.write_byte(0xFD).unwrap();
        io::write_int::<u16>(&mut writer, 3).unwrap();

        // A text cell replaces an invalid Unicode scalar with U+FFFD while its
        // reserved semantic content degrades to output.
        io::write_int::<u64>(
            &mut writer,
            grid::Cell {
                content: 0xD800,
                semantic_content: 3,
                ..Default::default()
            }
            .bits(),
        )
        .unwrap();

        // A declared grapheme suffix cannot fit the advertised zero capacity,
        // so decoding preserves the base character and drops the suffix.
        io::write_int::<u64>(
            &mut writer,
            grid::Cell {
                kind: 1,
                content: 'x' as u32,
                ..Default::default()
            }
            .bits(),
        )
        .unwrap();

        // Reserved palette content bits do not obscure the valid low palette
        // byte.
        io::write_int::<u64>(
            &mut writer,
            grid::Cell {
                kind: 2,
                content: 0xFFFF07,
                ..Default::default()
            }
            .bits(),
        )
        .unwrap();

        // The grapheme section carries the suffix for the second cell.
        io::write_int::<u32>(&mut writer, 1).unwrap();
        io::write_int::<u16>(&mut writer, 0).unwrap();
        io::write_int::<u16>(&mut writer, 1).unwrap();
        io::write_int::<u16>(&mut writer, 1).unwrap();
        io::write_int::<u32>(&mut writer, 0x0301).unwrap();

        let buffered = writer.buffered().to_vec();
        let mut reader = io::FixedReader::new(&buffered);
        let mut page = decode_payload(&mut reader).unwrap();
        page.verify_integrity().unwrap();

        let first = page.get_row_and_cell(0, 0);
        unsafe {
            assert!((*first.row).wrap());
            assert_eq!(
                terminal_page::SemanticPrompt::None,
                (*first.row).semantic_prompt(),
            );
            assert_eq!(0xFFFDu32, (*first.cell).codepoint());
            assert_eq!(terminal_page::Wide::Narrow, (*first.cell).wide());
            assert_eq!(
                terminal_page::SemanticContent::Output,
                (*first.cell).semantic_content(),
            );
            assert!(!(*first.cell).protected());
            assert!(!(*first.cell).has_grapheme());

            let second = *page.get_row_and_cell(1, 0).cell;
            assert_eq!('x' as u32, second.codepoint());
            assert!(!second.has_grapheme());

            let third = *page.get_row_and_cell(2, 0).cell;
            assert_eq!(
                terminal_page::ContentTag::BgColorPalette,
                third.content_tag(),
            );
            assert_eq!(7u8, third.content_color_palette());
            assert!(!third.has_grapheme());
        }

        page.deinit();
    }

    // test "decode validates dimensions"
    #[test]
    fn decode_validates_dimensions() {
        let cases = [
            Header {
                columns: 0,
                rows: 24,
                style_count: 0,
                hyperlink_count: 0,
                style_capacity: 0,
                hyperlink_capacity_bytes: 0,
                grapheme_capacity_bytes: 0,
                string_capacity_bytes: 0,
            },
            Header {
                columns: 80,
                rows: 0,
                style_count: 0,
                hyperlink_count: 0,
                style_capacity: 0,
                hyperlink_capacity_bytes: 0,
                grapheme_capacity_bytes: 0,
                string_capacity_bytes: 0,
            },
        ];

        for header in cases {
            let mut encoded = [0u8; Header::LEN];
            let mut writer = io::FixedWriter::new(&mut encoded);
            header.encode(&mut writer).unwrap();

            let buffered = writer.buffered().to_vec();
            let mut reader = io::FixedReader::new(&buffered);
            assert_eq!(
                Err(PayloadDecodeError::InvalidDimensions),
                decode_payload(&mut reader).map(|mut page| page.deinit()),
            );
        }
    }

    /// Encode just the fixed PAGE header and try to decode a payload from it.
    fn decode_payload_of(header: Header) -> Result<(), PayloadDecodeError> {
        let mut encoded = [0u8; Header::LEN];
        let mut writer = io::FixedWriter::new(&mut encoded);
        header.encode(&mut writer).unwrap();
        let buffered = writer.buffered().to_vec();
        let mut reader = io::FixedReader::new(&buffered);
        decode_payload(&mut reader).map(|mut page| page.deinit())
    }

    // rustty-only (R13, UPSTREAM-DEFECTS.md UD64): a PAGE header whose declared
    // capacity would make the native page allocation exceed
    // `size::MAX_PAGE_SIZE` is rejected as malformed input here, instead of
    // reaching `assert(layout.total_size <= size.max_page_size)` in
    // `createPageExt` — which, in the port, aborts the process from inside an
    // `extern "C"` entry point in every profile.
    #[test]
    fn decode_rejects_a_capacity_the_native_page_allocation_cannot_hold() {
        let base = Header {
            columns: 1,
            rows: 1,
            style_count: 0,
            hyperlink_count: 0,
            style_capacity: 0,
            hyperlink_capacity_bytes: 0,
            grapheme_capacity_bytes: 0,
            string_capacity_bytes: 0,
        };

        // Each of these is a single 20-byte PAGE header. The CRC is never
        // reached: the capacity is consumed before the payload checksum is
        // verified, which is exactly why the assertion was reachable.
        let rejected = [
            // The gap report's trigger: one oversized string capacity.
            Header {
                string_capacity_bytes: u32::MAX,
                ..base
            },
            Header {
                grapheme_capacity_bytes: u32::MAX,
                ..base
            },
            // 65535 x 65535 cells: ~34 GiB of cells alone, and on a 32-bit
            // target this is also where `Page::layout`'s `usize` arithmetic
            // would wrap.
            Header {
                columns: u16::MAX,
                rows: u16::MAX,
                ..base
            },
        ];
        for header in rejected {
            assert_eq!(
                Err(PayloadDecodeError::InvalidDimensions),
                decode_payload_of(header),
                "capacity should have been rejected: {header:?}",
            );
        }

        // POSITIVE REACH: a capacity a native page really can hold is NOT
        // rejected. It gets past the capacity check and fails on the grid
        // that these 20 bytes do not contain. Without this, the assertions
        // above would also pass if the decoder rejected every header.
        assert_eq!(
            Err(PayloadDecodeError::EndOfStream),
            decode_payload_of(Header {
                columns: 2,
                rows: 2,
                ..base
            }),
        );
    }

    // The constants `check_native_page_size` uses to bound `Page::layout`
    // before computing it. The gate is what keeps that computation inside a
    // 32-bit `usize` (wasm32), so the bound must really be an upper bound.
    #[test]
    fn the_page_size_bound_is_an_upper_bound_on_the_real_layout() {
        assert!(
            core::mem::size_of::<
                crate::ref_counted_set::Item<
                    TerminalStyle,
                    TerminalStyleId,
                    crate::size::CellCountInt,
                >,
            >() as u64
                <= STYLE_ITEM_SIZE_BOUND,
        );

        for cols in [1u16, 2, 80, 215, 1000, u16::MAX] {
            for rows in [1u16, 2, 24, 215, 1000, u16::MAX] {
                for styles in [0u16, 1, 128, u16::MAX] {
                    for hyperlink_bytes in [0u16, 64, u16::MAX] {
                        for grapheme_bytes in [0u32, 512, 8192, 1 << 20, u32::MAX] {
                            for string_bytes in [0u32, 512, 8192, 1 << 20, u32::MAX] {
                                let cap = TerminalPageCapacity {
                                    cols,
                                    rows,
                                    styles,
                                    hyperlink_bytes,
                                    grapheme_bytes,
                                    string_bytes,
                                };
                                let bound = declared_region_bytes(cap)
                                    .saturating_mul(LAYOUT_OVERHEAD_MULTIPLE)
                                    .saturating_add(LAYOUT_OVERHEAD_FIXED);
                                // `layout` is pure arithmetic and allocates
                                // nothing, and this test runs on a 64-bit
                                // host, so every capacity is computable here.
                                let total = TerminalPage::layout(cap).total_size as u64;
                                assert!(
                                    total <= bound,
                                    "layout {total} exceeds the bound {bound} for {cap:?}",
                                );
                            }
                        }
                    }
                }
            }
        }

        // The native standard capacity is comfortably accepted: the gate
        // rejects only capacities no page could hold.
        assert_eq!(Ok(()), check_native_page_size(crate::page::std_capacity()));
    }

    // rustty-only (R13, UPSTREAM-DEFECTS.md UD65): a decoded PAGE declares its
    // own dimensions, so it can hold more cells than the `u16` style refcount
    // can count. 256 x 256 = 65,536 cells sharing one style overflows it: a
    // panic in the port's checked profiles (which aborts through `extern "C"`)
    // and a wrap to zero in `release-fast`, after which a live style reads as
    // dead. Cells past the ceiling now take this function's own existing
    // degrade — the default style — and decoding completes.
    #[test]
    fn decode_degrades_cells_past_the_style_refcount_ceiling() {
        use crate::style::Flags;

        const COLS: u16 = 256;
        const ROWS: u16 = 256;

        let header = Header {
            columns: COLS,
            rows: ROWS,
            style_count: 1,
            hyperlink_count: 0,
            // Enough room that the style set really accepts the entry: a
            // capacity of 2 makes `add` fail, and `decode_payload_body`
            // degrades a style it cannot install to the default, which would
            // make this test pass for the wrong reason.
            style_capacity: 128,
            hyperlink_capacity_bytes: 0,
            grapheme_capacity_bytes: 0,
            string_capacity_bytes: 0,
        };

        let mut payload = io::AllocatingWriter::new();
        header.encode(&mut payload).unwrap();

        // One style table entry: encoded ID 1, a bold style.
        io::write_int::<u16>(&mut payload, 1).unwrap();
        style::encode(
            TerminalStyle {
                flags: Flags {
                    bold: true,
                    ..Default::default()
                },
                ..Default::default()
            },
            &mut payload,
        )
        .unwrap();

        // Every cell of every row carries encoded style ID 1: kind 0
        // (codepoint) in bits 0-1, the codepoint in bits 2-25, the style ID in
        // bits 26-41. The row flag byte selects the eight-byte transport.
        let word: u64 = ((b'A' as u64) << 2) | (1u64 << 26);
        for _ in 0..ROWS {
            payload.write_byte(0x30).unwrap();
            io::write_int::<u16>(&mut payload, COLS).unwrap();
            for _ in 0..COLS {
                io::write_int::<u64>(&mut payload, word).unwrap();
            }
        }
        io::write_int::<u32>(&mut payload, 0).unwrap(); // no grapheme suffixes

        let bytes = payload.written().to_vec();
        let mut reader = io::FixedReader::new(&bytes);
        // It decodes at all. Before the guard this panicked with "attempt to
        // add with overflow" inside the reference-counted set.
        let mut page = decode_payload(&mut reader).unwrap();

        // POSITIVE: the style really was installed and really was applied,
        // so the test is not passing because styling was skipped wholesale.
        assert_eq!(1, page.styles.count());
        let first_style = {
            let row = page.get_row(0);
            // SAFETY: in-bounds read of the decoded page's first cell.
            unsafe { (*(page.get_cells(row) as *const terminal_page::Cell)).style_id() }
        };
        assert_ne!(0, first_style);

        // The ceiling was reached, and the cells past it degraded to the
        // default style instead of overflowing the refcount.
        let last_style = {
            let row = page.get_row(ROWS as usize - 1);
            // SAFETY: in-bounds read of the decoded page's last cell.
            unsafe {
                (*(page.get_cells(row) as *const terminal_page::Cell).add(COLS as usize - 1))
                    .style_id()
            }
        };
        assert_eq!(0, last_style);

        // The surviving references sit at the ceiling (the style table's own
        // reference is surrendered after the grid, so this is one below it).
        let refs = unsafe { page.styles.ref_count(page.memory_ptr(), first_style) };
        assert!(
            refs >= crate::size::CellCountInt::MAX - 1,
            "expected a saturated refcount, got {refs}",
        );

        page.deinit();
    }

    // test "decode propagates a style table read failure"
    #[test]
    fn decode_propagates_a_style_table_read_failure() {
        /// A reader that succeeds for `bytes_before_failure` bytes, fails
        /// exactly once, and then succeeds again. (Zig: a custom
        /// `std.Io.Reader` vtable with an empty buffer; the stand-in uses a
        /// one-byte buffer, keeping every decode on its streaming path.)
        struct FailOnceReader<'a> {
            source: io::FixedReader<'a>,
            buf: [u8; 1],
            start: usize,
            end: usize,
            bytes_before_failure: usize,
            failed: bool,
        }

        impl<'a> FailOnceReader<'a> {
            fn init(bytes: &'a [u8], bytes_before_failure: usize) -> FailOnceReader<'a> {
                FailOnceReader {
                    source: io::FixedReader::new(bytes),
                    buf: [0; 1],
                    start: 0,
                    end: 0,
                    bytes_before_failure,
                    failed: false,
                }
            }
        }

        impl io::Reader for FailOnceReader<'_> {
            fn buffered(&self) -> &[u8] {
                &self.buf[self.start..self.end]
            }

            fn toss(&mut self, n: usize) {
                debug_assert!(n <= self.end - self.start);
                self.start += n;
            }

            fn refill(&mut self) -> Result<(), io::ReadError> {
                if self.start == self.end {
                    self.start = 0;
                    self.end = 0;
                }
                if self.bytes_before_failure == 0 && !self.failed {
                    self.failed = true;
                    return Err(io::ReadError::ReadFailed);
                }

                let space = self.buf.len() - self.end;
                let cap = if self.failed {
                    space
                } else {
                    space.min(self.bytes_before_failure)
                };
                let n = self
                    .source
                    .stream_to(&mut self.buf[self.end..self.end + cap])?;
                if !self.failed {
                    self.bytes_before_failure -= n;
                }
                self.end += n;
                Ok(())
            }

            fn take_buffered(&mut self, n: usize) -> &[u8] {
                debug_assert!(n <= self.end - self.start);
                let start = self.start;
                self.start += n;
                &self.buf[start..start + n]
            }
        }

        let header = Header {
            columns: 1,
            rows: 1,
            style_count: 1,
            hyperlink_count: 0,
            style_capacity: 8,
            hyperlink_capacity_bytes: 0,
            grapheme_capacity_bytes: 0,
            string_capacity_bytes: 0,
        };

        // If the style read error is swallowed, its sixteen zero bytes are then
        // misread as a valid empty grid and the unframed payload appears to decode.
        let mut encoded = [0u8; Header::LEN + 2 + style::LEN];
        let mut writer = io::FixedWriter::new(&mut encoded);
        header.encode(&mut writer).unwrap();
        io::write_int::<u16>(&mut writer, 1).unwrap();
        writer.splat_byte_all(0, style::LEN).unwrap();

        let buffered = writer.buffered().to_vec();
        let mut source = FailOnceReader::init(&buffered, Header::LEN + 2);
        match decode_payload(&mut source) {
            Err(err) => assert_eq!(PayloadDecodeError::ReadFailed, err),
            Ok(mut decoded) => {
                decoded.deinit();
                panic!("expected ReadFailed");
            }
        }
    }

    // test "decode normalizes duplicate default and invalid style entries"
    #[test]
    fn decode_normalizes_duplicate_default_and_invalid_style_entries() {
        use crate::style::Flags;

        let header = Header {
            columns: 1,
            rows: 1,
            style_count: 2,
            hyperlink_count: 0,
            style_capacity: 16,
            hyperlink_capacity_bytes: 512,
            grapheme_capacity_bytes: 0,
            string_capacity_bytes: 256,
        };
        let duplicate_style = TerminalStyle {
            flags: Flags {
                bold: true,
                ..Default::default()
            },
            ..Default::default()
        };

        // Generate a valid grid whose cell references a sparse style ID. The PAGE
        // table below can then give that ID duplicate, default, or invalid contents
        // without hand-authoring the cell wire format.
        let mut grid_page = TerminalPage::init(TerminalPageCapacity {
            styles: 8,
            ..TerminalPageCapacity::new(1, 1)
        })
        .unwrap();
        let grid_base = grid_page.memory_ptr();
        let encoded_style_id = unsafe {
            let released_style_a = grid_page
                .styles
                .add(
                    grid_base,
                    TerminalStyle {
                        flags: Flags {
                            italic: true,
                            ..Default::default()
                        },
                        ..Default::default()
                    },
                )
                .unwrap();
            let released_style_b = grid_page
                .styles
                .add(
                    grid_base,
                    TerminalStyle {
                        bg_color: crate::style::Color::Palette(1),
                        ..Default::default()
                    },
                )
                .unwrap();
            let encoded_style_id = grid_page.styles.add(grid_base, duplicate_style).unwrap();
            grid_page.styles.release(grid_base, released_style_a);
            grid_page.styles.release(grid_base, released_style_b);
            encoded_style_id
        };
        let grid_cell = grid_page.get_row_and_cell(0, 0);
        unsafe {
            *grid_cell.cell = terminal_page::Cell::init('A' as u32);
            (*grid_cell.cell).set_style_id(encoded_style_id);
            (*grid_cell.row).set_styled(true);
        }

        let mut grid_bytes = [0u8; 64];
        let mut grid_writer = io::FixedWriter::new(&mut grid_bytes);
        grid::encode(&grid_page, &mut grid_writer).unwrap();

        let mut encoded = io::AllocatingWriter::new();
        header.encode(&mut encoded).unwrap();
        io::write_int::<u16>(&mut encoded, 1).unwrap();
        style::encode(duplicate_style, &mut encoded).unwrap();
        io::write_int::<u16>(&mut encoded, encoded_style_id).unwrap();
        style::encode(duplicate_style, &mut encoded).unwrap();
        encoded.write_all(grid_writer.buffered()).unwrap();

        let mut reader = io::FixedReader::new(encoded.written());
        let mut duplicate_decoded = decode_payload(&mut reader).unwrap();
        assert_eq!(1usize, duplicate_decoded.styles.count());
        let duplicate_cell = unsafe { *duplicate_decoded.get_row_and_cell(0, 0).cell };
        assert!(duplicate_cell.style_id() != 0);
        unsafe {
            assert!(
                (*duplicate_decoded
                    .styles
                    .get(duplicate_decoded.memory_ptr(), duplicate_cell.style_id()))
                .flags
                .bold,
            );
        }

        let default_header = Header {
            columns: 1,
            rows: 1,
            style_count: 1,
            hyperlink_count: 0,
            style_capacity: 16,
            hyperlink_capacity_bytes: 0,
            grapheme_capacity_bytes: 0,
            string_capacity_bytes: 0,
        };
        let mut default_encoded = io::AllocatingWriter::new();
        default_header.encode(&mut default_encoded).unwrap();
        io::write_int::<u16>(&mut default_encoded, encoded_style_id).unwrap();
        style::encode(TerminalStyle::default(), &mut default_encoded).unwrap();
        default_encoded.write_all(grid_writer.buffered()).unwrap();

        let mut default_reader = io::FixedReader::new(default_encoded.written());
        let mut default_decoded = decode_payload(&mut default_reader).unwrap();
        unsafe {
            assert_eq!(
                0 as TerminalStyleId,
                (*default_decoded.get_row_and_cell(0, 0).cell).style_id(),
            );
        }

        // The strict style codec rejects this kind, but PAGE owns the fixed entry
        // boundary and can safely map the encoded ID to the default style. A zero
        // capacity hint also remains advisory.
        let mut invalid_header = default_header;
        invalid_header.style_capacity = 0;
        let mut invalid_encoded = io::AllocatingWriter::new();
        invalid_header.encode(&mut invalid_encoded).unwrap();
        io::write_int::<u16>(&mut invalid_encoded, encoded_style_id).unwrap();
        invalid_encoded.write_byte(3).unwrap();
        invalid_encoded.splat_byte_all(0, style::LEN - 1).unwrap();
        invalid_encoded.write_all(grid_writer.buffered()).unwrap();

        let mut invalid_reader = io::FixedReader::new(invalid_encoded.written());
        let mut invalid_decoded = decode_payload(&mut invalid_reader).unwrap();
        unsafe {
            assert_eq!(
                0 as TerminalStyleId,
                (*invalid_decoded.get_row_and_cell(0, 0).cell).style_id(),
            );
        }

        grid_page.deinit();
        duplicate_decoded.deinit();
        default_decoded.deinit();
        invalid_decoded.deinit();
    }

    // test "decode releases unused style table references"
    #[test]
    fn decode_releases_unused_style_table_references() {
        use crate::style::Flags;

        let header = Header {
            columns: 1,
            rows: 1,
            style_count: 1,
            hyperlink_count: 0,
            style_capacity: 8,
            hyperlink_capacity_bytes: 0,
            grapheme_capacity_bytes: 0,
            string_capacity_bytes: 0,
        };

        let mut empty_page = TerminalPage::init(TerminalPageCapacity::new(1, 1)).unwrap();
        let mut grid_bytes = [0u8; 32];
        let mut grid_writer = io::FixedWriter::new(&mut grid_bytes);
        grid::encode(&empty_page, &mut grid_writer).unwrap();

        let mut encoded = io::AllocatingWriter::new();
        header.encode(&mut encoded).unwrap();
        io::write_int::<u16>(&mut encoded, 7).unwrap();
        style::encode(
            TerminalStyle {
                flags: Flags {
                    bold: true,
                    ..Default::default()
                },
                ..Default::default()
            },
            &mut encoded,
        )
        .unwrap();
        encoded.write_all(grid_writer.buffered()).unwrap();

        let mut reader = io::FixedReader::new(encoded.written());
        let mut decoded = decode_payload(&mut reader).unwrap();
        assert_eq!(0usize, decoded.styles.count());

        // The dead table-only entry is absent from the very first re-encode.
        let mut canonical = [0u8; 64];
        let mut canonical_writer = io::FixedWriter::new(&mut canonical);
        encode_payload(&decoded, &mut canonical_writer).unwrap();
        let canonical_buffered = canonical_writer.buffered().to_vec();
        let mut canonical_reader = io::FixedReader::new(&canonical_buffered);
        let canonical_header = Header::decode(&mut canonical_reader).unwrap();
        assert_eq!(0u16, canonical_header.style_count);

        empty_page.deinit();
        decoded.deinit();
    }

    // test "decode reuses duplicate hyperlinks"
    #[test]
    fn decode_reuses_duplicate_hyperlinks() {
        let header = Header {
            columns: 1,
            rows: 1,
            style_count: 0,
            hyperlink_count: 2,
            style_capacity: 0,
            hyperlink_capacity_bytes: 512,
            grapheme_capacity_bytes: 0,
            string_capacity_bytes: 16,
        };
        let duplicate = TerminalHyperlink {
            id: terminal_hyperlink::HyperlinkId::Explicit(b"id".to_vec()),
            uri: b"uri".to_vec(),
        };

        // As with styles above, use a sparse native ID to make grid.encode produce
        // the cell reference whose table value this test deliberately duplicates.
        let mut grid_page = TerminalPage::init(TerminalPageCapacity {
            hyperlink_bytes: 512,
            string_bytes: 64,
            ..TerminalPageCapacity::new(1, 1)
        })
        .unwrap();
        let released_hyperlink_a = grid_page
            .insert_hyperlink(TerminalHyperlink {
                id: terminal_hyperlink::HyperlinkId::Implicit(1),
                uri: b"one".to_vec(),
            })
            .unwrap();
        let released_hyperlink_b = grid_page
            .insert_hyperlink(TerminalHyperlink {
                id: terminal_hyperlink::HyperlinkId::Implicit(2),
                uri: b"two".to_vec(),
            })
            .unwrap();
        let encoded_hyperlink_id = grid_page
            .insert_hyperlink(TerminalHyperlink {
                id: terminal_hyperlink::HyperlinkId::Implicit(3),
                uri: b"three".to_vec(),
            })
            .unwrap();
        release_page_hyperlink(&mut grid_page, released_hyperlink_a);
        release_page_hyperlink(&mut grid_page, released_hyperlink_b);
        let grid_cell = grid_page.get_row_and_cell(0, 0);
        unsafe {
            *grid_cell.cell = terminal_page::Cell::init('A' as u32);
        }
        grid_page
            .set_hyperlink(grid_cell.row, grid_cell.cell, encoded_hyperlink_id)
            .unwrap();

        let mut grid_bytes = [0u8; 64];
        let mut grid_writer = io::FixedWriter::new(&mut grid_bytes);
        grid::encode(&grid_page, &mut grid_writer).unwrap();

        let mut encoded = io::AllocatingWriter::new();
        header.encode(&mut encoded).unwrap();
        io::write_int::<u16>(&mut encoded, 1).unwrap();
        hyperlink::encode(&duplicate, &mut encoded).unwrap();
        io::write_int::<u16>(&mut encoded, encoded_hyperlink_id).unwrap();
        hyperlink::encode(&duplicate, &mut encoded).unwrap();
        encoded.write_all(grid_writer.buffered()).unwrap();

        let mut reader = io::FixedReader::new(encoded.written());
        let mut decoded = decode_payload(&mut reader).unwrap();
        assert_eq!(1usize, decoded.hyperlink_set.count());
        let cell = decoded.get_row_and_cell(0, 0).cell;
        unsafe {
            assert!((*cell).hyperlink());
        }
        let id = decoded.lookup_hyperlink(cell).unwrap();
        let base = decoded.memory_ptr();
        unsafe {
            let entry = decoded.hyperlink_set.get(base, id);
            assert_eq!(1u16, decoded.hyperlink_set.ref_count(base, id));
            assert_eq!(b"uri", (*entry).uri.slice(base));
        }

        // Overwriting the sole linked cell must make the deduplicated entry dead.
        // Any surviving reference belongs to the decode-time wire table, not the
        // native page.
        let row0 = decoded.get_row(0);
        decoded.clear_cells(row0, 0, 1);
        assert_eq!(0usize, decoded.hyperlink_set.count());

        grid_page.deinit();
        decoded.deinit();
    }

    // test "decode ignores empty hyperlink strings"
    #[test]
    fn decode_ignores_empty_hyperlink_strings() {
        let header = Header {
            columns: 1,
            rows: 1,
            style_count: 0,
            hyperlink_count: 1,
            style_capacity: 0,
            hyperlink_capacity_bytes: 0,
            grapheme_capacity_bytes: 0,
            string_capacity_bytes: 16,
        };
        let fixtures: [&[u8]; 4] = [
            // Empty implicit URI.
            b"\x01\x01\x00\x00\x00\x00\x00\x00\x00",
            // Empty explicit ID.
            b"\x02\x00\x00\x00\x00\x03\x00\x00\x00uri",
            // Empty explicit URI.
            b"\x02\x02\x00\x00\x00id\x00\x00\x00\x00",
            // Valid contents that cannot fit the zero-capacity native set.
            b"\x01\x01\x00\x00\x00\x03\x00\x00\x00uri",
        ];

        // Only the invalid hyperlink entry itself is hand-authored above. Generate
        // its cell reference through the native grid encoder.
        let mut grid_page = TerminalPage::init(TerminalPageCapacity {
            hyperlink_bytes: 256,
            string_bytes: 16,
            ..TerminalPageCapacity::new(1, 1)
        })
        .unwrap();
        let encoded_hyperlink_id = grid_page
            .insert_hyperlink(TerminalHyperlink {
                id: terminal_hyperlink::HyperlinkId::Implicit(1),
                uri: b"uri".to_vec(),
            })
            .unwrap();
        let grid_cell = grid_page.get_row_and_cell(0, 0);
        unsafe {
            *grid_cell.cell = terminal_page::Cell::init('A' as u32);
        }
        grid_page
            .set_hyperlink(grid_cell.row, grid_cell.cell, encoded_hyperlink_id)
            .unwrap();
        let mut grid_bytes = [0u8; 64];
        let mut grid_writer = io::FixedWriter::new(&mut grid_bytes);
        grid::encode(&grid_page, &mut grid_writer).unwrap();

        for fixture_bytes in fixtures {
            let mut encoded = io::AllocatingWriter::new();
            header.encode(&mut encoded).unwrap();
            io::write_int::<u16>(&mut encoded, encoded_hyperlink_id).unwrap();
            encoded.write_all(fixture_bytes).unwrap();

            // The cell refers to the ignored table entry. Its absent native
            // remapping must degrade to no hyperlink while preserving the cell.
            encoded.write_all(grid_writer.buffered()).unwrap();

            let mut reader = io::FixedReader::new(encoded.written());
            let mut decoded = decode_payload(&mut reader).unwrap();

            assert_eq!(0usize, decoded.hyperlink_set.count());
            let cell = decoded.get_row_and_cell(0, 0).cell;
            unsafe {
                assert_eq!('A' as u32, (*cell).codepoint());
                assert!(!(*cell).hyperlink());
            }
            assert_eq!(None, decoded.lookup_hyperlink(cell));

            decoded.deinit();
        }

        grid_page.deinit();
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
            assert_eq!(Err(io::ReadError::EndOfStream), Header::decode(&mut reader),);
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
        assert_eq!(
            Err(DiscardError::EndOfStream),
            discard(&mut truncated_source)
        );
    }
}
