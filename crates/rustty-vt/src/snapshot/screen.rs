//! SCREEN record payload encoding.
//!
//! One SCREEN record represents the live state for one terminal screen. The
//! record is followed immediately by the number of complete PAGE records
//! declared by `page_count`. They are the minimal suffix of native pages needed
//! to cover the active area and are ordered from oldest to newest. A decoder
//! uses the declared count, rather than another record tag, to find the end of
//! the screen's page sequence.
//!
//! The final screen-height rows are the active area. When its boundary falls
//! inside the first encoded page, that page also contains an incidental history
//! prefix. `history_rows` declares the complete logical history extent so a
//! client can size its scrollbar at READY without waiting for HISTORY. A client
//! may expose or ignore the resident overlap carried by SCREEN. The extent is
//! advisory; native row totals remain derived from decoded PAGE records.
//!
//! Screen dimensions are terminal-wide state and are not repeated here. Page
//! capacities, native page IDs and pointers, history availability, selection,
//! viewport, dirty state, and derived semantic-prompt state are also omitted.
//!
//! All integers are unsigned and little-endian.
//!
//! ## Binary Format
//!
//! A SCREEN record is followed immediately by its declared PAGE records:
//!
//! ```text
//!  +----------------------+
//!  | SCREEN record        |
//!  +----------------------+
//!  | PAGE record 0        |
//!  +----------------------+
//!  | ...                  |
//!  +----------------------+
//!  | PAGE record (n - 1)  |
//!  +----------------------+
//!
//!  n = page_count
//! ```
//!
//! The HISTORY record for this screen declares and sends the older complete
//! pages after the terminal becomes ready. The incidental prefix is already
//! present in SCREEN and is not sent again.
//!
//! The SCREEN payload begins with a fixed header. When the header says there is
//! no saved cursor, the payload is:
//!
//! ```text
//!   0 +----------------------+
//!     | Header               |
//!  53 +----------------------+
//!     | Cursor hyperlink     |
//!     | variable             |
//! end +----------------------+
//! ```
//!
//! When a saved cursor is present, it is inserted before the cursor hyperlink:
//!
//! ```text
//!   0 +----------------------+
//!     | Header               |
//!  53 +----------------------+
//!     | Saved cursor         |
//!  76 +----------------------+
//!     | Cursor hyperlink     |
//!     | variable             |
//! end +----------------------+
//! ```
//!
//! The cursor hyperlink begins with a one-byte kind. Zero means no hyperlink
//! and has no following bytes. Kinds one and two use the implicit and explicit
//! hyperlink encodings documented in `hyperlink.zig`.
//!
//! ### Header
//!
//! ```text
//!   0 +----------------------------------+
//!     | Screen key (u16)                 |
//!   2 +----------------------------------+
//!     | Page count (u16)                 |
//!   4 +----------------------------------+
//!     | Logical history rows (u64)       |
//!  12 +----------------------------------+
//!     | Cursor x (u16)                   |
//!  14 +----------------------------------+
//!     | Cursor y (u16)                   |
//!  16 +----------------------------------+
//!     | Cursor visual style (u8)         |
//!  17 +----------------------------------+
//!     | Cursor flags (u8)                |
//!  18 +----------------------------------+
//!     | Concrete cursor pen (Style)      |
//!  34 +----------------------------------+
//!     | Hyperlink implicit counter (u32) |
//!  38 +----------------------------------+
//!     | Current charset (CharsetState)   |
//!  40 +----------------------------------+
//!     | Protected mode (u8)              |
//!  41 +----------------------------------+
//!     | Kitty keyboard stack index (u8)  |
//!  42 +----------------------------------+
//!     | Kitty keyboard stack flags       |
//!     | 8 * u8                           |
//!  50 +----------------------------------+
//!     | Semantic-click kind (u8)         |
//!  51 +----------------------------------+
//!     | Semantic-click value (u8)        |
//!  52 +----------------------------------+
//!     | Saved-cursor-present (u8)        |
//!  53 +----------------------------------+
//! ```
//!
//! The concrete cursor pen uses the fixed style encoding from `style.zig`.
//!
//! ### Cursor flags
//!
//! ```text
//! bit 0 +--------------------------------+
//!       | Pending wrap                   |
//! bit 1 +--------------------------------+
//!       | Protected                      |
//! bit 2 +--------------------------------+
//!       | Semantic content               |
//!       | 2 bits                         |
//! bit 4 +--------------------------------+
//!       | Semantic-content-clear-EOL     |
//! bit 5 +--------------------------------+
//!       | Reserved, zero                 |
//!       | 3 bits                         |
//! bit 8 +--------------------------------+
//! ```
//!
//! ### Charset state
//!
//! The current and saved cursor charset states share this packed layout:
//!
//! ```text
//!  bit 0 +-------------------------------+
//!        | G0 charset                    |
//!        | 2 bits                        |
//!  bit 2 +-------------------------------+
//!        | G1 charset                    |
//!        | 2 bits                        |
//!  bit 4 +-------------------------------+
//!        | G2 charset                    |
//!        | 2 bits                        |
//!  bit 6 +-------------------------------+
//!        | G3 charset                    |
//!        | 2 bits                        |
//!  bit 8 +-------------------------------+
//!        | GL slot                       |
//!        | 2 bits                        |
//! bit 10 +-------------------------------+
//!        | GR slot                       |
//!        | 2 bits                        |
//! bit 12 +-------------------------------+
//!        | Single shift                  |
//!        | 3 bits                        |
//! bit 15 +-------------------------------+
//!        | Reserved, zero                |
//! bit 16 +-------------------------------+
//! ```
//!
//! The Kitty keyboard stack contains its current index followed by all eight
//! flag entries, including disabled entries.
//!
//! ### Saved cursor
//!
//! ```text
//!   0 +-----------------------------+
//!     | Saved cursor x (u16)        |
//!   2 +-----------------------------+
//!     | Saved cursor y (u16)        |
//!   4 +-----------------------------+
//!     | Saved cursor pen (Style)    |
//!  20 +-----------------------------+
//!     | Saved cursor flags (u8)     |
//!  21 +-----------------------------+
//!     | Saved charset state         |
//!  23 +-----------------------------+
//! ```
//!
//! ### Saved cursor flags
//!
//! ```text
//! bit 0 +---------------------------+
//!       | Protected                 |
//! bit 1 +---------------------------+
//!       | Pending wrap              |
//! bit 2 +---------------------------+
//!       | Origin                    |
//! bit 3 +---------------------------+
//!       | Reserved, zero            |
//!       | 5 bits                    |
//! bit 8 +---------------------------+
//! ```
//!
//! Native enum values used below are stable terminal registries. Their wire
//! widths are fixed independently by this payload. Changing those values or
//! any packed bit assignment requires a snapshot version bump.
//!
//! PORTS: ghostty src/terminal/snapshot/screen.zig (phase-4 wsnap-3; complete —
//! assessment B cluster C4's screen half, END STATE incl. b867a0f59's
//! lib.Enum'd registries, cafe7d5da saved-cursor clamping, d14847183
//! mixed-width pending wrap, e89ff37aa safe page encoding, and 4371871bc's
//! infallible `setLimit`).

// const std = @import("std");
// const build_options = @import("terminal_options");
// const Allocator = std.mem.Allocator;
// const hyperlink = @import("hyperlink.zig");
// const test_fixture = @import("fixture.zig");
// const io = @import("io.zig");
// const grid = @import("grid.zig");
// const page = @import("page.zig");
// const record = @import("record.zig");
// const style = @import("style.zig");
// const terminal_ansi = @import("../ansi.zig");
// const terminal_charsets = @import("../charsets.zig");
// const terminal_hyperlink = @import("../hyperlink.zig");
// const terminal_kitty = @import("../kitty.zig");
// const terminal_osc = @import("../osc.zig");
// const terminal_page = @import("../page.zig");
// const TerminalPageList = @import("../PageList.zig");
// const TerminalScreen = @import("../Screen.zig");
// const TerminalScreenKey = @import("../ScreenSet.zig").Key;
// const terminal_style = @import("../style.zig");
use super::hyperlink;
use super::io;
use super::page;
use super::record;
use super::style;
use crate::ansi as terminal_ansi;
use crate::charsets as terminal_charsets;
use crate::hyperlink as terminal_hyperlink;
use crate::kitty as terminal_kitty;
use crate::osc as terminal_osc;
use crate::page as terminal_page;
use crate::pagelist::builder::{Builder, FinishError};
use crate::pagelist::grow::IncreaseCapacityError;
use crate::pagelist::{self, Node, Pin};
use crate::point;
use crate::screen::{
    CharsetState, Cursor as ScreenCursor, CursorStyle, Screen as TerminalScreen, SemanticClick,
    SemanticPrompt as ScreenSemanticPrompt,
};
use crate::screen_set::Key as TerminalScreenKey;
use crate::style as terminal_style;

// PORT NOTE: the reference `Allocator.Error` maps to the crate's canonical
// OOM; the general-purpose allocator maps to `crate::alloc::Global`
// (the crate-wide treatment; see `screen_set.rs`).
use crate::alloc::Allocator;

// const TerminalHyperlink = terminal_hyperlink.Hyperlink;
pub type TerminalHyperlink = terminal_hyperlink::Hyperlink;
// const TerminalStyle = terminal_style.Style;
type TerminalStyle = terminal_style::Style;

/// Errors possible while encoding fixed SCREEN payload fields.
// const PayloadEncodeError = hyperlink.EncodeError || error{...};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PayloadEncodeError {
    // hyperlink.EncodeError
    WriteFailed,
    InvalidUri,
    InvalidExplicitId,
    StringTooLong,

    InvalidCursorFlags,
    InvalidCharsetState,
    InvalidKittyKeyboardIndex,
    InvalidKittyKeyboardFlags,
    InvalidSavedCursorFlags,
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

impl From<io::WriteFailed> for PayloadEncodeError {
    fn from(_: io::WriteFailed) -> PayloadEncodeError {
        PayloadEncodeError::WriteFailed
    }
}

/// Errors possible while encoding a SCREEN and its complete PAGE sequence.
// pub const EncodeError = Allocator.Error || PayloadEncodeError ||
//     page.EncodeError || error{ PageCountOverflow };
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EncodeError {
    // Allocator.Error
    OutOfMemory,
    // PayloadEncodeError
    WriteFailed,
    InvalidUri,
    InvalidExplicitId,
    StringTooLong,
    InvalidCursorFlags,
    InvalidCharsetState,
    InvalidKittyKeyboardIndex,
    InvalidKittyKeyboardFlags,
    InvalidSavedCursorFlags,
    // page.EncodeError (incl. record.Writer.FinishError)
    InvalidWideCell,
    TooManyGraphemes,
    PayloadTooLarge,

    /// The active area spans more pages than the SCREEN header can declare.
    PageCountOverflow,
}

impl From<PayloadEncodeError> for EncodeError {
    fn from(err: PayloadEncodeError) -> EncodeError {
        match err {
            PayloadEncodeError::WriteFailed => EncodeError::WriteFailed,
            PayloadEncodeError::InvalidUri => EncodeError::InvalidUri,
            PayloadEncodeError::InvalidExplicitId => EncodeError::InvalidExplicitId,
            PayloadEncodeError::StringTooLong => EncodeError::StringTooLong,
            PayloadEncodeError::InvalidCursorFlags => EncodeError::InvalidCursorFlags,
            PayloadEncodeError::InvalidCharsetState => EncodeError::InvalidCharsetState,
            PayloadEncodeError::InvalidKittyKeyboardIndex => EncodeError::InvalidKittyKeyboardIndex,
            PayloadEncodeError::InvalidKittyKeyboardFlags => EncodeError::InvalidKittyKeyboardFlags,
            PayloadEncodeError::InvalidSavedCursorFlags => EncodeError::InvalidSavedCursorFlags,
        }
    }
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

/// Encode one SCREEN and its minimal suffix of complete native pages.
///
/// The suffix begins with the page containing the active area's first row and
/// ends with the newest page. Completed records may already be emitted if a
/// later record fails; the missing READY marker makes that prefix invalid.
pub fn encode<W: io::Writer + ?Sized>(
    screen: &TerminalScreen,
    key: TerminalScreenKey,
    destination: &mut record::Writer<'_, W>,
) -> Result<(), EncodeError> {
    // The active top may fall inside this page, leaving an incidental history
    // prefix. Every earlier complete page is history and is omitted.
    let first: *mut Node = screen.pages.get_top_left(point::Tag::Active).node;
    let page_count: usize = {
        let mut page_count: usize = 0;
        let mut node = first;
        while !node.is_null() {
            page_count += 1;
            // SAFETY: `node` is a live node of this screen's PageList; `next`
            // links stay valid while the list is not mutated (encode only
            // reads).
            node = unsafe { (*node).next };
        }
        page_count
    };
    let encoded_page_count =
        u16::try_from(page_count).map_err(|_| EncodeError::PageCountOverflow)?;

    // SCREEN declares exactly how many immediately following PAGE records
    // belong to it.
    {
        let payload = destination.begin(record::Tag::Screen);
        // errdefer destination.cancel();
        if let Err(err) = encode_payload(screen, key, encoded_page_count, payload) {
            destination.cancel();
            return Err(err.into());
        }
        if let Err(err) = destination.finish() {
            destination.cancel();
            return Err(err.into());
        }
    }

    // Active pages are resident today, but use the representation-safe access
    // path so a future PageList compression-policy change cannot turn this
    // wire encoder's optimization invariant into undefined behavior.
    let mut node = first;
    while !node.is_null() {
        // SAFETY: `node` is live (see above); encode does not mutate the list.
        let mut preserved =
            unsafe { Node::page_preserving_state(node) }.map_err(|_| EncodeError::OutOfMemory)?;
        // defer preserved.deinit();
        // SAFETY: `preserved.page()` is a live page for the preserved scope.
        let result = page::encode(unsafe { &*preserved.page() }, destination);
        preserved.deinit();
        result?;
        // SAFETY: as above.
        node = unsafe { (*node).next };
    }
    Ok(())
}

/// Errors possible while restoring a SCREEN and its declared PAGE sequence.
// pub const DecodeError = PayloadDecodeError || page.DecodeError ||
//     record.Reader.InitError || record.Reader.FinishError ||
//     TerminalPageList.Builder.FinishError ||
//     TerminalPageList.IncreaseCapacityError || error{...};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecodeError {
    // PayloadDecodeError
    ReadFailed,
    EndOfStream,
    InvalidKey,
    // page.DecodeError
    InvalidDimensions,
    InvalidRowCellCount,
    InvalidKind,
    OutOfMemory,
    InvalidDestinationCapacity,
    Integrity(crate::page::IntegrityError),
    // record.Reader.InitError / FinishError
    InvalidTag,
    InvalidChecksum,
    PayloadNotExhausted,
    // TerminalPageList.Builder.FinishError
    InvalidPageDimensions,
    NoPages,
    InsufficientRows,
    // TerminalPageList.IncreaseCapacityError
    OutOfSpace,

    /// The next record is valid but is not a SCREEN.
    UnexpectedRecordTag,

    /// A SCREEN must declare at least one PAGE.
    InvalidPageCount,
}

impl From<PayloadDecodeError> for DecodeError {
    fn from(err: PayloadDecodeError) -> DecodeError {
        match err {
            PayloadDecodeError::ReadFailed => DecodeError::ReadFailed,
            PayloadDecodeError::EndOfStream => DecodeError::EndOfStream,
            PayloadDecodeError::InvalidKey => DecodeError::InvalidKey,
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

impl From<page::DecodeError> for DecodeError {
    fn from(err: page::DecodeError) -> DecodeError {
        match err {
            page::DecodeError::ReadFailed => DecodeError::ReadFailed,
            page::DecodeError::EndOfStream => DecodeError::EndOfStream,
            page::DecodeError::InvalidDimensions => DecodeError::InvalidDimensions,
            page::DecodeError::InvalidRowCellCount => DecodeError::InvalidRowCellCount,
            page::DecodeError::InvalidKind => DecodeError::InvalidKind,
            page::DecodeError::OutOfMemory => DecodeError::OutOfMemory,
            page::DecodeError::InvalidDestinationCapacity => {
                DecodeError::InvalidDestinationCapacity
            }
            page::DecodeError::InvalidTag => DecodeError::InvalidTag,
            page::DecodeError::InvalidChecksum => DecodeError::InvalidChecksum,
            page::DecodeError::PayloadNotExhausted => DecodeError::PayloadNotExhausted,
            page::DecodeError::Integrity(err) => DecodeError::Integrity(err),
            page::DecodeError::UnexpectedRecordTag => DecodeError::UnexpectedRecordTag,
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

impl From<FinishError> for DecodeError {
    fn from(err: FinishError) -> DecodeError {
        match err {
            FinishError::OutOfMemory => DecodeError::OutOfMemory,
            FinishError::InvalidDimensions => DecodeError::InvalidDimensions,
            FinishError::InvalidPageDimensions => DecodeError::InvalidPageDimensions,
            FinishError::NoPages => DecodeError::NoPages,
            FinishError::InsufficientRows => DecodeError::InsufficientRows,
        }
    }
}

impl From<IncreaseCapacityError> for DecodeError {
    fn from(err: IncreaseCapacityError) -> DecodeError {
        match err {
            IncreaseCapacityError::OutOfMemory => DecodeError::OutOfMemory,
            IncreaseCapacityError::OutOfSpace => DecodeError::OutOfSpace,
        }
    }
}

/// One decoded SCREEN sequence and the native screen identified by its header.
pub struct Decoded {
    pub key: TerminalScreenKey,
    /// Expected history extent at READY, including resident SCREEN overlap.
    pub history_rows: u64,
    pub screen: TerminalScreen,
}

impl Decoded {
    /// Release the decoded native screen before ownership is transferred.
    pub fn deinit(&mut self) {
        self.screen.deinit();
        // self.* = undefined;
    }
}

/// Restore one SCREEN and its declared PAGE records into native terminal state.
///
/// The caller supplies terminal-wide dimensions and policy. The record sequence
/// is decoded transactionally: on failure, all page, pin, hyperlink, and
/// temporary allocations are released and no partially restored Screen escapes.
/// The returned key selects the ScreenSet slot that owns the decoded screen.
// PORT NOTE: the reference threads `io_: std.Io` into the restored Screen and
// its kitty storage; the port's `Screen` has no `io` field (see its PORT
// NOTE), so the parameter has no counterpart here.
pub fn decode(
    source: &mut dyn io::Reader,
    alloc: Allocator,
    options: crate::screen::Options,
) -> Result<Decoded, DecodeError> {
    // Decode and finish the self-contained SCREEN record before consuming any
    // of the PAGE records which follow it.
    //
    // The optional saved cursor and cursor hyperlink are both part of the
    // SCREEN payload. Keep ownership of the decoded hyperlink locally until it
    // has been copied into the native Screen.
    let (header, saved_cursor, cursor_hyperlink) = {
        let mut record_reader = record::Reader::init(&mut *source)?;
        if record_reader.header.tag != record::Tag::Screen {
            return Err(DecodeError::UnexpectedRecordTag);
        }

        let payload_reader = record_reader.payload_reader();
        let header = Header::decode(payload_reader)?;
        let saved_cursor = if header.saved_cursor_present {
            Some(SavedCursor::decode(payload_reader)?)
        } else {
            None
        };
        let cursor_hyperlink = decode_cursor_hyperlink(payload_reader)?;
        // defer if (cursor_hyperlink) |*value| value.deinit(alloc);
        // PORT NOTE: the owned `Hyperlink` (its `Vec`s) drops at the end of
        // this function on every path, mirroring the deferred deinit.
        record_reader.finish()?;
        (header, saved_cursor, cursor_hyperlink)
    };

    let key = header.key;
    if header.page_count == 0 {
        return Err(DecodeError::InvalidPageCount);
    }

    // Dimensions are terminal-wide state supplied by the enclosing snapshot.
    // Validate them, and all cursor invariants which depend on them, before
    // allocating the declared pages.
    if options.cols == 0 || options.rows == 0 {
        return Err(DecodeError::InvalidDimensions);
    }

    // Build the native page list transactionally. Alternate screens never
    // retain scrollback, while primary screens inherit the caller's limits.
    let mut result: TerminalScreen = {
        // tripwire seam: the PageList builder's own allocation.
        #[cfg(test)]
        super::tw::check(super::tw::SnapshotFp::Alloc).map_err(|_| DecodeError::OutOfMemory)?;
        let mut builder = Builder::init(
            alloc,
            pagelist::Options {
                max_size: if key == TerminalScreenKey::Alternate {
                    Some(0)
                } else {
                    options.max_scrollback_bytes
                },
                max_lines: if key == TerminalScreenKey::Alternate {
                    Some(0)
                } else {
                    options.max_scrollback_lines
                },
                ..pagelist::Options::new(options.cols, options.rows)
            },
        )
        .map_err(|_| DecodeError::OutOfMemory)?;
        // defer builder.deinit();

        // Allocate each PAGE at its declared capacity and decode directly into
        // builder-owned storage, avoiding an intermediate page representation.
        //
        // Finishing validates that the decoded suffix covers the active area
        // and establishes the PageList's active viewport.
        let built: Result<pagelist::PageList, DecodeError> = (|| {
            for _ in 0..header.page_count {
                let mut decoder = page::Decoder::init(&mut *source)?;
                // tripwire seam: each decoded page's backing allocation.
                #[cfg(test)]
                super::tw::check(super::tw::SnapshotFp::Alloc)
                    .map_err(|_| DecodeError::OutOfMemory)?;
                let native_page = builder
                    .allocate_page(decoder.capacity())
                    .map_err(|_| DecodeError::OutOfMemory)?;
                // SAFETY: `native_page` is the freshly allocated builder-owned
                // page; the exclusive borrow lives only for this decode call
                // (the history.rs `decode_page_inner` precedent).
                decoder.decode(unsafe { &mut *native_page })?;
            }
            Ok(builder.finish()?)
        })();
        builder.deinit();
        let mut pages = built?;
        // errdefer pages.deinit(); — explicit on the fallible paths below.

        // Convert the payload-relative cursor position into the tracked native
        // pin and page-local row/cell pointers required by TerminalScreen.
        let cursor: ScreenCursor = {
            let y = header.cursor_y.min(options.rows - 1);

            // The active area can temporarily contain mixed-width pages after
            // a reflow. Locate the row at column zero, then clamp the encoded
            // x coordinate to the width of that physical page.
            let row_pin = pages
                .pin(point::Point::Active(point::Coordinate {
                    x: 0,
                    y: y as u32,
                }))
                .unwrap(); // orelse unreachable
                           // SAFETY: `row_pin.node` is a live node of `pages`.
            let node_cols = unsafe { Node::cols(row_pin.node) };
            let x = header.cursor_x.min(node_cols - 1);
            // tripwire seam: the cursor's tracked-pin allocation.
            #[cfg(test)]
            if super::tw::check(super::tw::SnapshotFp::Alloc).is_err() {
                pages.deinit();
                return Err(DecodeError::OutOfMemory);
            }
            let pin: *mut Pin = match pages.track_pin(Pin {
                node: row_pin.node,
                x,
                y: row_pin.y,
                garbage: false,
            }) {
                Ok(p) => p,
                Err(_) => {
                    pages.deinit();
                    return Err(DecodeError::OutOfMemory);
                }
            };
            // SAFETY: `pin` is a live tracked pin owned by `pages`.
            let rac = unsafe { (*pin).row_and_cell() };

            ScreenCursor {
                x,
                y,
                cursor_style: header.cursor_style,
                pending_wrap: header.cursor_flags.pending_wrap && x == node_cols - 1,
                protected: header.cursor_flags.protected,
                style: header.cursor_pen,
                hyperlink_implicit_id: header.hyperlink_implicit_id,
                semantic_content: header.cursor_flags.semantic_content,
                semantic_content_clear_eol: header.cursor_flags.semantic_content_clear_eol,
                page_pin: pin,
                page_row: rac.row,
                page_cell: rac.cell,
                // (Zig struct-literal defaults for the remaining fields.)
                style_id: terminal_style::DEFAULT_ID,
                hyperlink_id: 0,
                hyperlink: None,
            }
        };

        // Assemble the remaining native state from stable snapshot registries.
        // Derived state and page-local table references are repaired below.
        TerminalScreen {
            // .io = io_ — no counterpart (see the decode PORT NOTE).
            alloc,
            pages,
            no_scrollback: key == TerminalScreenKey::Alternate
                || options.max_scrollback_bytes == Some(0),
            cursor,
            saved_cursor: saved_cursor.map(|value| value.terminal(options.cols, options.rows)),
            charset: header.charset.clone(),
            protected_mode: header.protected_mode,
            kitty_keyboard: header.kitty_keyboard.terminal(),
            semantic_prompt: ScreenSemanticPrompt {
                seen: false,
                click: header.semantic_click,
            },
            // (Zig struct-literal defaults for the remaining fields.)
            selection: None,
            kitty_images: terminal_kitty::graphics_storage::ImageStorage::default(),
            dirty: crate::screen::Dirty::default(),
        }
    };
    // errdefer result.deinit(); — explicit on each fallible path below.

    // Reinsert the concrete cursor style into its page-local style table. This
    // existing Screen path grows or splits the page when the decoded table is
    // already full.
    if let Err(err) = result.manual_style_update() {
        result.deinit();
        return Err(err.into());
    }

    // Reinsert the cursor hyperlink into its page-local hyperlink table. For an
    // implicit link, temporarily seed the counter with the encoded link ID,
    // then restore the independent next-ID counter from the header.
    if let Some(value) = cursor_hyperlink.as_ref() {
        match &value.id {
            terminal_hyperlink::HyperlinkId::Explicit(id) => {
                if let Err(err) = result.start_hyperlink(&value.uri, Some(id)) {
                    result.deinit();
                    return Err(err.into());
                }
            }
            terminal_hyperlink::HyperlinkId::Implicit(id) => {
                result.cursor.hyperlink_implicit_id = *id;
                if let Err(err) = result.start_hyperlink(&value.uri, None) {
                    result.deinit();
                    return Err(err.into());
                }
                result.cursor.hyperlink_implicit_id = header.hyperlink_implicit_id;
            }
        }
    }

    // `seen` is derived state. Only resident prompt metadata participates
    // because omitted history is outside the restored Screen model.
    result.semantic_prompt.seen = 'seen: {
        if result.cursor.semantic_content == terminal_page::SemanticContent::Prompt {
            break 'seen true;
        }

        let mut node = result.pages.get_top_left(point::Tag::Screen).node;
        loop {
            // SAFETY: `node` is a live node of the freshly built PageList and
            // every builder-produced page is resident; shared reads only.
            let native_page = unsafe { &*Node::page_assume_resident(node) };
            for y in 0..native_page.size.rows as usize {
                let row_ptr = native_page.get_row(y);
                // SAFETY: live page row/cells; shared reads only.
                unsafe {
                    if (*row_ptr).semantic_prompt() != terminal_page::SemanticPrompt::None {
                        break 'seen true;
                    }

                    let cells: &[terminal_page::Cell] =
                        &(&*native_page.get_cells(row_ptr))[0..native_page.size.cols as usize];
                    for cell in cells {
                        if cell.semantic_content() == terminal_page::SemanticContent::Prompt {
                            break 'seen true;
                        }
                    }
                }
            }

            // SAFETY: as above.
            let next = unsafe { (*node).next };
            if next.is_null() {
                break 'seen false;
            }
            node = next;
        }
    };

    // Kitty image payloads are outside this record, but the restored Screen
    // must still receive the caller's terminal-wide storage policy.
    if crate::options::KITTY_GRAPHICS {
        // result.kitty_images.setLimit(io_, alloc, &result, ...);
        // result.kitty_images.image_limits = options.kitty_image_loading_limits;
        // PORT NOTE: the move-out shape mirrors `Screen::init` (see its PORT
        // NOTE): `set_limit` takes the storage plus a raw `*mut Screen`.
        let mut images = core::mem::take(&mut result.kitty_images);
        images.set_limit(
            alloc,
            &mut result as *mut TerminalScreen,
            options.kitty_image_storage_limit,
        );
        images.image_limits = options.kitty_image_loading_limits;
        result.kitty_images = images;
    }

    // All fallible reconstruction is complete; verify the native invariants
    // before transferring ownership to the caller.
    result.pages.assert_integrity();
    result.assert_integrity();
    Ok(Decoded {
        key,
        history_rows: header.history_rows,
        screen: result,
    })
}

/// Errors possible while decoding fixed SCREEN payload fields.
// const PayloadDecodeError = std.Io.Reader.Error || error{InvalidKey};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PayloadDecodeError {
    // std.Io.Reader.Error
    ReadFailed,
    EndOfStream,
    InvalidKey,
}

impl From<io::ReadError> for PayloadDecodeError {
    fn from(err: io::ReadError) -> PayloadDecodeError {
        match err {
            io::ReadError::ReadFailed => PayloadDecodeError::ReadFailed,
            io::ReadError::EndOfStream => PayloadDecodeError::EndOfStream,
        }
    }
}

/// Flags encoded after the cursor's visual shape.
// pub const CursorFlags = packed struct(u8) { ... };
// PORT NOTE: Zig `packed struct(u8)`; explicit fields plus `bits` (the
// `@bitCast` to the backing byte), the snapshot-crate treatment (see
// `grid::Cell`). `_padding` keeps the reference's reserved bits addressable
// for the encode-validation tests.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CursorFlags {
    pub pending_wrap: bool,
    pub protected: bool,
    pub semantic_content: terminal_page::SemanticContent,
    pub semantic_content_clear_eol: bool,
    pub _padding: u8,
}

impl Default for CursorFlags {
    // pending_wrap = false, protected = false, semantic_content = .output,
    // semantic_content_clear_eol = false, _padding = 0.
    fn default() -> CursorFlags {
        CursorFlags {
            pending_wrap: false,
            protected: false,
            semantic_content: terminal_page::SemanticContent::Output,
            semantic_content_clear_eol: false,
            _padding: 0,
        }
    }
}

// The wire bit positions (Zig: implicit in the packed struct; pinned by the
// "cursor, Kitty, and saved cursor flag bit layouts" test).
const CURSOR_PENDING_WRAP_SHIFT: u32 = 0;
const CURSOR_PROTECTED_SHIFT: u32 = 1;
const CURSOR_SEMANTIC_CONTENT_SHIFT: u32 = 2;
const CURSOR_SEMANTIC_CONTENT_CLEAR_EOL_SHIFT: u32 = 4;
const CURSOR_PADDING_SHIFT: u32 = 5;

impl CursorFlags {
    fn init(cursor: &ScreenCursor) -> CursorFlags {
        CursorFlags {
            pending_wrap: cursor.pending_wrap,
            protected: cursor.protected,
            semantic_content: cursor.semantic_content,
            semantic_content_clear_eol: cursor.semantic_content_clear_eol,
            _padding: 0,
        }
    }

    /// `@as(u8, @bitCast(self))`
    pub fn bits(self) -> u8 {
        (self.pending_wrap as u8) << CURSOR_PENDING_WRAP_SHIFT
            | (self.protected as u8) << CURSOR_PROTECTED_SHIFT
            | ((self.semantic_content as u8) & 0x3) << CURSOR_SEMANTIC_CONTENT_SHIFT
            | (self.semantic_content_clear_eol as u8) << CURSOR_SEMANTIC_CONTENT_CLEAR_EOL_SHIFT
            | (self._padding & 0x7) << CURSOR_PADDING_SHIFT
    }

    /// Decode the cursor flag registry, normalizing unknown semantic state.
    pub fn decode(reader: &mut dyn io::Reader) -> Result<CursorFlags, io::ReadError> {
        let raw = reader.take_byte()?;
        let semantic_content_raw: u8 = (raw >> CURSOR_SEMANTIC_CONTENT_SHIFT) & 0x3;
        let semantic_content = semantic_content_from_int(semantic_content_raw)
            .unwrap_or(terminal_page::SemanticContent::Output);

        Ok(CursorFlags {
            pending_wrap: raw & (1 << CURSOR_PENDING_WRAP_SHIFT) != 0,
            protected: raw & (1 << CURSOR_PROTECTED_SHIFT) != 0,
            semantic_content,
            semantic_content_clear_eol: raw & (1 << CURSOR_SEMANTIC_CONTENT_CLEAR_EOL_SHIFT) != 0,
            _padding: 0,
        })
    }
}

/// The packed wire bits shared by current and saved cursor charset state.
///
/// Native enum values are stable, but their backing type is `c_int` in C ABI
/// builds. The wire therefore stores only checked integer values at its fixed
/// bit widths.
// const CharsetBits = packed struct(u16) { ... };
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CharsetBits {
    g0: u8,           // u2
    g1: u8,           // u2
    g2: u8,           // u2
    g3: u8,           // u2
    gl: u8,           // u2
    gr: u8,           // u2
    single_shift: u8, // u3
    _padding: u8,     // u1
}

impl Default for CharsetBits {
    // g0..g3 = 0, gl = 0, gr = 2, single_shift = 0, _padding = 0.
    fn default() -> CharsetBits {
        CharsetBits {
            g0: 0,
            g1: 0,
            g2: 0,
            g3: 0,
            gl: 0,
            gr: 2,
            single_shift: 0,
            _padding: 0,
        }
    }
}

impl CharsetBits {
    /// `@as(u16, @bitCast(self))`
    fn bits(self) -> u16 {
        ((self.g0 as u16) & 0x3)
            | ((self.g1 as u16) & 0x3) << 2
            | ((self.g2 as u16) & 0x3) << 4
            | ((self.g3 as u16) & 0x3) << 6
            | ((self.gl as u16) & 0x3) << 8
            | ((self.gr as u16) & 0x3) << 10
            | ((self.single_shift as u16) & 0x7) << 12
            | ((self._padding as u16) & 0x1) << 15
    }

    /// `@as(CharsetBits, @bitCast(raw))`
    fn from_bits(raw: u16) -> CharsetBits {
        CharsetBits {
            g0: (raw & 0x3) as u8,
            g1: ((raw >> 2) & 0x3) as u8,
            g2: ((raw >> 4) & 0x3) as u8,
            g3: ((raw >> 6) & 0x3) as u8,
            gl: ((raw >> 8) & 0x3) as u8,
            gr: ((raw >> 10) & 0x3) as u8,
            single_shift: ((raw >> 12) & 0x7) as u8,
            _padding: ((raw >> 15) & 0x1) as u8,
        }
    }
}

// fn enumFromInt(comptime T: type, raw: anytype) ?T { ... }
// PORT NOTE: the Zig generic `enumFromInt` (checked cast + `std.enums.fromInt`)
// ports to one helper per stable registry; each maps the declared native
// declaration indices exactly.
fn semantic_content_from_int(raw: u8) -> Option<terminal_page::SemanticContent> {
    match raw {
        0 => Some(terminal_page::SemanticContent::Output),
        1 => Some(terminal_page::SemanticContent::Input),
        2 => Some(terminal_page::SemanticContent::Prompt),
        _ => None,
    }
}

fn screen_key_from_int(raw: u16) -> Option<TerminalScreenKey> {
    match raw {
        0 => Some(TerminalScreenKey::Primary),
        1 => Some(TerminalScreenKey::Alternate),
        _ => None,
    }
}

fn cursor_style_from_int(raw: u8) -> Option<CursorStyle> {
    match raw {
        0 => Some(CursorStyle::Bar),
        1 => Some(CursorStyle::Block),
        2 => Some(CursorStyle::Underline),
        3 => Some(CursorStyle::BlockHollow),
        _ => None,
    }
}

fn charset_from_int(raw: u8) -> Option<terminal_charsets::Charset> {
    match raw {
        0 => Some(terminal_charsets::Charset::Utf8),
        1 => Some(terminal_charsets::Charset::Ascii),
        2 => Some(terminal_charsets::Charset::British),
        3 => Some(terminal_charsets::Charset::DecSpecial),
        _ => None,
    }
}

fn slots_from_int(raw: u8) -> Option<terminal_charsets::Slots> {
    match raw {
        0 => Some(terminal_charsets::Slots::G0),
        1 => Some(terminal_charsets::Slots::G1),
        2 => Some(terminal_charsets::Slots::G2),
        3 => Some(terminal_charsets::Slots::G3),
        _ => None,
    }
}

fn protected_mode_from_int(raw: u8) -> Option<terminal_ansi::ProtectedMode> {
    match raw {
        0 => Some(terminal_ansi::ProtectedMode::Off),
        1 => Some(terminal_ansi::ProtectedMode::Iso),
        2 => Some(terminal_ansi::ProtectedMode::Dec),
        _ => None,
    }
}

fn click_events_from_int(raw: u8) -> Option<terminal_osc::semantic_prompt::ClickEvents> {
    match raw {
        0 => Some(terminal_osc::semantic_prompt::ClickEvents::Absolute),
        1 => Some(terminal_osc::semantic_prompt::ClickEvents::Relative),
        _ => None,
    }
}

fn click_from_int(raw: u8) -> Option<terminal_osc::semantic_prompt::Click> {
    match raw {
        0 => Some(terminal_osc::semantic_prompt::Click::Line),
        1 => Some(terminal_osc::semantic_prompt::Click::Multiple),
        2 => Some(terminal_osc::semantic_prompt::Click::ConservativeVertical),
        3 => Some(terminal_osc::semantic_prompt::Click::SmartVertical),
        _ => None,
    }
}

/// `@intFromEnum(std.meta.activeTag(semantic_click))` /
/// `@intFromEnum(value)` for the semantic-click registry.
fn semantic_click_kind_int(value: &SemanticClick) -> u8 {
    match value {
        SemanticClick::None => 0,
        SemanticClick::ClickEvents(_) => 1,
        SemanticClick::Cl(_) => 2,
    }
}

fn encode_charset_state(value: &CharsetState) -> Result<u16, PayloadEncodeError> {
    // PORT NOTE: every registry value here fits u2 by construction in the
    // Rust enums (4 variants each), mirroring the reference's checked
    // `std.math.cast(u2, @intFromEnum(...)) orelse error.InvalidCharsetState`.
    let g0 = value.charsets.get(terminal_charsets::Slots::G0) as u8;
    let g1 = value.charsets.get(terminal_charsets::Slots::G1) as u8;
    let g2 = value.charsets.get(terminal_charsets::Slots::G2) as u8;
    let g3 = value.charsets.get(terminal_charsets::Slots::G3) as u8;
    let gl = value.gl as u8;
    let gr = value.gr as u8;
    let single_shift: u8 = if let Some(slot) = value.single_shift {
        (slot as u8) + 1
    } else {
        0
    };
    if g0 > 3 || g1 > 3 || g2 > 3 || g3 > 3 || gl > 3 || gr > 3 {
        return Err(PayloadEncodeError::InvalidCharsetState);
    }

    let bits = CharsetBits {
        g0,
        g1,
        g2,
        g3,
        gl,
        gr,
        single_shift,
        _padding: 0,
    };
    Ok(bits.bits())
}

fn decode_charset_state(raw: u16) -> CharsetState {
    let bits = CharsetBits::from_bits(raw);

    let mut result = CharsetState::default();
    result.gl = slots_from_int(bits.gl).unwrap_or(result.gl);
    result.gr = slots_from_int(bits.gr).unwrap_or(result.gr);
    result.single_shift = if bits.single_shift == 0 {
        None
    } else if bits.single_shift <= 4 {
        slots_from_int(bits.single_shift - 1)
    } else {
        None
    };
    result.charsets.set(
        terminal_charsets::Slots::G0,
        charset_from_int(bits.g0).unwrap_or(terminal_charsets::Charset::Utf8),
    );
    result.charsets.set(
        terminal_charsets::Slots::G1,
        charset_from_int(bits.g1).unwrap_or(terminal_charsets::Charset::Utf8),
    );
    result.charsets.set(
        terminal_charsets::Slots::G2,
        charset_from_int(bits.g2).unwrap_or(terminal_charsets::Charset::Utf8),
    );
    result.charsets.set(
        terminal_charsets::Slots::G3,
        charset_from_int(bits.g3).unwrap_or(terminal_charsets::Charset::Utf8),
    );
    result
}

/// The complete fixed-size Kitty keyboard stack.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KittyKeyboard {
    pub index: u8,
    pub flags: [KittyKeyboardFlags; 8],
}

impl Default for KittyKeyboard {
    // index = 0, flags = @splat(.{}).
    fn default() -> KittyKeyboard {
        KittyKeyboard {
            index: 0,
            flags: [KittyKeyboardFlags::default(); 8],
        }
    }
}

/// One byte in the fixed Kitty keyboard flag stack.
// pub const Flags = packed struct(u8) { ... };
// PORT NOTE: nested `KittyKeyboard.Flags` hoisted to module scope (Rust nested
// type names, the crate-wide treatment); packed byte via `bits`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct KittyKeyboardFlags {
    pub disambiguate: bool,
    pub report_events: bool,
    pub report_alternates: bool,
    pub report_all: bool,
    pub report_associated: bool,
    pub _padding: u8, // u3
}

impl KittyKeyboardFlags {
    /// `@as(u8, @bitCast(self))`
    pub fn bits(self) -> u8 {
        (self.disambiguate as u8)
            | (self.report_events as u8) << 1
            | (self.report_alternates as u8) << 2
            | (self.report_all as u8) << 3
            | (self.report_associated as u8) << 4
            | (self._padding & 0x7) << 5
    }

    /// Decode one Kitty keyboard flag byte, ignoring reserved bits.
    pub fn decode(reader: &mut dyn io::Reader) -> Result<KittyKeyboardFlags, io::ReadError> {
        let raw = reader.take_byte()?;
        // var result: Flags = @bitCast(raw); result._padding = 0;
        Ok(KittyKeyboardFlags {
            disambiguate: raw & (1 << 0) != 0,
            report_events: raw & (1 << 1) != 0,
            report_alternates: raw & (1 << 2) != 0,
            report_all: raw & (1 << 3) != 0,
            report_associated: raw & (1 << 4) != 0,
            _padding: 0,
        })
    }
}

impl KittyKeyboard {
    fn init(value: &terminal_kitty::KeyFlagStack) -> KittyKeyboard {
        let mut result = KittyKeyboard {
            index: value.idx(),
            ..KittyKeyboard::default()
        };
        for (native, snapshot) in value.flags().iter().zip(result.flags.iter_mut()) {
            *snapshot = KittyKeyboardFlags {
                disambiguate: native.disambiguate,
                report_events: native.report_events,
                report_alternates: native.report_alternates,
                report_all: native.report_all,
                report_associated: native.report_associated,
                _padding: 0,
            };
        }
        result
    }

    fn terminal(&self) -> terminal_kitty::KeyFlagStack {
        let mut flags = [terminal_kitty::KeyFlags::DISABLED; 8];
        for (snapshot, native) in self.flags.iter().zip(flags.iter_mut()) {
            *native = terminal_kitty::KeyFlags {
                disambiguate: snapshot.disambiguate,
                report_events: snapshot.report_events,
                report_alternates: snapshot.report_alternates,
                report_all: snapshot.report_all,
                report_associated: snapshot.report_associated,
            };
        }
        // .idx = @intCast(self.index) — bounds-checked by `FlagStack::new`.
        terminal_kitty::KeyFlagStack::new(self.index, flags)
    }

    /// Decode the complete fixed Kitty keyboard stack.
    pub fn decode(reader: &mut dyn io::Reader) -> Result<KittyKeyboard, io::ReadError> {
        let index_raw = reader.take_byte()?;
        let index = if index_raw < 8 { index_raw } else { 0 };

        let mut flags = [KittyKeyboardFlags::default(); 8];
        for entry in flags.iter_mut() {
            *entry = KittyKeyboardFlags::decode(reader)?;
        }
        Ok(KittyKeyboard { index, flags })
    }
}

/// Decode semantic-click state, falling back to disabled for unknown values.
fn decode_semantic_click(reader: &mut dyn io::Reader) -> Result<SemanticClick, io::ReadError> {
    let kind = reader.take_byte()?;
    let value = reader.take_byte()?;

    Ok(match kind {
        0 => SemanticClick::None,
        1 => match click_events_from_int(value) {
            Some(v) => SemanticClick::ClickEvents(v),
            None => SemanticClick::None,
        },
        2 => match click_from_int(value) {
            Some(v) => SemanticClick::Cl(v),
            None => SemanticClick::None,
        },
        // kind orelse return .none
        _ => SemanticClick::None,
    })
}

/// The optional fixed-size saved cursor state.
#[derive(Clone, Debug)]
pub struct SavedCursor {
    pub x: u16,
    pub y: u16,
    pub pen: TerminalStyle,
    pub flags: SavedCursorFlags,
    pub charset: CharsetState,
}

/// Flags encoded with an optional saved cursor.
// pub const Flags = packed struct(u8) { ... };
// PORT NOTE: nested `SavedCursor.Flags` hoisted (see `KittyKeyboardFlags`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SavedCursorFlags {
    pub protected: bool,
    pub pending_wrap: bool,
    pub origin: bool,
    pub _padding: u8, // u5
}

impl SavedCursorFlags {
    /// `@as(u8, @bitCast(self))`
    pub fn bits(self) -> u8 {
        (self.protected as u8)
            | (self.pending_wrap as u8) << 1
            | (self.origin as u8) << 2
            | (self._padding & 0x1F) << 3
    }

    /// Decode saved cursor flags, ignoring reserved bits.
    pub fn decode(reader: &mut dyn io::Reader) -> Result<SavedCursorFlags, io::ReadError> {
        let raw = reader.take_byte()?;
        // var result: Flags = @bitCast(raw); result._padding = 0;
        Ok(SavedCursorFlags {
            protected: raw & (1 << 0) != 0,
            pending_wrap: raw & (1 << 1) != 0,
            origin: raw & (1 << 2) != 0,
            _padding: 0,
        })
    }
}

impl PartialEq for SavedCursor {
    // (`std.testing.expectEqualDeep` structural equality; the pen compares via
    // the style's packed equality.)
    fn eq(&self, other: &SavedCursor) -> bool {
        self.x == other.x
            && self.y == other.y
            && self.pen.eql(&other.pen)
            && self.flags == other.flags
            && self.charset == other.charset
    }
}

impl SavedCursor {
    /// Number of bytes written by `encode`, calculated using the encoder.
    ///
    /// PORT NOTE: the reference computes this at comptime with the encoder
    /// itself and `comptime`-asserts it is 23. The port pins the constant; the
    /// `saved_cursor_len_matches_encoder` test carries both the
    /// synchronization and the assert.
    pub const LEN: usize = 23;

    fn init(value: &crate::screen::SavedCursor) -> SavedCursor {
        SavedCursor {
            x: value.x,
            y: value.y,
            pen: value.style,
            flags: SavedCursorFlags {
                protected: value.protected,
                pending_wrap: value.pending_wrap,
                origin: value.origin,
                _padding: 0,
            },
            charset: value.charset.clone(),
        }
    }

    fn terminal(&self, cols: u16, rows: u16) -> crate::screen::SavedCursor {
        let x = self.x.min(cols - 1);
        crate::screen::SavedCursor {
            x,
            y: self.y.min(rows - 1),
            style: self.pen,
            protected: self.flags.protected,
            pending_wrap: self.flags.pending_wrap && x == cols - 1,
            origin: self.flags.origin,
            charset: self.charset.clone(),
        }
    }

    /// Encode one optional saved cursor value.
    pub fn encode(&self, writer: &mut dyn io::Writer) -> Result<(), PayloadEncodeError> {
        if self.flags._padding != 0 {
            return Err(PayloadEncodeError::InvalidSavedCursorFlags);
        }
        let charset = encode_charset_state(&self.charset)?;

        io::write_int::<u16>(writer, self.x)?;
        io::write_int::<u16>(writer, self.y)?;
        style::encode(self.pen, writer)?;
        writer.write_byte(self.flags.bits())?;
        io::write_int::<u16>(writer, charset)?;
        Ok(())
    }

    /// Decode and validate one optional saved cursor value.
    pub fn decode(reader: &mut dyn io::Reader) -> Result<SavedCursor, PayloadDecodeError> {
        Ok(SavedCursor {
            x: io::read_int::<u16>(reader)?,
            y: io::read_int::<u16>(reader)?,
            pen: style::decode_or_null(reader)?.unwrap_or_default(),
            flags: SavedCursorFlags::decode(reader)?,
            charset: decode_charset_state(io::read_int::<u16>(reader)?),
        })
    }

    /// The reference comptime `computeLen` (see `LEN`).
    #[cfg(test)]
    fn compute_len() -> usize {
        let mut buf = [0u8; 128];
        let mut writer = io::FixedWriter::new(&mut buf);
        let value = SavedCursor {
            x: 0,
            y: 0,
            pen: TerminalStyle::default(),
            flags: SavedCursorFlags::default(),
            charset: CharsetState::default(),
        };
        value.encode(&mut writer).unwrap();
        writer.end
    }
}

/// The fixed fields at the start of every SCREEN payload.
#[derive(Clone, Debug)]
pub struct Header {
    pub key: TerminalScreenKey,
    /// Complete pages in the minimal suffix covering the active area.
    pub page_count: u16,
    /// Advisory logical rows before the active area, including resident
    /// SCREEN overlap.
    pub history_rows: u64,
    pub cursor_x: u16,
    pub cursor_y: u16,
    pub cursor_style: CursorStyle,
    pub cursor_flags: CursorFlags,
    pub cursor_pen: TerminalStyle,
    pub hyperlink_implicit_id: u32,
    pub charset: CharsetState,
    pub protected_mode: terminal_ansi::ProtectedMode,
    pub kitty_keyboard: KittyKeyboard,
    pub semantic_click: SemanticClick,
    pub saved_cursor_present: bool,
}

impl PartialEq for Header {
    // (`std.testing.expectEqualDeep` structural equality; the pen compares via
    // the style's packed equality.)
    fn eq(&self, other: &Header) -> bool {
        self.key == other.key
            && self.page_count == other.page_count
            && self.history_rows == other.history_rows
            && self.cursor_x == other.cursor_x
            && self.cursor_y == other.cursor_y
            && self.cursor_style == other.cursor_style
            && self.cursor_flags == other.cursor_flags
            && self.cursor_pen.eql(&other.cursor_pen)
            && self.hyperlink_implicit_id == other.hyperlink_implicit_id
            && self.charset == other.charset
            && self.protected_mode == other.protected_mode
            && self.kitty_keyboard == other.kitty_keyboard
            && self.semantic_click == other.semantic_click
            && self.saved_cursor_present == other.saved_cursor_present
    }
}

impl Header {
    /// Number of bytes written by `encode`, calculated using the encoder.
    ///
    /// PORT NOTE: the reference computes this at comptime with the encoder
    /// itself and `comptime`-asserts it is 53. The port pins the constant; the
    /// `header_len_matches_encoder` test carries both the synchronization and
    /// the assert.
    pub const LEN: usize = 53;

    /// Initialize the fixed header from one native screen.
    fn init(screen: &TerminalScreen, key: TerminalScreenKey, page_count: u16) -> Header {
        Header {
            key,
            page_count,
            history_rows: (screen.pages.total_rows - screen.pages.rows as usize) as u64,
            cursor_x: screen.cursor.x,
            cursor_y: screen.cursor.y,
            cursor_style: screen.cursor.cursor_style,
            cursor_flags: CursorFlags::init(&screen.cursor),
            cursor_pen: screen.cursor.style,
            hyperlink_implicit_id: screen.cursor.hyperlink_implicit_id,
            charset: screen.charset.clone(),
            protected_mode: screen.protected_mode,
            kitty_keyboard: KittyKeyboard::init(&screen.kitty_keyboard),
            semantic_click: screen.semantic_prompt.click,
            saved_cursor_present: screen.saved_cursor.is_some(),
        }
    }

    /// Encode the fixed SCREEN payload header field by field.
    pub fn encode(&self, writer: &mut dyn io::Writer) -> Result<(), PayloadEncodeError> {
        // Validate packed cursor state before writing any bytes.
        if self.cursor_flags._padding != 0 {
            return Err(PayloadEncodeError::InvalidCursorFlags);
        }

        // Validate and narrow native charset enum values before writing.
        let charset = encode_charset_state(&self.charset)?;

        // Validate the complete Kitty keyboard stack.
        if self.kitty_keyboard.index as usize >= self.kitty_keyboard.flags.len() {
            return Err(PayloadEncodeError::InvalidKittyKeyboardIndex);
        }
        for flags in self.kitty_keyboard.flags.iter() {
            if flags._padding != 0 {
                return Err(PayloadEncodeError::InvalidKittyKeyboardFlags);
            }
        }

        // Screen identity, expected history extent, and cursor position.
        io::write_int::<u16>(writer, self.key as u16)?;
        io::write_int::<u16>(writer, self.page_count)?;
        io::write_int::<u64>(writer, self.history_rows)?;
        io::write_int::<u16>(writer, self.cursor_x)?;
        io::write_int::<u16>(writer, self.cursor_y)?;

        // Current cursor rendering state.
        writer.write_byte(self.cursor_style as u8)?;
        writer.write_byte(self.cursor_flags.bits())?;
        style::encode(self.cursor_pen, writer)?;
        io::write_int::<u32>(writer, self.hyperlink_implicit_id)?;

        // Charset and selective-erase state.
        io::write_int::<u16>(writer, charset)?;
        writer.write_byte(self.protected_mode as u8)?;

        // Keyboard modes and semantic-click state.
        writer.write_byte(self.kitty_keyboard.index)?;
        for flags in self.kitty_keyboard.flags.iter() {
            writer.write_byte(flags.bits())?;
        }
        writer.write_byte(semantic_click_kind_int(&self.semantic_click))?;
        match self.semantic_click {
            SemanticClick::None => writer.write_byte(0)?,
            SemanticClick::ClickEvents(value) => writer.write_byte(value as u8)?,
            SemanticClick::Cl(value) => writer.write_byte(value as u8)?,
        }

        // Presence of the optional saved-cursor suffix.
        writer.write_byte(self.saved_cursor_present as u8)?;
        Ok(())
    }

    /// Decode the fixed SCREEN payload header.
    ///
    /// The screen key remains structural because it routes the following PAGE
    /// sequence. Unknown semantic fields use native defaults instead.
    pub fn decode(reader: &mut dyn io::Reader) -> Result<Header, PayloadDecodeError> {
        // Screen identity, expected history extent, and cursor position.
        let key = screen_key_from_int(io::read_int::<u16>(reader)?)
            .ok_or(PayloadDecodeError::InvalidKey)?;
        let page_count = io::read_int::<u16>(reader)?;
        let history_rows = io::read_int::<u64>(reader)?;
        let cursor_x = io::read_int::<u16>(reader)?;
        let cursor_y = io::read_int::<u16>(reader)?;

        // Current cursor rendering state.
        let cursor_style = cursor_style_from_int(reader.take_byte()?).unwrap_or(CursorStyle::Block);
        let cursor_flags = CursorFlags::decode(reader)?;
        let cursor_pen: TerminalStyle = style::decode_or_null(reader)?.unwrap_or_default();
        let hyperlink_implicit_id = io::read_int::<u32>(reader)?;

        // Charset and selective-erase state.
        let charset = decode_charset_state(io::read_int::<u16>(reader)?);
        let protected_mode = protected_mode_from_int(reader.take_byte()?)
            .unwrap_or(terminal_ansi::ProtectedMode::Off);

        // Keyboard modes and semantic-click state.
        let kitty_keyboard = KittyKeyboard::decode(reader)?;
        let semantic_click = decode_semantic_click(reader)?;

        // Presence of the optional saved-cursor suffix.
        let saved_cursor_present = reader.take_byte()? != 0;

        Ok(Header {
            key,
            page_count,
            history_rows,
            cursor_x,
            cursor_y,

            cursor_style,
            cursor_flags,
            cursor_pen,
            hyperlink_implicit_id,

            charset,
            protected_mode,

            kitty_keyboard,
            semantic_click,

            saved_cursor_present,
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
            history_rows: 0,
            cursor_x: 0,
            cursor_y: 0,
            cursor_style: CursorStyle::Bar,
            cursor_flags: CursorFlags::default(),
            cursor_pen: TerminalStyle::default(),
            hyperlink_implicit_id: 0,
            charset: CharsetState::default(),
            protected_mode: terminal_ansi::ProtectedMode::Off,
            kitty_keyboard: KittyKeyboard::default(),
            semantic_click: SemanticClick::None,
            saved_cursor_present: false,
        };
        value.encode(&mut writer).unwrap();
        writer.end
    }
}

/// Encode the SCREEN payload, excluding its following PAGE records.
fn encode_payload(
    screen: &TerminalScreen,
    key: TerminalScreenKey,
    page_count: u16,
    writer: &mut dyn io::Writer,
) -> Result<(), PayloadEncodeError> {
    Header::init(screen, key, page_count).encode(writer)?;

    if let Some(saved_cursor) = screen.saved_cursor.as_ref() {
        SavedCursor::init(saved_cursor).encode(writer)?;
    }

    // const cursor_hyperlink: ?TerminalHyperlink =
    //     if (screen.cursor.hyperlink) |entry| entry.* else null;
    // SAFETY: the cursor's heap hyperlink pointer is live while the screen is
    // (owned via `Box::into_raw`; see `Cursor.hyperlink`); shared read only.
    let cursor_hyperlink: Option<&TerminalHyperlink> =
        screen.cursor.hyperlink.map(|entry| unsafe { &*entry });
    encode_cursor_hyperlink(cursor_hyperlink, writer)?;
    Ok(())
}

/// The variable cursor hyperlink at the end of every SCREEN payload.
// pub const CursorHyperlink = ?TerminalHyperlink;
pub type CursorHyperlink = Option<TerminalHyperlink>;

/// Encode the optional cursor hyperlink at the end of a SCREEN payload.
pub fn encode_cursor_hyperlink(
    value: Option<&TerminalHyperlink>,
    writer: &mut dyn io::Writer,
) -> Result<(), hyperlink::EncodeError> {
    if let Some(entry) = value {
        return hyperlink::encode(entry, writer);
    }
    writer.write_byte(0)?;
    Ok(())
}

/// Decode one allocator-owned optional cursor hyperlink.
///
/// The caller owns a non-null returned value and must call `Hyperlink.deinit`.
pub fn decode_cursor_hyperlink(
    reader: &mut dyn io::Reader,
) -> Result<CursorHyperlink, io::ReadError> {
    if reader.peek_byte()? == 0 {
        let _ = reader.take_byte()?;
        return Ok(None);
    }

    match hyperlink::decode(reader) {
        Ok(value) => Ok(Some(value)),
        Err(hyperlink::DecodeError::ReadFailed) => Err(io::ReadError::ReadFailed),

        // The cursor hyperlink is the final SCREEN payload field, so its
        // record boundary lets us discard an invalid or unrepresentable value
        // without losing the following PAGE sequence.
        Err(hyperlink::DecodeError::EndOfStream)
        | Err(hyperlink::DecodeError::OutOfMemory)
        | Err(hyperlink::DecodeError::InvalidKind)
        | Err(hyperlink::DecodeError::InvalidUri)
        | Err(hyperlink::DecodeError::InvalidExplicitId) => {
            reader.discard_remaining()?;
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::fixture;
    use super::super::grid;
    use super::*;
    use crate::screen::Options as ScreenOptions;
    use crate::snapshot::io::Reader as _;
    use crate::snapshot::io::Writer as _;

    // const test_header_fixture = test_fixture.parse(@embedFile("testdata/screen-header-v1.hex"));
    fn test_header_fixture() -> Vec<u8> {
        fixture::parse(include_str!("testdata/screen-header-v1.hex"))
    }

    // const test_saved_cursor_fixture = test_fixture.parse(@embedFile("testdata/screen-saved-cursor-v1.hex"));
    fn test_saved_cursor_fixture() -> Vec<u8> {
        fixture::parse(include_str!("testdata/screen-saved-cursor-v1.hex"))
    }

    fn screen_options(cols: u16, rows: u16, max_scrollback_bytes: Option<usize>) -> ScreenOptions {
        ScreenOptions {
            cols,
            rows,
            max_scrollback_bytes,
            ..ScreenOptions::default()
        }
    }

    fn test_charset_state() -> CharsetState {
        let mut result = CharsetState {
            gl: terminal_charsets::Slots::G1,
            gr: terminal_charsets::Slots::G3,
            single_shift: Some(terminal_charsets::Slots::G2),
            ..CharsetState::default()
        };
        result.charsets.set(
            terminal_charsets::Slots::G0,
            terminal_charsets::Charset::Utf8,
        );
        result.charsets.set(
            terminal_charsets::Slots::G1,
            terminal_charsets::Charset::Ascii,
        );
        result.charsets.set(
            terminal_charsets::Slots::G2,
            terminal_charsets::Charset::British,
        );
        result.charsets.set(
            terminal_charsets::Slots::G3,
            terminal_charsets::Charset::DecSpecial,
        );
        result
    }

    fn test_header() -> Header {
        Header {
            key: TerminalScreenKey::Alternate,
            page_count: 0x0203,
            history_rows: 0x0102030405060708,
            cursor_x: 0x0405,
            cursor_y: 0x0607,
            cursor_style: CursorStyle::BlockHollow,
            cursor_flags: CursorFlags {
                pending_wrap: true,
                semantic_content: terminal_page::SemanticContent::Prompt,
                semantic_content_clear_eol: true,
                ..CursorFlags::default()
            },
            cursor_pen: TerminalStyle {
                fg_color: terminal_style::Color::None,
                bg_color: terminal_style::Color::Palette(0x7f),
                underline_color: terminal_style::Color::Rgb(crate::color::RGB {
                    r: 0x12,
                    g: 0x34,
                    b: 0x56,
                }),
                flags: terminal_style::Flags {
                    bold: true,
                    italic: true,
                    faint: true,
                    blink: true,
                    inverse: true,
                    invisible: true,
                    strikethrough: true,
                    overline: true,
                    underline: crate::sgr::Underline::Curly,
                },
            },
            hyperlink_implicit_id: 0x0a0b0c0d,
            charset: test_charset_state(),
            protected_mode: terminal_ansi::ProtectedMode::Dec,
            kitty_keyboard: KittyKeyboard {
                index: 7,
                flags: [
                    KittyKeyboardFlags {
                        disambiguate: true,
                        ..KittyKeyboardFlags::default()
                    },
                    KittyKeyboardFlags {
                        report_events: true,
                        ..KittyKeyboardFlags::default()
                    },
                    KittyKeyboardFlags {
                        report_alternates: true,
                        ..KittyKeyboardFlags::default()
                    },
                    KittyKeyboardFlags {
                        report_all: true,
                        ..KittyKeyboardFlags::default()
                    },
                    KittyKeyboardFlags {
                        report_associated: true,
                        ..KittyKeyboardFlags::default()
                    },
                    KittyKeyboardFlags {
                        disambiguate: true,
                        report_events: true,
                        report_alternates: true,
                        report_all: true,
                        report_associated: true,
                        _padding: 0,
                    },
                    KittyKeyboardFlags::default(),
                    KittyKeyboardFlags {
                        disambiguate: true,
                        report_associated: true,
                        ..KittyKeyboardFlags::default()
                    },
                ],
            },
            semantic_click: SemanticClick::Cl(terminal_osc::semantic_prompt::Click::SmartVertical),
            saved_cursor_present: true,
        }
    }

    fn test_saved_cursor() -> SavedCursor {
        SavedCursor {
            x: 0x0102,
            y: 0x0304,
            pen: TerminalStyle::default(),
            flags: SavedCursorFlags {
                protected: true,
                pending_wrap: true,
                origin: true,
                _padding: 0,
            },
            charset: test_charset_state(),
        }
    }

    // comptime { assert(Header.len == 53); assert(SavedCursor.len == 23); }
    #[test]
    fn header_len_matches_encoder() {
        assert_eq!(Header::LEN, Header::compute_len());
        assert_eq!(Header::LEN, 53);
    }

    #[test]
    fn saved_cursor_len_matches_encoder() {
        assert_eq!(SavedCursor::LEN, SavedCursor::compute_len());
        assert_eq!(SavedCursor::LEN, 23);
    }

    // test "SCREEN header golden encoding and decoding"
    #[test]
    fn screen_header_golden_encoding_and_decoding() {
        let test_header_fixture = test_header_fixture();

        let mut encoded = [0u8; Header::LEN];
        let mut writer = io::FixedWriter::new(&mut encoded);
        test_header().encode(&mut writer).unwrap();

        fixture::expect_equal(
            fixture::Kind::Bytes,
            "src/snapshot/testdata/screen-header-v1.hex",
            "snapshot_fixture-screen-header-v1.hex",
            &test_header_fixture,
            writer.buffered(),
        );
        assert_eq!(Header::LEN, test_header_fixture.len());

        // Exercise the streaming path with less buffered data than every
        // integer.
        let mut source = io::FixedReader::new(&test_header_fixture);
        let mut buffer = [0u8; 1];
        let mut limited = io::LimitedReader::new(&mut source, io::Limit::Unlimited, &mut buffer);

        assert_eq!(test_header(), Header::decode(&mut limited).unwrap());
    }

    // test "native enum values used by the SCREEN format"
    #[test]
    fn native_enum_values_used_by_the_screen_format() {
        let keys = [TerminalScreenKey::Primary, TerminalScreenKey::Alternate];
        for (expected, value) in keys.iter().enumerate() {
            assert_eq!(expected, *value as usize);
        }

        let cursor_styles = [
            CursorStyle::Bar,
            CursorStyle::Block,
            CursorStyle::Underline,
            CursorStyle::BlockHollow,
        ];
        for (expected, value) in cursor_styles.iter().enumerate() {
            assert_eq!(expected, *value as usize);
        }

        let semantic_contents = [
            terminal_page::SemanticContent::Output,
            terminal_page::SemanticContent::Input,
            terminal_page::SemanticContent::Prompt,
        ];
        for (expected, value) in semantic_contents.iter().enumerate() {
            assert_eq!(expected, *value as usize);
        }

        let charsets = [
            terminal_charsets::Charset::Utf8,
            terminal_charsets::Charset::Ascii,
            terminal_charsets::Charset::British,
            terminal_charsets::Charset::DecSpecial,
        ];
        for (expected, value) in charsets.iter().enumerate() {
            assert_eq!(expected, *value as usize);
        }

        let slots = [
            terminal_charsets::Slots::G0,
            terminal_charsets::Slots::G1,
            terminal_charsets::Slots::G2,
            terminal_charsets::Slots::G3,
        ];
        for (expected, value) in slots.iter().enumerate() {
            assert_eq!(expected, *value as usize);
        }

        let protected_modes = [
            terminal_ansi::ProtectedMode::Off,
            terminal_ansi::ProtectedMode::Iso,
            terminal_ansi::ProtectedMode::Dec,
        ];
        for (expected, value) in protected_modes.iter().enumerate() {
            assert_eq!(expected, *value as usize);
        }

        let click_kinds = [
            SemanticClick::None,
            SemanticClick::ClickEvents(terminal_osc::semantic_prompt::ClickEvents::Absolute),
            SemanticClick::Cl(terminal_osc::semantic_prompt::Click::Line),
        ];
        for (expected, value) in click_kinds.iter().enumerate() {
            assert_eq!(expected, semantic_click_kind_int(value) as usize);
        }

        let click_events = [
            terminal_osc::semantic_prompt::ClickEvents::Absolute,
            terminal_osc::semantic_prompt::ClickEvents::Relative,
        ];
        for (expected, value) in click_events.iter().enumerate() {
            assert_eq!(expected, *value as usize);
        }

        let clicks = [
            terminal_osc::semantic_prompt::Click::Line,
            terminal_osc::semantic_prompt::Click::Multiple,
            terminal_osc::semantic_prompt::Click::ConservativeVertical,
            terminal_osc::semantic_prompt::Click::SmartVertical,
        ];
        for (expected, value) in clicks.iter().enumerate() {
            assert_eq!(expected, *value as usize);
        }
    }

    // test "cursor, Kitty, and saved cursor flag bit layouts"
    #[test]
    fn cursor_kitty_and_saved_cursor_flag_bit_layouts() {
        struct CursorCase {
            value: CursorFlags,
            expected: u8,
        }
        let cursor_cases = [
            CursorCase {
                value: CursorFlags {
                    pending_wrap: true,
                    ..CursorFlags::default()
                },
                expected: 1 << 0,
            },
            CursorCase {
                value: CursorFlags {
                    protected: true,
                    ..CursorFlags::default()
                },
                expected: 1 << 1,
            },
            CursorCase {
                value: CursorFlags {
                    semantic_content: terminal_page::SemanticContent::Input,
                    ..CursorFlags::default()
                },
                expected: 1 << 2,
            },
            CursorCase {
                value: CursorFlags {
                    semantic_content: terminal_page::SemanticContent::Prompt,
                    ..CursorFlags::default()
                },
                expected: 2 << 2,
            },
            CursorCase {
                value: CursorFlags {
                    semantic_content_clear_eol: true,
                    ..CursorFlags::default()
                },
                expected: 1 << 4,
            },
        ];
        for case in cursor_cases {
            assert_eq!(case.expected, case.value.bits());
        }

        struct KittyCase {
            value: KittyKeyboardFlags,
            expected: u8,
        }
        let kitty_cases = [
            KittyCase {
                value: KittyKeyboardFlags {
                    disambiguate: true,
                    ..KittyKeyboardFlags::default()
                },
                expected: 1 << 0,
            },
            KittyCase {
                value: KittyKeyboardFlags {
                    report_events: true,
                    ..KittyKeyboardFlags::default()
                },
                expected: 1 << 1,
            },
            KittyCase {
                value: KittyKeyboardFlags {
                    report_alternates: true,
                    ..KittyKeyboardFlags::default()
                },
                expected: 1 << 2,
            },
            KittyCase {
                value: KittyKeyboardFlags {
                    report_all: true,
                    ..KittyKeyboardFlags::default()
                },
                expected: 1 << 3,
            },
            KittyCase {
                value: KittyKeyboardFlags {
                    report_associated: true,
                    ..KittyKeyboardFlags::default()
                },
                expected: 1 << 4,
            },
        ];
        for case in kitty_cases {
            assert_eq!(case.expected, case.value.bits());
        }

        struct SavedCase {
            value: SavedCursorFlags,
            expected: u8,
        }
        let saved_cases = [
            SavedCase {
                value: SavedCursorFlags {
                    protected: true,
                    ..SavedCursorFlags::default()
                },
                expected: 1 << 0,
            },
            SavedCase {
                value: SavedCursorFlags {
                    pending_wrap: true,
                    ..SavedCursorFlags::default()
                },
                expected: 1 << 1,
            },
            SavedCase {
                value: SavedCursorFlags {
                    origin: true,
                    ..SavedCursorFlags::default()
                },
                expected: 1 << 2,
            },
        ];
        for case in saved_cases {
            assert_eq!(case.expected, case.value.bits());
        }
    }

    // test "header encoding rejects invalid state"
    #[test]
    fn header_encoding_rejects_invalid_state() {
        let mut encoded = [0u8; Header::LEN];

        let mut invalid_cursor_flags = test_header();
        invalid_cursor_flags.cursor_flags._padding = 1;
        let mut cursor_flags_writer = io::FixedWriter::new(&mut encoded);
        assert_eq!(
            Err(PayloadEncodeError::InvalidCursorFlags),
            invalid_cursor_flags.encode(&mut cursor_flags_writer),
        );

        let mut invalid_kitty_index = test_header();
        invalid_kitty_index.kitty_keyboard.index = 8;
        let mut kitty_index_writer = io::FixedWriter::new(&mut encoded);
        assert_eq!(
            Err(PayloadEncodeError::InvalidKittyKeyboardIndex),
            invalid_kitty_index.encode(&mut kitty_index_writer),
        );

        let mut invalid_kitty_flags = test_header();
        invalid_kitty_flags.kitty_keyboard.flags[3]._padding = 1;
        let mut kitty_flags_writer = io::FixedWriter::new(&mut encoded);
        assert_eq!(
            Err(PayloadEncodeError::InvalidKittyKeyboardFlags),
            invalid_kitty_flags.encode(&mut kitty_flags_writer),
        );
    }

    // test "header decoding rejects structural values"
    #[test]
    fn header_decoding_rejects_structural_values() {
        let valid = [0u8; Header::LEN];

        let mut invalid_key = valid;
        invalid_key[0] = 2;
        let mut key_reader = io::FixedReader::new(&invalid_key);
        assert_eq!(
            Err(PayloadDecodeError::InvalidKey),
            Header::decode(&mut key_reader),
        );
    }

    // test "header decoding normalizes unknown semantic values"
    #[test]
    fn header_decoding_normalizes_unknown_semantic_values() {
        let mut fixture_bytes = [0u8; Header::LEN];

        fixture_bytes[16] = 4; // Unknown cursor style.
        fixture_bytes[17] = 0xFF; // Known flags, unknown semantic value, reserved bits.
        fixture_bytes[18] = 3; // Unknown cursor foreground color kind.
        fixture_bytes[39] = 0xD0; // Invalid single shift plus a reserved bit.
        fixture_bytes[40] = 3; // Unknown protected mode.
        fixture_bytes[41] = 8; // Out-of-range Kitty keyboard index.
        for b in fixture_bytes[42..50].iter_mut() {
            *b = 0xFF; // Known flags plus reserved bits.
        }
        fixture_bytes[50] = 3; // Unknown semantic-click kind.
        fixture_bytes[51] = 0xFF;
        fixture_bytes[52] = 2; // Noncanonical true.

        let mut reader = io::FixedReader::new(&fixture_bytes);
        let decoded = Header::decode(&mut reader).unwrap();

        assert_eq!(CursorStyle::Block, decoded.cursor_style);
        assert!(decoded.cursor_pen.default_style());
        assert!(decoded.cursor_flags.pending_wrap);
        assert!(decoded.cursor_flags.protected);
        assert_eq!(
            terminal_page::SemanticContent::Output,
            decoded.cursor_flags.semantic_content,
        );
        assert!(decoded.cursor_flags.semantic_content_clear_eol);
        assert_eq!(0u8, decoded.cursor_flags._padding);

        assert_eq!(None, decoded.charset.single_shift);
        assert_eq!(terminal_charsets::Slots::G0, decoded.charset.gr);
        assert_eq!(terminal_ansi::ProtectedMode::Off, decoded.protected_mode);
        assert_eq!(0u8, decoded.kitty_keyboard.index);
        for flags in decoded.kitty_keyboard.flags.iter() {
            assert!(flags.disambiguate);
            assert!(flags.report_events);
            assert!(flags.report_alternates);
            assert!(flags.report_all);
            assert!(flags.report_associated);
            assert_eq!(0u8, flags._padding);
        }
        assert_eq!(SemanticClick::None, decoded.semantic_click);
        assert!(decoded.saved_cursor_present);

        // Known semantic-click kinds with unknown or noncanonical values also
        // degrade to disabled while consuming both fixed bytes.
        let invalid_semantic_clicks: [[u8; 2]; 3] = [[0, 1], [1, 2], [2, 4]];
        for invalid in invalid_semantic_clicks {
            let mut semantic_fixture = [0u8; Header::LEN];
            semantic_fixture[50] = invalid[0];
            semantic_fixture[51] = invalid[1];
            let mut semantic_reader = io::FixedReader::new(&semantic_fixture);
            let semantic_decoded = Header::decode(&mut semantic_reader).unwrap();
            assert_eq!(SemanticClick::None, semantic_decoded.semantic_click);
        }
    }

    // test "header decoding rejects every truncation"
    #[test]
    fn header_decoding_rejects_every_truncation() {
        let test_header_fixture = test_header_fixture();
        for fixture_len in 0..Header::LEN {
            let mut reader = io::FixedReader::new(&test_header_fixture[0..fixture_len]);
            assert_eq!(
                Err(PayloadDecodeError::EndOfStream),
                Header::decode(&mut reader),
            );
        }
    }

    // test "native SCREEN payload omits absent optional state"
    #[test]
    fn native_screen_payload_omits_absent_optional_state() {
        let mut screen = TerminalScreen::init(Allocator, screen_options(2, 2, Some(0))).unwrap();
        // defer screen.deinit();

        let mut encoded = [0u8; Header::LEN + 1];
        let end = {
            let mut writer = io::FixedWriter::new(&mut encoded);
            encode_payload(&screen, TerminalScreenKey::Primary, 1, &mut writer).unwrap();
            writer.end
        };
        assert_eq!(Header::LEN + 1, end);

        let mut reader = io::FixedReader::new(&encoded[0..end]);
        let header = Header::decode(&mut reader).unwrap();
        assert_eq!(TerminalScreenKey::Primary, header.key);
        assert_eq!(1u16, header.page_count);
        assert_eq!(0u64, header.history_rows);
        assert!(!header.saved_cursor_present);
        assert!(decode_cursor_hyperlink(&mut reader).unwrap().is_none());

        screen.deinit();
    }

    // test "saved cursor golden encoding and decoding"
    #[test]
    fn saved_cursor_golden_encoding_and_decoding() {
        let test_saved_cursor_fixture = test_saved_cursor_fixture();

        let mut encoded = [0u8; SavedCursor::LEN];
        let mut writer = io::FixedWriter::new(&mut encoded);
        test_saved_cursor().encode(&mut writer).unwrap();
        fixture::expect_equal(
            fixture::Kind::Bytes,
            "src/snapshot/testdata/screen-saved-cursor-v1.hex",
            "snapshot_fixture-screen-saved-cursor-v1.hex",
            &test_saved_cursor_fixture,
            writer.buffered(),
        );
        assert_eq!(SavedCursor::LEN, test_saved_cursor_fixture.len());

        let mut source = io::FixedReader::new(&test_saved_cursor_fixture);
        let mut buffer = [0u8; 1];
        let mut limited = io::LimitedReader::new(&mut source, io::Limit::Unlimited, &mut buffer);
        assert_eq!(
            test_saved_cursor(),
            SavedCursor::decode(&mut limited).unwrap(),
        );
    }

    // test "saved cursor encoding rejects and decoding normalizes invalid state"
    #[test]
    fn saved_cursor_encoding_rejects_and_decoding_normalizes_invalid_state() {
        let mut encoded = [0u8; SavedCursor::LEN];

        let mut invalid_flags = test_saved_cursor();
        invalid_flags.flags._padding = 1;
        let mut flags_writer = io::FixedWriter::new(&mut encoded);
        assert_eq!(
            Err(PayloadEncodeError::InvalidSavedCursorFlags),
            invalid_flags.encode(&mut flags_writer),
        );

        let mut invalid = [0u8; SavedCursor::LEN];
        invalid[4] = 3; // Unknown saved-cursor foreground color kind.
        invalid[20] = 0xF9; // Protected plus reserved bits.
        invalid[22] = 0xD0; // Invalid single shift plus a reserved bit.
        let mut reader = io::FixedReader::new(&invalid);
        let decoded = SavedCursor::decode(&mut reader).unwrap();

        assert!(decoded.pen.default_style());
        assert!(decoded.flags.protected);
        assert!(!decoded.flags.pending_wrap);
        assert!(!decoded.flags.origin);
        assert_eq!(0u8, decoded.flags._padding);
        assert_eq!(None, decoded.charset.single_shift);
        assert_eq!(terminal_charsets::Slots::G0, decoded.charset.gr);
    }

    // test "saved cursor decoding rejects every truncation"
    #[test]
    fn saved_cursor_decoding_rejects_every_truncation() {
        let test_saved_cursor_fixture = test_saved_cursor_fixture();
        for fixture_len in 0..SavedCursor::LEN {
            let mut reader = io::FixedReader::new(&test_saved_cursor_fixture[0..fixture_len]);
            assert_eq!(
                Err(PayloadDecodeError::EndOfStream),
                SavedCursor::decode(&mut reader),
            );
        }
    }

    // test "cursor hyperlink null encoding and decoding"
    #[test]
    fn cursor_hyperlink_null_encoding_and_decoding() {
        let mut none_encoded = [0u8; 1];
        let buffered_len = {
            let mut none_writer = io::FixedWriter::new(&mut none_encoded);
            encode_cursor_hyperlink(None, &mut none_writer).unwrap();
            assert_eq!(b"\x00", none_writer.buffered());
            none_writer.end
        };

        let mut none_reader = io::FixedReader::new(&none_encoded[0..buffered_len]);
        assert!(decode_cursor_hyperlink(&mut none_reader).unwrap().is_none());
    }

    // test "framed native SCREEN and PAGE sequence"
    #[test]
    fn framed_native_screen_and_page_sequence() {
        let mut screen = TerminalScreen::init(Allocator, screen_options(8, 8, Some(0))).unwrap();
        // defer screen.deinit();

        // Configure the live cursor state represented by the fixed header.
        screen.cursor_absolute(7, 6);
        screen.cursor.cursor_style = CursorStyle::BlockHollow;
        screen.cursor.pending_wrap = true;
        screen.cursor.protected = false;
        screen.cursor.semantic_content = terminal_page::SemanticContent::Prompt;
        screen.cursor.semantic_content_clear_eol = true;
        screen.cursor.style = test_header().cursor_pen;
        screen.cursor.hyperlink_implicit_id = 0x0a0b0c0d;

        // Configure the screen-wide modes and packed charset state.
        let charset = test_charset_state();
        screen.charset = charset.clone();
        screen.protected_mode = terminal_ansi::ProtectedMode::Dec;
        screen.kitty_keyboard = terminal_kitty::KeyFlagStack::new(
            7,
            [
                terminal_kitty::KeyFlags {
                    disambiguate: true,
                    ..Default::default()
                },
                terminal_kitty::KeyFlags {
                    report_events: true,
                    ..Default::default()
                },
                terminal_kitty::KeyFlags {
                    report_alternates: true,
                    ..Default::default()
                },
                terminal_kitty::KeyFlags {
                    report_all: true,
                    ..Default::default()
                },
                terminal_kitty::KeyFlags {
                    report_associated: true,
                    ..Default::default()
                },
                terminal_kitty::KeyFlags {
                    disambiguate: true,
                    report_events: true,
                    report_alternates: true,
                    report_all: true,
                    report_associated: true,
                },
                terminal_kitty::KeyFlags::default(),
                terminal_kitty::KeyFlags {
                    disambiguate: true,
                    report_associated: true,
                    ..Default::default()
                },
            ],
        );
        screen.semantic_prompt = ScreenSemanticPrompt {
            seen: true,
            click: SemanticClick::Cl(terminal_osc::semantic_prompt::Click::SmartVertical),
        };

        // Configure the optional cursor state encoded after the fixed header.
        screen.saved_cursor = Some(crate::screen::SavedCursor {
            x: 0x0102,
            y: 0x0304,
            style: TerminalStyle::default(),
            protected: true,
            pending_wrap: true,
            origin: true,
            charset,
        });
        screen.start_hyperlink(b"cursor-uri", None).unwrap();

        let mut destination = io::AllocatingWriter::new();
        destination.write_all(b"prefix").unwrap();
        let mut stream = record::Writer::init(&mut destination);
        encode(&screen, TerminalScreenKey::Alternate, &mut stream).unwrap();

        assert_eq!(b"prefix", &stream.destination().written()[0..6]);

        // The complete sequence restores directly into native Screen/PageList
        // state, including page-local cursor references.
        let written = stream.destination().written().to_vec();
        stream.deinit();
        let mut restore_source = io::FixedReader::new(&written[6..]);
        let mut decoded = decode(
            &mut restore_source,
            Allocator,
            screen_options(8, 8, Some(0)),
        )
        .unwrap();
        // defer decoded.deinit();
        assert_eq!(TerminalScreenKey::Alternate, decoded.key);
        let restored = &mut decoded.screen;

        assert!(restored.no_scrollback);
        assert_eq!(7u16, restored.cursor.x);
        assert_eq!(6u16, restored.cursor.y);
        assert_eq!(CursorStyle::BlockHollow, restored.cursor.cursor_style);
        assert!(restored.cursor.pending_wrap);
        assert_eq!(
            terminal_page::SemanticContent::Prompt,
            restored.cursor.semantic_content,
        );
        assert!(restored.cursor.style.eql(&test_header().cursor_pen));
        assert!(restored.cursor.style_id != terminal_style::DEFAULT_ID);
        assert_eq!(0x0a0b0c0eu32, restored.cursor.hyperlink_implicit_id);
        // SAFETY: the restored cursor hyperlink is a live heap pointer owned
        // by the restored screen; shared read only.
        let restored_hyperlink = unsafe { &*restored.cursor.hyperlink.unwrap() };
        assert_eq!(b"cursor-uri", restored_hyperlink.uri.as_slice());
        match &restored_hyperlink.id {
            terminal_hyperlink::HyperlinkId::Implicit(id) => {
                assert_eq!(0x0a0b0c0du32, *id);
            }
            terminal_hyperlink::HyperlinkId::Explicit(_) => panic!("expected implicit id"),
        }

        let restored_saved = restored.saved_cursor.as_ref().unwrap();
        assert_eq!(7u16, restored_saved.x);
        assert_eq!(7u16, restored_saved.y);
        assert!(restored_saved.protected);
        assert!(restored_saved.pending_wrap);
        assert!(restored_saved.origin);
        assert_eq!(
            terminal_charsets::Charset::Ascii,
            restored.charset.charsets.get(terminal_charsets::Slots::G1),
        );
        assert_eq!(terminal_ansi::ProtectedMode::Dec, restored.protected_mode);
        assert_eq!(7u8, restored.kitty_keyboard.idx());
        assert!(restored.semantic_prompt.seen);
        assert_eq!(
            SemanticClick::Cl(terminal_osc::semantic_prompt::Click::SmartVertical),
            restored.semantic_prompt.click,
        );

        // Restored page-local cursor IDs are immediately usable by normal
        // writes.
        restored.test_write_string(b"Z").unwrap();
        let written_cell = restored
            .pages
            .get_cell(point::Point::Active(point::Coordinate { x: 0, y: 7 }))
            .unwrap();
        // SAFETY: `written_cell` points at live page memory of `restored`;
        // shared reads only.
        unsafe {
            assert_eq!('Z' as u32, (*written_cell.cell).codepoint());
            assert_eq!(restored.cursor.style_id, (*written_cell.cell).style_id());
            assert_eq!(
                restored.cursor.hyperlink_id,
                (*Node::page(written_cell.node))
                    .lookup_hyperlink(written_cell.cell)
                    .unwrap(),
            );
            assert_eq!(
                terminal_page::SemanticContent::Prompt,
                (*written_cell.cell).semantic_content(),
            );
        }
        assert_eq!(Err(io::ReadError::EndOfStream), restore_source.take_byte());

        decoded.deinit();
        screen.deinit();
    }

    // test "SCREEN encodes the minimal complete-page active suffix"
    #[test]
    fn screen_encodes_the_minimal_complete_page_active_suffix() {
        let page_rows = {
            let mut probe =
                TerminalScreen::init(Allocator, screen_options(80, 1, Some(0))).unwrap();
            // SAFETY: the top-left node is live within the probe screen.
            let rows =
                unsafe { Node::capacity(probe.pages.get_top_left(point::Tag::Screen).node).rows };
            probe.deinit();
            rows
        };
        let screen_rows = page_rows + 1;

        let mut screen =
            TerminalScreen::init(Allocator, screen_options(80, screen_rows, None)).unwrap();
        // defer screen.deinit();

        // Create one complete historical page followed by a mixed page whose
        // first row is history and whose remaining rows begin the active area.
        screen.cursor_absolute(0, screen_rows - 1);
        for _ in 0..screen_rows {
            screen.test_write_string(b"\n").unwrap();
        }
        assert_eq!(3usize, screen.pages.total_pages());

        let active_top = screen.pages.get_top_left(point::Tag::Active);
        let historical = screen.pages.get_top_left(point::Tag::Screen).node;
        // SAFETY: live nodes of this screen's PageList; shared reads only.
        let mixed = unsafe { (*historical).next };
        let newest = unsafe { (*mixed).next };
        assert_eq!(mixed, active_top.node);
        assert_eq!(1u16, active_top.y);
        assert!(unsafe { (*newest).next }.is_null());

        // Mark each native page so the encoded sequence proves both omission
        // and oldest-to-newest ordering. The mixed page's marker is in its
        // incidental history prefix and must survive because complete pages
        // are encoded.
        // SAFETY: live resident pages; exclusive writes to distinct cells.
        unsafe {
            *(*Node::page(historical)).get_row_and_cell(0, 0).cell =
                terminal_page::Cell::init('A' as u32);
            *(*Node::page(mixed)).get_row_and_cell(0, 0).cell =
                terminal_page::Cell::init('B' as u32);
            *(*Node::page(newest)).get_row_and_cell(0, 0).cell =
                terminal_page::Cell::init('C' as u32);
        }

        let mut destination = io::AllocatingWriter::new();
        let mut stream = record::Writer::init(&mut destination);
        encode(&screen, TerminalScreenKey::Primary, &mut stream).unwrap();

        let written = stream.destination().written().to_vec();
        stream.deinit();

        let mut source = io::FixedReader::new(&written);
        let mut screen_record = record::Reader::init(&mut source).unwrap();
        assert_eq!(record::Tag::Screen, screen_record.header.tag);

        let payload_reader = screen_record.payload_reader();
        let header = Header::decode(payload_reader).unwrap();
        assert_eq!(TerminalScreenKey::Primary, header.key);
        assert_eq!(2u16, header.page_count);
        assert_eq!(
            (screen.pages.total_rows - screen.pages.rows as usize) as u64,
            header.history_rows,
        );
        assert!(!header.saved_cursor_present);
        assert!(decode_cursor_hyperlink(payload_reader).unwrap().is_none());
        screen_record.finish().unwrap();

        let mut decoded_mixed = page::decode(&mut source).unwrap();
        // defer decoded_mixed.deinit();
        assert_eq!('B' as u32, unsafe {
            (*decoded_mixed.get_row_and_cell(0, 0).cell).codepoint()
        });

        let mut decoded_newest = page::decode(&mut source).unwrap();
        // defer decoded_newest.deinit();
        assert_eq!('C' as u32, unsafe {
            (*decoded_newest.get_row_and_cell(0, 0).cell).codepoint()
        });
        assert_eq!(Err(io::ReadError::EndOfStream), source.take_byte());
        decoded_newest.deinit();
        decoded_mixed.deinit();

        // Native restoration keeps the incidental history prefix and positions
        // the active top within the first decoded page.
        let mut restore_source = io::FixedReader::new(&written);
        let mut decoded = decode(
            &mut restore_source,
            Allocator,
            screen_options(80, screen_rows, None),
        )
        .unwrap();
        // defer decoded.deinit();
        assert_eq!(TerminalScreenKey::Primary, decoded.key);
        assert_eq!(header.history_rows, decoded.history_rows);
        let restored = &mut decoded.screen;

        assert_eq!(2usize, restored.pages.total_pages());
        let restored_top = restored.pages.get_top_left(point::Tag::Active);
        assert_eq!(1u16, restored_top.y);
        assert_eq!(
            restored_top.y as usize,
            restored.pages.total_rows - restored.pages.rows as usize,
        );
        assert!(
            decoded.history_rows
                > (restored.pages.total_rows - restored.pages.rows as usize) as u64
        );
        // SAFETY: live resident pages of the restored screen; shared reads.
        unsafe {
            assert_eq!(
                'B' as u32,
                (*(*Node::page(restored.pages.get_top_left(point::Tag::Screen).node))
                    .get_row_and_cell(0, 0)
                    .cell)
                    .codepoint(),
            );
            assert_eq!(
                'C' as u32,
                (*(*Node::page((*restored.pages.get_top_left(point::Tag::Screen).node).next))
                    .get_row_and_cell(0, 0)
                    .cell)
                    .codepoint(),
            );
        }

        // The restored PageList resumes ordinary scrolling and row growth.
        restored.test_write_string(b"\n").unwrap();
        restored.pages.assert_integrity();
        restored.assert_integrity();
        assert_eq!(Err(io::ReadError::EndOfStream), restore_source.take_byte());

        decoded.deinit();
        screen.deinit();
    }

    // test "SCREEN encoding preserves a compressed suffix page"
    #[test]
    fn screen_encoding_preserves_a_compressed_suffix_page() {
        use crate::compress as compression;

        let mut screen = TerminalScreen::init(Allocator, screen_options(8, 2, Some(0))).unwrap();
        // defer screen.deinit();
        let cell = screen
            .pages
            .get_cell(point::Point::Active(point::Coordinate { x: 0, y: 0 }))
            .unwrap();
        // SAFETY: `cell.cell` is live page memory of `screen`.
        unsafe {
            *cell.cell = terminal_page::Cell::init('A' as u32);
        }

        // Force the active suffix into the representation that current
        // PageList policy normally reserves for history. This models a future
        // policy change and makes pageAssumeResident an invalid tagged-union
        // access.
        let node = screen.pages.get_top_left(point::Tag::Active).node;
        // SAFETY: `node` is live and currently resident.
        let compressed = unsafe {
            let resident = Node::page_assume_resident(node);
            let scratch_len =
                compression::page::required_scratch((*resident).memory_len()).unwrap();
            let mut scratch = vec![0u8; scratch_len];
            let mut table: compression::lz4::HashTable = [0; compression::lz4::HASH_TABLE_LEN];
            compression::page::CompressedPage::init(&*resident, &mut scratch, &mut table)
                .unwrap()
                .unwrap()
        };
        // SAFETY: `node` is live and nothing borrows its storage here; this is
        // the reference `node.data = .{ .compressed = compressed }` (the
        // pagelist tests' established idiom).
        unsafe {
            (*node).data = crate::pagelist::NodeData::Compressed(compressed);
            assert_eq!(crate::pagelist::Storage::Compressed, Node::storage(node));
        }

        let mut destination = io::AllocatingWriter::new();
        let mut stream = record::Writer::init(&mut destination);
        encode(&screen, TerminalScreenKey::Primary, &mut stream).unwrap();

        // Encoding borrows or clones through PreservedPage and never changes
        // the source node's storage representation.
        // SAFETY: `node` is still live.
        assert_eq!(crate::pagelist::Storage::Compressed, unsafe {
            Node::storage(node)
        });

        let written = stream.destination().written().to_vec();
        stream.deinit();
        let mut source = io::FixedReader::new(&written);
        let mut decoded = decode(&mut source, Allocator, screen_options(8, 2, Some(0))).unwrap();
        // defer decoded.deinit();
        let restored_cell = decoded
            .screen
            .pages
            .get_cell(point::Point::Active(point::Coordinate { x: 0, y: 0 }))
            .unwrap();
        // SAFETY: live page memory of the restored screen; shared read.
        assert_eq!('A' as u32, unsafe { (*restored_cell.cell).codepoint() });

        decoded.deinit();
        screen.deinit();
    }

    // test "SCREEN restoration normalizes invalid cursor positions"
    #[test]
    fn screen_restoration_normalizes_invalid_cursor_positions() {
        let mut screen = TerminalScreen::init(Allocator, screen_options(2, 2, Some(0))).unwrap();
        // defer screen.deinit();

        struct Case {
            x: u16,
            y: u16,
            pending_wrap: bool,
            expected_x: u16,
            expected_y: u16,
            expected_pending_wrap: bool,
        }
        let cases = [
            // Out-of-range coordinates clamp to the bottom-right. Pending wrap
            // is valid there and can be retained.
            Case {
                x: u16::MAX,
                y: u16::MAX,
                pending_wrap: true,
                expected_x: 1,
                expected_y: 1,
                expected_pending_wrap: true,
            },
            // Pending wrap away from the final column would trip native
            // printing assertions, so retain the position and clear only that
            // flag.
            Case {
                x: 0,
                y: 0,
                pending_wrap: true,
                expected_x: 0,
                expected_y: 0,
                expected_pending_wrap: false,
            },
        ];

        for case in cases {
            let mut destination = io::AllocatingWriter::new();
            let mut stream = record::Writer::init(&mut destination);

            let mut header = Header::init(&screen, TerminalScreenKey::Primary, 1);
            header.cursor_x = case.x;
            header.cursor_y = case.y;
            header.cursor_flags.pending_wrap = case.pending_wrap;
            header.saved_cursor_present = false;

            {
                let screen_payload = stream.begin(record::Tag::Screen);
                // errdefer stream.cancel();
                header.encode(screen_payload).unwrap();
                screen_payload.write_byte(0).unwrap();
                stream.finish().unwrap();
            }

            // SAFETY: the active node is live and resident; encode only reads.
            let native_page = unsafe {
                &*Node::page_assume_resident(screen.pages.get_top_left(point::Tag::Active).node)
            };
            page::encode(native_page, &mut stream).unwrap();

            let written = stream.destination().written().to_vec();
            stream.deinit();
            let mut source = io::FixedReader::new(&written);
            let mut decoded =
                decode(&mut source, Allocator, screen_options(2, 2, Some(0))).unwrap();
            // defer decoded.deinit();

            assert_eq!(case.expected_x, decoded.screen.cursor.x);
            assert_eq!(case.expected_y, decoded.screen.cursor.y);
            assert_eq!(
                case.expected_pending_wrap,
                decoded.screen.cursor.pending_wrap
            );
            decoded.screen.assert_integrity();
            decoded.deinit();
        }

        screen.deinit();
    }

    // test "SCREEN validates pending wrap against a mixed-width cursor page"
    #[test]
    fn screen_validates_pending_wrap_against_a_mixed_width_cursor_page() {
        let mut screen = TerminalScreen::init(Allocator, screen_options(8, 1, Some(0))).unwrap();
        // defer screen.deinit();

        // Model a lazily reflowed active page that is narrower than the
        // terminal. Column three is its physical final column even though it
        // is not column seven of the current terminal dimensions.
        let mut narrow_page =
            terminal_page::Page::init(terminal_page::Capacity::new(4, 1)).unwrap();
        // defer narrow_page.deinit();

        let mut destination = io::AllocatingWriter::new();
        let mut stream = record::Writer::init(&mut destination);

        let mut header = Header::init(&screen, TerminalScreenKey::Primary, 1);
        header.cursor_x = 3;
        header.cursor_y = 0;
        header.cursor_flags.pending_wrap = true;
        header.saved_cursor_present = false;

        {
            let screen_payload = stream.begin(record::Tag::Screen);
            // errdefer stream.cancel();
            header.encode(screen_payload).unwrap();
            screen_payload.write_byte(0).unwrap();
            stream.finish().unwrap();
        }
        page::encode(&narrow_page, &mut stream).unwrap();

        let written = stream.destination().written().to_vec();
        stream.deinit();
        let mut source = io::FixedReader::new(&written);
        let mut decoded = decode(&mut source, Allocator, screen_options(8, 1, Some(0))).unwrap();
        // defer decoded.deinit();

        // SAFETY: the cursor pin's node is live within the decoded screen.
        assert_eq!(4u16, unsafe {
            Node::cols((*decoded.screen.cursor.page_pin).node)
        });
        assert_eq!(3u16, decoded.screen.cursor.x);
        assert!(decoded.screen.cursor.pending_wrap);
        decoded.screen.assert_integrity();

        decoded.deinit();
        narrow_page.deinit();
        screen.deinit();
    }

    // test "SCREEN clamps a decoded saved cursor to terminal dimensions"
    #[test]
    fn screen_clamps_a_decoded_saved_cursor_to_terminal_dimensions() {
        let mut screen = TerminalScreen::init(Allocator, screen_options(2, 2, Some(0))).unwrap();
        // defer screen.deinit();

        let mut destination = io::AllocatingWriter::new();
        let mut stream = record::Writer::init(&mut destination);

        let mut header = Header::init(&screen, TerminalScreenKey::Primary, 1);
        header.saved_cursor_present = true;
        {
            let screen_payload = stream.begin(record::Tag::Screen);
            header.encode(screen_payload).unwrap();
            SavedCursor {
                x: u16::MAX,
                y: u16::MAX,
                pen: TerminalStyle::default(),
                flags: SavedCursorFlags {
                    pending_wrap: true,
                    ..SavedCursorFlags::default()
                },
                charset: CharsetState::default(),
            }
            .encode(screen_payload)
            .unwrap();
            screen_payload.write_byte(0).unwrap();
            stream.finish().unwrap();
        }
        // SAFETY: the active node is live and resident; encode only reads.
        let native_page = unsafe {
            &*Node::page_assume_resident(screen.pages.get_top_left(point::Tag::Active).node)
        };
        page::encode(native_page, &mut stream).unwrap();

        let written = stream.destination().written().to_vec();
        stream.deinit();
        let mut source = io::FixedReader::new(&written);
        let mut decoded = decode(&mut source, Allocator, screen_options(2, 2, Some(0))).unwrap();
        // defer decoded.deinit();

        let saved = decoded.screen.saved_cursor.as_ref().unwrap();
        assert_eq!(1u16, saved.x);
        assert_eq!(1u16, saved.y);
        assert!(saved.pending_wrap);

        decoded.deinit();
        screen.deinit();
    }

    // test "SCREEN restoration rejects invalid and incomplete sequences"
    #[test]
    fn screen_restoration_rejects_invalid_and_incomplete_sequences() {
        let mut screen = TerminalScreen::init(Allocator, screen_options(2, 2, Some(0))).unwrap();
        // defer screen.deinit();

        let mut destination = io::AllocatingWriter::new();
        let mut stream = record::Writer::init(&mut destination);
        encode(&screen, TerminalScreenKey::Primary, &mut stream).unwrap();
        let written = stream.destination().written().to_vec();
        stream.deinit();

        // Every truncation must fail without leaking a partially restored
        // PageList, cursor pin, or cursor-owned state.
        for fixture_len in 0..written.len() {
            let mut source = io::FixedReader::new(&written[0..fixture_len]);
            if let Ok(mut restored) = decode(&mut source, Allocator, screen_options(2, 2, Some(0)))
            {
                restored.deinit();
                panic!("truncated sequence decoded");
            }
        }

        // Native PageList construction is transactional on allocation failure.
        //
        // PORT NOTE: `FailingAllocator{ .fail_index = 0 }` → the pagelist
        // Builder's finish tripwire (`FINISH_TW`, the crate's established
        // failure-injection seam for native PageList construction).
        {
            crate::pagelist::builder::FINISH_TW.with(|tw| {
                tw.borrow_mut().errorAlways(
                    crate::pagelist::builder::FinishFp::ViewportPinTrack,
                    crate::pagelist::OutOfMemory,
                )
            });
            let mut source = io::FixedReader::new(&written);
            assert_eq!(
                Err(DecodeError::OutOfMemory),
                decode(&mut source, Allocator, screen_options(2, 2, Some(0)))
                    .map(|mut d| d.deinit()),
            );
            crate::pagelist::builder::FINISH_TW
                .with(|tw| tw.borrow_mut().end(rustty_util::tripwire::ResetMode::Reset))
                .unwrap();
        }

        // A syntactically valid SCREEN cannot declare an empty PAGE sequence.
        let mut empty_sequence = io::AllocatingWriter::new();
        let mut empty_stream = record::Writer::init(&mut empty_sequence);
        {
            let payload = empty_stream.begin(record::Tag::Screen);
            // errdefer empty_stream.cancel();
            Header::init(&screen, TerminalScreenKey::Primary, 0)
                .encode(payload)
                .unwrap();
            payload.write_byte(0).unwrap();
            empty_stream.finish().unwrap();
        }
        let empty_written = empty_stream.destination().written().to_vec();
        empty_stream.deinit();
        let mut empty_source = io::FixedReader::new(&empty_written);
        assert_eq!(
            Err(DecodeError::InvalidPageCount),
            decode(&mut empty_source, Allocator, screen_options(2, 2, Some(0)),)
                .map(|mut d| d.deinit()),
        );

        screen.deinit();
    }

    // test "SCREEN sequence failure preserves preceding bytes"
    #[test]
    fn screen_sequence_failure_preserves_preceding_bytes() {
        let mut screen = TerminalScreen::init(Allocator, screen_options(2, 2, Some(0))).unwrap();
        // defer screen.deinit();

        // PORT NOTE: the reference gives the record.Writer a
        // `FailingAllocator{ .fail_index = 0 }` so the first record's scratch
        // buffering fails with WriteFailed before anything reaches the
        // destination. The port's scratch is an infallible Vec; the same
        // observable failure — WriteFailed with the destination still exactly
        // "prefix" — comes from a destination with no room past the prefix.
        let mut destination_bytes = *b"prefix";
        let mut destination = io::FixedWriter::new(&mut destination_bytes);
        destination.end = 6; // The prefix is already written.
        let mut stream = record::Writer::init(&mut destination);

        assert_eq!(
            Err(EncodeError::WriteFailed),
            encode(&screen, TerminalScreenKey::Primary, &mut stream),
        );
        assert_eq!(b"prefix", stream.destination().buffered());
        stream.deinit();

        screen.deinit();
    }

    // test "SCREEN decode ignores an invalid cursor hyperlink"
    #[test]
    fn screen_decode_ignores_an_invalid_cursor_hyperlink() {
        let mut screen = TerminalScreen::init(Allocator, screen_options(1, 1, Some(0))).unwrap();
        // defer screen.deinit();

        let mut destination = io::AllocatingWriter::new();
        let mut stream = record::Writer::init(&mut destination);

        let mut header = test_header();
        header.key = TerminalScreenKey::Primary;
        header.page_count = 1;
        header.cursor_x = 0;
        header.cursor_y = 0;
        header.cursor_flags.pending_wrap = false;
        header.saved_cursor_present = false;

        {
            let screen_payload = stream.begin(record::Tag::Screen);
            // errdefer stream.cancel();
            header.encode(screen_payload).unwrap();
            screen_payload.write_byte(1).unwrap(); // Implicit hyperlink.
            io::write_int::<u32>(screen_payload, 1).unwrap();
            io::write_int::<u32>(screen_payload, 0).unwrap(); // Empty URI.
            stream.finish().unwrap();
        }

        // SAFETY: the active node is live and resident; encode only reads.
        let native_page = unsafe {
            &*Node::page_assume_resident(screen.pages.get_top_left(point::Tag::Active).node)
        };
        page::encode(native_page, &mut stream).unwrap();

        let written = stream.destination().written().to_vec();
        stream.deinit();
        let mut source = io::FixedReader::new(&written);
        let mut decoded = decode(&mut source, Allocator, screen_options(1, 1, Some(0))).unwrap();
        // defer decoded.deinit();
        assert!(decoded.screen.cursor.hyperlink.is_none());

        decoded.deinit();
        screen.deinit();
    }

    // test "SCREEN decode ignores a PAGE with an empty hyperlink URI"
    #[test]
    fn screen_decode_ignores_a_page_with_an_empty_hyperlink_uri() {
        let mut destination = io::AllocatingWriter::new();
        let mut stream = record::Writer::init(&mut destination);

        let mut header = test_header();
        header.key = TerminalScreenKey::Primary;
        header.page_count = 1;
        header.cursor_x = 0;
        header.cursor_y = 0;
        header.cursor_flags.pending_wrap = false;
        header.saved_cursor_present = false;

        {
            let screen_payload = stream.begin(record::Tag::Screen);
            // errdefer stream.cancel();
            header.encode(screen_payload).unwrap();
            screen_payload.write_byte(0).unwrap();
            stream.finish().unwrap();
        }

        let page_header = page::Header {
            columns: 1,
            rows: 1,
            style_count: 0,
            hyperlink_count: 1,
            style_capacity: 16,
            hyperlink_capacity_bytes: 512,
            grapheme_capacity_bytes: 0,
            string_capacity_bytes: 0,
        };
        {
            let page_payload = stream.begin(record::Tag::Page);
            // errdefer stream.cancel();
            page_header.encode(page_payload).unwrap();
            io::write_int::<u16>(page_payload, 1).unwrap(); // terminal_hyperlink.Id
            page_payload.write_byte(1).unwrap(); // Implicit hyperlink.
            io::write_int::<u32>(page_payload, 1).unwrap();
            io::write_int::<u32>(page_payload, 0).unwrap(); // Empty URI.

            // One narrow codepoint cell refers to the hyperlink table entry
            // above. Since that entry is ignored, the cell must restore
            // without a hyperlink.
            page_payload.write_byte(0x30).unwrap(); // row flags, full cell width
            io::write_int::<u16>(page_payload, 1).unwrap(); // cell count
            io::write_int::<u64>(
                page_payload,
                grid::Cell {
                    content: 'A' as u32,
                    hyperlink: true,
                    hyperlink_id: 1,
                    ..grid::Cell::default()
                }
                .bits(),
            )
            .unwrap();
            io::write_int::<u32>(page_payload, 0).unwrap(); // grapheme section
            stream.finish().unwrap();
        }

        let written = stream.destination().written().to_vec();
        stream.deinit();
        let mut source = io::FixedReader::new(&written);
        let mut decoded = decode(&mut source, Allocator, screen_options(1, 1, Some(0))).unwrap();
        // defer decoded.deinit();

        // SAFETY: the cursor's page cell is live page memory of the decoded
        // screen; shared reads only.
        unsafe {
            assert_eq!('A' as u32, (*decoded.screen.cursor.page_cell).codepoint());
            assert!(!(*decoded.screen.cursor.page_cell).hyperlink());
        }

        // Resizing copies the restored cells into a wider page. The ignored
        // hyperlink must not leave a zero-length PageEntry to duplicate later.
        decoded
            .screen
            .resize(crate::screen::Resize {
                cols: 2,
                rows: 1,
                reflow: true,
                prompt_redraw: terminal_osc::semantic_prompt::Redraw::False,
            })
            .unwrap();
        // SAFETY: as above (the resize refreshed the cursor pointers).
        assert!(!unsafe { (*decoded.screen.cursor.page_cell).hyperlink() });

        decoded.deinit();
    }
}
