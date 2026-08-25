//! TERMINAL record payload encoding.
//!
//! One TERMINAL record contains terminal-wide state shared by every screen. It
//! is the first record in a snapshot and declares which SCREEN sequences follow
//! it. Screen contents, cursors, and history are encoded by `screen.zig` and
//! `history.zig`.
//!
//! Snapshot version 1 supports the primary screen and an optional alternate
//! screen. `screen_count` is therefore one or two. SCREEN records identify
//! their destination by key and may appear in either order. Canonical encoders
//! write primary first, then alternate when present. `active_screen_key` must
//! name one of those declared screens.
//!
//! All integers are unsigned and little-endian.
//!
//! ## Binary Format
//!
//! The payload begins with a fixed header followed by terminal-wide variable
//! state. The enclosing record supplies the payload boundary.
//!
//! ```text
//!   0 +--------------------------+
//!     | Header                   |
//! 103 +--------------------------+
//!     | Tab stops                |
//!     | ceil(columns / 8) bytes  |
//!     +--------------------------+
//!     | Original palette         |
//!     | 256 * RGB                |
//!     +--------------------------+
//!     | Palette override mask    |
//!     | 32 bytes                 |
//!     +--------------------------+
//!     | Palette overrides        |
//!     | 3 * popcount(mask) bytes |
//!     +--------------------------+
//!     | PWD length (u32)         |
//!     +--------------------------+
//!     | PWD bytes                |
//!     +--------------------------+
//!     | Title length (u32)       |
//!     +--------------------------+
//!     | Title bytes              |
//! end +--------------------------+
//! ```
//!
//! ### Header
//!
//! ```text
//!   0 +-----------------------------------+
//!     | Columns (u16)                     |
//!   2 +-----------------------------------+
//!     | Rows (u16)                        |
//!   4 +-----------------------------------+
//!     | Pixel width (u32)                 |
//!   8 +-----------------------------------+
//!     | Pixel height (u32)                |
//!  12 +-----------------------------------+
//!     | Scrolling-region top (u16)        |
//!  14 +-----------------------------------+
//!     | Scrolling-region bottom (u16)     |
//!  16 +-----------------------------------+
//!     | Scrolling-region left (u16)       |
//!  18 +-----------------------------------+
//!     | Scrolling-region right (u16)      |
//!  20 +-----------------------------------+
//!     | Status display (u8)               |
//!  21 +-----------------------------------+
//!     | Active screen key (u16)           |
//!  23 +-----------------------------------+
//!     | Screen count (u16)                |
//!  25 +-----------------------------------+
//!     | Previous codepoint (u32)          |
//!  29 +-----------------------------------+
//!     | Cursor is-default (u8)            |
//!  30 +-----------------------------------+
//!     | Cursor default visual style (u8)  |
//!  31 +-----------------------------------+
//!     | Cursor default blink policy (u8)  |
//!  32 +-----------------------------------+
//!     | Shell-redraw policy (u8)          |
//!  33 +-----------------------------------+
//!     | Modify-other-keys level 2 (u8)    |
//!  34 +-----------------------------------+
//!     | Mouse event (u8)                  |
//!  35 +-----------------------------------+
//!     | Mouse format (u8)                 |
//!  36 +-----------------------------------+
//!     | Mouse shift-capture policy (u8)   |
//!  37 +-----------------------------------+
//!     | Mouse shape (u8)                  |
//!  38 +-----------------------------------+
//!     | Password-input flag (u8)          |
//!  39 +-----------------------------------+
//!     | Current modes (ModePacked)        |
//!  47 +-----------------------------------+
//!     | Saved modes (ModePacked)          |
//!  55 +-----------------------------------+
//!     | Default modes (ModePacked)        |
//!  63 +-----------------------------------+
//!     | Background color (DynamicRGB)     |
//!  71 +-----------------------------------+
//!     | Foreground color (DynamicRGB)     |
//!  79 +-----------------------------------+
//!     | Cursor color (DynamicRGB)         |
//!  87 +-----------------------------------+
//!     | Maximum scrollback bytes (u64)    |
//!  95 +-----------------------------------+
//!     | Maximum scrollback rows (u64)     |
//! 103 +-----------------------------------+
//! ```
//!
//! The previous codepoint is a Unicode scalar value or `0xffffffff` when there
//! is no previous character. Boolean fields are zero or one.
//! Cursor blink and mouse shift-capture policies encode null as zero, false as
//! one, and true as two.
//! Canonical encoders use only the documented enum and boolean values.
//! Decoders replace unknown semantic values with neutral native defaults,
//! discard invalid previous codepoints, and reset each invalid scrolling axis
//! to the terminal's full extent. Dimensions and screen count remain
//! structural.
//!
//! A scrollback limit of `0xffffffffffffffff` means unlimited. Every other
//! value is finite, including zero. The values are the source primary
//! PageList's explicit policies, not its dimension-adjusted effective limits.
//! Alternate-screen no-scrollback behavior is derived from the screen key.
//!
//! ### Tab stops
//!
//! One bit is encoded for each column, least-significant bit first within each
//! byte. Bit zero of the first byte is column zero. Unused high bits in the last
//! byte are canonically zero and ignored by decoders.
//!
//! ### Dynamic RGB
//!
//! The three terminal dynamic colors share this layout:
//!
//! ```text
//! 0 +----------------------------+
//!   | Default present (u8)       |
//! 1 +----------------------------+
//!   | Default RGB                |
//! 4 +----------------------------+
//!   | Override present (u8)      |
//! 5 +----------------------------+
//!   | Override RGB               |
//! 8 +----------------------------+
//! ```
//!
//! Presence values are zero or one. The corresponding RGB bytes must be zero
//! when a value is absent. Decoders treat every other presence value as absent
//! and ignore RGB bytes belonging to an absent value.
//!
//! ### Palette
//!
//! The complete original 256-color palette is encoded first in index order.
//! Each mask bit then states whether the current palette entry overrides that
//! original value. Bit zero of the first mask byte is palette index zero.
//! Override RGB values follow in increasing index order with no index or count
//! fields.
//!
//! ### Modes
//!
//! Current, saved, and default modes use the same stable bit registry:
//!
//! ```text
//! bit  0  disable_keyboard
//! bit  1  insert
//! bit  2  send_receive_mode
//! bit  3  linefeed
//! bit  4  cursor_keys
//! bit  5  132_column
//! bit  6  slow_scroll
//! bit  7  reverse_colors
//! bit  8  origin
//! bit  9  wraparound
//! bit 10  autorepeat
//! bit 11  mouse_event_x10
//! bit 12  cursor_blinking
//! bit 13  cursor_visible
//! bit 14  enable_mode_3
//! bit 15  reverse_wrap
//! bit 16  alt_screen_legacy
//! bit 17  keypad_keys
//! bit 18  backarrow_key_mode
//! bit 19  enable_left_and_right_margin
//! bit 20  mouse_event_normal
//! bit 21  mouse_event_button
//! bit 22  mouse_event_any
//! bit 23  focus_event
//! bit 24  mouse_format_utf8
//! bit 25  mouse_format_sgr
//! bit 26  mouse_alternate_scroll
//! bit 27  mouse_format_urxvt
//! bit 28  mouse_format_sgr_pixels
//! bit 29  ignore_keypad_with_numlock
//! bit 30  alt_esc_prefix
//! bit 31  alt_sends_escape
//! bit 32  reverse_wrap_extended
//! bit 33  alt_screen
//! bit 34  save_cursor
//! bit 35  alt_screen_save_cursor_clear_enter
//! bit 36  bracketed_paste
//! bit 37  synchronized_output
//! bit 38  grapheme_cluster
//! bit 39  report_color_scheme
//! bit 40  report_visibility
//! bit 41  in_band_size_reports
//! bits 42-63  reserved, zero
//! ```
//!
//! This is the packed field order of native `ModePacked`. Its layout is
//! well-defined and is the snapshot registry. Moving or adding a native mode
//! therefore requires a snapshot version bump. Decoders ignore reserved bits.
//!
//! ## Field classification
//!
//! The TERMINAL payload encodes terminal dimensions, pixel dimensions,
//! scrolling region, status display, tab stops, PWD, title, color state,
//! previous character, all three mode sets, cursor defaults, mouse behavior,
//! password-input state, source scrollback policies, the active screen key, and
//! the set of declared screens.
//!
//! SCREEN and HISTORY sequences encode each Screen's pages, cursor, saved
//! cursor, charset, protected mode, Kitty keyboard stack, semantic-click state,
//! and complete history. The semantic-prompt `seen` bit is derived from
//! restored resident content.
//!
//! Native allocators, I/O implementations, pools, pointers, page IDs, byte
//! accounting, tracked pins, and ScreenSet generations are reconstructed.
//! PageList row totals and the active viewport are derived from decoded pages.
//! The destination surface supplies its own focus state. Selections, scrolled
//! viewports, selection-scroll activity, dirty flags, search flags, hyperlink
//! hover state, and incremental compression state are presentation or cache
//! state and reset during restore.
//!
//! Kitty image state and glyph glossary registrations are unsupported by this
//! snapshot version and are ignored during capture. Unicode virtual placeholder
//! cells are preserved as grid content, but their image and placement state is
//! not restored. Build-time terminal behavior, Unicode width policy, parser
//! continuation, and external callbacks are version-level or caller-local
//! requirements.
//!
//! Native enum declaration indices and mode bit positions used by this format
//! are snapshot-version registries. Changing any of them requires a snapshot
//! version bump.
//!
//! PORTS: ghostty src/terminal/snapshot/terminal.zig (phase-4 wsnap-3;
//! complete — assessment B cluster C4's terminal half, END STATE incl.
//! 9d1c6a921 terminal robustness and e2e74fecb's history-size-hint move to
//! SCREEN).

// const std = @import("std");
// const Allocator = std.mem.Allocator;
// const test_fixture = @import("fixture.zig");
// const io = @import("io.zig");
// const record = @import("record.zig");
// const terminal_ansi = @import("../ansi.zig");
// const terminal_color = @import("../color.zig");
// const terminal_modes = @import("../modes.zig");
// const terminal_mouse = @import("../mouse.zig");
// const terminal_osc = @import("../osc.zig");
// const Terminal = @import("../Terminal.zig");
// const TerminalScreen = @import("../Screen.zig");
// const TerminalScreenKey = @import("../ScreenSet.zig").Key;
// const TerminalTabstops = @import("../Tabstops.zig");
use super::io;
use super::record;
use crate::ansi as terminal_ansi;
use crate::color as terminal_color;
use crate::modes as terminal_modes;
use crate::mouse as terminal_mouse;
use crate::osc as terminal_osc;
use crate::screen::CursorStyle;
use crate::screen_set::Key as TerminalScreenKey;
use crate::tabstops::Tabstops as TerminalTabstops;
use crate::terminal::{
    Colors as TerminalColors, Cursor as TerminalCursor, MouseShiftCapture,
    Options as TerminalOptions, ScrollingRegion, Terminal,
};

// PORT NOTE: the reference `Allocator.Error` maps to the crate's canonical
// OOM; the general-purpose allocator maps to `allocator_api2::alloc::Global`
// (the crate-wide treatment; see `snapshot/screen.rs`).
use allocator_api2::alloc::Global as Allocator;

// const ModeBits = @typeInfo(terminal_modes.ModePacked).@"struct".backing_integer.?;
// PORT NOTE: the port's `ModePacked` is a `u64` newtype whose low
// `ModePacked::BITS` (42) bits are the packed registry, so `ModeBits` maps to
// this mask (the `@truncate` to the backing integer on decode).
const MODE_BITS_MASK: u64 = (1u64 << terminal_modes::ModePacked::BITS) - 1;

/// Reuse the terminal's semantic RGB value without adopting its memory layout
/// as a wire encoding.
pub type RGB = terminal_color::RGB;

/// Reuse the terminal's semantic dynamic color without adopting its memory
/// layout as a wire encoding.
pub type DynamicRGB = terminal_color::DynamicRGB;

// const ValidationError = error{...};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ValidationError {
    InvalidDimensions,
    InvalidScrollingRegion,
    InvalidScreenCount,
    InvalidActiveScreenKey,
    InvalidPreviousCodepoint,
    InvalidScrollbackLimit,
}

// const HeaderInitError = error{ScrollbackLimitOverflow};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HeaderInitError {
    ScrollbackLimitOverflow,
}

/// Errors possible while encoding a TERMINAL payload.
// const PayloadEncodeError = std.Io.Writer.Error || ValidationError || error{...};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PayloadEncodeError {
    // std.Io.Writer.Error
    WriteFailed,
    // ValidationError
    InvalidDimensions,
    InvalidScrollingRegion,
    InvalidScreenCount,
    InvalidActiveScreenKey,
    InvalidPreviousCodepoint,
    InvalidScrollbackLimit,

    InvalidTabStops,
    InvalidPalette,
    StringTooLong,
}

impl From<io::WriteFailed> for PayloadEncodeError {
    fn from(_: io::WriteFailed) -> PayloadEncodeError {
        PayloadEncodeError::WriteFailed
    }
}

impl From<ValidationError> for PayloadEncodeError {
    fn from(err: ValidationError) -> PayloadEncodeError {
        match err {
            ValidationError::InvalidDimensions => PayloadEncodeError::InvalidDimensions,
            ValidationError::InvalidScrollingRegion => PayloadEncodeError::InvalidScrollingRegion,
            ValidationError::InvalidScreenCount => PayloadEncodeError::InvalidScreenCount,
            ValidationError::InvalidActiveScreenKey => PayloadEncodeError::InvalidActiveScreenKey,
            ValidationError::InvalidPreviousCodepoint => {
                PayloadEncodeError::InvalidPreviousCodepoint
            }
            ValidationError::InvalidScrollbackLimit => PayloadEncodeError::InvalidScrollbackLimit,
        }
    }
}

/// Errors possible while decoding a TERMINAL payload.
// const PayloadDecodeError = std.Io.Reader.Error || Allocator.Error || error{...};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PayloadDecodeError {
    // std.Io.Reader.Error
    ReadFailed,
    EndOfStream,
    // Allocator.Error
    OutOfMemory,

    /// Terminal dimensions control every following native allocation.
    InvalidDimensions,

    /// The count determines how many SCREEN and HISTORY sequences follow.
    InvalidScreenCount,
}

impl From<io::ReadError> for PayloadDecodeError {
    fn from(err: io::ReadError) -> PayloadDecodeError {
        match err {
            io::ReadError::ReadFailed => PayloadDecodeError::ReadFailed,
            io::ReadError::EndOfStream => PayloadDecodeError::EndOfStream,
        }
    }
}

/// The fixed fields at the start of every TERMINAL payload.
#[derive(Clone, Debug, PartialEq)]
pub struct Header {
    // Terminal geometry and its current scrolling region.
    pub columns: u16,
    pub rows: u16,
    pub width_px: u32,
    pub height_px: u32,

    pub scrolling_region_top: u16,
    pub scrolling_region_bottom: u16,
    pub scrolling_region_left: u16,
    pub scrolling_region_right: u16,

    // Status display, screen routing, and character context.
    pub status_display: terminal_ansi::StatusDisplay,
    pub active_screen_key: TerminalScreenKey,
    pub screen_count: u16,
    /// PORT NOTE: the reference `?u21` — the port stores codepoints as `u32`
    /// (the crate-wide treatment of `u21`); the scalar-range checks below keep
    /// the same value domain.
    pub previous_codepoint: Option<u32>,

    // Cursor presentation defaults shared by the screens.
    pub cursor_is_default: bool,
    pub cursor_default_style: CursorStyle,
    pub cursor_default_blink: Option<bool>,

    // Terminal input, semantic redraw, and pointer behavior.
    pub shell_redraw: terminal_osc::semantic_prompt::Redraw,
    pub modify_other_keys_2: bool,
    pub mouse_event: terminal_mouse::Event,
    pub mouse_format: terminal_mouse::Format,
    pub mouse_shift_capture: Option<bool>,
    pub mouse_shape: terminal_mouse::Shape,
    pub password_input: bool,

    // Runtime, saved, and reset mode sets.
    pub current_modes: terminal_modes::ModePacked,
    pub saved_modes: terminal_modes::ModePacked,
    pub default_modes: terminal_modes::ModePacked,

    // Terminal-wide dynamic color state.
    pub background: DynamicRGB,
    pub foreground: DynamicRGB,
    pub cursor_color: DynamicRGB,

    // Explicit primary-screen scrollback policies.
    pub max_scrollback_bytes: Option<u64>,
    pub max_scrollback_rows: Option<u64>,
}

impl Header {
    /// Number of bytes written by `encode`, calculated using the encoder.
    ///
    /// PORT NOTE: the reference computes this at comptime with the encoder
    /// itself and `comptime`-asserts it is 103. The port pins the constant;
    /// the `header_len_matches_encoder` test carries both the synchronization
    /// and the assert.
    pub const LEN: usize = 103;

    /// Capture the terminal-wide native state represented by this header.
    fn init(terminal: &Terminal) -> Result<Header, HeaderInitError> {
        let primary = terminal.screens.get(TerminalScreenKey::Primary).unwrap();
        // SAFETY: `primary` is a live screen owned by the terminal's screen
        // set; shared reads of its PageList limit policies only.
        let (bytes_explicit, lines_explicit) = unsafe {
            (
                (*primary).pages.limits.bytes.explicit,
                (*primary).pages.limits.lines.explicit,
            )
        };

        Ok(Header {
            // Terminal geometry and its current scrolling region.
            columns: terminal.cols,
            rows: terminal.rows,
            width_px: terminal.width_px,
            height_px: terminal.height_px,
            scrolling_region_top: terminal.scrolling_region.top,
            scrolling_region_bottom: terminal.scrolling_region.bottom,
            scrolling_region_left: terminal.scrolling_region.left,
            scrolling_region_right: terminal.scrolling_region.right,

            // Status display, screen routing, and character context.
            status_display: terminal.status_display,
            active_screen_key: terminal.screens.active_key,
            screen_count: if terminal.screens.get(TerminalScreenKey::Alternate).is_none() {
                1
            } else {
                2
            },
            previous_codepoint: terminal.previous_char,

            // Cursor presentation defaults shared by the screens.
            cursor_is_default: terminal.cursor.is_default,
            cursor_default_style: terminal.cursor.default_style,
            cursor_default_blink: terminal.cursor.default_blink,

            // Terminal input, semantic redraw, and pointer behavior.
            shell_redraw: terminal.flags.shell_redraws_prompt,
            modify_other_keys_2: terminal.flags.modify_other_keys_2,
            mouse_event: terminal.flags.mouse_event,
            mouse_format: terminal.flags.mouse_format,
            mouse_shift_capture: match terminal.flags.mouse_shift_capture {
                MouseShiftCapture::Null => None,
                MouseShiftCapture::False => Some(false),
                MouseShiftCapture::True => Some(true),
            },
            mouse_shape: terminal.mouse_shape,
            password_input: terminal.flags.password_input,

            // Runtime, saved, and reset mode sets.
            current_modes: terminal.modes.values,
            saved_modes: terminal.modes.saved,
            default_modes: terminal.modes.default,

            // Terminal-wide dynamic color state.
            background: terminal.colors.background,
            foreground: terminal.colors.foreground,
            cursor_color: terminal.colors.cursor,

            // Explicit primary-screen scrollback policies.
            max_scrollback_bytes: encode_scrollback_limit(bytes_explicit)?,
            max_scrollback_rows: encode_scrollback_limit(lines_explicit)?,
        })
    }

    /// Encode the fixed TERMINAL payload header field by field.
    pub fn encode(&self, writer: &mut dyn io::Writer) -> Result<(), PayloadEncodeError> {
        self.validate()?;

        // Terminal geometry and its current scrolling region.
        io::write_int::<u16>(writer, self.columns)?;
        io::write_int::<u16>(writer, self.rows)?;
        io::write_int::<u32>(writer, self.width_px)?;
        io::write_int::<u32>(writer, self.height_px)?;
        io::write_int::<u16>(writer, self.scrolling_region_top)?;
        io::write_int::<u16>(writer, self.scrolling_region_bottom)?;
        io::write_int::<u16>(writer, self.scrolling_region_left)?;
        io::write_int::<u16>(writer, self.scrolling_region_right)?;

        // Status display, screen routing, and character context.
        writer.write_byte(self.status_display as u8)?;
        io::write_int::<u16>(writer, self.active_screen_key as u16)?;
        io::write_int::<u16>(writer, self.screen_count)?;
        io::write_int::<u32>(
            writer,
            match self.previous_codepoint {
                Some(value) => value,
                None => u32::MAX,
            },
        )?;

        // Cursor presentation defaults shared by the screens.
        writer.write_byte(self.cursor_is_default as u8)?;
        writer.write_byte(self.cursor_default_style as u8)?;
        writer.write_byte(encode_optional_bool(self.cursor_default_blink))?;

        // Terminal input, semantic redraw, and pointer behavior.
        writer.write_byte(self.shell_redraw as u8)?;
        writer.write_byte(self.modify_other_keys_2 as u8)?;
        writer.write_byte(self.mouse_event as u8)?;
        writer.write_byte(self.mouse_format as u8)?;
        writer.write_byte(encode_optional_bool(self.mouse_shift_capture))?;
        writer.write_byte(self.mouse_shape as u8)?;
        writer.write_byte(self.password_input as u8)?;

        // Runtime, saved, and reset mode sets. ModePacked occupies 42 bits;
        // its eight-byte wire slots zero-extend the native packed value.
        let mode_values = [self.current_modes, self.saved_modes, self.default_modes];
        for value in mode_values {
            // const bits: ModeBits = @bitCast(value);
            io::write_int::<u64>(writer, value.0)?;
        }

        // Terminal-wide dynamic color state.
        encode_dynamic_rgb(self.background, writer)?;
        encode_dynamic_rgb(self.foreground, writer)?;
        encode_dynamic_rgb(self.cursor_color, writer)?;

        // Explicit primary-screen scrollback policies.
        io::write_int::<u64>(writer, self.max_scrollback_bytes.unwrap_or(u64::MAX))?;
        io::write_int::<u64>(writer, self.max_scrollback_rows.unwrap_or(u64::MAX))?;
        Ok(())
    }

    /// Decode the fixed TERMINAL payload header, normalizing semantic values.
    pub fn decode(reader: &mut dyn io::Reader) -> Result<Header, PayloadDecodeError> {
        // Terminal geometry and its current scrolling region.
        let columns = io::read_int::<u16>(reader)?;
        let rows = io::read_int::<u16>(reader)?;
        let width_px = io::read_int::<u32>(reader)?;
        let height_px = io::read_int::<u32>(reader)?;
        let scrolling_region_top = io::read_int::<u16>(reader)?;
        let scrolling_region_bottom = io::read_int::<u16>(reader)?;
        let scrolling_region_left = io::read_int::<u16>(reader)?;
        let scrolling_region_right = io::read_int::<u16>(reader)?;

        // Status display, screen routing, and character context.
        let status_display = status_display_from_int(reader.take_byte()?)
            .unwrap_or(terminal_ansi::StatusDisplay::Main);
        let active_screen_key_raw = io::read_int::<u16>(reader)?;
        let screen_count = io::read_int::<u16>(reader)?;
        let previous_raw = io::read_int::<u32>(reader)?;
        let previous_codepoint: Option<u32> = 'previous: {
            if previous_raw == u32::MAX {
                break 'previous None;
            }
            // const value = std.math.cast(u21, previous_raw) orelse break :previous null;
            if previous_raw >= (1u32 << 21) {
                break 'previous None;
            }
            let value = previous_raw;
            if value > 0x10FFFF || (0xD800..=0xDFFF).contains(&value) {
                break 'previous None;
            }
            Some(value)
        };

        // Cursor presentation defaults shared by the screens.
        let cursor_is_default = match reader.take_byte()? {
            0 => false,
            1 => true,
            _ => true,
        };
        let cursor_default_style =
            cursor_style_from_int(reader.take_byte()?).unwrap_or(CursorStyle::Block);
        let cursor_default_blink: Option<bool> = match reader.take_byte()? {
            0 => None,
            1 => Some(false),
            2 => Some(true),
            _ => None,
        };

        // Terminal input, semantic redraw, and pointer behavior.
        let shell_redraw = redraw_from_int(reader.take_byte()?)
            .unwrap_or(terminal_osc::semantic_prompt::Redraw::True);
        let modify_other_keys_2 = match reader.take_byte()? {
            0 => false,
            1 => true,
            _ => false,
        };
        let mouse_event =
            mouse_event_from_int(reader.take_byte()?).unwrap_or(terminal_mouse::Event::None);
        let mouse_format =
            mouse_format_from_int(reader.take_byte()?).unwrap_or(terminal_mouse::Format::X10);
        let mouse_shift_capture: Option<bool> = match reader.take_byte()? {
            0 => None,
            1 => Some(false),
            2 => Some(true),
            _ => None,
        };
        let mouse_shape =
            mouse_shape_from_int(reader.take_byte()?).unwrap_or(terminal_mouse::Shape::Text);
        let password_input = match reader.take_byte()? {
            0 => false,
            1 => true,
            _ => false,
        };

        // Runtime, saved, and reset mode sets.
        let mut mode_values = [terminal_modes::ModePacked(0); 3];
        for value in mode_values.iter_mut() {
            let raw = io::read_int::<u64>(reader)?;
            // const bits: ModeBits = @truncate(raw);
            *value = terminal_modes::ModePacked(raw & MODE_BITS_MASK);
        }

        // Terminal-wide dynamic color state.
        let background = decode_dynamic_rgb(reader)?;
        let foreground = decode_dynamic_rgb(reader)?;
        let cursor_color = decode_dynamic_rgb(reader)?;

        // Explicit primary-screen scrollback policies.
        let max_scrollback_bytes = decode_optional_limit(io::read_int::<u64>(reader)?);
        let max_scrollback_rows = decode_optional_limit(io::read_int::<u64>(reader)?);

        // Dimensions and sequence count are the only header fields that define
        // following allocation or record boundaries.
        if columns == 0 || rows == 0 {
            return Err(PayloadDecodeError::InvalidDimensions);
        }
        if screen_count != 1 && screen_count != 2 {
            return Err(PayloadDecodeError::InvalidScreenCount);
        }

        let scrolling_region_vertical_valid =
            scrolling_region_top <= scrolling_region_bottom && scrolling_region_bottom < rows;
        let scrolling_region_horizontal_valid =
            scrolling_region_left <= scrolling_region_right && scrolling_region_right < columns;
        let active_screen_key: TerminalScreenKey = 'active: {
            let decoded = match screen_key_from_int(active_screen_key_raw) {
                Some(decoded) => decoded,
                None => break 'active TerminalScreenKey::Primary,
            };
            if decoded == TerminalScreenKey::Alternate && screen_count != 2 {
                break 'active TerminalScreenKey::Primary;
            }
            decoded
        };

        let result = Header {
            columns,
            rows,
            width_px,
            height_px,

            scrolling_region_top: if scrolling_region_vertical_valid {
                scrolling_region_top
            } else {
                0
            },
            scrolling_region_bottom: if scrolling_region_vertical_valid {
                scrolling_region_bottom
            } else {
                rows - 1
            },
            scrolling_region_left: if scrolling_region_horizontal_valid {
                scrolling_region_left
            } else {
                0
            },
            scrolling_region_right: if scrolling_region_horizontal_valid {
                scrolling_region_right
            } else {
                columns - 1
            },

            status_display,
            active_screen_key,
            screen_count,
            previous_codepoint,

            cursor_is_default,
            cursor_default_style,
            cursor_default_blink,

            shell_redraw,
            modify_other_keys_2,
            mouse_event,
            mouse_format,
            mouse_shift_capture,
            mouse_shape,
            password_input,

            current_modes: mode_values[0],
            saved_modes: mode_values[1],
            default_modes: mode_values[2],

            background,
            foreground,
            cursor_color,

            max_scrollback_bytes,
            max_scrollback_rows,
        };
        Ok(result)
    }

    fn validate(&self) -> Result<(), ValidationError> {
        if self.columns == 0 || self.rows == 0 {
            return Err(ValidationError::InvalidDimensions);
        }
        if self.scrolling_region_top > self.scrolling_region_bottom
            || self.scrolling_region_bottom >= self.rows
            || self.scrolling_region_left > self.scrolling_region_right
            || self.scrolling_region_right >= self.columns
        {
            return Err(ValidationError::InvalidScrollingRegion);
        }

        if self.screen_count != 1 && self.screen_count != 2 {
            return Err(ValidationError::InvalidScreenCount);
        }
        if self.active_screen_key == TerminalScreenKey::Alternate && self.screen_count != 2 {
            return Err(ValidationError::InvalidActiveScreenKey);
        }

        if let Some(value) = self.previous_codepoint {
            if value > 0x10FFFF || (0xD800..=0xDFFF).contains(&value) {
                return Err(ValidationError::InvalidPreviousCodepoint);
            }
        }

        if self.max_scrollback_bytes == Some(u64::MAX) || self.max_scrollback_rows == Some(u64::MAX)
        {
            return Err(ValidationError::InvalidScrollbackLimit);
        }
        Ok(())
    }

    /// The reference comptime `computeLen` (see `LEN`).
    #[cfg(test)]
    fn compute_len() -> usize {
        let mut buf = [0u8; 128];
        let mut writer = io::FixedWriter::new(&mut buf);
        let value = Header {
            columns: 1,
            rows: 1,
            width_px: 0,
            height_px: 0,
            scrolling_region_top: 0,
            scrolling_region_bottom: 0,
            scrolling_region_left: 0,
            scrolling_region_right: 0,
            status_display: terminal_ansi::StatusDisplay::Main,
            active_screen_key: TerminalScreenKey::Primary,
            screen_count: 1,
            previous_codepoint: None,
            cursor_is_default: true,
            cursor_default_style: CursorStyle::Block,
            cursor_default_blink: None,
            shell_redraw: terminal_osc::semantic_prompt::Redraw::True,
            modify_other_keys_2: false,
            mouse_event: terminal_mouse::Event::None,
            mouse_format: terminal_mouse::Format::X10,
            mouse_shift_capture: None,
            mouse_shape: terminal_mouse::Shape::Text,
            password_input: false,
            current_modes: terminal_modes::ModePacked(0),
            saved_modes: terminal_modes::ModePacked(0),
            default_modes: terminal_modes::ModePacked(0),
            background: DynamicRGB::UNSET,
            foreground: DynamicRGB::UNSET,
            cursor_color: DynamicRGB::UNSET,
            max_scrollback_bytes: None,
            max_scrollback_rows: None,
        };
        value.encode(&mut writer).unwrap();
        writer.end
    }
}

/// Allocator-owned semantic state decoded from one TERMINAL payload.
///
/// The palette and fixed header are values. `tabstops`, `pwd`, and `title`
/// own allocations and are released by `deinit`.
struct DecodedPayload {
    header: Header,
    tabstops: TerminalTabstops,
    palette: terminal_color::DynamicPalette,
    pwd: Vec<u8>,
    title: Vec<u8>,
}

impl DecodedPayload {
    fn deinit(self, alloc: Allocator) {
        self.tabstops.deinit(alloc);
        // alloc.free(self.pwd); alloc.free(self.title); — the Vecs free on
        // drop. self.* = undefined;
    }
}

/// Encode one complete TERMINAL payload without record framing.
fn encode_payload(
    header: &Header,
    tabstops: &TerminalTabstops,
    palette: &terminal_color::DynamicPalette,
    pwd: &[u8],
    title: &[u8],
    writer: &mut dyn io::Writer,
) -> Result<(), PayloadEncodeError> {
    // Validate the complete semantic value before writing any bytes.
    header.validate()?;
    if tabstops.cols != header.columns as usize {
        return Err(PayloadEncodeError::InvalidTabStops);
    }
    if pwd.len() > u32::MAX as usize || title.len() > u32::MAX as usize {
        return Err(PayloadEncodeError::StringTooLong);
    }
    for (index, (original, current)) in palette
        .original
        .iter()
        .zip(palette.current.iter())
        .enumerate()
    {
        if !palette.mask.is_set(index) && !original.eql(*current) {
            return Err(PayloadEncodeError::InvalidPalette);
        }
    }

    header.encode(writer)?;

    // Tab stops are packed least-significant bit first.
    let tabstop_len = (header.columns as usize + 7) / 8;
    for byte_index in 0..tabstop_len {
        let mut raw: u8 = 0;
        for bit in 0..8 {
            let column = byte_index * 8 + bit;
            if column >= header.columns as usize {
                break;
            }
            if tabstops.get(column) {
                raw |= 1u8 << bit;
            }
        }
        writer.write_byte(raw)?;
    }

    // Encode the original palette, then a compact mask and only the active
    // overrides in increasing palette-index order.
    for value in palette.original.iter() {
        encode_rgb(*value, writer)?;
    }
    for byte_index in 0..32 {
        let mut raw: u8 = 0;
        for bit in 0..8 {
            if palette.mask.is_set(byte_index * 8 + bit) {
                raw |= 1u8 << bit;
            }
        }
        writer.write_byte(raw)?;
    }
    for (index, value) in palette.current.iter().enumerate() {
        if palette.mask.is_set(index) {
            encode_rgb(*value, writer)?;
        }
    }

    // Length-prefixed terminal strings end the payload.
    io::write_int::<u32>(writer, pwd.len() as u32)?;
    writer.write_all(pwd)?;
    io::write_int::<u32>(writer, title.len() as u32)?;
    writer.write_all(title)?;
    Ok(())
}

/// `alloc.alloc(u8, len)` / `try TerminalTabstops.init(...)` — the crate's
/// tripwire seam standing in for the reference tests' FailingAllocator at the
/// payload decoder's fallible allocations.
#[inline]
fn decode_alloc_check() -> Result<(), PayloadDecodeError> {
    #[cfg(test)]
    super::tw::check(super::tw::SnapshotFp::Alloc)
        .map_err(|_| PayloadDecodeError::OutOfMemory)?;
    Ok(())
}

/// Decode one complete allocator-owned TERMINAL payload.
///
/// The enclosing record reader remains responsible for verifying exact payload
/// exhaustion. On success, the caller owns the result and must call `deinit`.
fn decode_payload(
    reader: &mut dyn io::Reader,
    alloc: Allocator,
) -> Result<DecodedPayload, PayloadDecodeError> {
    let header = Header::decode(reader)?;

    // Restore the native tab-stop representation from its packed wire bits.
    decode_alloc_check()?;
    let mut tabstops = TerminalTabstops::init(alloc, header.columns as usize, 0)
        .map_err(|_| PayloadDecodeError::OutOfMemory)?;
    // errdefer tabstops.deinit(alloc);
    let tabstop_len = (header.columns as usize + 7) / 8;
    for byte_index in 0..tabstop_len {
        let raw = match reader.take_byte() {
            Ok(raw) => raw,
            Err(err) => {
                tabstops.deinit(alloc);
                return Err(err.into());
            }
        };
        for bit in 0..8 {
            let mask = 1u8 << bit;
            let column = byte_index * 8 + bit;
            if column < header.columns as usize && raw & mask != 0 {
                tabstops.set(column);
            }
        }
    }

    // Reconstruct the dynamic palette from its original values and sparse
    // current-value overrides.
    //
    // PORT NOTE: the reference leaves `original` undefined and fills every
    // entry; the Rust array is zero-initialized before the same full fill.
    // The `errdefer tabstops.deinit(alloc)` remains live across these reads.
    let mut original: terminal_color::Palette = [RGB::default(); 256];
    for value in original.iter_mut() {
        *value = match decode_rgb(reader) {
            Ok(rgb) => rgb,
            Err(err) => {
                tabstops.deinit(alloc);
                return Err(err.into());
            }
        };
    }
    let mut override_mask = [0u8; 32];
    if let Err(err) = reader.read_slice_all(&mut override_mask) {
        tabstops.deinit(alloc);
        return Err(err.into());
    }

    let mut palette: terminal_color::DynamicPalette = terminal_color::DynamicPalette::init(original);
    for index in 0..256usize {
        let mask = 1u8 << (index % 8);
        if override_mask[index / 8] & mask != 0 {
            let rgb = match decode_rgb(reader) {
                Ok(rgb) => rgb,
                Err(err) => {
                    tabstops.deinit(alloc);
                    return Err(err.into());
                }
            };
            palette.set(index as u8, rgb);
        }
    }

    // Read the two allocator-owned terminal strings last so all earlier
    // fixed-size validation happens before allocating them.
    //
    // PORT NOTE: the reference `alloc.alloc(u8, len)` + `readSliceAll` pairs
    // map to fallible zeroed Vecs filled in place; the errdefer frees become
    // the error-arm drops below (the Vecs free on drop).
    let read_string = |reader: &mut dyn io::Reader| -> Result<Vec<u8>, PayloadDecodeError> {
        let len = io::read_int::<u32>(reader)? as usize;
        decode_alloc_check()?;
        let mut bytes: Vec<u8> = Vec::new();
        bytes
            .try_reserve_exact(len)
            .map_err(|_| PayloadDecodeError::OutOfMemory)?;
        bytes.resize(len, 0);
        reader.read_slice_all(&mut bytes)?;
        Ok(bytes)
    };
    let pwd = match read_string(reader) {
        Ok(pwd) => pwd,
        Err(err) => {
            tabstops.deinit(alloc);
            return Err(err);
        }
    };
    let title = match read_string(reader) {
        Ok(title) => title,
        Err(err) => {
            tabstops.deinit(alloc);
            return Err(err);
        }
    };

    Ok(DecodedPayload {
        header,
        tabstops,
        palette,
        pwd,
        title,
    })
}

/// Errors possible while encoding one native TERMINAL record.
// pub const EncodeError = HeaderInitError || PayloadEncodeError || record.Writer.FinishError;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EncodeError {
    // HeaderInitError
    ScrollbackLimitOverflow,
    // PayloadEncodeError
    WriteFailed,
    InvalidDimensions,
    InvalidScrollingRegion,
    InvalidScreenCount,
    InvalidActiveScreenKey,
    InvalidPreviousCodepoint,
    InvalidScrollbackLimit,
    InvalidTabStops,
    InvalidPalette,
    StringTooLong,
    // record.Writer.FinishError
    PayloadTooLarge,
}

impl From<HeaderInitError> for EncodeError {
    fn from(err: HeaderInitError) -> EncodeError {
        match err {
            HeaderInitError::ScrollbackLimitOverflow => EncodeError::ScrollbackLimitOverflow,
        }
    }
}

impl From<PayloadEncodeError> for EncodeError {
    fn from(err: PayloadEncodeError) -> EncodeError {
        match err {
            PayloadEncodeError::WriteFailed => EncodeError::WriteFailed,
            PayloadEncodeError::InvalidDimensions => EncodeError::InvalidDimensions,
            PayloadEncodeError::InvalidScrollingRegion => EncodeError::InvalidScrollingRegion,
            PayloadEncodeError::InvalidScreenCount => EncodeError::InvalidScreenCount,
            PayloadEncodeError::InvalidActiveScreenKey => EncodeError::InvalidActiveScreenKey,
            PayloadEncodeError::InvalidPreviousCodepoint => EncodeError::InvalidPreviousCodepoint,
            PayloadEncodeError::InvalidScrollbackLimit => EncodeError::InvalidScrollbackLimit,
            PayloadEncodeError::InvalidTabStops => EncodeError::InvalidTabStops,
            PayloadEncodeError::InvalidPalette => EncodeError::InvalidPalette,
            PayloadEncodeError::StringTooLong => EncodeError::StringTooLong,
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

/// Encode terminal-wide native state as one framed TERMINAL record.
///
/// State not represented by this snapshot version is ignored. Payload failures
/// emit no part of the TERMINAL record.
pub fn encode<W: io::Writer + ?Sized>(
    terminal: &Terminal,
    destination: &mut record::Writer<'_, W>,
) -> Result<(), EncodeError> {
    let header = Header::init(terminal)?;

    let payload = destination.begin(record::Tag::Terminal);
    // errdefer destination.cancel();
    if let Err(err) = encode_payload(
        &header,
        &terminal.tabstops,
        &terminal.colors.palette,
        terminal.get_pwd().unwrap_or(b""),
        terminal.get_title().unwrap_or(b""),
        payload,
    ) {
        destination.cancel();
        return Err(err.into());
    }
    if let Err(err) = destination.finish() {
        destination.cancel();
        return Err(err.into());
    }
    Ok(())
}

/// Errors possible while decoding one native TERMINAL record.
// pub const DecodeError = PayloadDecodeError || record.Reader.InitError ||
//     record.Reader.FinishError || error{UnexpectedRecordTag};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecodeError {
    // PayloadDecodeError
    ReadFailed,
    EndOfStream,
    OutOfMemory,
    InvalidDimensions,
    InvalidScreenCount,
    // record.Reader.InitError
    InvalidTag,
    // record.Reader.FinishError
    InvalidChecksum,
    PayloadNotExhausted,

    /// The next record is valid but is not a TERMINAL.
    UnexpectedRecordTag,
}

impl From<PayloadDecodeError> for DecodeError {
    fn from(err: PayloadDecodeError) -> DecodeError {
        match err {
            PayloadDecodeError::ReadFailed => DecodeError::ReadFailed,
            PayloadDecodeError::EndOfStream => DecodeError::EndOfStream,
            PayloadDecodeError::OutOfMemory => DecodeError::OutOfMemory,
            PayloadDecodeError::InvalidDimensions => DecodeError::InvalidDimensions,
            PayloadDecodeError::InvalidScreenCount => DecodeError::InvalidScreenCount,
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

/// Decode one framed TERMINAL record directly into native terminal state.
///
/// The returned terminal owns every decoded allocation and contains empty
/// primary and optional alternate screens with the declared routing. Later
/// SCREEN records replace those screens in place. On failure, all decoded and
/// partially initialized native state is released.
pub fn decode(source: &mut dyn io::Reader, alloc: Allocator) -> Result<Terminal, DecodeError> {
    // Finish the complete record before constructing native terminal state.
    let mut payload: DecodedPayload = {
        let mut record_reader = record::Reader::init(source)?;
        if record_reader.header.tag != record::Tag::Terminal {
            return Err(DecodeError::UnexpectedRecordTag);
        }

        let payload = decode_payload(record_reader.payload_reader(), alloc)?;
        // errdefer payload.deinit(alloc);
        if let Err(err) = record_reader.finish() {
            payload.deinit(alloc);
            return Err(err.into());
        }
        payload
    };
    // defer payload.deinit(alloc); — consumed at every exit below.

    let header = payload.header.clone();
    let max_scrollback_bytes = native_scrollback_limit(header.max_scrollback_bytes);
    let max_scrollback_lines = native_scrollback_limit(header.max_scrollback_rows);

    // Terminal.init establishes native pools, pins, defaults, and ownership.
    //
    // PORT NOTE: the reference relies on the remaining `Terminal.Options`
    // field defaults (the Kitty image knobs); rustty-vt `Options` has no
    // `Default`, so the (ghostty-artifact) defaults are spelled out (the
    // crate-wide treatment; see `terminal.rs`'s option sites).
    let mut result = match Terminal::init(
        alloc,
        TerminalOptions {
            cols: header.columns,
            rows: header.rows,
            max_scrollback_bytes,
            max_scrollback_lines,
            colors: TerminalColors {
                background: header.background,
                foreground: header.foreground,
                cursor: header.cursor_color,
                palette: payload.palette,
            },
            default_modes: header.default_modes,
            default_cursor_style: header.cursor_default_style,
            default_cursor_blink: header.cursor_default_blink,
            kitty_image_storage_limit: 320 * 1000 * 1000,
            kitty_image_loading_limits: crate::kitty::graphics_image::Limits::DIRECT,
        },
    ) {
        Ok(result) => result,
        Err(_) => {
            payload.deinit(alloc);
            return Err(DecodeError::OutOfMemory);
        }
    };
    // errdefer result.deinit(alloc);

    // Recreate the optional screen now so ScreenSet routing is complete before
    // its empty screens are replaced by the following SCREEN records.
    if header.screen_count == 2 {
        if let Err(_) = result.screens.get_init(
            alloc,
            TerminalScreenKey::Alternate,
            crate::screen::Options {
                cols: header.columns,
                rows: header.rows,
                max_scrollback_bytes: Some(0),
                ..crate::screen::Options::default()
            },
        ) {
            result.deinit(alloc);
            payload.deinit(alloc);
            return Err(DecodeError::OutOfMemory);
        }
    }
    result.screens.switch_to(header.active_screen_key);

    // Restore terminal-wide runtime state that is not an initialization
    // policy. Presentation-only flags retain Terminal.init defaults.
    result.width_px = header.width_px;
    result.height_px = header.height_px;
    result.scrolling_region = ScrollingRegion {
        top: header.scrolling_region_top,
        bottom: header.scrolling_region_bottom,
        left: header.scrolling_region_left,
        right: header.scrolling_region_right,
    };
    result.status_display = header.status_display;
    result.previous_char = header.previous_codepoint;
    result.cursor = TerminalCursor {
        is_default: header.cursor_is_default,
        default_style: header.cursor_default_style,
        default_blink: header.cursor_default_blink,
    };
    result.modes = terminal_modes::ModeState {
        values: header.current_modes,
        saved: header.saved_modes,
        default: header.default_modes,
    };
    result.flags.shell_redraws_prompt = header.shell_redraw;
    result.flags.modify_other_keys_2 = header.modify_other_keys_2;
    result.flags.mouse_event = header.mouse_event;
    result.flags.mouse_format = header.mouse_format;
    result.flags.mouse_shift_capture = match header.mouse_shift_capture {
        Some(true) => MouseShiftCapture::True,
        Some(false) => MouseShiftCapture::False,
        None => MouseShiftCapture::Null,
    };
    result.flags.password_input = header.password_input;
    result.mouse_shape = header.mouse_shape;

    // Native strings retain trailing NUL sentinels. Setters construct those
    // without exposing that detail in the wire format.
    if let Err(_) = result.set_pwd(&payload.pwd) {
        result.deinit(alloc);
        payload.deinit(alloc);
        return Err(DecodeError::OutOfMemory);
    }
    if let Err(_) = result.set_title(&payload.title) {
        result.deinit(alloc);
        payload.deinit(alloc);
        return Err(DecodeError::OutOfMemory);
    }

    // Transfer decoded tab-stop ownership last. The deferred payload cleanup
    // releases the table originally allocated by Terminal.init.
    //
    // const initialized_tabstops = result.tabstops;
    // result.tabstops = payload.tabstops;
    // payload.tabstops = initialized_tabstops;
    core::mem::swap(&mut result.tabstops, &mut payload.tabstops);

    // SAFETY: `screens.active` is a live screen owned by the freshly built
    // terminal; the integrity check reads only.
    unsafe { (*result.screens.active).pages.assert_integrity() };
    payload.deinit(alloc);
    Ok(result)
}

// fn enumFromInt(comptime T: type, raw: anytype) ?T { ... }
// PORT NOTE: the reference's generic `enumFromInt` maps to the per-enum
// `*_from_int` narrowing helpers below (the crate's closed-enum convention;
// the declaration indices are this format's registry).

fn status_display_from_int(raw: u8) -> Option<terminal_ansi::StatusDisplay> {
    match raw {
        0 => Some(terminal_ansi::StatusDisplay::Main),
        1 => Some(terminal_ansi::StatusDisplay::StatusLine),
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

fn redraw_from_int(raw: u8) -> Option<terminal_osc::semantic_prompt::Redraw> {
    match raw {
        0 => Some(terminal_osc::semantic_prompt::Redraw::True),
        1 => Some(terminal_osc::semantic_prompt::Redraw::False),
        2 => Some(terminal_osc::semantic_prompt::Redraw::Last),
        _ => None,
    }
}

fn mouse_event_from_int(raw: u8) -> Option<terminal_mouse::Event> {
    match raw {
        0 => Some(terminal_mouse::Event::None),
        1 => Some(terminal_mouse::Event::X10),
        2 => Some(terminal_mouse::Event::Normal),
        3 => Some(terminal_mouse::Event::Button),
        4 => Some(terminal_mouse::Event::Any),
        _ => None,
    }
}

fn mouse_format_from_int(raw: u8) -> Option<terminal_mouse::Format> {
    match raw {
        0 => Some(terminal_mouse::Format::X10),
        1 => Some(terminal_mouse::Format::Utf8),
        2 => Some(terminal_mouse::Format::Sgr),
        3 => Some(terminal_mouse::Format::Urxvt),
        4 => Some(terminal_mouse::Format::SgrPixels),
        _ => None,
    }
}

fn mouse_shape_from_int(raw: u8) -> Option<terminal_mouse::Shape> {
    use terminal_mouse::Shape;
    Some(match raw {
        0 => Shape::Default,
        1 => Shape::ContextMenu,
        2 => Shape::Help,
        3 => Shape::Pointer,
        4 => Shape::Progress,
        5 => Shape::Wait,
        6 => Shape::Cell,
        7 => Shape::Crosshair,
        8 => Shape::Text,
        9 => Shape::VerticalText,
        10 => Shape::Alias,
        11 => Shape::Copy,
        12 => Shape::Move,
        13 => Shape::NoDrop,
        14 => Shape::NotAllowed,
        15 => Shape::Grab,
        16 => Shape::Grabbing,
        17 => Shape::AllScroll,
        18 => Shape::ColResize,
        19 => Shape::RowResize,
        20 => Shape::NResize,
        21 => Shape::EResize,
        22 => Shape::SResize,
        23 => Shape::WResize,
        24 => Shape::NeResize,
        25 => Shape::NwResize,
        26 => Shape::SeResize,
        27 => Shape::SwResize,
        28 => Shape::EwResize,
        29 => Shape::NsResize,
        30 => Shape::NeswResize,
        31 => Shape::NwseResize,
        32 => Shape::ZoomIn,
        33 => Shape::ZoomOut,
        _ => return None,
    })
}

fn encode_optional_bool(value: Option<bool>) -> u8 {
    match value {
        Some(present) => {
            if present {
                2
            } else {
                1
            }
        }
        None => 0,
    }
}

fn encode_rgb(value: RGB, writer: &mut dyn io::Writer) -> Result<(), io::WriteFailed> {
    writer.write_byte(value.r)?;
    writer.write_byte(value.g)?;
    writer.write_byte(value.b)?;
    Ok(())
}

fn decode_rgb(reader: &mut dyn io::Reader) -> Result<RGB, io::ReadError> {
    Ok(RGB {
        r: reader.take_byte()?,
        g: reader.take_byte()?,
        b: reader.take_byte()?,
    })
}

fn encode_dynamic_rgb(value: DynamicRGB, writer: &mut dyn io::Writer) -> Result<(), io::WriteFailed> {
    if let Some(rgb) = value.default {
        writer.write_byte(1)?;
        encode_rgb(rgb, writer)?;
    } else {
        writer.write_byte(0)?;
        writer.splat_byte_all(0, 3)?;
    }

    if let Some(rgb) = value.override_ {
        writer.write_byte(1)?;
        encode_rgb(rgb, writer)?;
    } else {
        writer.write_byte(0)?;
        writer.splat_byte_all(0, 3)?;
    }
    Ok(())
}

fn decode_dynamic_rgb(reader: &mut dyn io::Reader) -> Result<DynamicRGB, io::ReadError> {
    let default_present = reader.take_byte()?;
    let default_rgb = decode_rgb(reader)?;
    let default: Option<RGB> = if default_present == 1 {
        Some(default_rgb)
    } else {
        None
    };

    let override_present = reader.take_byte()?;
    let override_rgb = decode_rgb(reader)?;
    let override_: Option<RGB> = if override_present == 1 {
        Some(override_rgb)
    } else {
        None
    };

    Ok(DynamicRGB { default, override_ })
}

fn decode_optional_limit(raw: u64) -> Option<u64> {
    if raw == u64::MAX {
        None
    } else {
        Some(raw)
    }
}

fn encode_scrollback_limit(value: usize) -> Result<Option<u64>, HeaderInitError> {
    if value == usize::MAX {
        return Ok(None);
    }
    let result =
        u64::try_from(value).map_err(|_| HeaderInitError::ScrollbackLimitOverflow)?;
    if result == u64::MAX {
        return Err(HeaderInitError::ScrollbackLimitOverflow);
    }
    Ok(Some(result))
}

fn native_scrollback_limit(value: Option<u64>) -> Option<usize> {
    let present = value?;
    Some(usize::try_from(present.min((usize::MAX - 1) as u64)).unwrap())
}

#[cfg(test)]
mod tests {
    use super::super::fixture;
    use super::*;
    use crate::snapshot::io::Writer as _;

    // const test_header: Header = header: { ... };
    fn test_header() -> Header {
        let mut current_modes = terminal_modes::ModePacked(0);
        current_modes.set_mode(terminal_modes::Mode::DisableKeyboard, true);
        let mut saved_modes = terminal_modes::ModePacked(0);
        saved_modes.set_mode(terminal_modes::Mode::InBandSizeReports, true);
        let mut default_modes = current_modes;
        default_modes.set_mode(terminal_modes::Mode::InBandSizeReports, true);

        Header {
            columns: 0x0102,
            rows: 0x0304,
            width_px: 0x05060708,
            height_px: 0x090a0b0c,

            scrolling_region_top: 1,
            scrolling_region_bottom: 2,
            scrolling_region_left: 3,
            scrolling_region_right: 4,

            status_display: terminal_ansi::StatusDisplay::StatusLine,
            active_screen_key: TerminalScreenKey::Alternate,
            screen_count: 2,
            previous_codepoint: Some('A' as u32),

            cursor_is_default: true,
            cursor_default_style: CursorStyle::BlockHollow,
            cursor_default_blink: Some(true),

            shell_redraw: terminal_osc::semantic_prompt::Redraw::Last,
            modify_other_keys_2: true,
            mouse_event: terminal_mouse::Event::Any,
            mouse_format: terminal_mouse::Format::SgrPixels,
            mouse_shift_capture: Some(false),
            mouse_shape: terminal_mouse::Shape::ZoomOut,
            password_input: true,

            current_modes,
            saved_modes,
            default_modes,

            background: DynamicRGB {
                default: Some(RGB { r: 1, g: 2, b: 3 }),
                override_: None,
            },
            foreground: DynamicRGB {
                default: None,
                override_: Some(RGB { r: 4, g: 5, b: 6 }),
            },
            cursor_color: DynamicRGB {
                default: Some(RGB { r: 7, g: 8, b: 9 }),
                override_: Some(RGB {
                    r: 10,
                    g: 11,
                    b: 12,
                }),
            },

            max_scrollback_bytes: None,
            max_scrollback_rows: Some(0x0102030405060708),
        }
    }

    // const test_header_fixture = test_fixture.parse(@embedFile("testdata/terminal-header-v1.hex"));
    fn test_header_fixture() -> Vec<u8> {
        fixture::parse(include_str!("testdata/terminal-header-v1.hex"))
    }

    // comptime { assert(Header.len == 103); }
    #[test]
    fn header_len_matches_encoder() {
        assert_eq!(Header::LEN, Header::compute_len());
        assert_eq!(Header::LEN, 103);
    }

    // test "TERMINAL mode bit layout"
    #[test]
    fn terminal_mode_bit_layout() {
        // try std.testing.expectEqual(@as(usize, 42), @bitSizeOf(ModePacked));
        assert_eq!(42usize, terminal_modes::ModePacked::BITS);

        let mut first = terminal_modes::ModePacked(0);
        first.set_mode(terminal_modes::Mode::DisableKeyboard, true);
        assert_eq!(1u64 << 0, first.0);

        let mut visibility = terminal_modes::ModePacked(0);
        visibility.set_mode(terminal_modes::Mode::ReportVisibility, true);
        assert_eq!(1u64 << 40, visibility.0);

        let mut last = terminal_modes::ModePacked(0);
        last.set_mode(terminal_modes::Mode::InBandSizeReports, true);
        assert_eq!(1u64 << 41, last.0);
    }

    // test "TERMINAL header golden encoding and decoding"
    #[test]
    fn terminal_header_golden_encoding_and_decoding() {
        let test_header_fixture = test_header_fixture();

        let mut encoded = [0u8; Header::LEN];
        let mut writer = io::FixedWriter::new(&mut encoded);
        test_header().encode(&mut writer).unwrap();
        fixture::expect_equal(
            fixture::Kind::Bytes,
            "src/snapshot/testdata/terminal-header-v1.hex",
            "snapshot_fixture-terminal-header-v1.hex",
            &test_header_fixture,
            writer.buffered(),
        );
        assert_eq!(Header::LEN, test_header_fixture.len());

        // Exercise the streaming path with less buffered data than every integer.
        let mut source = io::FixedReader::new(&test_header_fixture);
        let mut buffer = [0u8; 1];
        let mut limited = io::LimitedReader::new(&mut source, io::Limit::Unlimited, &mut buffer);
        assert_eq!(test_header(), Header::decode(&mut limited).unwrap());

        for fixture_len in 0..Header::LEN {
            let mut truncated = io::FixedReader::new(&test_header_fixture[0..fixture_len]);
            assert_eq!(
                Err(PayloadDecodeError::EndOfStream),
                Header::decode(&mut truncated),
            );
        }
    }

    // test "TERMINAL header encoding rejects noncanonical values"
    #[test]
    fn terminal_header_encoding_rejects_noncanonical_values() {
        // Semantic values are validated before encoding writes anything.
        let mut invalid_header = test_header();
        invalid_header.columns = 0;
        let mut encoded = [0u8; Header::LEN];
        let mut writer = io::FixedWriter::new(&mut encoded);
        assert_eq!(
            Err(PayloadEncodeError::InvalidDimensions),
            invalid_header.encode(&mut writer),
        );
        assert_eq!(0usize, writer.end);

        let mut invalid_header = test_header();
        invalid_header.max_scrollback_bytes = Some(u64::MAX);
        let mut writer = io::FixedWriter::new(&mut encoded);
        assert_eq!(
            Err(PayloadEncodeError::InvalidScrollbackLimit),
            invalid_header.encode(&mut writer),
        );
        assert_eq!(0usize, writer.end);
    }

    // test "TERMINAL header decoding rejects structural values"
    #[test]
    fn terminal_header_decoding_rejects_structural_values() {
        let mut invalid_dimensions = test_header_fixture();
        invalid_dimensions[0] = 0;
        invalid_dimensions[1] = 0;
        let mut dimensions_reader = io::FixedReader::new(&invalid_dimensions);
        assert_eq!(
            Err(PayloadDecodeError::InvalidDimensions),
            Header::decode(&mut dimensions_reader),
        );

        let mut invalid_screen_count = test_header_fixture();
        invalid_screen_count[23] = 3;
        let mut screen_count_reader = io::FixedReader::new(&invalid_screen_count);
        assert_eq!(
            Err(PayloadDecodeError::InvalidScreenCount),
            Header::decode(&mut screen_count_reader),
        );
    }

    // test "TERMINAL header decoding normalizes semantic values"
    #[test]
    fn terminal_header_decoding_normalizes_semantic_values() {
        let mut fixture_bytes = test_header_fixture();

        // Unknown enum and boolean encodings use their neutral native defaults.
        fixture_bytes[20] = 2;
        fixture_bytes[21] = 2;
        fixture_bytes[22] = 0;
        fixture_bytes[29] = 2;
        fixture_bytes[30] = 4;
        fixture_bytes[31] = 3;
        fixture_bytes[32] = 3;
        fixture_bytes[33] = 2;
        fixture_bytes[34] = 5;
        fixture_bytes[35] = 5;
        fixture_bytes[36] = 3;
        fixture_bytes[37] = 34;
        fixture_bytes[38] = 2;

        // Reserved mode bits are ignored while known low bits survive.
        fixture_bytes[44] = 4;

        // Unknown presence and nonzero absent-value bytes both become absent.
        fixture_bytes[63] = 2;
        fixture_bytes[68] = 1;
        fixture_bytes[72] = 0xAA;

        // An invalid scrolling region becomes the full terminal region.
        fixture_bytes[14] = 0x04;
        fixture_bytes[15] = 0x03;

        // Surrogates cannot become native previous-character state.
        fixture_bytes[25] = 0x00;
        fixture_bytes[26] = 0xD8;
        fixture_bytes[27] = 0x00;
        fixture_bytes[28] = 0x00;

        let mut reader = io::FixedReader::new(&fixture_bytes);
        let decoded = Header::decode(&mut reader).unwrap();
        assert_eq!(terminal_ansi::StatusDisplay::Main, decoded.status_display);
        assert_eq!(TerminalScreenKey::Primary, decoded.active_screen_key);
        assert_eq!(0u16, decoded.scrolling_region_top);
        assert_eq!(test_header().rows - 1, decoded.scrolling_region_bottom);
        assert_eq!(
            test_header().scrolling_region_left,
            decoded.scrolling_region_left,
        );
        assert_eq!(
            test_header().scrolling_region_right,
            decoded.scrolling_region_right,
        );
        assert_eq!(None, decoded.previous_codepoint);
        assert!(decoded.cursor_is_default);
        assert_eq!(CursorStyle::Block, decoded.cursor_default_style);
        assert_eq!(None, decoded.cursor_default_blink);
        assert_eq!(
            terminal_osc::semantic_prompt::Redraw::True,
            decoded.shell_redraw,
        );
        assert!(!decoded.modify_other_keys_2);
        assert_eq!(terminal_mouse::Event::None, decoded.mouse_event);
        assert_eq!(terminal_mouse::Format::X10, decoded.mouse_format);
        assert_eq!(None, decoded.mouse_shift_capture);
        assert_eq!(terminal_mouse::Shape::Text, decoded.mouse_shape);
        assert!(!decoded.password_input);
        assert!(decoded
            .current_modes
            .get_mode(terminal_modes::Mode::DisableKeyboard));
        assert_eq!(DynamicRGB::UNSET, decoded.background);
        assert_eq!(None, decoded.foreground.default);
        assert_eq!(RGB { r: 4, g: 5, b: 6 }, decoded.foreground.override_.unwrap());

        // Each scrolling axis is normalized independently.
        let mut invalid_horizontal = test_header_fixture();
        invalid_horizontal[18] = 0x02;
        invalid_horizontal[19] = 0x01;
        let mut horizontal_reader = io::FixedReader::new(&invalid_horizontal);
        let horizontal = Header::decode(&mut horizontal_reader).unwrap();
        assert_eq!(
            test_header().scrolling_region_top,
            horizontal.scrolling_region_top,
        );
        assert_eq!(
            test_header().scrolling_region_bottom,
            horizontal.scrolling_region_bottom,
        );
        assert_eq!(0u16, horizontal.scrolling_region_left);
        assert_eq!(test_header().columns - 1, horizontal.scrolling_region_right);

        // Alternate cannot be active when only the primary screen is declared.
        let mut missing_active_screen = test_header_fixture();
        missing_active_screen[23] = 1;
        let mut active_screen_reader = io::FixedReader::new(&missing_active_screen);
        assert_eq!(
            TerminalScreenKey::Primary,
            Header::decode(&mut active_screen_reader)
                .unwrap()
                .active_screen_key,
        );

        let largest_finite = u64::MAX - 1;
        assert_eq!(
            usize::try_from(largest_finite.min((usize::MAX - 1) as u64)).unwrap(),
            native_scrollback_limit(Some(largest_finite)).unwrap(),
        );
    }

    // test "TERMINAL payload round trip"
    #[test]
    fn terminal_payload_round_trip() {
        // Include a partial final tab-stop byte so its bit order and padding
        // are both exercised.
        let mut tabstops =
            TerminalTabstops::init(Allocator, test_header().columns as usize, 0).unwrap();
        // defer tabstops.deinit(testing.allocator);
        tabstops.set(0);
        tabstops.set(8);
        tabstops.set(test_header().columns as usize - 1);

        // Use overrides at both ends of the palette to make ordering explicit.
        let mut palette = terminal_color::DynamicPalette::init(terminal_color::DEFAULT);
        palette.set(0, RGB { r: 1, g: 2, b: 3 });
        palette.set(255, RGB { r: 4, g: 5, b: 6 });

        let mut destination = io::AllocatingWriter::new();
        encode_payload(
            &test_header(),
            &tabstops,
            &palette,
            b"file:///tmp/example",
            b"snapshot title",
            &mut destination,
        )
        .unwrap();

        // Tab stops and the sparse palette mask both use least-significant-bit
        // first ordering.
        let bytes = destination.written();
        let tabstop_len = (test_header().columns as usize + 7) / 8;
        assert_eq!(0x01u8, bytes[Header::LEN]);
        assert_eq!(0x01u8, bytes[Header::LEN + 1]);
        assert_eq!(0x02u8, bytes[Header::LEN + tabstop_len - 1]);
        let palette_mask_offset = Header::LEN + tabstop_len + 256 * 3;
        assert_eq!(0x01u8, bytes[palette_mask_offset]);
        assert_eq!(0x80u8, bytes[palette_mask_offset + 31]);

        // Decode through a one-byte reader buffer to cover streaming of the
        // large fixed palette and allocator-owned strings.
        let mut source = io::FixedReader::new(bytes);
        let mut buffer = [0u8; 1];
        let mut limited = io::LimitedReader::new(&mut source, io::Limit::Unlimited, &mut buffer);
        let decoded = decode_payload(&mut limited, Allocator).unwrap();
        // defer decoded.deinit(testing.allocator);

        assert_eq!(test_header(), decoded.header);
        for column in 0..test_header().columns as usize {
            assert_eq!(tabstops.get(column), decoded.tabstops.get(column));
        }
        assert_eq!(palette, decoded.palette);
        assert_eq!(b"file:///tmp/example", decoded.pwd.as_slice());
        assert_eq!(b"snapshot title", decoded.title.as_slice());

        decoded.deinit(Allocator);
        tabstops.deinit(Allocator);
    }

    // test "TERMINAL payload rejects noncanonical state"
    #[test]
    fn terminal_payload_rejects_noncanonical_state() {
        // A tab-stop set for different dimensions cannot represent this header.
        let mut tabstops =
            TerminalTabstops::init(Allocator, test_header().columns as usize - 1, 0).unwrap();
        // defer tabstops.deinit(testing.allocator);
        let mut palette = terminal_color::DynamicPalette::init(terminal_color::DEFAULT);
        let mut encoded = [0u8; 2048];
        let mut writer = io::FixedWriter::new(&mut encoded);
        assert_eq!(
            Err(PayloadEncodeError::InvalidTabStops),
            encode_payload(&test_header(), &tabstops, &palette, b"", b"", &mut writer),
        );
        assert_eq!(0usize, writer.end);

        // Unmasked current colors must remain equal to their originals.
        tabstops.cols = test_header().columns as usize;
        palette.current[7] = RGB { r: 1, g: 2, b: 3 };
        let mut writer = io::FixedWriter::new(&mut encoded);
        assert_eq!(
            Err(PayloadEncodeError::InvalidPalette),
            encode_payload(&test_header(), &tabstops, &palette, b"", b"", &mut writer),
        );
        assert_eq!(0usize, writer.end);

        tabstops.deinit(Allocator);
    }

    // test "TERMINAL payload ignores tab-stop padding"
    #[test]
    fn terminal_payload_ignores_tab_stop_padding() {
        let tabstops =
            TerminalTabstops::init(Allocator, test_header().columns as usize, 0).unwrap();
        // defer tabstops.deinit(testing.allocator);
        let mut palette = terminal_color::DynamicPalette::init(terminal_color::DEFAULT);
        palette.set(1, RGB { r: 1, g: 2, b: 3 });

        let mut destination = io::AllocatingWriter::new();
        encode_payload(
            &test_header(),
            &tabstops,
            &palette,
            b"pwd",
            b"title",
            &mut destination,
        )
        .unwrap();

        // Only two bits in the last byte correspond to terminal columns.
        let tabstop_len = (test_header().columns as usize + 7) / 8;
        let last_tabstop = Header::LEN + tabstop_len - 1;
        let mut invalid_padding = destination.written().to_vec();
        // defer testing.allocator.free(invalid_padding);
        invalid_padding[last_tabstop] |= 1 << 7;
        let mut padding_reader = io::FixedReader::new(&invalid_padding);
        let decoded = decode_payload(&mut padding_reader, Allocator).unwrap();
        // defer decoded.deinit(testing.allocator);
        for column in 0..test_header().columns as usize {
            assert_eq!(tabstops.get(column), decoded.tabstops.get(column));
        }

        decoded.deinit(Allocator);
        tabstops.deinit(Allocator);
    }

    // test "TERMINAL record encodes native terminal state"
    #[test]
    fn terminal_record_encodes_native_terminal_state() {
        let mut terminal = Terminal::init(
            Allocator,
            terminal_options(10, 3, Some(12_345), Some(67)),
        )
        .unwrap();
        // defer terminal.deinit(testing.allocator);

        // Exercise every native header section plus sentinel-backed strings.
        terminal.width_px = 800;
        terminal.height_px = 600;
        terminal.scrolling_region = ScrollingRegion {
            top: 1,
            bottom: 2,
            left: 2,
            right: 8,
        };
        terminal.status_display = terminal_ansi::StatusDisplay::StatusLine;
        terminal.previous_char = Some('X' as u32);
        terminal.cursor = TerminalCursor {
            is_default: false,
            default_style: CursorStyle::Underline,
            default_blink: Some(true),
        };
        terminal.flags.shell_redraws_prompt = terminal_osc::semantic_prompt::Redraw::Last;
        terminal.flags.modify_other_keys_2 = true;
        terminal.flags.mouse_event = terminal_mouse::Event::Button;
        terminal.flags.mouse_format = terminal_mouse::Format::Sgr;
        terminal.flags.mouse_shift_capture = MouseShiftCapture::True;
        terminal.flags.password_input = true;
        terminal.mouse_shape = terminal_mouse::Shape::Pointer;
        terminal
            .modes
            .values
            .set_mode(terminal_modes::Mode::DisableKeyboard, true);
        terminal
            .modes
            .saved
            .set_mode(terminal_modes::Mode::InBandSizeReports, true);
        terminal
            .modes
            .default
            .set_mode(terminal_modes::Mode::BracketedPaste, true);
        terminal.colors.background = DynamicRGB {
            default: Some(RGB { r: 1, g: 2, b: 3 }),
            override_: Some(RGB { r: 4, g: 5, b: 6 }),
        };
        terminal.colors.palette.set(7, RGB { r: 7, g: 8, b: 9 });
        terminal.tabstops.reset(0);
        terminal.tabstops.set(1);
        terminal.tabstops.set(9);
        terminal.set_pwd(b"file:///tmp/native").unwrap();
        terminal.set_title(b"native title").unwrap();

        // Encode and restore through the public record codec.
        let mut destination = io::AllocatingWriter::new();
        let mut stream = record::Writer::init(&mut destination);
        // defer stream.deinit();
        encode(&terminal, &mut stream).unwrap();

        let written = stream.destination().written().to_vec();
        stream.deinit();
        let mut source = io::FixedReader::new(&written);
        let mut restored = decode(&mut source, Allocator).unwrap();
        // defer restored.deinit(testing.allocator);

        assert_eq!(10u16, restored.cols);
        assert_eq!(3u16, restored.rows);
        assert_eq!(800u32, restored.width_px);
        assert_eq!(600u32, restored.height_px);
        assert_eq!(terminal.scrolling_region, restored.scrolling_region);
        assert_eq!(terminal.status_display, restored.status_display);
        assert_eq!(TerminalScreenKey::Primary, restored.screens.active_key);
        assert!(restored.screens.get(TerminalScreenKey::Alternate).is_none());
        assert_eq!(Some('X' as u32), restored.previous_char);
        assert_eq!(terminal.cursor, restored.cursor);
        assert_eq!(
            terminal.flags.shell_redraws_prompt,
            restored.flags.shell_redraws_prompt,
        );
        assert_eq!(
            terminal.flags.modify_other_keys_2,
            restored.flags.modify_other_keys_2,
        );
        assert_eq!(terminal.flags.mouse_event, restored.flags.mouse_event);
        assert_eq!(terminal.flags.mouse_format, restored.flags.mouse_format);
        assert_eq!(
            terminal.flags.mouse_shift_capture,
            restored.flags.mouse_shift_capture,
        );
        assert_eq!(terminal.flags.password_input, restored.flags.password_input);
        assert_eq!(terminal.mouse_shape, restored.mouse_shape);
        assert_eq!(terminal.modes, restored.modes);
        assert_eq!(terminal.colors, restored.colors);
        // SAFETY: `screens.active` is a live screen owned by `restored`;
        // shared reads only.
        unsafe {
            assert_eq!(
                12_345usize,
                (*restored.screens.active).pages.limits.bytes.explicit,
            );
            assert_eq!(
                67usize,
                (*restored.screens.active).pages.limits.lines.explicit,
            );
        }
        assert!(restored.tabstops.get(1));
        assert!(restored.tabstops.get(9));
        assert_eq!(b"file:///tmp/native", restored.get_pwd().unwrap());
        assert_eq!(b"native title", restored.get_title().unwrap());

        restored.deinit(Allocator);
        terminal.deinit(Allocator);
    }

    // test "TERMINAL record declares an initialized alternate screen"
    #[test]
    fn terminal_record_declares_an_initialized_alternate_screen() {
        let mut terminal = Terminal::init(Allocator, terminal_options(10, 3, None, None)).unwrap();
        // defer terminal.deinit(testing.allocator);
        terminal
            .switch_screen(TerminalScreenKey::Alternate)
            .unwrap();

        let mut destination = io::AllocatingWriter::new();
        let mut stream = record::Writer::init(&mut destination);
        // defer stream.deinit();
        encode(&terminal, &mut stream).unwrap();

        let written = stream.destination().written().to_vec();
        stream.deinit();
        let mut source = io::FixedReader::new(&written);
        let mut restored = decode(&mut source, Allocator).unwrap();
        // defer restored.deinit(testing.allocator);

        let primary = restored.screens.get(TerminalScreenKey::Primary).unwrap();
        let alternate = restored.screens.get(TerminalScreenKey::Alternate).unwrap();
        assert_eq!(TerminalScreenKey::Alternate, restored.screens.active_key);
        assert_eq!(alternate, restored.screens.active);
        assert!(primary != alternate);
        // SAFETY: `alternate` is a live screen owned by `restored`; shared
        // reads only.
        unsafe {
            assert_eq!(10u16, (*alternate).pages.cols);
            assert_eq!(3u16, (*alternate).pages.rows);
            assert_eq!(0usize, (*alternate).pages.limits.bytes.explicit);
        }

        restored.deinit(Allocator);
        terminal.deinit(Allocator);
    }

    // test "TERMINAL payload failure emits no incomplete record"
    #[test]
    fn terminal_payload_failure_emits_no_incomplete_record() {
        let mut terminal = Terminal::init(Allocator, terminal_options(10, 3, None, None)).unwrap();
        // defer terminal.deinit(testing.allocator);

        let mut destination = io::AllocatingWriter::new();
        destination.write_all(b"prefix").unwrap();
        let mut stream = record::Writer::init(&mut destination);
        // defer stream.deinit();

        // Payload validation occurs in the reusable record scratch buffer. The
        // invalid TERMINAL is never emitted to the destination.
        terminal.colors.palette.current[7] = RGB { r: 1, g: 2, b: 3 };
        assert_eq!(
            Err(EncodeError::InvalidPalette),
            encode(&terminal, &mut stream),
        );
        assert_eq!(b"prefix", stream.destination().written());

        stream.deinit();
        terminal.deinit(Allocator);
    }

    // test "TERMINAL record decoding rejects malformed input transactionally"
    #[test]
    fn terminal_record_decoding_rejects_malformed_input_transactionally() {
        // A valid record of another type is not accepted as TERMINAL state.
        let mut wrong_tag = io::AllocatingWriter::new();
        let mut wrong_tag_stream = record::Writer::init(&mut wrong_tag);
        // defer wrong_tag_stream.deinit();
        {
            let _ = wrong_tag_stream.begin(record::Tag::Screen);
            // errdefer wrong_tag_stream.cancel();
            wrong_tag_stream.finish().unwrap();
        }
        let wrong_tag_written = wrong_tag_stream.destination().written().to_vec();
        wrong_tag_stream.deinit();
        let mut wrong_tag_source = io::FixedReader::new(&wrong_tag_written);
        assert_eq!(
            Err(DecodeError::UnexpectedRecordTag),
            decode(&mut wrong_tag_source, Allocator).map(|mut t| t.deinit(Allocator)),
        );

        // Build one canonical record for exhaustive truncation and allocation
        // failure checks below.
        let mut terminal = Terminal::init(Allocator, terminal_options(10, 3, None, None)).unwrap();
        // defer terminal.deinit(testing.allocator);
        terminal.set_pwd(b"pwd").unwrap();
        terminal.set_title(b"title").unwrap();

        let mut encoded = io::AllocatingWriter::new();
        let mut encoded_stream = record::Writer::init(&mut encoded);
        // defer encoded_stream.deinit();
        encode(&terminal, &mut encoded_stream).unwrap();

        let encoded_written = encoded_stream.destination().written().to_vec();
        encoded_stream.deinit();
        for fixture_len in 0..encoded_written.len() {
            let mut source = io::FixedReader::new(&encoded_written[0..fixture_len]);
            match decode(&mut source, Allocator) {
                Ok(mut restored) => {
                    restored.deinit(Allocator);
                    panic!("truncated record decoded");
                }
                Err(_) => continue,
            }
        }

        // var failing = testing.FailingAllocator.init(testing.allocator, .{ .fail_index = 0 });
        // PORT NOTE: FailingAllocator → the SNAPSHOT_TW tripwire seam (the
        // subsystem's failure-injection convention; see mod.rs).
        super::super::tw::error_always(super::super::tw::SnapshotFp::Alloc);
        let mut failing_source = io::FixedReader::new(&encoded_written);
        assert_eq!(
            Err(DecodeError::OutOfMemory),
            decode(&mut failing_source, Allocator).map(|mut t| t.deinit(Allocator)),
        );
        super::super::tw::end(rustty_util::tripwire::ResetMode::Reset).unwrap();

        terminal.deinit(Allocator);
    }

    // PORT NOTE: the reference `Terminal.init(testing.io, testing.allocator,
    // .{ .cols, .rows, ... })` relies on `Terminal.Options` field defaults;
    // this helper supplies them (see `terminal.rs`'s test `opts`).
    fn terminal_options(
        cols: u16,
        rows: u16,
        max_scrollback_bytes: Option<usize>,
        max_scrollback_lines: Option<usize>,
    ) -> TerminalOptions {
        TerminalOptions {
            cols,
            rows,
            // PORT NOTE: the reference `Options.max_scrollback_bytes` default
            // is 10,000 bytes; tests that do not name it inherit that value.
            max_scrollback_bytes: Some(max_scrollback_bytes.unwrap_or(10_000)),
            max_scrollback_lines,
            colors: TerminalColors::default(),
            default_modes: terminal_modes::ModePacked::default(),
            default_cursor_style: CursorStyle::Block,
            default_cursor_blink: Some(false),
            kitty_image_storage_limit: 320 * 1000 * 1000,
            kitty_image_loading_limits: crate::kitty::graphics_image::Limits::DIRECT,
        }
    }
}
