//! Grid (rows, cells, and grapheme suffixes) encoding.
//!
//! A grid contains the rows and cells of a terminal page. Its dimensions are
//! supplied by the containing record rather than repeated here. The encoder
//! writes exactly `rows` row records followed by one grapheme suffix section.
//!
//! All records are tightly packed with no padding between them. All integers
//! are unsigned and little-endian.
//!
//! The layout is designed so the common case decodes with bulk copies: cells
//! are fixed-size words, trailing default cells are elided per row, and the
//! variable-length grapheme suffixes live outside the row data.
//!
//! ```text
//! +---------------------------+
//! | Row 0                     |
//! +---------------------------+
//! | ...                       |
//! +---------------------------+
//! | Row (rows - 1)            |
//! +---------------------------+
//! | Grapheme suffix section   |
//! +---------------------------+
//! ```
//!
//! ## Row
//!
//! Each row has the following format:
//!
//! | Offset | Size              | Field                      |
//! | -----: | ----------------: | :------------------------- |
//! |      0 |                 1 | Row flags                  |
//! |      1 |                 2 | Encoded cell count (`u16`) |
//! |      3 | `width` * `count` | Encoded cells              |
//!
//! The row flag byte has the following format:
//!
//! | Bits | Field                     |
//! | ---: | :------------------------ |
//! |    0 | Wrap                      |
//! |    1 | Wrap continuation         |
//! |  2-3 | Semantic prompt           |
//! |  4-5 | Encoded cell width        |
//! |  6-7 | Reserved, zero            |
//!
//! Semantic prompt values are:
//!
//! | Value | Meaning             |
//! | ----: | :------------------ |
//! |     0 | None                |
//! |     1 | Prompt              |
//! |     2 | Prompt continuation |
//!
//! Value 3 is not emitted in snapshot version 1. Decoders treat it as none.
//!
//! The encoded cell count must not exceed the grid's column count; it is a
//! structural field and decoders reject larger values. Cells at and beyond
//! the count are the default cell: all sixty-four bits zero, meaning an
//! empty narrow codepoint cell with the default style and no hyperlink.
//! Canonical encoders emit exactly through the row's last non-default cell,
//! so a fully default row has a zero count and no cell words.
//!
//! ## Encoded cell width
//!
//! The two width bits select how many bytes encode each of the row's
//! cells: `1 << width` is the size, so zero through three select one, two,
//! four, or eight bytes. Narrower widths are truncated transports of the
//! same cell word: a cell qualifies for a width when all of its higher
//! word bits are zero.
//!
//! | Width | Bytes | Encoded value and admitted cells                     |
//! | ----: | ----: | :--------------------------------------------------- |
//! |     0 |     1 | Codepoint at or below U+00FF; all other bits zero    |
//! |     1 |     2 | Codepoint at or below U+FFFF; all other bits zero    |
//! |     2 |     4 | Word bits 0-31: any content kind and codepoint, style |
//! |       |       | IDs 1-63, narrow, no flags, no hyperlink             |
//! |     3 |     8 | The complete word                                    |
//!
//! Widths zero and one store the codepoint value itself, which is word
//! bits 2-25 shifted down; the reconstructed word is the codepoint shifted
//! left by two. Width two stores the word's low half unshifted. This makes
//! every width a zero-extension on decode and a truncation on encode.
//!
//! Canonical encoders choose each row's smallest admissible width, which
//! follows directly from the bitwise OR of the row's cell words. Decoders
//! use the declared width for framing and accept rows encoded wider than
//! necessary. The width of a row with a zero cell count is canonically
//! zero and carries no meaning.
//!
//! Native row cache flags are not encoded. In particular, the Kitty virtual
//! placeholder hint is derived while decoding cells containing U+10EEEE.
//!
//! ## Cell
//!
//! Each cell is one 64-bit little-endian word, transported at the row's
//! encoded cell width as described above:
//!
//! ```text
//!  bit 0 +-------------------------------+
//!        | Content kind                  |
//!        | 2 bits                        |
//!  bit 2 +-------------------------------+
//!        | Content                       |
//!        | 24 bits                       |
//! bit 26 +-------------------------------+
//!        | Style ID                      |
//!        | 16 bits                       |
//! bit 42 +-------------------------------+
//!        | Width kind                    |
//!        | 2 bits                        |
//! bit 44 +-------------------------------+
//!        | Protected                     |
//! bit 45 +-------------------------------+
//!        | Hyperlink flag                |
//! bit 46 +-------------------------------+
//!        | Semantic content              |
//!        | 2 bits                        |
//! bit 48 +-------------------------------+
//!        | Hyperlink ID                  |
//!        | 16 bits                       |
//! bit 64 +-------------------------------+
//! ```
//!
//! Content kinds are:
//!
//! | Value | Meaning                          |
//! | ----: | :------------------------------- |
//! |     0 | Codepoint                        |
//! |     1 | Codepoint with grapheme suffix   |
//! |     2 | Palette background               |
//! |     3 | RGB background                   |
//!
//! The content kind selects the layout of the 24-bit content field. All
//! content bit positions below are relative to the start of the content
//! field, so content bit 0 is cell word bit 2.
//!
//! For codepoint content (kinds 0 and 1), the content field is a Unicode
//! scalar value. Values above U+10FFFF and surrogates decode as U+FFFD.
//! Kind 1 additionally declares that the grapheme suffix section contains
//! one entry for this cell; the cell is otherwise identical to kind 0.
//!
//! ```text
//!  bit 0 +-------------------------------+
//!        | Unicode scalar value          |
//!        | 24 bits                       |
//! bit 24 +-------------------------------+
//! ```
//!
//! For a palette background (kind 2), the low byte is the palette index.
//! The remaining content bits are reserved, canonically zero, and ignored
//! by decoders.
//!
//! ```text
//!  bit 0 +-------------------------------+
//!        | Palette index                 |
//!        | 8 bits                        |
//!  bit 8 +-------------------------------+
//!        | Reserved, zero                |
//!        | 16 bits                       |
//! bit 24 +-------------------------------+
//! ```
//!
//! For an RGB background (kind 3), the content field is one byte per
//! channel:
//!
//! ```text
//!  bit 0 +-------------------------------+
//!        | Red                           |
//!        | 8 bits                        |
//!  bit 8 +-------------------------------+
//!        | Green                         |
//!        | 8 bits                        |
//! bit 16 +-------------------------------+
//!        | Blue                          |
//!        | 8 bits                        |
//! bit 24 +-------------------------------+
//! ```
//!
//! Width kinds are:
//!
//! | Value | Meaning     |
//! | ----: | :---------- |
//! |     0 | Narrow      |
//! |     1 | Wide        |
//! |     2 | Spacer tail |
//! |     3 | Spacer head |
//!
//! Semantic content values are:
//!
//! | Value | Meaning |
//! | ----: | :------ |
//! |     0 | Output  |
//! |     1 | Input   |
//! |     2 | Prompt  |
//!
//! Value 3 is not emitted in snapshot version 1. Decoders treat it as output.
//!
//! Style and hyperlink ID zero mean no style and no hyperlink. Other IDs
//! refer to entries in the containing record's separate style and hyperlink
//! tables. The hyperlink flag is set exactly when the hyperlink ID is
//! nonzero; decoders derive cell linkage from the remapped ID and ignore the
//! flag itself.
//!
//! ## Grapheme suffix section
//!
//! The section begins with an entry count followed by that many entries:
//!
//! | Offset | Size    | Field                |
//! | -----: | ------: | :------------------- |
//! |      0 |       4 | Entry count (`u32`)  |
//! |      4 | varies  | Entries              |
//!
//! Each entry:
//!
//! | Offset | Size        | Field                       |
//! | -----: | ----------: | :-------------------------- |
//! |      0 |           2 | Row (`u16`)                 |
//! |      2 |           2 | Column (`u16`)              |
//! |      4 |           2 | Codepoint count (`u16`)     |
//! |      6 | 4 * `count` | Codepoints (`u32` each)     |
//!
//! Each codepoint is a Unicode scalar; invalid scalars are individually
//! ignored by decoders. Canonical encoders emit exactly one entry, with at
//! least one codepoint, for every kind 1 cell, in ascending row-then-column
//! order. Decoders consume every declared entry and ignore entries whose
//! target is out of range, is not a codepoint cell, has a zero codepoint, or
//! already received an entry. A kind 1 cell that receives no suffix
//! codepoints decodes as a plain codepoint cell.
//!
//! PORTS: ghostty src/terminal/snapshot/grid.zig
//!
//! PORT NOTE (vector paths): the reference vectorizes the bulk codec with Zig
//! `@Vector` shuffles (encode 973f619a2, decode 2aaad3ca9). The port keeps the
//! same window structure — four-cell encode steps, sixteen-transported-byte
//! widen steps, overlapping final windows — as scalar loops over `u64` cell
//! words obtained through `Cell::to_bits`/`from_bits` on owned page storage
//! (never `*Cell`-to-`*u64` reinterpretation; see the crate raw-pointer
//! discipline), which LLVM can auto-vectorize. Byte-for-byte behavior,
//! including the overlapping-store tails, is identical.

// const std = @import("std");
// const assert = std.debug.assert;
// const builtin = @import("builtin");
// const Allocator = std.mem.Allocator;
// const test_fixture = @import("fixture.zig");
// const io = @import("io.zig");
// const terminal_hyperlink = @import("../hyperlink.zig");
// const terminal_page = @import("../page.zig");
// const terminal_style = @import("../style.zig");
use super::io;
use crate::hyperlink as terminal_hyperlink;
use crate::page as terminal_page;
#[cfg(test)]
use crate::style as terminal_style;

/// The Kitty virtual-placement placeholder remains wire-relevant even when
/// runtime Kitty graphics support is compiled out.
const KITTY_VIRTUAL_PLACEHOLDER: u32 = 0x10EEEE;

use terminal_hyperlink::Id as TerminalHyperlinkId;
use terminal_page::Cell as TerminalCell;
use terminal_page::Page as TerminalPage;
use terminal_page::Row as TerminalRow;
#[cfg(test)]
use terminal_style::Id as TerminalStyleId;

/// The header before every row's encoded cells.
///
/// The semantic prompt is a raw integer for the same reason as the wire
/// cell fields: decoders must accept its reserved value without
/// instantiating an invalid native enum. The width enum is exhaustive over
/// its two bits, so every header byte bit-casts to a valid value.
// PORT NOTE: Zig `packed struct(u8)`; the port keeps explicit fields plus
// `bits`/`from_bits` honoring the exact bit layout (the snapshot/style.rs
// `Flags` idiom).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Row {
    pub wrap: bool,
    pub wrap_continuation: bool,
    /// `semantic_prompt: u2`
    pub semantic_prompt: u8,
    pub cell_width: EncodedWidth,
    /// `_padding: u2`
    pub _padding: u8,
}

impl Default for Row {
    fn default() -> Row {
        Row {
            wrap: false,
            wrap_continuation: false,
            semantic_prompt: 0,
            cell_width: EncodedWidth::One,
            _padding: 0,
        }
    }
}

impl Row {
    /// `@bitCast(row)` to the packed `u8`.
    pub fn bits(self) -> u8 {
        (self.wrap as u8)
            | (self.wrap_continuation as u8) << 1
            | (self.semantic_prompt & 0x3) << 2
            | (self.cell_width as u8) << 4
            | (self._padding & 0x3) << 6
    }

    /// `@bitCast(raw)` from the packed `u8`. Every bit pattern is valid.
    pub fn from_bits(raw: u8) -> Row {
        Row {
            wrap: raw & (1 << 0) != 0,
            wrap_continuation: raw & (1 << 1) != 0,
            semantic_prompt: (raw >> 2) & 0x3,
            cell_width: EncodedWidth::from_u2(((raw >> 4) & 0x3) as u64),
            _padding: (raw >> 6) & 0x3,
        }
    }
}

/// The wire layout of one encoded cell. This is its own registry: the bit
/// positions and field meanings are part of the snapshot format and are
/// documented above independently of the native cell.
///
/// Enum-like fields are raw integers because decoders must accept reserved
/// values (for example semantic content 3) without instantiating an invalid
/// native enum.
// PORT NOTE: Zig `packed struct(u64)`; explicit fields plus `bits`/`from_bits`
// (see `Row`). The nested `Kind`/`EncodedWidth` types are hoisted to module
// scope (Rust nested type names, the crate-wide treatment).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Cell {
    /// `kind: u2` — determines how `content` is interpreted (`Kind`).
    pub kind: u8,
    /// `content: u24`
    pub content: u32,
    pub style_id: u16,
    /// `width: u2`
    pub width: u8,
    pub protected: bool,
    pub hyperlink: bool,
    /// `semantic_content: u2`
    pub semantic_content: u8,
    pub hyperlink_id: u16,
}

// The wire bit positions (Zig: implicit in the packed struct; pinned by the
// "cell wire layout registry" test).
const CELL_KIND_SHIFT: u32 = 0;
const CELL_CONTENT_SHIFT: u32 = 2;
const CELL_STYLE_ID_SHIFT: u32 = 26;
const CELL_WIDTH_SHIFT: u32 = 42;
const CELL_PROTECTED_SHIFT: u32 = 44;
const CELL_HYPERLINK_SHIFT: u32 = 45;
const CELL_SEMANTIC_CONTENT_SHIFT: u32 = 46;
const CELL_HYPERLINK_ID_SHIFT: u32 = 48;

impl Cell {
    /// `@as(u64, @bitCast(cell))`
    pub fn bits(self) -> u64 {
        ((self.kind as u64) & 0x3) << CELL_KIND_SHIFT
            | ((self.content as u64) & 0xFF_FFFF) << CELL_CONTENT_SHIFT
            | (self.style_id as u64) << CELL_STYLE_ID_SHIFT
            | ((self.width as u64) & 0x3) << CELL_WIDTH_SHIFT
            | (self.protected as u64) << CELL_PROTECTED_SHIFT
            | (self.hyperlink as u64) << CELL_HYPERLINK_SHIFT
            | ((self.semantic_content as u64) & 0x3) << CELL_SEMANTIC_CONTENT_SHIFT
            | (self.hyperlink_id as u64) << CELL_HYPERLINK_ID_SHIFT
    }

    /// `@as(Cell, @bitCast(word))`. Any word is a valid wire cell because its
    /// fields are raw integers.
    pub fn from_bits(word: u64) -> Cell {
        Cell {
            kind: ((word >> CELL_KIND_SHIFT) & 0x3) as u8,
            content: ((word >> CELL_CONTENT_SHIFT) & 0xFF_FFFF) as u32,
            style_id: ((word >> CELL_STYLE_ID_SHIFT) & 0xFFFF) as u16,
            width: ((word >> CELL_WIDTH_SHIFT) & 0x3) as u8,
            protected: (word >> CELL_PROTECTED_SHIFT) & 1 != 0,
            hyperlink: (word >> CELL_HYPERLINK_SHIFT) & 1 != 0,
            semantic_content: ((word >> CELL_SEMANTIC_CONTENT_SHIFT) & 0x3) as u8,
            hyperlink_id: ((word >> CELL_HYPERLINK_ID_SHIFT) & 0xFFFF) as u16,
        }
    }
}

/// Determines how `content` is interpreted. (Zig: `Cell.Kind`.)
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Codepoint = 0,
    CodepointGrapheme = 1,
    BgColorPalette = 2,
    BgColorRgb = 3,
}

impl Kind {
    /// `@as(Cell.Kind, @enumFromInt(wire.kind))` — exhaustive over two bits.
    fn from_u2(v: u8) -> Kind {
        match v & 0x3 {
            0 => Kind::Codepoint,
            1 => Kind::CodepointGrapheme,
            2 => Kind::BgColorPalette,
            3 => Kind::BgColorRgb,
            _ => unreachable!(),
        }
    }
}

/// The encoded cell width declared by a row header: how many bytes
/// transport each of the row's cell words. See the format
/// documentation above for the value each width transports and the
/// cells it admits.
///
/// `truncate` and `extend` are the transport transform itself, so the
/// admission masks derive from them rather than being maintained by
/// hand: a cell word is admitted exactly when it round-trips.
///
/// (Zig: `Cell.EncodedWidth`; hoisted, see the `Cell` PORT NOTE. The
/// reference's comptime `Int(self)` transport type becomes the low
/// `size()` bytes of a `u64` value.)
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EncodedWidth {
    One = 0,
    Two = 1,
    Four = 2,
    Eight = 3,
}

impl EncodedWidth {
    /// The number of bytes transporting one cell word.
    pub fn size(self) -> usize {
        1usize << (self as usize)
    }

    /// `@as(EncodedWidth, @enumFromInt(bits))` — exhaustive over two bits.
    fn from_u2(v: u64) -> EncodedWidth {
        match v & 0x3 {
            0 => EncodedWidth::One,
            1 => EncodedWidth::Two,
            2 => EncodedWidth::Four,
            3 => EncodedWidth::Eight,
            _ => unreachable!(),
        }
    }

    /// Truncate one cell word to its transported value. Only words
    /// admitted by this width round-trip; `select` proves that for
    /// every cell in a row before an encoder may use it.
    pub fn truncate(self, word: u64) -> u64 {
        match self {
            // The bare codepoint, shifted down from the content field.
            EncodedWidth::One => (Cell::from_bits(word).content as u64) & 0xFF,
            EncodedWidth::Two => (Cell::from_bits(word).content as u64) & 0xFFFF,

            // The word itself.
            EncodedWidth::Four => word & 0xFFFF_FFFF,
            EncodedWidth::Eight => word,
        }
    }

    /// Widen one transported value back to its cell word.
    pub fn extend(self, value: u64) -> u64 {
        match self {
            EncodedWidth::One | EncodedWidth::Two => Cell {
                content: value as u32,
                ..Cell::default()
            }
            .bits(),
            EncodedWidth::Four | EncodedWidth::Eight => value,
        }
    }

    /// The word bits a cell may use and still round-trip through this
    /// width.
    pub fn mask(self) -> u64 {
        self.extend(match self {
            EncodedWidth::One => u8::MAX as u64,
            EncodedWidth::Two => u16::MAX as u64,
            EncodedWidth::Four => u32::MAX as u64,
            EncodedWidth::Eight => u64::MAX,
        })
    }

    /// The smallest width admitting the word, typically the bitwise
    /// OR of every cell word in a row.
    pub fn select(word: u64) -> EncodedWidth {
        for width in [EncodedWidth::One, EncodedWidth::Two, EncodedWidth::Four] {
            if word & !width.mask() == 0 {
                return width;
            }
        }
        EncodedWidth::Eight
    }
}

/// Whether the native cell's in-memory layout matches the wire cell layout
/// bit for bit, with the wire hyperlink ID occupying the native padding.
///
/// The wire format does not require this: it is an optimization. When it
/// holds, whole rows encode and decode as bulk copies. If the native layout
/// ever diverges, the portable field-by-field codec below remains correct
/// and this constant simply becomes false.
///
/// PORT NOTE: the reference computes this at comptime from three
/// bit-pattern probes pairing native and wire cells. The native accessors
/// are not `const fn`, so the port pins the constant and carries the exact
/// probes in the `cell_wire_layout_registry` test (the crate's
/// comptime-assert-to-test idiom; see `record::Header::LEN`). If a native
/// layout change fails that test, flip this to false and the portable codec
/// takes over automatically.
const NATIVE_MATCHES_WIRE: bool = true;

/// Whether rows of cells can be copied between native and wire storage
/// without per-cell transformation.
const BULK_CODEC: bool = NATIVE_MATCHES_WIRE && cfg!(target_endian = "little");

/// pub const EncodeError = std.Io.Writer.Error || error{...};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EncodeError {
    // std.Io.Writer.Error
    WriteFailed,

    /// Wide and spacer cells do not form a valid row.
    InvalidWideCell,

    /// One cell's grapheme suffix exceeds the entry's u16 codepoint count.
    TooManyGraphemes,
}

impl From<io::WriteFailed> for EncodeError {
    fn from(_: io::WriteFailed) -> EncodeError {
        EncodeError::WriteFailed
    }
}

/// Encode every row, cell, and grapheme suffix directly from a page.
///
/// If an error is returned, partial data may have been written. If you
/// want transactional writing, the caller is responsible for using something
/// like a seekable stream and rolling back.
pub fn encode(page: &TerminalPage, writer: &mut dyn io::Writer) -> Result<(), EncodeError> {
    // defer page.assertIntegrity();
    let result = encode_inner(page, writer);
    page.assert_integrity();
    result
}

/// The bulk row emit chunk: header plus encoded cells are assembled here and
/// emitted with one `write_all` per row.
const EMIT_CHUNK: usize = 4096;

fn encode_inner(page: &TerminalPage, writer: &mut dyn io::Writer) -> Result<(), EncodeError> {
    // PERF(port): the bulk row chunk is hoisted out of the row loop so its
    // zero-fill happens once per grid, not once per row (the reference has no
    // buffer at all here — it writes into the writer's spare capacity, which
    // is never zeroed). Declared-in-loop, the 4 KiB memset per ~100-byte row
    // was ~36% of encode time.
    let mut out = [0u8; EMIT_CHUNK];
    for y in 0..page.size.rows as usize {
        let row_ptr = page.get_row(y);
        let cells_ptr = page.get_cells(row_ptr);
        // SAFETY: `row_ptr`/`cells_ptr` point into the live page mapping;
        // encoding only reads the page (`&self` methods), so the shared
        // borrows below never conflict with a mutation.
        let row: &TerminalRow = unsafe { &*row_ptr };
        let cells: &[TerminalCell] = unsafe { &*cells_ptr };

        // Trailing default cells decode implicitly: wide/spacer pairs and
        // hyperlinked or styled cells are always nonzero, so eliding the
        // zero suffix never drops encoded state. The scan also accumulates
        // the OR of the row's cell words to select its encoded cell width.
        let (count, word_or) = scan_row(cells);

        // Validate the wide state of every encoded cell so we don't encode
        // corrupt data. The width bits of the OR word witness whether any
        // encoded cell is non-narrow at all; rows without them, the common
        // case, satisfy every pair rule vacuously. Trailing default cells
        // are narrow, so checking the encoded prefix against the full row
        // width covers every pair.
        let wide_mask: u64 = Cell {
            width: 3,
            ..Cell::default()
        }
        .bits();
        if word_or & wide_mask != 0 {
            for (x, cell) in cells[0..count].iter().enumerate() {
                match cell.wide() {
                    terminal_page::Wide::Narrow => {}
                    terminal_page::Wide::Wide => {
                        if x + 1 == cells.len()
                            || cells[x + 1].wide() != terminal_page::Wide::SpacerTail
                        {
                            return Err(EncodeError::InvalidWideCell);
                        }
                    }
                    terminal_page::Wide::SpacerTail => {
                        if x == 0 || cells[x - 1].wide() != terminal_page::Wide::Wide {
                            return Err(EncodeError::InvalidWideCell);
                        }
                    }
                    terminal_page::Wide::SpacerHead => {
                        if x + 1 != cells.len() || !row.wrap() {
                            return Err(EncodeError::InvalidWideCell);
                        }
                    }
                }
            }
        }

        // Canonical rows use the smallest admissible width.
        let cell_width = EncodedWidth::select(word_or);

        let row_header = Row {
            wrap: row.wrap(),
            wrap_continuation: row.wrap_continuation(),
            semantic_prompt: row.semantic_prompt() as u8,
            cell_width,
            _padding: 0,
        };

        // The bulk codec emits the row header and its encoded cells directly
        // into the destination's spare buffer capacity, so a row costs no
        // writer call at all instead of one for the header and one per cell
        // chunk.
        //
        // PORT NOTE: the reference writes into the Zig writer's
        // `unusedCapacitySlice`; the `io::Writer` stand-in has no
        // spare-capacity API, so the port assembles the row in a stack chunk
        // and emits it with one `write_all` (same one-call-per-row property,
        // identical bytes). Rows larger than the chunk take the generic path
        // below, mirroring the reference's insufficient-capacity bail.
        if BULK_CODEC {
            match cell_width {
                EncodedWidth::One | EncodedWidth::Two | EncodedWidth::Four => {
                    let size = cell_width.size();
                    let needed = 3 + count * size;
                    if needed <= EMIT_CHUNK {
                        out[0] = row_header.bits();
                        out[1..3].copy_from_slice(&(count as u16).to_le_bytes());
                        encode_narrow_into(cell_width, cells, count, &mut out[3..needed]);
                        writer.write_all(&out[..needed])?;
                        continue;
                    }
                }
                EncodedWidth::Eight => {
                    // Hyperlink IDs live in a native side table, but we
                    // embed them in ours, so if we have any hyperlinks we
                    // need to fall back to the loop below.
                    let witness = Cell::from_bits(word_or);
                    if !witness.hyperlink && witness.hyperlink_id == 0 {
                        let needed = 3 + count * 8;
                        if needed <= EMIT_CHUNK {
                            out[0] = row_header.bits();
                            out[1..3].copy_from_slice(&(count as u16).to_le_bytes());
                            for (i, cell) in cells[0..count].iter().enumerate() {
                                out[3 + i * 8..3 + i * 8 + 8]
                                    .copy_from_slice(&cell.to_bits().to_le_bytes());
                            }
                            writer.write_all(&out[..needed])?;
                            continue;
                        }
                    }
                }
            }
        }

        // Row header: flags then the encoded cell count.
        {
            let mut header_bytes = [0u8; 3];
            header_bytes[0] = row_header.bits();
            header_bytes[1..3].copy_from_slice(&(count as u16).to_le_bytes());
            writer.write_all(&header_bytes)?;
        }

        match cell_width {
            EncodedWidth::One | EncodedWidth::Two | EncodedWidth::Four => {
                encode_narrow_cells(cell_width, &cells[0..count], writer)?;
            }

            EncodedWidth::Eight => {
                // Hyperlink IDs live in a native side table, but we embed
                // them in ours, so if we have any hyperlinks we need
                // to fallback to the loop below.
                if BULK_CODEC {
                    let witness = Cell::from_bits(word_or);
                    if !witness.hyperlink && witness.hyperlink_id == 0 {
                        // (Zig: `writeAll(sliceAsBytes(cells[0..count]))` —
                        // the port streams the words in bounded chunks
                        // through the stand-in writer.)
                        let mut chunk = [0u8; 1024];
                        let mut i: usize = 0;
                        while i < count {
                            let n = (count - i).min(chunk.len() / 8);
                            for (j, cell) in cells[i..i + n].iter().enumerate() {
                                chunk[j * 8..j * 8 + 8]
                                    .copy_from_slice(&cell.to_bits().to_le_bytes());
                            }
                            writer.write_all(&chunk[..n * 8])?;
                            i += n;
                        }
                        continue;
                    }
                }

                for cell in &cells[0..count] {
                    let link_id: TerminalHyperlinkId = if cell.hyperlink() {
                        page.lookup_hyperlink(cell).unwrap()
                    } else {
                        0
                    };
                    io::write_int::<u64>(writer, cell_bits(*cell, link_id))?;
                }
            }
        }
    }

    encode_graphemes(page, writer)
}

/// The encoded cell count (through the last nonzero cell) and the bitwise
/// OR of every encoded cell word for one row.
fn scan_row(cells: &[TerminalCell]) -> (usize, u64) {
    if BULK_CODEC {
        // Count the zero cells using vectorized instructions
        // (PORT NOTE: four-word windows; see the module vector-paths note).
        let mut count = cells.len();
        while count >= 4 {
            let tail_or = cells[count - 4].to_bits()
                | cells[count - 3].to_bits()
                | cells[count - 2].to_bits()
                | cells[count - 1].to_bits();
            if tail_or != 0 {
                break;
            }
            count -= 4;
        }
        while count > 0 && cells[count - 1].to_bits() == 0 {
            count -= 1;
        }

        // Accumulate the OR of the classification vectorized, with the
        // scalar tail continuing from where the vector loop stopped.
        let word_or: u64 = {
            let mut acc = [0u64; 4];
            let mut i: usize = 0;
            while i + 4 <= count {
                acc[0] |= cells[i].to_bits();
                acc[1] |= cells[i + 1].to_bits();
                acc[2] |= cells[i + 2].to_bits();
                acc[3] |= cells[i + 3].to_bits();
                i += 4;
            }
            let mut word_or = acc[0] | acc[1] | acc[2] | acc[3];
            while i < count {
                word_or |= cells[i].to_bits();
                i += 1;
            }
            word_or
        };

        return (count, word_or);
    }

    // Scalar path, count backwards
    let mut count: usize = cells.len();
    while count > 0 && cells[count - 1].is_zero() {
        count -= 1;
    }
    let mut word_or: u64 = 0;
    for cell in &cells[0..count] {
        word_or |= classify_word(cell);
    }
    (count, word_or)
}

/// The word used to select a row's encoded cell width. This is the cell's
/// wire word with the hyperlink flag reflecting the native cell, so linked
/// cells and nonzero native padding disqualify every narrow width.
#[inline]
fn classify_word(cell: &TerminalCell) -> u64 {
    if NATIVE_MATCHES_WIRE {
        return cell.to_bits();
    }

    let mut wire = Cell::from_bits(cell_bits(*cell, 0));
    wire.hyperlink = cell.hyperlink();
    wire.bits()
}

// PORT NOTE / PERF(port): the reference passes the encoded width as a
// COMPTIME parameter (`inline .one, .two, .four => |width|` at the row
// dispatch, `comptime width` on every helper), monomorphizing the cell
// loops so each per-cell store is a fixed-size write. The const-generic
// `SIZE` on the `_sized` helpers below is that comptime parameter's Rust
// spelling; the runtime-`width` wrappers keep the reference's call shape.
// A single runtime-`width` instantiation made every store a dynamic-length
// `copy_from_slice` — one libc `memcpy` call PER CELL, measured as ~4x on
// the encode benchmark.

/// `width.truncate(word)` with the width as a compile-time constant.
#[inline(always)]
fn truncate_sized<const SIZE: usize>(word: u64) -> u64 {
    match SIZE {
        // The bare codepoint, shifted down from the content field.
        1 => (Cell::from_bits(word).content as u64) & 0xFF,
        2 => (Cell::from_bits(word).content as u64) & 0xFFFF,
        // The word itself.
        4 => word & 0xFFFF_FFFF,
        _ => word,
    }
}

/// Write one row's cells truncated to the given encoded width.
///
/// Every admitted cell round-trips exactly because the row's width
/// selection proved every cell word survives `truncate` then `extend`.
fn encode_narrow_cells(
    width: EncodedWidth,
    cells: &[TerminalCell],
    writer: &mut dyn io::Writer,
) -> Result<(), io::WriteFailed> {
    match width {
        EncodedWidth::One => encode_narrow_cells_sized::<1>(cells, writer),
        EncodedWidth::Two => encode_narrow_cells_sized::<2>(cells, writer),
        EncodedWidth::Four => encode_narrow_cells_sized::<4>(cells, writer),
        EncodedWidth::Eight => encode_narrow_cells_sized::<8>(cells, writer),
    }
}

fn encode_narrow_cells_sized<const SIZE: usize>(
    cells: &[TerminalCell],
    writer: &mut dyn io::Writer,
) -> Result<(), io::WriteFailed> {
    let mut chunk = [0u8; 1024];
    let mut i: usize = 0;
    while i < cells.len() {
        let n = (cells.len() - i).min(chunk.len() / SIZE);
        for (j, cell) in cells[i..i + n].iter().enumerate() {
            let value = truncate_sized::<SIZE>(classify_word(cell));
            chunk[j * SIZE..j * SIZE + SIZE].copy_from_slice(&value.to_le_bytes()[..SIZE]);
        }
        writer.write_all(&chunk[..n * SIZE])?;
        i += n;
    }
    Ok(())
}

/// Truncate one row's cell words into `out` at the given encoded width.
fn encode_narrow_into(width: EncodedWidth, cells: &[TerminalCell], count: usize, out: &mut [u8]) {
    match width {
        EncodedWidth::One => encode_narrow_into_sized::<1>(cells, count, out),
        EncodedWidth::Two => encode_narrow_into_sized::<2>(cells, count, out),
        EncodedWidth::Four => encode_narrow_into_sized::<4>(cells, count, out),
        EncodedWidth::Eight => encode_narrow_into_sized::<8>(cells, count, out),
    }
}

fn encode_narrow_into_sized<const SIZE: usize>(
    cells: &[TerminalCell],
    count: usize,
    out: &mut [u8],
) {
    assert!(BULK_CODEC);

    let mut j: usize = 0;
    while j + 4 <= count {
        encode_narrow_step::<SIZE>(cells, j, out);
        j += 4;
    }
    if j < count {
        if count >= 4 {
            // Reprocess the final full window with overlapping stores,
            // which rewrite the same truncated values.
            encode_narrow_step::<SIZE>(cells, count - 4, out);
        } else {
            while j < count {
                let value = truncate_sized::<SIZE>(cells[j].to_bits());
                out[j * SIZE..j * SIZE + SIZE].copy_from_slice(&value.to_le_bytes()[..SIZE]);
                j += 1;
            }
        }
    }
}

/// Emit four truncated cell words starting at cell index `j`.
/// (PORT NOTE: the reference's two-lane shift + byte shuffle; the packed
/// integer below is that shuffle's scalar spelling — the four truncated
/// values land in one `4 * SIZE`-byte store instead of four dynamic-length
/// copies. See the module vector-paths note.)
#[inline]
fn encode_narrow_step<const SIZE: usize>(cells: &[TerminalCell], j: usize, out: &mut [u8]) {
    if SIZE == 8 {
        for k in 0..4 {
            let value = truncate_sized::<SIZE>(cells[j + k].to_bits());
            out[(j + k) * 8..(j + k) * 8 + 8].copy_from_slice(&value.to_le_bytes());
        }
        return;
    }
    let mut packed: u128 = 0;
    for k in 0..4 {
        let value = truncate_sized::<SIZE>(cells[j + k].to_bits());
        packed |= (value as u128) << (8 * SIZE * k);
    }
    out[j * SIZE..j * SIZE + SIZE * 4].copy_from_slice(&packed.to_le_bytes()[..SIZE * 4]);
}

/// Encode the grapheme suffix section for every kind 1 cell in the grid.
fn encode_graphemes(page: &TerminalPage, writer: &mut dyn io::Writer) -> Result<(), EncodeError> {
    // Every grapheme cell owns exactly one entry in the page's grapheme
    // map, so the section header comes straight from the page without a
    // counting pass over the grid. Rows without the native grapheme hint
    // contain no grapheme cells in any intact page.
    let entries = page.grapheme_count();
    io::write_int::<u32>(writer, entries as u32)?;
    if entries == 0 {
        return Ok(());
    }

    // Entries are batched into a local buffer so hot pages perform one
    // writer call per flush instead of several per entry.
    let mut buffer = [0u8; 4096];
    let mut used: usize = 0;
    let mut emitted: usize = 0;
    for y in 0..page.size.rows as usize {
        let row_ptr = page.get_row(y);
        // SAFETY: live page row; encoding only reads (see `encode_inner`).
        let row: &TerminalRow = unsafe { &*row_ptr };
        if !row.grapheme() {
            continue;
        }
        // SAFETY: as above.
        let cells: &[TerminalCell] = unsafe { &*page.get_cells(row_ptr) };
        for (x, cell) in cells.iter().enumerate() {
            if !cell.has_grapheme() {
                continue;
            }
            let cps = page.lookup_grapheme(cell).unwrap();
            if cps.len() > u16::MAX as usize {
                return Err(EncodeError::TooManyGraphemes);
            }
            emitted += 1;

            let needed = 6 + cps.len() * 4;
            if buffer.len() - used < needed {
                writer.write_all(&buffer[0..used])?;
                used = 0;
            }
            if needed > buffer.len() {
                // An entry larger than the whole buffer streams directly.
                io::write_int::<u16>(writer, y as u16)?;
                io::write_int::<u16>(writer, x as u16)?;
                io::write_int::<u16>(writer, cps.len() as u16)?;
                for cp in cps {
                    io::write_int::<u32>(writer, *cp)?;
                }
                continue;
            }

            buffer[used..used + 2].copy_from_slice(&(y as u16).to_le_bytes());
            buffer[used + 2..used + 4].copy_from_slice(&(x as u16).to_le_bytes());
            buffer[used + 4..used + 6].copy_from_slice(&(cps.len() as u16).to_le_bytes());
            used += 6;
            for cp in cps {
                buffer[used..used + 4].copy_from_slice(&cp.to_le_bytes());
                used += 4;
            }
        }
    }
    writer.write_all(&buffer[0..used])?;

    // The declared count is trusted by decoders for framing, so the grid
    // must have produced exactly that many entries.
    assert!(emitted == entries);
    Ok(())
}

/// pub const DecodeError = std.Io.Reader.Error || error{...};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecodeError {
    // std.Io.Reader.Error
    ReadFailed,
    EndOfStream,

    /// A row declares more encoded cells than the grid has columns.
    InvalidRowCellCount,
}

impl From<io::ReadError> for DecodeError {
    fn from(err: io::ReadError) -> DecodeError {
        match err {
            io::ReadError::ReadFailed => DecodeError::ReadFailed,
            io::ReadError::EndOfStream => DecodeError::EndOfStream,
        }
    }
}

/// Maps encoded style table IDs to page-assigned style IDs.
pub type StyleRemap = Remap;

/// Maps encoded hyperlink table IDs to page-assigned hyperlink IDs.
pub type HyperlinkRemap = Remap;

/// Decode every row, cell, and grapheme suffix directly into an
/// initialized, empty page.
///
/// The grid does not encode dimensions, so `page` must already have the
/// exact row and column count expected by the containing record. Capacity
/// hints are advisory. Graphemes and cell hyperlink references that do not
/// fit are discarded without affecting the rest of the grid.
///
/// Style and hyperlink table entries must be inserted into `page` before
/// calling this function, with their encoded and page-assigned IDs recorded
/// in `style_remap` and `hyperlink_remap`. ID zero always means the default
/// style or no hyperlink. A nonzero ID missing from its remap is also
/// treated as zero so unknown table references do not prevent the rest of
/// the grid from decoding.
///
/// Invalid semantic data is normalized into a degraded form while
/// preserving the declared byte boundaries. Unknown semantic values use
/// their neutral variants, invalid Unicode becomes U+FFFD, invalid optional
/// data is ignored, and malformed wide-cell relationships become narrow
/// cells. The encoded cell count is the only structural field.
pub fn decode(
    page: &mut TerminalPage,
    reader: &mut dyn io::Reader,
    style_remap: &StyleRemap,
    hyperlink_remap: &HyperlinkRemap,
) -> Result<(), DecodeError> {
    for y in 0..page.size.rows as usize {
        // Read the row header and cell count.
        let (row_header, count): (Row, u16) = {
            // The staged payload path has every header buffered.
            let mut row_header_bytes = [0u8; 3];
            if reader.buffered_len() >= 3 {
                row_header_bytes.copy_from_slice(&reader.buffered()[0..3]);
                reader.toss(3);
            } else {
                reader.read_slice_all(&mut row_header_bytes)?;
            }

            // Every bit pattern is a valid header: booleans and the exhaustive
            // width enum decode directly, and the raw semantic value gets a
            // default below. Reserved bits do not change the known fields.
            let row_header = Row::from_bits(row_header_bytes[0]);
            let count = u16::from_le_bytes(row_header_bytes[1..3].try_into().unwrap());
            (row_header, count)
        };

        // A fully default row needs no work at all: decoded pages start
        // zeroed, which is exactly the default row and cell state.
        if row_header.bits() == 0 && count == 0 {
            continue;
        }

        // Update the row fields through one load and store instead of a
        // read-modify-write per packed field.
        let row = page.get_row(y);
        // SAFETY: `row` points at the live page row; the load/modify/store
        // below is the only access until the next page call.
        unsafe {
            let mut row_value = *row;
            row_value.set_wrap(row_header.wrap);
            row_value.set_wrap_continuation(row_header.wrap_continuation);
            row_value.set_semantic_prompt(match row_header.semantic_prompt {
                0 => terminal_page::SemanticPrompt::None,
                1 => terminal_page::SemanticPrompt::Prompt,
                2 => terminal_page::SemanticPrompt::PromptContinuation,
                // std.enums.fromInt(...) orelse .none
                _ => terminal_page::SemanticPrompt::None,
            });
            *row = row_value;
        }

        let cells = page.get_cells(row);
        let cells_len = cells.len();
        if count as usize > cells_len {
            return Err(DecodeError::InvalidRowCellCount);
        }
        if count == 0 {
            continue;
        }
        let count = count as usize;

        // The encoded cell width is framing: it determines exactly how many
        // bytes this row occupies.
        match row_header.cell_width {
            width @ (EncodedWidth::One | EncodedWidth::Two) => {
                // SAFETY: the narrow decoder touches only these cells and the
                // reader — no interleaved page access — so the short-lived
                // exclusive slice is sound.
                let cell_slice: &mut [TerminalCell] =
                    unsafe { core::slice::from_raw_parts_mut(cells as *mut TerminalCell, count) };
                decode_narrow_cells(width, reader, cell_slice)?;
            }
            EncodedWidth::Four => {
                let _ = decode_word_cells(
                    EncodedWidth::Four,
                    page,
                    row,
                    cells,
                    count,
                    reader,
                    style_remap,
                    hyperlink_remap,
                )?;
            }
            EncodedWidth::Eight => {
                if BULK_CODEC {
                    // Read the encoded words directly into page storage,
                    // then normalize them in place. The raw words are only
                    // ever touched as integers until normalization makes
                    // them valid cells.
                    {
                        // SAFETY: `TerminalCell` is `repr(transparent)` over
                        // `u64` (no padding); on this little-endian bulk-codec
                        // target the wire bytes are the storage bytes. The
                        // byte view is dropped before any page call.
                        let bytes: &mut [u8] = unsafe {
                            core::slice::from_raw_parts_mut(
                                cells as *mut TerminalCell as *mut u8,
                                count * 8,
                            )
                        };
                        reader.read_slice_all(bytes)?;
                    }
                    let mut row_or: u64 = 0;
                    for x in 0..count {
                        // SAFETY: in-bounds raw read of the just-filled word.
                        let word = unsafe { (*(cells as *mut TerminalCell).add(x)).to_bits() };
                        row_or |= word;
                        apply_cell(page, row, cells, x, word, style_remap, hyperlink_remap);
                    }
                    normalize_wide_row(EncodedWidth::Eight, row, cells, count, row_or);
                } else {
                    let _ = decode_word_cells(
                        EncodedWidth::Eight,
                        page,
                        row,
                        cells,
                        count,
                        reader,
                        style_remap,
                        hyperlink_remap,
                    )?;
                }

                // The implicit cell after a short row is narrow, which
                // resolves a trailing wide marker exactly like an explicit
                // narrow neighbor. Only full-width rows can encode wide
                // markers.
                // SAFETY: in-bounds raw access; no other alias is live.
                unsafe {
                    let last = (cells as *mut TerminalCell).add(count - 1);
                    if count < cells_len && (*last).wide() == terminal_page::Wide::Wide {
                        (*last).set_wide(terminal_page::Wide::Narrow);
                    }
                }
            }
        }
    }

    decode_graphemes(page, reader)
}

/// Decode one row of width-one or width-two cells.
///
/// These widths admit only bare codepoints, so cells store directly with at
/// most Unicode scalar validation: no styles, hyperlinks, wide pairs, or
/// row hints are reachable and no normalization state is needed.
fn decode_narrow_cells(
    width: EncodedWidth,
    reader: &mut dyn io::Reader,
    cells: &mut [TerminalCell],
) -> Result<(), DecodeError> {
    let size = width.size();

    // The staged payload path has the complete row buffered, making this
    // one bounds check followed by a vectorizable widening loop.
    let total = cells.len() * size;
    if reader.buffered_len() >= total {
        // (The borrow of the reader's buffer ends before `toss`.)
        widen_cells(width, &reader.buffered()[0..total], cells);
        reader.toss(total);
        return Ok(());
    }

    // Streaming sources fall back to bounded chunks.
    let mut chunk = [0u8; 1024];
    let mut i: usize = 0;
    while i < cells.len() {
        let n = (cells.len() - i).min(chunk.len() / size);
        reader.read_slice_all(&mut chunk[0..n * size])?;
        widen_cells(width, &chunk[0..n * size], &mut cells[i..i + n]);
        i += n;
    }
    Ok(())
}

/// Store codepoint-valued encoded cells of the given width.
//
// PORT NOTE / PERF(port): the width is the reference's comptime parameter;
// see the note above `truncate_sized`. The `_sized` split monomorphizes the
// widening loops per width.
fn widen_cells(width: EncodedWidth, bytes: &[u8], cells: &mut [TerminalCell]) {
    match width {
        EncodedWidth::One => widen_cells_sized::<1>(bytes, cells),
        EncodedWidth::Two => widen_cells_sized::<2>(bytes, cells),
        EncodedWidth::Four => widen_cells_sized::<4>(bytes, cells),
        EncodedWidth::Eight => widen_cells_sized::<8>(bytes, cells),
    }
}

/// `width.extend(value)` with the width as a compile-time constant.
#[inline(always)]
fn extend_sized<const SIZE: usize>(value: u64) -> u64 {
    match SIZE {
        1 | 2 => Cell {
            content: value as u32,
            ..Cell::default()
        }
        .bits(),
        _ => value,
    }
}

fn widen_cells_sized<const SIZE: usize>(bytes: &[u8], cells: &mut [TerminalCell]) {
    // With the bulk codec layout this is a pure widening store: sixteen
    // transported bytes per step, widening each encoded value into u64 word
    // position and shifting it into the content field. (PORT NOTE: the
    // reference shuffles against a zero vector; see the module vector-paths
    // note. Zig 0.16 disables loop auto-vectorization, so its scalar loop
    // would issue one widening store per cell.)
    if BULK_CODEC {
        let step = 16 / SIZE;
        let mut i: usize = 0;
        while i + step <= cells.len() {
            widen_step::<SIZE>(bytes, cells, i);
            i += step;
        }
        if i < cells.len() {
            if cells.len() >= step {
                // Reprocess the final full window with overlapping stores,
                // which rewrite the same widened values.
                let start = cells.len() - step;
                widen_step::<SIZE>(bytes, cells, start);
            } else {
                while i < cells.len() {
                    cells[i] =
                        TerminalCell::from_bits(extend_sized::<SIZE>(widen_value_sized::<SIZE>(
                            &bytes[i * SIZE..],
                        )));
                    i += 1;
                }
            }
        }
        return;
    }

    // When the native cell matches the wire word, this is a pure widening
    // loop over integers.
    if NATIVE_MATCHES_WIRE {
        for (i, cell) in cells.iter_mut().enumerate() {
            *cell = TerminalCell::from_bits(extend_sized::<SIZE>(widen_value_sized::<SIZE>(
                &bytes[i * SIZE..],
            )));
        }
        return;
    }

    for (i, cell) in cells.iter_mut().enumerate() {
        store_cell_ref(
            cell,
            extend_sized::<SIZE>(widen_value_sized::<SIZE>(&bytes[i * SIZE..])),
        );
    }
}

/// Widen sixteen transported bytes into their cell words at cell index `i`.
/// Width two first degrades surrogate lanes to U+FFFD, matching
/// `widen_value`. (PORT NOTE: scalar form of the reference's vector step;
/// see the module vector-paths note.)
// PERF(port): plain `#[inline]` lost to the cost model (a real symbol + a
// `bl` per 16 cells in `widen_cells`, zero vectorization); forcing it lets
// LLVM auto-vectorize the widening loop — the same treatment that already
// gives the encode side 32 cells per iteration.
#[inline(always)]
fn widen_step<const SIZE: usize>(bytes: &[u8], cells: &mut [TerminalCell], i: usize) {
    let step = 16 / SIZE;
    for k in 0..step {
        // Width two admits surrogates, which degrade to U+FFFD exactly
        // like `widen_value`. Width one cannot encode an invalid scalar.
        let value = widen_value_sized::<SIZE>(&bytes[(i + k) * SIZE..]);
        cells[i + k] = TerminalCell::from_bits(extend_sized::<SIZE>(value));
    }
}

/// Read and validate one narrow transported value, with the width as a
/// compile-time constant.
#[inline(always)]
fn widen_value_sized<const SIZE: usize>(bytes: &[u8]) -> u64 {
    let value: u64 = match SIZE {
        1 => bytes[0] as u64,
        2 => u16::from_le_bytes(bytes[0..2].try_into().unwrap()) as u64,
        4 => u32::from_le_bytes(bytes[0..4].try_into().unwrap()) as u64,
        _ => u64::from_le_bytes(bytes[0..8].try_into().unwrap()),
    };

    // Width one cannot encode an invalid scalar. Width two admits
    // surrogates, which degrade exactly like their full-width form.
    // PERF(port): a value-select rather than an early `return` — the
    // reference does this branchlessly with `@select` on a vector; the
    // control-flow form blocked vectorization of the widening loop.
    if SIZE != 1 && !valid_scalar(value as u32) {
        0xFFFD
    } else {
        value
    }
}

/// Decode one row of width-four or fallback full-width cells through the
/// complete per-cell normalization path.
///
/// Returns the bitwise OR of every decoded wire word so callers can gate
/// the trailing wide-pair resolution pass without a second scan.
#[allow(clippy::too_many_arguments)]
fn decode_word_cells(
    width: EncodedWidth,
    page: &mut TerminalPage,
    row: *mut TerminalRow,
    cells: *mut [TerminalCell],
    count: usize,
    reader: &mut dyn io::Reader,
    style_remap: &StyleRemap,
    hyperlink_remap: &HyperlinkRemap,
) -> Result<u64, DecodeError> {
    let size = width.size();
    let mut row_or: u64 = 0;

    // The staged payload path has the complete row buffered.
    let total = count * size;
    if reader.buffered_len() >= total {
        // (The reader's buffer borrow and the page mutations in `apply_cell`
        // touch disjoint objects; the borrow ends before `toss`.)
        let bytes = &reader.buffered()[0..total];
        for x in 0..count {
            let bits = width.extend(widen_raw(width, &bytes[x * size..]));
            row_or |= bits;
            apply_cell(page, row, cells, x, bits, style_remap, hyperlink_remap);
        }
        reader.toss(total);
        normalize_wide_row(width, row, cells, count, row_or);
        return Ok(row_or);
    }

    // Streaming sources fall back to per-cell reads.
    for x in 0..count {
        let bits = width.extend(read_transported(width, reader)?);
        row_or |= bits;
        apply_cell(page, row, cells, x, bits, style_remap, hyperlink_remap);
    }
    normalize_wide_row(width, row, cells, count, row_or);
    Ok(row_or)
}

/// `std.mem.readInt(width.Int(), bytes, .little)` — the raw transported
/// value without narrow-scalar validation (widths four and eight).
#[inline]
fn widen_raw(width: EncodedWidth, bytes: &[u8]) -> u64 {
    match width {
        EncodedWidth::One => bytes[0] as u64,
        EncodedWidth::Two => u16::from_le_bytes(bytes[0..2].try_into().unwrap()) as u64,
        EncodedWidth::Four => u32::from_le_bytes(bytes[0..4].try_into().unwrap()) as u64,
        EncodedWidth::Eight => u64::from_le_bytes(bytes[0..8].try_into().unwrap()),
    }
}

/// `io.readInt(reader, width.Int())` for the streaming fallback.
#[inline]
fn read_transported(
    width: EncodedWidth,
    reader: &mut dyn io::Reader,
) -> Result<u64, io::ReadError> {
    Ok(match width {
        EncodedWidth::One => reader.take_byte()? as u64,
        EncodedWidth::Two => io::read_int::<u16>(reader)? as u64,
        EncodedWidth::Four => io::read_int::<u32>(reader)? as u64,
        EncodedWidth::Eight => io::read_int::<u64>(reader)?,
    })
}

/// Resolve wide-pair relationships for one decoded row.
///
/// Cell decoding stores every cell unresolved, so this pass applies
/// `normalize_wide` in order, which is equivalent to interleaving it with
/// the stores. Rows whose word OR carries no wide bits are already
/// normalized: every cell is narrow. Width four and narrower transports
/// cannot encode wide bits at all, so those rows skip the check entirely.
#[inline]
fn normalize_wide_row(
    width: EncodedWidth,
    row: *const TerminalRow,
    cells: *mut [TerminalCell],
    count: usize,
    row_or: u64,
) {
    if width != EncodedWidth::Eight {
        return;
    }
    let wide_mask: u64 = Cell {
        width: 3,
        ..Cell::default()
    }
    .bits();
    if row_or & wide_mask == 0 {
        return;
    }
    for x in 0..count {
        normalize_wide(row, cells, x);
    }
}

/// Whether the value is a valid Unicode scalar value.
#[inline]
fn valid_scalar(cp: u32) -> bool {
    cp <= 0x10FFFF && !(0xD800..=0xDFFF).contains(&cp)
}

/// Normalize one encoded cell word and store it at `cells[x]`.
///
/// This owns every per-cell decode rule except grapheme suffixes and
/// wide-pair resolution: content validation, reserved-value degradation,
/// and style and hyperlink remapping with reference counting. Callers run
/// `normalize_wide_row` over the stored row afterward.
fn apply_cell(
    page: &mut TerminalPage,
    row: *mut TerminalRow,
    cells: *mut [TerminalCell],
    x: usize,
    bits_wire: u64,
    style_remap: &StyleRemap,
    hyperlink_remap: &HyperlinkRemap,
) {
    // SAFETY: `x` is in bounds of the row's cells; the pointer is used for
    // narrow raw accesses only (never a long-lived reference across the page
    // calls below).
    let cell: *mut TerminalCell = unsafe { (cells as *mut TerminalCell).add(x) };

    // The default cell is the common case and needs no normalization,
    // reference counting, or table lookups.
    if bits_wire == 0 {
        store_cell(cell, 0);
        return;
    }

    // Any word is a valid wire cell because its fields are raw integers,
    // so all normalization below is plain field access.
    let mut wire = Cell::from_bits(bits_wire);

    // Hyperlink linkage is derived from the remapped ID below. The stored
    // native cell starts unlinked either way.
    let link_encoded = wire.hyperlink_id;
    wire.hyperlink_id = 0;
    wire.hyperlink = false;

    match Kind::from_u2(wire.kind) {
        // Kind 1 differs from 0 only by declaring a grapheme suffix section
        // entry, which reattaches through the native grapheme APIs later.
        Kind::Codepoint | Kind::CodepointGrapheme => {
            wire.kind = Kind::Codepoint as u8;
            if !valid_scalar(wire.content) {
                wire.content = 0xFFFD;
            }

            // Kitty image and placement state is not part of this snapshot
            // version, but the placeholder is still a valid Unicode scalar.
            // Preserve it and derive the native row hint so later row
            // operations remain correct.
            if wire.content == KITTY_VIRTUAL_PLACEHOLDER {
                // SAFETY: live page row; narrow raw access.
                unsafe { (*row).set_kitty_virtual_placeholder(true) };
            }
        }

        // Only the palette index is meaningful; the remaining content bits
        // are reserved and must not obscure it.
        Kind::BgColorPalette => wire.content = wire.content as u8 as u32,

        Kind::BgColorRgb => {}
    }

    // Reserved semantic content degrades to plain output.
    wire.semantic_content = match wire.semantic_content {
        // std.enums.fromInt(SemanticContent, ...) orelse .output
        0 | 1 | 2 => wire.semantic_content,
        _ => terminal_page::SemanticContent::Output as u8,
    };

    // IDs belong to the encoded page. Translate them to IDs assigned by the
    // destination page before storing them on cells. The table owns one
    // reference and each decoded cell owns one additional reference.
    if wire.style_id != 0 {
        wire.style_id = style_remap.get(wire.style_id);

        // PORT NOTE: reference defect (R13, UPSTREAM-DEFECTS.md UD65).
        // `grid.zig:1259-1265` calls `page.styles.use` once per styled cell,
        // and that is an unguarded `ref += 1` on a `u16`
        // (`ref_counted_set.zig:365-379`). A decoded PAGE declares its own
        // `columns` x `rows`, so a 256x256 page whose cells all carry one
        // style overflows it: a ReleaseSafe panic, and in ReleaseFast a wrap
        // to zero that makes a live style read as dead; the very next `use`
        // then breaks `use`'s own `assert(ref > 0)` (undefined behaviour),
        // before the post-grid release loop runs.
        //
        // This is NOT decoder-only. A terminal wide enough to leave the
        // standard capacity gets `std_capacity` with only `cols` changed
        // (`PageList.zig:535-540`): 215 rows x cols, over ten million cells.
        // In Ghostty that is 47,932 columns or more in a shipped macOS arm64
        // build, 46,908 on Linux aarch64 and 48,047 in a macOS test build
        // (it depends on the platform's page size); the port's thresholds
        // are the same since its style set is laid out with the reference's
        // item geometry (N88). Ordinary styled output on such a terminal overflows the
        // same refcount through `printSliceFill` and `printCell` (measured,
        // UPSTREAM-DEFECTS.md UD80). The port's set now saturates
        // and pins every increment at the maximum instead (UD80,
        // `ref_counted_set.rs` `bump_ref`), so that path no longer overflows.
        //
        // The safe handling HERE is this function's OWN existing degrade: an
        // ID it cannot honour stores the cell with the default style (that is
        // what the `!= 0` check above already does for an unmapped ID, and
        // what the hyperlink half below does when `set_hyperlink` fails). It
        // loses this cell's style and nothing else, and invents no new
        // behaviour. Rejecting the whole page would also work but would refuse
        // pages that decode correctly today, including snapshots of those
        // wide terminals. Note that a degraded page leaves the style at the
        // ceiling (65,534 once the style table's own reference is released);
        // a later cursor-style `add` plus a styled print over it reaches the
        // maximum and pins there (UD80) instead of overflowing.
        //
        // SAFETY: the remapped ID was assigned by this page's style set.
        if wire.style_id != 0
            && unsafe {
                page.styles
                    .ref_count_saturated(page.memory_ptr(), wire.style_id)
            }
        {
            wire.style_id = 0;
        }

        if wire.style_id != 0 {
            // SAFETY: the remapped ID was assigned by this page's style set.
            unsafe {
                page.styles.use_id(page.memory_ptr(), wire.style_id);
                (*row).set_styled(true);
            }
        }
    }

    store_cell(cell, wire.bits());

    if link_encoded != 0 {
        let link_native = hyperlink_remap.get(link_encoded);
        if link_native == 0 {
            return;
        }

        // setHyperlink records the cell mapping but intentionally does not
        // increment the set's reference count. If its map is full, undo our
        // reference and leave this cell unlinked.
        // SAFETY: the remapped ID was assigned by this page's hyperlink set.
        unsafe { page.hyperlink_set.use_id(page.memory_ptr(), link_native) };
        if page.set_hyperlink(row, cell, link_native).is_err() {
            let page_ptr: *mut TerminalPage = page;
            page.hyperlink_set.context.set_page(page_ptr);
            // SAFETY: releasing the reference taken above; the table still
            // holds its own reference, so the entry stays live.
            unsafe { page.hyperlink_set.release(page.memory_ptr(), link_native) };
        }
    }
}

/// Resolve wide-pair relationships for the cell at `x` against its already
/// normalized predecessors.
fn normalize_wide(row: *const TerminalRow, cells: *mut [TerminalCell], x: usize) {
    let len = cells.len();
    let base = cells as *mut TerminalCell;
    // SAFETY: all indices below are in bounds of the row's cells; accesses
    // are narrow raw reads/writes with no other live alias.
    unsafe {
        // A following cell resolves whether the previous wide marker owns a
        // tail. Normalize the current marker immediately when possible,
        // including a wide marker at the row end.
        if x > 0
            && (*base.add(x - 1)).wide() == terminal_page::Wide::Wide
            && (*base.add(x)).wide() != terminal_page::Wide::SpacerTail
        {
            (*base.add(x - 1)).set_wide(terminal_page::Wide::Narrow);
        }
        match (*base.add(x)).wide() {
            terminal_page::Wide::Narrow => {}

            // A non-final wide marker remains pending until the next cell.
            terminal_page::Wide::Wide => {
                if x + 1 == len {
                    (*base.add(x)).set_wide(terminal_page::Wide::Narrow);
                }
            }

            terminal_page::Wide::SpacerTail => {
                if x == 0 || (*base.add(x - 1)).wide() != terminal_page::Wide::Wide {
                    (*base.add(x)).set_wide(terminal_page::Wide::Narrow);
                }
            }

            terminal_page::Wide::SpacerHead => {
                if x + 1 != len || !(*row).wrap() {
                    (*base.add(x)).set_wide(terminal_page::Wide::Narrow);
                }
            }
        }
    }
}

/// Decode the grapheme suffix section into already decoded cells.
fn decode_graphemes(
    page: &mut TerminalPage,
    reader: &mut dyn io::Reader,
) -> Result<(), DecodeError> {
    let entries = io::read_int::<u32>(reader)?;
    for _ in 0..entries {
        // The staged and borrowed payload paths have every header buffered.
        let (y, x, cp_count): (u16, u16, u16) = {
            if reader.buffered_len() >= 6 {
                let bytes: [u8; 6] = reader.buffered()[0..6].try_into().unwrap();
                reader.toss(6);
                (
                    u16::from_le_bytes(bytes[0..2].try_into().unwrap()),
                    u16::from_le_bytes(bytes[2..4].try_into().unwrap()),
                    u16::from_le_bytes(bytes[4..6].try_into().unwrap()),
                )
            } else {
                (
                    io::read_int::<u16>(reader)?,
                    io::read_int::<u16>(reader)?,
                    io::read_int::<u16>(reader)?,
                )
            }
        };

        // Resolve the target cell. Entries whose target cannot carry a
        // suffix are optional detail: their codepoints are consumed to
        // preserve framing and then dropped.
        let target: Option<(*mut TerminalRow, *mut TerminalCell)> = 'target: {
            if y as usize >= page.size.rows as usize || x as usize >= page.size.cols as usize {
                break 'target None;
            }
            let row = page.get_row(y as usize);
            let cell: *mut TerminalCell =
                unsafe { (page.get_cells(row) as *mut TerminalCell).add(x as usize) };

            // Cell decoding stores every kind 1 cell as a plain codepoint,
            // so this also gives duplicate entries first-wins semantics.
            // SAFETY: in-bounds raw reads of the live cell.
            unsafe {
                if (*cell).content_tag() != terminal_page::ContentTag::Codepoint {
                    break 'target None;
                }
                if (*cell).content_codepoint() == 0 {
                    break 'target None;
                }
            }
            Some((row, cell))
        };

        // Always consume every declared codepoint. Invalid scalars and NUL
        // are not meaningful grapheme suffix components and are ignored. A
        // dropped entry's codepoints are discarded in bulk.
        let (row, cell) = match target {
            Some(resolved) => resolved,
            None => {
                reader.discard_all(cp_count as usize * 4)?;
                continue;
            }
        };

        let mut accept = true;
        let mut index: usize = 0;
        while index < cp_count as usize {
            let buffered_len = reader.buffered_len();
            if buffered_len >= 4 {
                let n = (cp_count as usize - index).min(buffered_len / 4);
                // (The reader's buffer borrow and the page mutations in
                // `apply_grapheme_suffix` touch disjoint objects.)
                let bytes = &reader.buffered()[0..n * 4];
                for i in 0..n {
                    apply_grapheme_suffix(
                        page,
                        row,
                        cell,
                        &mut accept,
                        u32::from_le_bytes(bytes[i * 4..i * 4 + 4].try_into().unwrap()),
                    );
                }
                reader.toss(n * 4);
                index += n;
            } else {
                let cp = io::read_int::<u32>(reader)?;
                apply_grapheme_suffix(page, row, cell, &mut accept, cp);
                index += 1;
            }
        }
    }
    Ok(())
}

/// Attach one decoded suffix codepoint to its resolved target cell.
///
/// If native capacity is exhausted, any prefix already attached is removed
/// so the cell never exposes a truncated cluster, and `accept` latches
/// false so the entry's remaining codepoints are consumed but dropped.
fn apply_grapheme_suffix(
    page: &mut TerminalPage,
    row: *mut TerminalRow,
    cell: *mut TerminalCell,
    accept: &mut bool,
    cp: u32,
) {
    if !*accept {
        return;
    }
    if cp == 0 || !valid_scalar(cp) {
        return;
    }

    if page.append_grapheme(row, cell, cp).is_err() {
        // SAFETY: narrow raw read; the page call above has returned.
        if unsafe { (*cell).has_grapheme() } {
            page.clear_grapheme(cell);
            page.update_row_grapheme_flag(row);
        }
        *accept = false;
    }
}

/// The encoded word for one native cell and its hyperlink ID.
fn cell_bits(cell: TerminalCell, link_id: TerminalHyperlinkId) -> u64 {
    if NATIVE_MATCHES_WIRE {
        let mut wire = Cell::from_bits(cell.to_bits());
        wire.hyperlink = link_id != 0;
        wire.hyperlink_id = link_id;
        return wire.bits();
    }

    let wire = Cell {
        kind: cell.content_tag() as u8,
        content: match cell.content_tag() {
            terminal_page::ContentTag::Codepoint | terminal_page::ContentTag::CodepointGrapheme => {
                cell.content_codepoint()
            }
            terminal_page::ContentTag::BgColorPalette => cell.content_color_palette() as u32,
            terminal_page::ContentTag::BgColorRgb => {
                let rgb = cell.content_color_rgb();
                (rgb.r as u32) | ((rgb.g as u32) << 8) | ((rgb.b as u32) << 16)
            }
        },
        style_id: cell.style_id(),
        width: match cell.wide() {
            terminal_page::Wide::Narrow => 0,
            terminal_page::Wide::Wide => 1,
            terminal_page::Wide::SpacerTail => 2,
            terminal_page::Wide::SpacerHead => 3,
        },
        protected: cell.protected(),
        hyperlink: link_id != 0,
        semantic_content: match cell.semantic_content() {
            terminal_page::SemanticContent::Output => 0,
            terminal_page::SemanticContent::Input => 1,
            terminal_page::SemanticContent::Prompt => 2,
        },
        hyperlink_id: link_id,
    };
    wire.bits()
}

/// Store one normalized, hyperlink-free wire word as a native cell.
fn store_cell(cell: *mut TerminalCell, bits: u64) {
    if NATIVE_MATCHES_WIRE {
        // SAFETY: caller guarantees `cell` points at a live page cell with no
        // other live alias.
        unsafe { *cell = TerminalCell::from_bits(bits) };
        return;
    }

    // SAFETY: as above.
    unsafe { store_cell_ref(&mut *cell, bits) };
}

/// The portable (non-layout-matching) arm of `store_cell`, shared with the
/// `widen_cells` fallback.
fn store_cell_ref(cell: &mut TerminalCell, bits: u64) {
    let wire = Cell::from_bits(bits);
    let mut native = TerminalCell::init(0);
    match Kind::from_u2(wire.kind) {
        Kind::Codepoint | Kind::CodepointGrapheme => {
            native.set_content_codepoint(wire.content);
        }
        Kind::BgColorPalette => {
            native.set_content_tag(terminal_page::ContentTag::BgColorPalette);
            native.set_content_color_palette(wire.content as u8);
        }
        Kind::BgColorRgb => {
            native.set_content_tag(terminal_page::ContentTag::BgColorRgb);
            native.set_content_color_rgb(terminal_page::CellRgb {
                r: wire.content as u8,
                g: (wire.content >> 8) as u8,
                b: (wire.content >> 16) as u8,
            });
        }
    }
    native.set_style_id(wire.style_id);
    native.set_wide(match wire.width {
        0 => terminal_page::Wide::Narrow,
        1 => terminal_page::Wide::Wide,
        2 => terminal_page::Wide::SpacerTail,
        3 => terminal_page::Wide::SpacerHead,
        _ => unreachable!(),
    });
    native.set_protected(wire.protected);
    native.set_semantic_content(match wire.semantic_content {
        0 => terminal_page::SemanticContent::Output,
        1 => terminal_page::SemanticContent::Input,
        2 => terminal_page::SemanticContent::Prompt,
        _ => unreachable!("normalized before store"),
    });
    *cell = native;
}

/// Maps encoded table IDs to IDs assigned by the destination page.
///
/// Build this by inserting each decoded table entry into the page, then
/// recording the encoded ID and the ID returned by the page's set. ID zero
/// is implicit and does not need an entry. Lookup must be cheap because the
/// grid decoder consults it for every styled or linked cell, so this is a
/// direct-indexed table rather than a hash map.
///
/// PORT NOTE: the reference is `fn Remap(comptime Id: type)` instantiated
/// at `TerminalStyleId` and `TerminalHyperlinkId`; both are `u16`
/// (`size::CellCountInt`), so the port is one concrete type with two
/// aliases. The `seen` bit set is a minimal `DynamicBitSetUnmanaged`
/// stand-in. Allocation goes through the global allocator (the crate-wide
/// treatment of plain `Allocator` parameters).
pub struct Remap {
    /// One slot for every possible encoded ID. (`pub const capacity`.)
    // pub const capacity = std.math.maxInt(Id) + 1;
    /// Indexed by encoded ID; zero means unmapped or default.
    pub entries: Vec<u16>,

    /// Tracks IDs that received an entry, including ones mapped to the
    /// default, so callers can give duplicate table entries first-wins
    /// semantics. (Zig: `std.DynamicBitSetUnmanaged`.)
    pub seen: Vec<u64>,
}

impl Remap {
    /// One slot for every possible encoded ID.
    pub const CAPACITY: usize = u16::MAX as usize + 1;

    /// A remap with no entries at all: every lookup is unmapped. Use
    /// this instead of `init` when the encoded table is empty so pages
    /// without styles or hyperlinks allocate nothing.
    pub const fn empty() -> Remap {
        Remap {
            entries: Vec::new(),
            seen: Vec::new(),
        }
    }

    pub fn init() -> Remap {
        Remap {
            entries: vec![0u16; Self::CAPACITY],
            seen: vec![0u64; Self::CAPACITY / 64],
        }
    }

    // deinit: dropped.

    /// Record one encoded-to-native mapping. Illegal on `empty`.
    pub fn put(&mut self, encoded: u16, native: u16) {
        assert!(!self.is_seen(encoded));
        self.entries[encoded as usize] = native;
        self.seen[encoded as usize / 64] |= 1u64 << (encoded as usize % 64);
    }

    /// Whether the encoded ID already has an entry, even a default
    /// one. Illegal on `empty`.
    pub fn contains(&self, encoded: u16) -> bool {
        self.is_seen(encoded)
    }

    fn is_seen(&self, encoded: u16) -> bool {
        self.seen[encoded as usize / 64] & (1u64 << (encoded as usize % 64)) != 0
    }

    /// The native ID for an encoded ID, or zero when unmapped.
    #[inline]
    pub fn get(&self, encoded: u16) -> u16 {
        if self.entries.is_empty() {
            return 0;
        }
        self.entries[encoded as usize]
    }

    /// `seen.iterator(.{})` — ascending indices of every recorded encoded ID.
    pub fn seen_iter(&self) -> impl Iterator<Item = usize> + '_ {
        self.seen.iter().enumerate().flat_map(|(word_index, word)| {
            (0..64)
                .filter(move |bit| word & (1u64 << bit) != 0)
                .map(move |bit| word_index * 64 + bit)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::fixture;
    use super::*;
    use crate::page::Capacity;
    use crate::snapshot::io::Writer as _;

    // const test_golden_fixture = test_fixture.parse(@embedFile("testdata/grid-v1.hex"));
    fn test_golden_fixture() -> Vec<u8> {
        fixture::parse(include_str!("testdata/grid-v1.hex"))
    }

    // test "cell wire layout registry"
    #[test]
    fn cell_wire_layout_registry() {
        // The wire bit positions are format constants. The shifts and masks
        // derive from the packed struct, so pin the struct itself to the
        // documented registry so an edit cannot silently move wire bits.
        assert_eq!(0, CELL_KIND_SHIFT);
        assert_eq!(2, CELL_CONTENT_SHIFT);
        assert_eq!(26, CELL_STYLE_ID_SHIFT);
        assert_eq!(42, CELL_WIDTH_SHIFT);
        assert_eq!(44, CELL_PROTECTED_SHIFT);
        assert_eq!(45, CELL_HYPERLINK_SHIFT);
        assert_eq!(46, CELL_SEMANTIC_CONTENT_SHIFT);
        assert_eq!(48, CELL_HYPERLINK_ID_SHIFT);

        // The native cell currently matches the wire registry, which enables
        // the bulk row codec. If this fails, the native layout diverged: either
        // restore it or accept the portable codec and update this expectation.
        //
        // PORT NOTE: these are the reference's three comptime probes (one per
        // content payload), evaluated at test time because the native
        // accessors are not `const fn` (see the `NATIVE_MATCHES_WIRE` note).
        // Probe 1: codepoint-grapheme content with style, wide, protected,
        // and nonzero native padding (the wire hyperlink-ID position).
        {
            let mut native = TerminalCell::from_bits(0);
            native.set_content_tag(terminal_page::ContentTag::CodepointGrapheme);
            native.set_content_codepoint(0x10FFFF);
            native.set_style_id(0xBEEF);
            native.set_wide(terminal_page::Wide::SpacerTail);
            native.set_protected(true);
            // `._padding = 0x1D2C` — the native padding bits (48..63).
            let native_bits = native.to_bits() | (0x1D2Cu64 << 48);
            let wire = Cell {
                kind: 1,
                content: 0x10FFFF,
                style_id: 0xBEEF,
                width: 2,
                protected: true,
                // The native padding position, canonical wire or not.
                hyperlink_id: 0x1D2C,
                ..Cell::default()
            };
            assert_eq!(native_bits, wire.bits());
        }
        // Probe 2: palette background, spacer head, hyperlink, input.
        {
            let mut native = TerminalCell::from_bits(0);
            native.set_content_tag(terminal_page::ContentTag::BgColorPalette);
            native.set_content_color_palette(0xAB);
            native.set_wide(terminal_page::Wide::SpacerHead);
            native.set_hyperlink(true);
            native.set_semantic_content(terminal_page::SemanticContent::Input);
            let wire = Cell {
                kind: 2,
                content: 0xAB,
                width: 3,
                hyperlink: true,
                semantic_content: 1,
                ..Cell::default()
            };
            assert_eq!(native.to_bits(), wire.bits());
        }
        // Probe 3: RGB background, wide, prompt.
        {
            let mut native = TerminalCell::from_bits(0);
            native.set_content_tag(terminal_page::ContentTag::BgColorRgb);
            native.set_content_color_rgb(terminal_page::CellRgb {
                r: 0x12,
                g: 0x34,
                b: 0x56,
            });
            native.set_wide(terminal_page::Wide::Wide);
            native.set_semantic_content(terminal_page::SemanticContent::Prompt);
            let wire = Cell {
                kind: 3,
                content: 0x563412,
                width: 1,
                semantic_content: 2,
                ..Cell::default()
            };
            assert_eq!(native.to_bits(), wire.bits());
        }

        assert!(NATIVE_MATCHES_WIRE);
    }

    // test "encoded cell width transport registry"
    #[test]
    fn encoded_cell_width_transport_registry() {
        // Pin the transported value and admission mask of every width against
        // the documented format: widths one and two carry the bare codepoint,
        // width four the low word half, width eight the complete word.
        let word: u64 = Cell {
            kind: 1,
            content: 0xABCDEF,
            style_id: 0x1234,
            hyperlink_id: 0x5678,
            ..Cell::default()
        }
        .bits();
        assert_eq!(0xEFu64, EncodedWidth::One.truncate(word));
        assert_eq!(0xCDEFu64, EncodedWidth::Two.truncate(word));
        assert_eq!(word & 0xFFFF_FFFF, EncodedWidth::Four.truncate(word));
        assert_eq!(word, EncodedWidth::Eight.truncate(word));

        assert_eq!(0x0000_0000_0000_03FCu64, EncodedWidth::One.mask());
        assert_eq!(0x0000_0000_0003_FFFCu64, EncodedWidth::Two.mask());
        assert_eq!(0x0000_0000_FFFF_FFFFu64, EncodedWidth::Four.mask());
        assert_eq!(0xFFFF_FFFF_FFFF_FFFFu64, EncodedWidth::Eight.mask());

        // Selection returns the smallest admissible width, and every admitted
        // word round-trips through its transport.
        struct Case {
            cell: Cell,
            width: EncodedWidth,
        }
        let cases = [
            Case {
                cell: Cell::default(),
                width: EncodedWidth::One,
            },
            Case {
                cell: Cell {
                    content: 0xFF,
                    ..Cell::default()
                },
                width: EncodedWidth::One,
            },
            Case {
                cell: Cell {
                    content: 0x100,
                    ..Cell::default()
                },
                width: EncodedWidth::Two,
            },
            Case {
                cell: Cell {
                    content: 0xFFFF,
                    ..Cell::default()
                },
                width: EncodedWidth::Two,
            },
            Case {
                cell: Cell {
                    content: 0x10000,
                    ..Cell::default()
                },
                width: EncodedWidth::Four,
            },
            Case {
                cell: Cell {
                    kind: 2,
                    content: 7,
                    ..Cell::default()
                },
                width: EncodedWidth::Four,
            },
            Case {
                cell: Cell {
                    content: 'a' as u32,
                    style_id: 63,
                    ..Cell::default()
                },
                width: EncodedWidth::Four,
            },
            Case {
                cell: Cell {
                    content: 'a' as u32,
                    style_id: 64,
                    ..Cell::default()
                },
                width: EncodedWidth::Eight,
            },
            Case {
                cell: Cell {
                    width: 1,
                    content: 'a' as u32,
                    ..Cell::default()
                },
                width: EncodedWidth::Eight,
            },
            Case {
                cell: Cell {
                    protected: true,
                    ..Cell::default()
                },
                width: EncodedWidth::Eight,
            },
            Case {
                cell: Cell {
                    semantic_content: 1,
                    ..Cell::default()
                },
                width: EncodedWidth::Eight,
            },
            Case {
                cell: Cell {
                    hyperlink: true,
                    hyperlink_id: 1,
                    ..Cell::default()
                },
                width: EncodedWidth::Eight,
            },
        ];
        for case in cases {
            let case_word: u64 = case.cell.bits();
            assert_eq!(case.width, EncodedWidth::select(case_word));
            assert_eq!(case_word, case.width.extend(case.width.truncate(case_word)),);
        }
    }

    // test "grid golden encoding and decoding"
    #[test]
    fn grid_golden_encoding_and_decoding() {
        let test_golden_fixture = test_golden_fixture();

        let capacity = Capacity {
            cols: 3,
            rows: 4,
            styles: 0,
            hyperlink_bytes: 0,
            grapheme_bytes: 64,
            string_bytes: 0,
        };
        let mut source = TerminalPage::init(capacity).unwrap();

        // The first row covers semantic metadata, a wide-cell pair, and a
        // multi-codepoint grapheme.
        let wide = source.get_row_and_cell(0, 0);
        unsafe {
            (*wide.row).set_semantic_prompt(terminal_page::SemanticPrompt::Prompt);
            *wide.cell = TerminalCell::init('A' as u32);
            (*wide.cell).set_wide(terminal_page::Wide::Wide);
            (*wide.cell).set_protected(true);
            (*wide.cell).set_semantic_content(terminal_page::SemanticContent::Prompt);
        }

        let tail = source.get_row_and_cell(1, 0);
        unsafe {
            (*tail.cell).set_wide(terminal_page::Wide::SpacerTail);
            (*tail.cell).set_semantic_content(terminal_page::SemanticContent::Input);
        }

        let grapheme = source.get_row_and_cell(2, 0);
        unsafe { *grapheme.cell = TerminalCell::init('x' as u32) };
        source
            .set_graphemes(grapheme.row, grapheme.cell, &[0x0301, 0x0302])
            .unwrap();

        // The second row covers both background-color kinds and a wrapped spacer
        // head with the remaining row semantic value.
        let palette = source.get_row_and_cell(0, 1);
        unsafe {
            (*palette.cell).set_content_tag(terminal_page::ContentTag::BgColorPalette);
            (*palette.cell).set_content_color_palette(7);
        }

        let rgb = source.get_row_and_cell(1, 1);
        unsafe {
            (*rgb.cell).set_content_tag(terminal_page::ContentTag::BgColorRgb);
            (*rgb.cell).set_content_color_rgb(terminal_page::CellRgb {
                r: 0xaa,
                g: 0xbb,
                b: 0xcc,
            });
            (*rgb.cell).set_protected(true);
        }

        let head = source.get_row_and_cell(2, 1);
        unsafe {
            (*head.cell).set_wide(terminal_page::Wide::SpacerHead);
            (*head.row).set_wrap(true);
            (*head.row).set_wrap_continuation(true);
            (*head.row).set_semantic_prompt(terminal_page::SemanticPrompt::PromptContinuation);
        }

        // The third row is bare ASCII text with an interior blank, which uses
        // the one-byte encoded cell width.
        unsafe {
            *source.get_row_and_cell(0, 2).cell = TerminalCell::init('h' as u32);
            *source.get_row_and_cell(2, 2).cell = TerminalCell::init('i' as u32);

            // The fourth row needs the two-byte width for a codepoint above U+00FF.
            *source.get_row_and_cell(0, 3).cell = TerminalCell::init(0x0416); // Ж
            *source.get_row_and_cell(1, 3).cell = TerminalCell::init('!' as u32);
        }

        let mut encoded = [0u8; 160];
        let mut writer = io::FixedWriter::new(&mut encoded);
        encode(&source, &mut writer).unwrap();
        fixture::expect_equal(
            fixture::Kind::Bytes,
            "src/snapshot/testdata/grid-v1.hex",
            "snapshot_fixture-grid-v1.hex",
            &test_golden_fixture,
            writer.buffered(),
        );

        let mut destination = TerminalPage::init(capacity).unwrap();
        let style_remap = StyleRemap::init();
        let hyperlink_remap = HyperlinkRemap::init();

        // Decode the checked-in reference through a one-byte reader buffer.
        let mut fixture_reader = io::FixedReader::new(&test_golden_fixture);
        let mut read_buffer = [0u8; 1];
        let mut limited =
            io::LimitedReader::new(&mut fixture_reader, io::Limit::Unlimited, &mut read_buffer);
        decode(
            &mut destination,
            &mut limited,
            &style_remap,
            &hyperlink_remap,
        )
        .unwrap();
        destination.verify_integrity().unwrap();

        let decoded_wide = destination.get_row_and_cell(0, 0);
        unsafe {
            assert_eq!('A' as u32, (*decoded_wide.cell).codepoint());
            assert_eq!(terminal_page::Wide::Wide, (*decoded_wide.cell).wide());
            assert!((*decoded_wide.cell).protected());
            assert_eq!(
                terminal_page::SemanticContent::Prompt,
                (*decoded_wide.cell).semantic_content(),
            );
            assert_eq!(
                terminal_page::SemanticPrompt::Prompt,
                (*decoded_wide.row).semantic_prompt(),
            );
        }

        let decoded_tail = destination.get_row_and_cell(1, 0);
        unsafe {
            assert_eq!(terminal_page::Wide::SpacerTail, (*decoded_tail.cell).wide());
            assert_eq!(
                terminal_page::SemanticContent::Input,
                (*decoded_tail.cell).semantic_content(),
            );
        }

        let decoded_grapheme = destination.get_row_and_cell(2, 0);
        assert_eq!(
            &[0x0301u32, 0x0302][..],
            destination.lookup_grapheme(decoded_grapheme.cell).unwrap(),
        );

        let decoded_palette = destination.get_row_and_cell(0, 1);
        unsafe {
            assert_eq!(
                terminal_page::ContentTag::BgColorPalette,
                (*decoded_palette.cell).content_tag(),
            );
            assert_eq!(7u8, (*decoded_palette.cell).content_color_palette());
        }

        let decoded_rgb = destination.get_row_and_cell(1, 1);
        unsafe {
            assert_eq!(
                terminal_page::ContentTag::BgColorRgb,
                (*decoded_rgb.cell).content_tag(),
            );
            assert_eq!(
                terminal_page::CellRgb {
                    r: 0xaa,
                    g: 0xbb,
                    b: 0xcc,
                },
                (*decoded_rgb.cell).content_color_rgb(),
            );
            assert!((*decoded_rgb.cell).protected());
        }

        let decoded_head = destination.get_row_and_cell(2, 1);
        unsafe {
            assert_eq!(terminal_page::Wide::SpacerHead, (*decoded_head.cell).wide());
            assert!((*decoded_head.row).wrap());
            assert!((*decoded_head.row).wrap_continuation());
            assert_eq!(
                terminal_page::SemanticPrompt::PromptContinuation,
                (*decoded_head.row).semantic_prompt(),
            );
        }

        unsafe {
            assert_eq!(
                'h' as u32,
                (*destination.get_row_and_cell(0, 2).cell).codepoint(),
            );
            assert!((*destination.get_row_and_cell(1, 2).cell).is_zero());
            assert_eq!(
                'i' as u32,
                (*destination.get_row_and_cell(2, 2).cell).codepoint(),
            );
            assert_eq!(
                0x0416u32,
                (*destination.get_row_and_cell(0, 3).cell).codepoint(),
            );
            assert_eq!(
                '!' as u32,
                (*destination.get_row_and_cell(1, 3).cell).codepoint(),
            );
        }

        // A re-encode proves the decoded native page retains every wire field.
        let mut reencoded = [0u8; 160];
        let mut rewriter = io::FixedWriter::new(&mut reencoded);
        encode(&destination, &mut rewriter).unwrap();
        assert_eq!(&test_golden_fixture[..], rewriter.buffered());

        source.deinit();
        destination.deinit();
    }

    // test "grid elides trailing default cells"
    #[test]
    fn grid_elides_trailing_default_cells() {
        let mut page = TerminalPage::init(Capacity::new(80, 3)).unwrap();

        // Row 0 is fully default. Row 1 has content in columns zero and two.
        // Row 2 has one protected-only cell at column four.
        let middle = page.get_row_and_cell(2, 1);
        unsafe {
            *middle.cell = TerminalCell::init('b' as u32);
            *page.get_row_and_cell(0, 1).cell = TerminalCell::init('a' as u32);
            (*page.get_row_and_cell(4, 2).cell).set_protected(true);
        }

        let mut encoded = [0u8; 256];
        let mut writer = io::FixedWriter::new(&mut encoded);
        encode(&page, &mut writer).unwrap();

        // 3 bytes per row header and cells only through the last non-default
        // cell, followed by an empty grapheme section. The text row uses the
        // one-byte width while the protected flag forces the full width.
        assert_eq!(3 + (3 + 3 * 1) + (3 + 5 * 8) + 4, writer.buffered().len());

        let mut destination = TerminalPage::init(Capacity::new(80, 3)).unwrap();
        let style_remap = StyleRemap::init();
        let hyperlink_remap = HyperlinkRemap::init();

        let buffered = writer.buffered().to_vec();
        let mut reader = io::FixedReader::new(&buffered);
        decode(
            &mut destination,
            &mut reader,
            &style_remap,
            &hyperlink_remap,
        )
        .unwrap();
        destination.verify_integrity().unwrap();

        unsafe {
            assert_eq!(
                'a' as u32,
                (*destination.get_row_and_cell(0, 1).cell).codepoint(),
            );
            assert_eq!(
                'b' as u32,
                (*destination.get_row_and_cell(2, 1).cell).codepoint(),
            );
            assert!((*destination.get_row_and_cell(4, 2).cell).protected());
            assert!((*destination.get_row_and_cell(79, 1).cell).is_zero());
        }

        page.deinit();
        destination.deinit();
    }

    // test "grid rejects a row cell count above the column count"
    #[test]
    fn grid_rejects_a_row_cell_count_above_the_column_count() {
        let mut page = TerminalPage::init(Capacity::new(2, 1)).unwrap();
        let style_remap = StyleRemap::init();
        let hyperlink_remap = HyperlinkRemap::init();

        let mut payload = [0u8; 64];
        let mut writer = io::FixedWriter::new(&mut payload);
        writer.write_byte(Row::default().bits()).unwrap();
        io::write_int::<u16>(&mut writer, 3).unwrap();
        for _ in 0..3 {
            io::write_int::<u64>(&mut writer, 0).unwrap();
        }
        io::write_int::<u32>(&mut writer, 0).unwrap();

        let buffered = writer.buffered().to_vec();
        let mut reader = io::FixedReader::new(&buffered);
        assert_eq!(
            Err(DecodeError::InvalidRowCellCount),
            decode(&mut page, &mut reader, &style_remap, &hyperlink_remap),
        );

        page.deinit();
    }

    // test "grid normalizes incomplete wide cells"
    #[test]
    fn grid_normalizes_incomplete_wide_cells() {
        // One column puts the wide cell at the row end. Two columns put an
        // ordinary narrow cell after it. Neither case supplies a spacer tail.
        // Both rely on the implicit narrow cell rule when the tail is elided.
        for columns in [1u16, 2] {
            let capacity = Capacity::new(columns, 1);

            // Craft the malformed row directly on the wire. Decoding keeps its
            // content but makes the incomplete wide cell narrow.
            let mut payload = [0u8; 64];
            let mut payload_writer = io::FixedWriter::new(&mut payload);
            payload_writer
                .write_byte(
                    Row {
                        cell_width: EncodedWidth::Eight,
                        ..Row::default()
                    }
                    .bits(),
                )
                .unwrap();
            io::write_int::<u16>(&mut payload_writer, columns).unwrap();
            io::write_int::<u64>(
                &mut payload_writer,
                Cell {
                    width: 1, // wide
                    content: 'A' as u32,
                    ..Cell::default()
                }
                .bits(),
            )
            .unwrap();
            if columns > 1 {
                io::write_int::<u64>(
                    &mut payload_writer,
                    Cell {
                        content: 'B' as u32,
                        ..Cell::default()
                    }
                    .bits(),
                )
                .unwrap();
            }
            io::write_int::<u32>(&mut payload_writer, 0).unwrap();

            let mut destination = TerminalPage::init(capacity).unwrap();
            let style_remap = StyleRemap::init();
            let hyperlink_remap = HyperlinkRemap::init();

            let buffered = payload_writer.buffered().to_vec();
            let mut payload_reader = io::FixedReader::new(&buffered);
            decode(
                &mut destination,
                &mut payload_reader,
                &style_remap,
                &hyperlink_remap,
            )
            .unwrap();
            destination.verify_integrity().unwrap();

            unsafe {
                assert_eq!(
                    'A' as u32,
                    (*destination.get_row_and_cell(0, 0).cell).codepoint(),
                );
                assert_eq!(
                    terminal_page::Wide::Narrow,
                    (*destination.get_row_and_cell(0, 0).cell).wide(),
                );
                if columns > 1 {
                    let second = *destination.get_row_and_cell(1, 0).cell;
                    assert_eq!('B' as u32, second.codepoint());
                    assert_eq!(terminal_page::Wide::Narrow, second.wide());
                }
            }

            destination.deinit();
        }

        // A wide marker whose spacer tail was elided by a short cell count is
        // also normalized, and the trailing cells stay default.
        let mut page = TerminalPage::init(Capacity::new(4, 1)).unwrap();
        let style_remap = StyleRemap::init();
        let hyperlink_remap = HyperlinkRemap::init();

        let mut payload = [0u8; 64];
        let mut writer = io::FixedWriter::new(&mut payload);
        writer
            .write_byte(
                Row {
                    cell_width: EncodedWidth::Eight,
                    ..Row::default()
                }
                .bits(),
            )
            .unwrap();
        io::write_int::<u16>(&mut writer, 1).unwrap();
        io::write_int::<u64>(
            &mut writer,
            Cell {
                width: 1, // wide
                content: 'W' as u32,
                ..Cell::default()
            }
            .bits(),
        )
        .unwrap();
        io::write_int::<u32>(&mut writer, 0).unwrap();

        let buffered = writer.buffered().to_vec();
        let mut reader = io::FixedReader::new(&buffered);
        decode(&mut page, &mut reader, &style_remap, &hyperlink_remap).unwrap();
        page.verify_integrity().unwrap();
        unsafe {
            assert_eq!(
                terminal_page::Wide::Narrow,
                (*page.get_row_and_cell(0, 0).cell).wide(),
            );
            assert!((*page.get_row_and_cell(1, 0).cell).is_zero());
        }

        page.deinit();
    }

    // test "grid normalizes reserved cell values"
    #[test]
    fn grid_normalizes_reserved_cell_values() {
        let mut page = TerminalPage::init(Capacity::new(3, 1)).unwrap();
        let style_remap = StyleRemap::init();
        let hyperlink_remap = HyperlinkRemap::init();

        let mut payload = [0u8; 64];
        let mut writer = io::FixedWriter::new(&mut payload);

        // Reserved row flag bits six and seven are ignored while the unknown
        // semantic-prompt value degrades to none and wrap survives. Bits four
        // and five select the full encoded cell width.
        writer.write_byte(0xFD).unwrap();
        io::write_int::<u16>(&mut writer, 3).unwrap();

        // A surrogate codepoint with reserved semantic content 3.
        io::write_int::<u64>(
            &mut writer,
            Cell {
                content: 0xD800,
                semantic_content: 3,
                ..Cell::default()
            }
            .bits(),
        )
        .unwrap();

        // Reserved palette content bits do not obscure the palette index.
        io::write_int::<u64>(
            &mut writer,
            Cell {
                kind: 2,
                content: 0xFFFF07,
                ..Cell::default()
            }
            .bits(),
        )
        .unwrap();

        // An unknown style reference degrades to the default style, and an
        // unknown hyperlink reference leaves the cell unlinked.
        io::write_int::<u64>(
            &mut writer,
            Cell {
                content: 'x' as u32,
                style_id: 5,
                hyperlink: true,
                hyperlink_id: 9,
                ..Cell::default()
            }
            .bits(),
        )
        .unwrap();
        io::write_int::<u32>(&mut writer, 0).unwrap();

        let buffered = writer.buffered().to_vec();
        let mut reader = io::FixedReader::new(&buffered);
        decode(&mut page, &mut reader, &style_remap, &hyperlink_remap).unwrap();
        page.verify_integrity().unwrap();

        let first = page.get_row_and_cell(0, 0);
        unsafe {
            assert!((*first.row).wrap());
            assert_eq!(
                terminal_page::SemanticPrompt::None,
                (*first.row).semantic_prompt(),
            );
            assert_eq!(0xFFFDu32, (*first.cell).codepoint());
            assert_eq!(
                terminal_page::SemanticContent::Output,
                (*first.cell).semantic_content(),
            );
        }

        unsafe {
            let second = *page.get_row_and_cell(1, 0).cell;
            assert_eq!(
                terminal_page::ContentTag::BgColorPalette,
                second.content_tag(),
            );
            assert_eq!(7u8, second.content_color_palette());
        }

        let third_ptr = page.get_row_and_cell(2, 0).cell;
        let third = unsafe { *third_ptr };
        assert_eq!('x' as u32, third.codepoint());
        assert_eq!(0 as TerminalStyleId, third.style_id());
        assert!(!third.hyperlink());
        assert_eq!(None, page.lookup_hyperlink(third_ptr));

        page.deinit();
    }

    // test "grid drops undeliverable grapheme entries"
    #[test]
    fn grid_drops_undeliverable_grapheme_entries() {
        let mut page = TerminalPage::init(Capacity {
            grapheme_bytes: 64,
            ..Capacity::new(4, 1)
        })
        .unwrap();
        let style_remap = StyleRemap::init();
        let hyperlink_remap = HyperlinkRemap::init();

        let mut payload = [0u8; 128];
        let mut writer = io::FixedWriter::new(&mut payload);
        writer
            .write_byte(
                Row {
                    cell_width: EncodedWidth::Eight,
                    ..Row::default()
                }
                .bits(),
            )
            .unwrap();
        io::write_int::<u16>(&mut writer, 4).unwrap();
        // A kind 1 cell that receives a valid entry.
        io::write_int::<u64>(
            &mut writer,
            Cell {
                kind: 1,
                content: 'x' as u32,
                ..Cell::default()
            }
            .bits(),
        )
        .unwrap();
        // A kind 1 cell without an entry decodes as a plain codepoint.
        io::write_int::<u64>(
            &mut writer,
            Cell {
                kind: 1,
                content: 'y' as u32,
                ..Cell::default()
            }
            .bits(),
        )
        .unwrap();
        // A background cell cannot carry a suffix.
        io::write_int::<u64>(
            &mut writer,
            Cell {
                kind: 2,
                content: 7,
                ..Cell::default()
            }
            .bits(),
        )
        .unwrap();
        // An empty codepoint cannot carry a suffix.
        io::write_int::<u64>(&mut writer, Cell::default().bits()).unwrap();

        io::write_int::<u32>(&mut writer, 5).unwrap();
        // Valid entry with one invalid scalar dropped from within it.
        io::write_int::<u16>(&mut writer, 0).unwrap();
        io::write_int::<u16>(&mut writer, 0).unwrap();
        io::write_int::<u16>(&mut writer, 4).unwrap();
        io::write_int::<u32>(&mut writer, 0x0301).unwrap();
        io::write_int::<u32>(&mut writer, 0).unwrap();
        io::write_int::<u32>(&mut writer, 0xD800).unwrap();
        io::write_int::<u32>(&mut writer, 0x0302).unwrap();
        // Duplicate entry for the same cell is consumed and dropped.
        io::write_int::<u16>(&mut writer, 0).unwrap();
        io::write_int::<u16>(&mut writer, 0).unwrap();
        io::write_int::<u16>(&mut writer, 1).unwrap();
        io::write_int::<u32>(&mut writer, 0x0303).unwrap();
        // Entry for the background cell is consumed and dropped.
        io::write_int::<u16>(&mut writer, 0).unwrap();
        io::write_int::<u16>(&mut writer, 2).unwrap();
        io::write_int::<u16>(&mut writer, 1).unwrap();
        io::write_int::<u32>(&mut writer, 0x0301).unwrap();
        // Entry for the empty cell is consumed and dropped.
        io::write_int::<u16>(&mut writer, 0).unwrap();
        io::write_int::<u16>(&mut writer, 3).unwrap();
        io::write_int::<u16>(&mut writer, 1).unwrap();
        io::write_int::<u32>(&mut writer, 0x0301).unwrap();
        // Entry outside the grid is consumed and dropped.
        io::write_int::<u16>(&mut writer, 7).unwrap();
        io::write_int::<u16>(&mut writer, 0).unwrap();
        io::write_int::<u16>(&mut writer, 1).unwrap();
        io::write_int::<u32>(&mut writer, 0x0301).unwrap();

        let buffered = writer.buffered().to_vec();
        let mut reader = io::FixedReader::new(&buffered);
        decode(&mut page, &mut reader, &style_remap, &hyperlink_remap).unwrap();
        page.verify_integrity().unwrap();

        let first = page.get_row_and_cell(0, 0);
        assert_eq!(
            &[0x0301u32, 0x0302][..],
            page.lookup_grapheme(first.cell).unwrap(),
        );
        unsafe {
            let second = *page.get_row_and_cell(1, 0).cell;
            assert_eq!('y' as u32, second.codepoint());
            assert!(!second.has_grapheme());
            assert!(!(*page.get_row_and_cell(2, 0).cell).has_grapheme());
            assert!(!(*page.get_row_and_cell(3, 0).cell).has_grapheme());
        }

        page.deinit();
    }

    // test "grid drops a complete grapheme when capacity fails mid-cluster"
    #[test]
    fn grid_drops_a_complete_grapheme_when_capacity_fails_mid_cluster() {
        let mut page = TerminalPage::init(Capacity {
            grapheme_bytes: 512,
            ..Capacity::new(4, 1)
        })
        .unwrap();
        let style_remap = StyleRemap::init();
        let hyperlink_remap = HyperlinkRemap::init();

        let mut payload = [0u8; 1024];
        let mut writer = io::FixedWriter::new(&mut payload);
        writer
            .write_byte(
                Row {
                    cell_width: EncodedWidth::Eight,
                    ..Row::default()
                }
                .bits(),
            )
            .unwrap();
        io::write_int::<u16>(&mut writer, 4).unwrap();
        for i in 0..4u32 {
            io::write_int::<u64>(
                &mut writer,
                Cell {
                    kind: 1,
                    content: 'x' as u32 + i,
                    ..Cell::default()
                }
                .bits(),
            )
            .unwrap();
        }

        // BitmapAllocator rounds this capacity to 64 four-codepoint chunks. The
        // first three cells consume 34 chunks. On the fourth cell, appending the
        // 61st suffix needs 16 new chunks while the old 15-chunk slice is still
        // live, exceeding capacity and forcing the complete suffix to be dropped.
        io::write_int::<u32>(&mut writer, 4).unwrap();
        for (x, count) in [64u16, 64, 8, 61].iter().enumerate() {
            io::write_int::<u16>(&mut writer, 0).unwrap();
            io::write_int::<u16>(&mut writer, x as u16).unwrap();
            io::write_int::<u16>(&mut writer, *count).unwrap();
            for i in 0..*count as u32 {
                io::write_int::<u32>(&mut writer, 0x0300 + i).unwrap();
            }
        }

        let buffered = writer.buffered().to_vec();
        let mut reader = io::FixedReader::new(&buffered);
        decode(&mut page, &mut reader, &style_remap, &hyperlink_remap).unwrap();
        page.verify_integrity().unwrap();

        let cell = page.get_row_and_cell(3, 0);
        unsafe {
            assert_eq!('x' as u32 + 3, (*cell.cell).codepoint());
            assert!(!(*cell.cell).has_grapheme());
            assert!((*cell.row).grapheme());
        }
        assert_eq!(3usize, page.grapheme_count());

        page.deinit();
    }

    // test "grid encodes rows at their narrowest width"
    #[test]
    fn grid_encodes_rows_at_their_narrowest_width() {
        let mut page = TerminalPage::init(Capacity {
            styles: 8,
            ..Capacity::new(2, 4)
        })
        .unwrap();

        // Width zero: bare Latin-1 text.
        unsafe {
            *page.get_row_and_cell(0, 0).cell = TerminalCell::init('A' as u32);
            *page.get_row_and_cell(1, 0).cell = TerminalCell::init(0xFF);

            // Width one: any BMP codepoint.
            *page.get_row_and_cell(0, 1).cell = TerminalCell::init(0x0100);
        }

        // Width two: a small style ID and a background color.
        let style_id = unsafe {
            page.styles
                .add(
                    page.memory_ptr(),
                    crate::style::Style {
                        flags: crate::style::Flags {
                            bold: true,
                            ..Default::default()
                        },
                        ..Default::default()
                    },
                )
                .unwrap()
        };
        assert!(style_id <= 63);
        let styled = page.get_row_and_cell(0, 2);
        unsafe {
            *styled.cell = TerminalCell::init('s' as u32);
            (*styled.cell).set_style_id(style_id);
            (*styled.row).set_styled(true);
        }
        let bg = page.get_row_and_cell(1, 2);
        unsafe {
            (*bg.cell).set_content_tag(terminal_page::ContentTag::BgColorPalette);
            (*bg.cell).set_content_color_palette(7);
        }

        // Width three: a protected cell.
        unsafe {
            (*page.get_row_and_cell(0, 3).cell).set_protected(true);
        }

        let mut encoded = [0u8; 128];
        let mut writer = io::FixedWriter::new(&mut encoded);
        encode(&page, &mut writer).unwrap();

        let bytes = writer.buffered().to_vec();
        assert_eq!(
            (3 + 2 * 1) + (3 + 1 * 2) + (3 + 2 * 4) + (3 + 1 * 8) + 4,
            bytes.len(),
        );

        // Each row header carries the expected width bits.
        assert_eq!(0u8 << 4, bytes[0] & 0x30);
        assert_eq!(1u8 << 4, bytes[5] & 0x30);
        assert_eq!(2u8 << 4, bytes[10] & 0x30);
        assert_eq!(3u8 << 4, bytes[21] & 0x30);

        let mut destination = TerminalPage::init(Capacity {
            styles: 8,
            ..Capacity::new(2, 4)
        })
        .unwrap();
        let mut style_remap = StyleRemap::init();
        let hyperlink_remap = HyperlinkRemap::init();
        let native_style = unsafe {
            destination
                .styles
                .add(
                    destination.memory_ptr(),
                    crate::style::Style {
                        flags: crate::style::Flags {
                            bold: true,
                            ..Default::default()
                        },
                        ..Default::default()
                    },
                )
                .unwrap()
        };
        style_remap.put(style_id, native_style);

        let mut reader = io::FixedReader::new(&bytes);
        decode(
            &mut destination,
            &mut reader,
            &style_remap,
            &hyperlink_remap,
        )
        .unwrap();
        destination.verify_integrity().unwrap();

        unsafe {
            assert_eq!(
                'A' as u32,
                (*destination.get_row_and_cell(0, 0).cell).codepoint(),
            );
            assert_eq!(
                0xFFu32,
                (*destination.get_row_and_cell(1, 0).cell).codepoint(),
            );
            assert_eq!(
                0x0100u32,
                (*destination.get_row_and_cell(0, 1).cell).codepoint(),
            );
        }
        let decoded_styled = destination.get_row_and_cell(0, 2);
        unsafe {
            assert_eq!('s' as u32, (*decoded_styled.cell).codepoint());
            assert_eq!(native_style, (*decoded_styled.cell).style_id());
            assert!((*decoded_styled.row).styled());
            assert_eq!(
                7u8,
                (*destination.get_row_and_cell(1, 2).cell).content_color_palette(),
            );
            assert!((*destination.get_row_and_cell(0, 3).cell).protected());
        }

        page.deinit();
        destination.deinit();
    }

    // test "grid decodes non-canonical cell widths"
    #[test]
    fn grid_decodes_non_canonical_cell_widths() {
        let mut page = TerminalPage::init(Capacity::new(2, 1)).unwrap();
        let style_remap = StyleRemap::init();
        let hyperlink_remap = HyperlinkRemap::init();

        // A bare ASCII row encoded at the full width is wasteful but valid.
        let mut payload = [0u8; 64];
        let mut writer = io::FixedWriter::new(&mut payload);
        writer
            .write_byte(
                Row {
                    cell_width: EncodedWidth::Eight,
                    ..Row::default()
                }
                .bits(),
            )
            .unwrap();
        io::write_int::<u16>(&mut writer, 2).unwrap();
        io::write_int::<u64>(
            &mut writer,
            Cell {
                content: 'o' as u32,
                ..Cell::default()
            }
            .bits(),
        )
        .unwrap();
        io::write_int::<u64>(
            &mut writer,
            Cell {
                content: 'k' as u32,
                ..Cell::default()
            }
            .bits(),
        )
        .unwrap();
        io::write_int::<u32>(&mut writer, 0).unwrap();

        let buffered = writer.buffered().to_vec();
        let mut reader = io::FixedReader::new(&buffered);
        decode(&mut page, &mut reader, &style_remap, &hyperlink_remap).unwrap();
        page.verify_integrity().unwrap();
        unsafe {
            assert_eq!('o' as u32, (*page.get_row_and_cell(0, 0).cell).codepoint());
            assert_eq!('k' as u32, (*page.get_row_and_cell(1, 0).cell).codepoint());
        }

        // Re-encoding canonicalizes the row back to the one-byte width.
        let mut reencoded = [0u8; 16];
        let mut rewriter = io::FixedWriter::new(&mut reencoded);
        encode(&page, &mut rewriter).unwrap();
        assert_eq!(3 + 2 + 4, rewriter.buffered().len());

        page.deinit();
    }

    // test "grid normalizes surrogates in two-byte cells"
    #[test]
    fn grid_normalizes_surrogates_in_two_byte_cells() {
        let mut page = TerminalPage::init(Capacity::new(2, 1)).unwrap();
        let style_remap = StyleRemap::init();
        let hyperlink_remap = HyperlinkRemap::init();

        let mut payload = [0u8; 16];
        let mut writer = io::FixedWriter::new(&mut payload);
        writer
            .write_byte(
                Row {
                    cell_width: EncodedWidth::Two,
                    ..Row::default()
                }
                .bits(),
            )
            .unwrap();
        io::write_int::<u16>(&mut writer, 2).unwrap();
        io::write_int::<u16>(&mut writer, 0xD800).unwrap();
        io::write_int::<u16>(&mut writer, 0x0416).unwrap();
        io::write_int::<u32>(&mut writer, 0).unwrap();

        let buffered = writer.buffered().to_vec();
        let mut reader = io::FixedReader::new(&buffered);
        decode(&mut page, &mut reader, &style_remap, &hyperlink_remap).unwrap();
        page.verify_integrity().unwrap();
        unsafe {
            assert_eq!(0xFFFDu32, (*page.get_row_and_cell(0, 0).cell).codepoint());
            assert_eq!(0x0416u32, (*page.get_row_and_cell(1, 0).cell).codepoint());
        }

        page.deinit();
    }

    // test "grid four-byte cells run full normalization"
    #[test]
    fn grid_four_byte_cells_run_full_normalization() {
        let mut page = TerminalPage::init(Capacity {
            styles: 8,
            ..Capacity::new(3, 1)
        })
        .unwrap();
        let style_remap = StyleRemap::init();
        let hyperlink_remap = HyperlinkRemap::init();

        let mut payload = [0u8; 32];
        let mut writer = io::FixedWriter::new(&mut payload);
        writer
            .write_byte(
                Row {
                    cell_width: EncodedWidth::Four,
                    ..Row::default()
                }
                .bits(),
            )
            .unwrap();
        io::write_int::<u16>(&mut writer, 3).unwrap();

        // The Kitty placeholder does not fit two-byte cells but fits here
        // and must still derive the native row hint.
        io::write_int::<u32>(
            &mut writer,
            Cell {
                content: KITTY_VIRTUAL_PLACEHOLDER,
                ..Cell::default()
            }
            .bits() as u32,
        )
        .unwrap();

        // An unknown small style reference degrades to the default style.
        io::write_int::<u32>(
            &mut writer,
            Cell {
                content: 'x' as u32,
                style_id: 63,
                ..Cell::default()
            }
            .bits() as u32,
        )
        .unwrap();

        // Reserved palette content bits are cleared at this width too.
        io::write_int::<u32>(
            &mut writer,
            Cell {
                kind: 2,
                content: 0xFFFF07,
                ..Cell::default()
            }
            .bits() as u32,
        )
        .unwrap();
        io::write_int::<u32>(&mut writer, 0).unwrap();

        let buffered = writer.buffered().to_vec();
        let mut reader = io::FixedReader::new(&buffered);
        decode(&mut page, &mut reader, &style_remap, &hyperlink_remap).unwrap();
        page.verify_integrity().unwrap();

        let first = page.get_row_and_cell(0, 0);
        unsafe {
            assert_eq!(KITTY_VIRTUAL_PLACEHOLDER, (*first.cell).codepoint());
            assert!((*first.row).kitty_virtual_placeholder());

            let second = *page.get_row_and_cell(1, 0).cell;
            assert_eq!('x' as u32, second.codepoint());
            assert_eq!(0 as TerminalStyleId, second.style_id());

            let third = *page.get_row_and_cell(2, 0).cell;
            assert_eq!(
                terminal_page::ContentTag::BgColorPalette,
                third.content_tag(),
            );
            assert_eq!(7u8, third.content_color_palette());
        }

        page.deinit();
    }
}
