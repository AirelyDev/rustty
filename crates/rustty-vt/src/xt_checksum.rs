//! DECRQCRA (Request Checksum of Rectangular Area) and XTCHECKSUM
//! (select the checksum variant DECRQCRA computes).
//!
//! The checksum follows xterm's `xtermCheckRect`. No real VT420 was
//! available to compare against while this was written, so xterm is the
//! reference:
//!
//! - `xtermCheckRect`: https://github.com/ThomasDickey/xterm-snapshots/blob/xterm-411/screen.c#L3162-L3290
//! - `xtermCharSetDec`: https://github.com/ThomasDickey/xterm-snapshots/blob/xterm-411/charsets.c#L608
//!
//! PORTS: ghostty src/terminal/xt_checksum.zig (9272a2f7c, 5f0d4febc,
//! b2f557c6c, cc0276778, 01b9f4958 end state at a60e9e2a5).

// const std = @import("std");
// const testing = std.testing;
// const PageList = @import("PageList.zig");
// const Screen = @import("Screen.zig");
// const Selection = @import("Selection.zig");
// const Terminal = @import("Terminal.zig");
// const pagepkg = @import("page.zig");
// const point = @import("point.zig");
// const style = @import("style.zig");
use crate::page as pagepkg;
use crate::pagelist::iter::Direction;
use crate::pagelist::pin::CellSubset;
use crate::pagelist::PageList;
use crate::point;
use crate::screen::Screen;
use crate::selection::Selection;
use crate::sgr;
use crate::style;
use crate::terminal::ScrollingRegion;

/// The XTCHECKSUM (CSI Ps # y) bits. The zero value is the DEC checksum,
/// which is also what a full reset restores.
///
/// These are xterm's `CSBITS`, which it also takes from the
/// `checksumExtension` resource:
///
/// - `CSBITS`: https://github.com/ThomasDickey/xterm-snapshots/blob/xterm-411/ptyx.h#L488-L496
/// - XTCHECKSUM: https://github.com/ThomasDickey/xterm-snapshots/blob/xterm-411/charproc.c#L6044-L6050
/// - Reset to `checksumExtension`: https://github.com/ThomasDickey/xterm-snapshots/blob/xterm-411/charproc.c#L14424-L14426
/// - `checksumExtension` docs: https://github.com/ThomasDickey/xterm-snapshots/blob/xterm-411/xterm.man#L2870-L2896
/// - XTCHECKSUM docs: https://github.com/ThomasDickey/xterm-snapshots/blob/xterm-411/ctlseqs.ms#L2562-L2569
///
/// xterm's documentation disagrees with its code in two places, and we
/// follow the code. The docs describe bit 3 as "omit checksum for cells
/// not explicitly initialized", but `csDRAWN` does the opposite: without
/// it those cells are skipped, and with it they count as spaces. The man
/// page also lists a bit 5, "do not mask cell value to 7 bits", which
/// `xtermCheckRect` never looks at.
// pub const Flags = packed struct(u5) {
//     positive: bool = false,
//     no_attributes: bool = false,
//     no_trim: bool = false,
//     undrawn: bool = false,
//     full: bool = false,
// };
//
// PORT NOTE (DECISIONS P8, R1 Q5): the `packed struct(u5)` is a
// `#[repr(transparent)]` `u8` newtype whose low 5 bits are the fields in
// declaration order (bit 0 = `positive` ... bit 4 = `full`), so it
// round-trips through `@bitCast` (`from_bits`/`bits`) exactly as the reference
// does for the action's C value, the C option and the XTCHECKSUM parameter.
// The upper 3 bits are always zero (the `u5` invariant). The reference field
// defaults (all `false`) are `Flags::default()`; each field is a const
// accessor plus a `with_*` const builder standing in for a `.{ .x = true }`
// literal.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Flags(u8);

impl Flags {
    const POSITIVE: u8 = 1 << 0;
    const NO_ATTRIBUTES: u8 = 1 << 1;
    const NO_TRIM: u8 = 1 << 2;
    const UNDRAWN: u8 = 1 << 3;
    const FULL: u8 = 1 << 4;

    /// The mask of the `u5` backing integer.
    const MASK: u8 = 0x1F;

    /// `@as(Flags, @bitCast(@as(u5, bits)))`.
    ///
    /// The argument is a `u5`; a value outside of 5 bits is a caller bug (the
    /// reference cannot even express it), so it is a debug assertion, and the
    /// value is masked so release builds keep the `u5` invariant. Callers that
    /// want the reference's `@truncate` mask first.
    #[inline]
    pub const fn from_bits(bits: u8) -> Flags {
        debug_assert!(bits <= Self::MASK);
        Flags(bits & Self::MASK)
    }

    /// `@as(u5, @bitCast(flags))`.
    #[inline]
    pub const fn bits(self) -> u8 {
        self.0
    }

    #[inline]
    const fn get(self, bit: u8) -> bool {
        self.0 & bit != 0
    }

    #[inline]
    const fn with(self, bit: u8, v: bool) -> Flags {
        if v {
            Flags(self.0 | bit)
        } else {
            Flags(self.0 & !bit)
        }
    }

    /// Don't negate the result.
    // positive: bool = false,
    #[inline]
    pub const fn positive(self) -> bool {
        self.get(Self::POSITIVE)
    }

    /// Don't add the VT100 video attributes to each cell.
    // no_attributes: bool = false,
    #[inline]
    pub const fn no_attributes(self) -> bool {
        self.get(Self::NO_ATTRIBUTES)
    }

    /// Don't omit blanks. The DEC checksum only counts a plain space if it
    /// is the first cell of the rectangle.
    // no_trim: bool = false,
    #[inline]
    pub const fn no_trim(self) -> bool {
        self.get(Self::NO_TRIM)
    }

    /// Count cells that were never written to as spaces instead of
    /// skipping them.
    // undrawn: bool = false,
    #[inline]
    pub const fn undrawn(self) -> bool {
        self.get(Self::UNDRAWN)
    }

    /// Use the full codepoint rather than the DEC 8-bit value. Wide
    /// spacers are skipped and, as in xterm, combining marks are not
    /// counted in this mode.
    // full: bool = false,
    #[inline]
    pub const fn full(self) -> bool {
        self.get(Self::FULL)
    }

    /// `flags.positive = v`
    #[inline]
    pub const fn with_positive(self, v: bool) -> Flags {
        self.with(Self::POSITIVE, v)
    }

    /// `flags.no_attributes = v`
    #[inline]
    pub const fn with_no_attributes(self, v: bool) -> Flags {
        self.with(Self::NO_ATTRIBUTES, v)
    }

    /// `flags.no_trim = v`
    #[inline]
    pub const fn with_no_trim(self, v: bool) -> Flags {
        self.with(Self::NO_TRIM, v)
    }

    /// `flags.undrawn = v`
    #[inline]
    pub const fn with_undrawn(self, v: bool) -> Flags {
        self.with(Self::UNDRAWN, v)
    }

    /// `flags.full = v`
    #[inline]
    pub const fn with_full(self, v: bool) -> Flags {
        self.with(Self::FULL, v)
    }
}

/// A DECRQCRA request as it arrives in `CSI Pi ; Pg ; Pt ; Pl ; Pb ; Pr * y`.
/// The coordinates are 1-based and zero means the parameter was omitted.
/// The page number (Pg) is ignored since we only have one page.
// pub const Request = extern struct {
//     id: u16 = 0,
//     top: u16 = 0,
//     left: u16 = 0,
//     bottom: u16 = 0,
//     right: u16 = 0,
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Request {
    pub id: u16,
    pub top: u16,
    pub left: u16,
    pub bottom: u16,
    pub right: u16,
}

impl Request {
    /// Resolve the request to a rectangle selection of the active area.
    /// If `origin` is set (DECOM), coordinates are relative to its
    /// top-left, as in xterm. Coordinates are clamped to the screen.
    /// Returns null if the rectangle is empty.
    pub fn selection(
        &self,
        pages: &PageList,
        origin: Option<ScrollingRegion>,
    ) -> Option<Selection> {
        let rows = pages.rows;
        let cols = pages.cols;
        if rows == 0 || cols == 0 {
            return None;
        }
        let top_margin: u16 = if let Some(o) = origin { o.top } else { 0 };
        let left_margin: u16 = if let Some(o) = origin { o.left } else { 0 };
        let top = limit(self.top, 1, top_margin, rows);
        let left = limit(self.left, 1, left_margin, cols);
        let bottom = limit(self.bottom, rows, top_margin, rows);
        let right = limit(self.right, cols, left_margin, cols);
        if top > bottom || left > right {
            return None;
        }
        // return .init(
        //     pages.pin(.{ .active = .{ .x = left - 1, .y = top - 1 } }) orelse return null,
        //     pages.pin(.{ .active = .{ .x = right - 1, .y = bottom - 1 } }) orelse return null,
        //     true,
        // );
        Some(Selection::init(
            pages.pin(point::Point::Active(point::Coordinate {
                x: left - 1,
                y: u32::from(top - 1),
            }))?,
            pages.pin(point::Point::Active(point::Coordinate {
                x: right - 1,
                y: u32::from(bottom - 1),
            }))?,
            true,
        ))
    }
}

// fn limit(v: u16, default: u16, margin: u16, max: u16) u16 {
//     const n = if (v == 0) default else v;
//     return std.math.clamp(n +| margin, 1, max);
// }
//
// PORT NOTE: `selection` returns before calling this when `max` is zero, so
// `clamp`'s `min <= max` precondition holds (`1 <= max`); `+|` is
// `saturating_add`.
#[inline]
fn limit(v: u16, default: u16, margin: u16, max: u16) -> u16 {
    let n = if v == 0 { default } else { v };
    n.saturating_add(margin).clamp(1, max)
}

/// Compute the checksum of a rectangle selection.
//
// PORT NOTE (page-access discipline, R1 §2.5): the cells are read through the
// raw `*mut [Cell]` `Pin::cells` returns, one `Cell` copied out at a time; no
// `&[Cell]` is formed, so nothing borrowed from the page is live across
// `Pin::style`/`Pin::grapheme`, which re-derive the page from the node. The
// rectangle is always in the active area, whose pages are never compressed.
pub fn compute(screen: &Screen, sel_: Option<Selection>, flags: Flags) -> u16 {
    // An empty rectangle still gets a reply, with a sum of zero.
    let Some(sel) = sel_ else { return 0 };
    let tl = sel.top_left(screen);
    let br = sel.bottom_right(screen);

    // Everything is summed modulo 2^16 since only 16 bits are reported.
    let mut sum: u16 = 0;

    // The DEC checksum omits plain spaces except for the very first cell
    // counted in the rectangle.
    let mut first = true;

    let mut it = tl.row_iterator(Direction::RightDown, Some(br));
    while let Some(row) = it.next() {
        // Pages may be narrower than the rectangle, as in xterm, where
        // a line may be shorter than the screen.
        let cells: *mut [pagepkg::Cell] = row.cells(CellSubset::All);
        let len = cells.len();
        let left = core::cmp::min(tl.x as usize, len);
        let right = core::cmp::min(br.x as usize + 1, len);

        // for (cells[left..right]) |*cell| {
        for i in left..right {
            // SAFETY: `i < right <= len`, the length of the row's cell slice
            // `Pin::cells` returned for a live node of `screen.pages`.
            let cell_ptr: *const pagepkg::Cell = unsafe { (cells as *const pagepkg::Cell).add(i) };
            // SAFETY: `cell_ptr` is in bounds (above) and the page is not
            // mutated during this `&Screen` read.
            let cell: pagepkg::Cell = unsafe { core::ptr::read(cell_ptr) };

            let mut ch: u16 = match value(&cell, flags) {
                Value::Skip => continue,
                Value::Undrawn => {
                    if flags.no_trim() || flags.undrawn() {
                        u16::from(b' ')
                    } else {
                        continue;
                    }
                }
                Value::Value(v) => v,
            };

            let s = row.style(cell_ptr);
            if !flags.no_attributes() {
                ch = ch.wrapping_add(attributes(&cell, &s));
            }

            if flags.no_trim() {
                sum = sum.wrapping_add(ch);

                // xterm adds combining marks only in the DEC mode, and
                // only to the untrimmed sum.
                if !flags.full() && cell.has_grapheme() {
                    if let Some(cps) = row.grapheme(cell_ptr) {
                        // for (cps) |cp| sum +%= @truncate(cp);
                        for &cp in cps {
                            sum = sum.wrapping_add(cp as u16);
                        }
                    }
                }
            } else if first || ch != u16::from(b' ') || extended(&s) {
                sum = sum.wrapping_add(ch);
            }

            first = flags.no_trim();
        }

        if !flags.no_trim() {
            first = false;
        }
    }

    // return if (flags.positive) sum else 0 -% sum;
    if flags.positive() {
        sum
    } else {
        0u16.wrapping_sub(sum)
    }
}

// const Value = union(enum) { ... };
enum Value {
    /// Not counted at all.
    Skip,

    /// Never written to.
    Undrawn,

    /// The value of the character, before attributes.
    Value(u16),
}

fn value(cell: &pagepkg::Cell, flags: Flags) -> Value {
    // The right half of a wide character. xterm stores a placeholder
    // outside of 8 bits there, so the DEC checksum counts it as ESC.
    if cell.wide() == pagepkg::Wide::SpacerTail {
        return if flags.full() {
            Value::Skip
        } else {
            Value::Value(0x1B)
        };
    }

    if !cell.has_text() {
        return Value::Undrawn;
    }

    let cp = cell.codepoint();
    // if (flags.full) return .{ .value = @truncate(cp) };
    if flags.full() {
        return Value::Value(cp as u16);
    }

    // The DEC 8-bit value, as xterm's `xtermCharSetDec` produces it for
    // the ASCII character set. xterm works from the byte as it was
    // received, before character set translation, so it counts DEC line
    // drawing as the letter that was sent. We only have the translated
    // codepoint, so those count as characters outside of 8 bits.
    Value::Value(match cp {
        0x7F | 0xFF => 0,
        // => @intCast(cp & 0x7F),
        0x20..=0x7E | 0x80..=0x9F | 0xA1..=0xFE => (cp & 0x7F) as u16,
        _ => 0x1B,
    })
}

/// The VT100 video attributes xterm adds to each cell.
fn attributes(cell: &pagepkg::Cell, s: &style::Style) -> u16 {
    let mut result: u16 = 0;
    if cell.protected() {
        result += 0x04;
    }
    if s.flags.invisible {
        result += 0x08;
    }
    if s.flags.underline != sgr::Underline::None {
        result += 0x10;
    }
    if s.flags.inverse {
        result += 0x20;
    }
    if s.flags.blink {
        result += 0x40;
    }
    if s.flags.bold {
        result += 0x80;
    }
    result
}

/// Attributes that xterm doesn't add to the sum but that still keep a
/// space from being trimmed.
fn extended(s: &style::Style) -> bool {
    s.flags.faint
        || s.flags.italic
        || s.flags.strikethrough
        || s.flags.underline == sgr::Underline::Double
}

/// The largest reply `encode` writes.
// pub const max_encode_size = "\x1bP65535!~FFFF\x1b\\".len;
pub const MAX_ENCODE_SIZE: usize = b"\x1bP65535!~FFFF\x1b\\".len();

/// Encode the DECRQCRA reply, `DCS Pi ! ~ XXXX ST`.
//
// PORT NOTE: `*std.Io.Writer` maps to `core::fmt::Write` (the crate's
// `FixedWriter` is the `std.Io.Writer.fixed` stand-in); `{X:0>4}` is `{:04X}`.
pub fn encode<W: core::fmt::Write + ?Sized>(
    writer: &mut W,
    id: u16,
    sum: u16,
) -> core::fmt::Result {
    // try writer.print("\x1bP{d}!~{X:0>4}\x1b\\", .{ id, sum });
    write!(writer, "\x1bP{id}!~{sum:04X}\x1b\\")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::alloc::Global;
    use crate::fixed_writer::FixedWriter;
    use crate::terminal::{Options, Terminal};

    // PORT NOTE: the reference tests call `Terminal.init(.., .{ .cols, .rows })`
    // and rely on `Options` field defaults; Rust `Options` has none, so this
    // supplies them verbatim (as `terminal/tests.rs`'s `opts` does).
    fn opts(cols: crate::size::CellCountInt, rows: crate::size::CellCountInt) -> Options {
        Options {
            cols,
            rows,
            max_scrollback_bytes: Some(10_000),
            max_scrollback_lines: None,
            colors: crate::terminal::Colors {
                background: crate::color::DynamicRGB::UNSET,
                foreground: crate::color::DynamicRGB::UNSET,
                cursor: crate::color::DynamicRGB::UNSET,
                palette: crate::color::DynamicPalette::default_palette(),
            },
            default_modes: crate::modes::ModePacked::default(),
            default_cursor_style: crate::screen::CursorStyle::Block,
            default_cursor_blink: Some(false),
            default_xt_checksum: Flags::default(),
            kitty_image_storage_limit: 320 * 1000 * 1000,
            kitty_image_loading_limits: crate::kitty::graphics_image::Limits::DIRECT,
        }
    }

    // PORT NOTE (SB discipline): the reference binds `s = t.screens.active`
    // once and keeps using it across `t.printString`. Here `s` is the raw
    // `*mut Screen` and a `&Screen` is formed only for each read, after the
    // mutations, so no shared reference is live across a write through `t`.
    fn screen<'a>(s: *mut Screen) -> &'a Screen {
        // SAFETY: `s` is the terminal's active screen, alive until `deinit`.
        unsafe { &*s }
    }

    fn active(x: crate::size::CellCountInt, y: u32) -> point::Point {
        point::Point::Active(point::Coordinate { x, y })
    }

    // test "xt_checksum: encode"
    #[test]
    fn xt_checksum_encode() {
        let mut buf = [0u8; MAX_ENCODE_SIZE];
        let mut writer = FixedWriter::new(&mut buf);
        encode(&mut writer, 65535, 0xFFFF).unwrap();
        assert_eq!(b"\x1bP65535!~FFFF\x1b\\".as_slice(), writer.buffered());

        let mut writer = FixedWriter::new(&mut buf);
        encode(&mut writer, 1, 0x1A).unwrap();
        assert_eq!(b"\x1bP1!~001A\x1b\\".as_slice(), writer.buffered());
    }

    // test "xt_checksum: selection defaults and clamping"
    #[test]
    fn xt_checksum_selection_defaults_and_clamping() {
        let mut t = Terminal::init(Global, opts(10, 5)).unwrap();
        let pages = &screen(t.screens.active).pages;

        {
            let sel = Request::default().selection(pages, None).unwrap();
            assert!(sel.rectangle);
            assert_eq!(
                active(0, 0),
                pages
                    .point_from_pin(point::Tag::Active, sel.start())
                    .unwrap(),
            );
            assert_eq!(
                active(9, 4),
                pages.point_from_pin(point::Tag::Active, sel.end()).unwrap(),
            );
        }

        {
            let req = Request {
                top: 2,
                left: 3,
                bottom: 99,
                right: 99,
                ..Request::default()
            };
            let sel = req.selection(pages, None).unwrap();
            assert_eq!(
                active(2, 1),
                pages
                    .point_from_pin(point::Tag::Active, sel.start())
                    .unwrap(),
            );
            assert_eq!(
                active(9, 4),
                pages.point_from_pin(point::Tag::Active, sel.end()).unwrap(),
            );
        }

        // try testing.expectEqual(null, ...)
        assert!(Request {
            top: 3,
            bottom: 2,
            ..Request::default()
        }
        .selection(pages, None)
        .is_none());
        assert!(Request {
            left: 3,
            right: 2,
            ..Request::default()
        }
        .selection(pages, None)
        .is_none());

        t.deinit(Global);
    }

    // test "xt_checksum: selection origin mode"
    #[test]
    fn xt_checksum_selection_origin_mode() {
        let mut t = Terminal::init(Global, opts(10, 5)).unwrap();
        let pages = &screen(t.screens.active).pages;
        let region = ScrollingRegion {
            top: 1,
            bottom: 3,
            left: 2,
            right: 8,
        };

        {
            let sel = Request::default().selection(pages, Some(region)).unwrap();
            assert_eq!(
                active(2, 1),
                pages
                    .point_from_pin(point::Tag::Active, sel.start())
                    .unwrap(),
            );
            assert_eq!(
                active(9, 4),
                pages.point_from_pin(point::Tag::Active, sel.end()).unwrap(),
            );
        }

        {
            let req = Request {
                top: 1,
                left: 1,
                bottom: 1,
                right: 1,
                ..Request::default()
            };
            let sel = req.selection(pages, Some(region)).unwrap();
            assert_eq!(
                active(2, 1),
                pages
                    .point_from_pin(point::Tag::Active, sel.start())
                    .unwrap(),
            );
            assert_eq!(
                active(2, 1),
                pages.point_from_pin(point::Tag::Active, sel.end()).unwrap(),
            );
        }

        t.deinit(Global);
    }

    // test "xt_checksum: DEC trims blanks and skips undrawn cells"
    #[test]
    fn xt_checksum_dec_trims_blanks_and_skips_undrawn_cells() {
        let mut t = Terminal::init(Global, opts(10, 5)).unwrap();
        let s = t.screens.active;
        t.print_string(b"a b").unwrap();

        // Only the first row, the whole width. The space is omitted and the
        // unwritten cells are skipped.
        let req = Request {
            top: 1,
            bottom: 1,
            ..Request::default()
        };
        let sel = req.selection(&screen(s).pages, None);
        assert_eq!(
            0u16.wrapping_sub(u16::from(b'a') + u16::from(b'b')),
            compute(screen(s), sel, Flags::default()),
        );
        assert_eq!(
            u16::from(b'a') + u16::from(b'b'),
            compute(screen(s), sel, Flags::default().with_positive(true)),
        );

        // Untrimmed counts the space and the undrawn cells as spaces.
        assert_eq!(
            u16::from(b'a') + u16::from(b'b') + u16::from(b' ') * 8,
            compute(
                screen(s),
                sel,
                Flags::default().with_positive(true).with_no_trim(true),
            ),
        );

        // Undrawn cells are spaces, which are then trimmed.
        assert_eq!(
            u16::from(b'a') + u16::from(b'b'),
            compute(
                screen(s),
                sel,
                Flags::default().with_positive(true).with_undrawn(true),
            ),
        );

        t.deinit(Global);
    }

    // test "xt_checksum: DEC counts a leading space"
    #[test]
    fn xt_checksum_dec_counts_a_leading_space() {
        let mut t = Terminal::init(Global, opts(10, 5)).unwrap();
        let s = t.screens.active;
        t.print_string(b" a\n b").unwrap();

        // The very first counted cell is kept even if it's a space, but not
        // the first cell of later rows.
        let req = Request::default();
        let sel = req.selection(&screen(s).pages, None);
        assert_eq!(
            u16::from(b' ') + u16::from(b'a') + u16::from(b'b'),
            compute(screen(s), sel, Flags::default().with_positive(true)),
        );

        t.deinit(Global);
    }

    // test "xt_checksum: empty rectangle"
    #[test]
    fn xt_checksum_empty_rectangle() {
        let mut t = Terminal::init(Global, opts(10, 5)).unwrap();
        let s = t.screens.active;
        t.print_string(b"abc").unwrap();
        let req = Request {
            left: 3,
            right: 2,
            ..Request::default()
        };
        assert_eq!(
            0,
            compute(
                screen(s),
                req.selection(&screen(s).pages, None),
                Flags::default(),
            ),
        );

        t.deinit(Global);
    }

    // test "xt_checksum: rectangle bounds"
    #[test]
    fn xt_checksum_rectangle_bounds() {
        let mut t = Terminal::init(Global, opts(10, 5)).unwrap();
        let s = t.screens.active;
        t.print_string(b"abc\ndef\nghi").unwrap();
        let req = Request {
            top: 2,
            left: 2,
            bottom: 3,
            right: 3,
            ..Request::default()
        };
        let sel = req.selection(&screen(s).pages, None);
        assert_eq!(
            u16::from(b'e') + u16::from(b'f') + u16::from(b'h') + u16::from(b'i'),
            compute(screen(s), sel, Flags::default().with_positive(true)),
        );

        t.deinit(Global);
    }

    // test "xt_checksum: attributes"
    #[test]
    fn xt_checksum_attributes() {
        let mut t = Terminal::init(Global, opts(10, 5)).unwrap();
        let s = t.screens.active;
        t.set_attribute(sgr::Attribute::Bold).unwrap();
        t.set_attribute(sgr::Attribute::Underline(sgr::Underline::Single))
            .unwrap();
        t.print_string(b"a").unwrap();
        t.set_attribute(sgr::Attribute::Unset).unwrap();
        t.set_attribute(sgr::Attribute::Inverse).unwrap();
        t.set_attribute(sgr::Attribute::Blink).unwrap();
        t.print_string(b" ").unwrap();
        t.set_attribute(sgr::Attribute::Unset).unwrap();
        t.set_attribute(sgr::Attribute::Invisible).unwrap();
        t.set_protected_mode(crate::ansi::ProtectedMode::Dec);
        t.print_string(b"b").unwrap();

        let req = Request {
            top: 1,
            bottom: 1,
            ..Request::default()
        };
        let sel = req.selection(&screen(s).pages, None);
        assert_eq!(
            u16::from(b'a')
                + 0x80
                + 0x10
                + u16::from(b' ')
                + 0x20
                + 0x40
                + u16::from(b'b')
                + 0x08
                + 0x04,
            compute(screen(s), sel, Flags::default().with_positive(true)),
        );
        assert_eq!(
            u16::from(b'a') + u16::from(b'b'),
            compute(
                screen(s),
                sel,
                Flags::default()
                    .with_positive(true)
                    .with_no_attributes(true),
            ),
        );

        t.deinit(Global);
    }

    // test "xt_checksum: extended attributes keep a space"
    #[test]
    fn xt_checksum_extended_attributes_keep_a_space() {
        let mut t = Terminal::init(Global, opts(10, 5)).unwrap();
        let s = t.screens.active;
        t.print_string(b"a").unwrap();
        t.set_attribute(sgr::Attribute::Italic).unwrap();
        t.print_string(b" ").unwrap();

        let req = Request {
            top: 1,
            bottom: 1,
            ..Request::default()
        };
        let sel = req.selection(&screen(s).pages, None);
        assert_eq!(
            u16::from(b'a') + u16::from(b' '),
            compute(screen(s), sel, Flags::default().with_positive(true)),
        );

        t.deinit(Global);
    }

    // test "xt_checksum: DEC 8-bit values"
    #[test]
    fn xt_checksum_dec_8_bit_values() {
        let mut t = Terminal::init(Global, opts(10, 5)).unwrap();
        let s = t.screens.active;

        // é is 0xE9, masked to 7 bits; the euro sign is outside of 8 bits.
        t.print_string("é€".as_bytes()).unwrap();
        let req = Request {
            top: 1,
            bottom: 1,
            ..Request::default()
        };
        let sel = req.selection(&screen(s).pages, None);
        assert_eq!(
            0x69u16 + 0x1B,
            compute(screen(s), sel, Flags::default().with_positive(true)),
        );
        assert_eq!(
            0xE9u16 + 0x20AC,
            compute(
                screen(s),
                sel,
                Flags::default().with_positive(true).with_full(true),
            ),
        );

        t.deinit(Global);
    }

    // test "xt_checksum: wide characters"
    #[test]
    fn xt_checksum_wide_characters() {
        let mut t = Terminal::init(Global, opts(10, 5)).unwrap();
        let s = t.screens.active;
        t.print_string("橋".as_bytes()).unwrap();

        let req = Request {
            top: 1,
            bottom: 1,
            ..Request::default()
        };
        let sel = req.selection(&screen(s).pages, None);
        assert_eq!(
            0x1Bu16 + 0x1B,
            compute(screen(s), sel, Flags::default().with_positive(true)),
        );
        assert_eq!(
            0x6A4Bu16,
            compute(
                screen(s),
                sel,
                Flags::default().with_positive(true).with_full(true),
            ),
        );

        t.deinit(Global);
    }

    // test "xt_checksum: combining marks"
    #[test]
    fn xt_checksum_combining_marks() {
        let mut t = Terminal::init(Global, opts(10, 5)).unwrap();
        let s = t.screens.active;
        t.modes.set(crate::modes::Mode::GraphemeCluster, true);
        t.print_string("e\u{301}".as_bytes()).unwrap();

        let req = Request {
            top: 1,
            bottom: 1,
            right: 1,
            ..Request::default()
        };
        let sel = req.selection(&screen(s).pages, None);
        assert_eq!(
            u16::from(b'e'),
            compute(screen(s), sel, Flags::default().with_positive(true)),
        );
        assert_eq!(
            u16::from(b'e') + 0x301,
            compute(
                screen(s),
                sel,
                Flags::default().with_positive(true).with_no_trim(true),
            ),
        );
        assert_eq!(
            u16::from(b'e'),
            compute(
                screen(s),
                sel,
                Flags::default()
                    .with_positive(true)
                    .with_no_trim(true)
                    .with_full(true),
            ),
        );

        t.deinit(Global);
    }
}
