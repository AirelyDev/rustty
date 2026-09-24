//! Port of `libvaxis` `Parser.zig` (libvaxis 0.6.0: 1331 LOC, 36 test) — the ANSI input parser: the
//! ground/escape/csi/osc/dcs/sos/pm/apc/ss2/ss3 state machine, the Kitty keyboard
//! protocol (`u21` codepoint + modifier decode), SGR + legacy xterm mouse, bracketed
//! paste + OSC-52 clipboard paste, the legacy/tilde/SS3 key tables, XTWINOPS in-band
//! resize, DECRPM/DSR/primary-DA/kitty-multi-cursor capability probes, color-report +
//! color-scheme reports, and the c1b `uucode` grapheme-text path.
//!
//! PORT NOTE (THE KEY DECISION, VAXIS_C3A_DESIGN §2): `parse` maps `&[u8] →
//! (Option<Event>, n)` and is PURE (no `Tty`/`Loop`/`queue` coupling — those are chunk
//! 3b). A returned `Key.text` borrows EITHER `input` (ground path) OR the Parser's
//! internal `buf` (CSI paths), so `parse<'a>(&'a mut self, input: &'a [u8]) ->
//! Result<ParseResult<'a>, ParseError>` unifies both under one `'a`. Holding the result
//! keeps `self` mutably borrowed — the faithful-and-safe reading of Zig's ring-buffer
//! caveat (Zig lets a second `parse` clobber `buf` under a still-live prior slice; Rust
//! forbids it — a strictly sound tightening).
//!
//! PORT NOTE (dropped): the Zig `paste_allocator` param + the error-union's OOM arm are
//! dropped (crate precedent — `Vec` self-allocates, infallible; `Paste` owns its `Vec`,
//! no "caller must free"). The `log.warn`/`log.debug` calls are dropped (crate
//! precedent). `std.debug.assert` → `debug_assert!`.
#![allow(non_upper_case_globals)]

use crate::event::Event;
use crate::key::{self, Key, Modifiers};
use crate::mouse::{self, Mouse};
use crate::{cell, uucode, Winsize};
use rustty_zigstd::fmt::{parse_int_type, parse_unsigned_type, IntType};

/// The return type of our parse method. Contains an Event and the number of
/// bytes read from the buffer.
///
/// PORT NOTE: Zig `Result` → `ParseResult` (avoids shadowing `std::result::Result`).
pub struct ParseResult<'a> {
    pub event: Option<Event<'a>>,
    pub n: usize,
}

/// The error set of [`Parser::parse`].
///
/// PORT NOTE: Zig's `parse` error set = `error.InvalidUTF8` (parseGround) ∪
/// `Color.rgbFromSpec` errors ∪ base64 `Decoder` errors ∪ `Allocator.Error` (dropped —
/// `Vec` is infallible). No test exercises `Err` (all 34 assert `Ok`).
// PORT NOTE (WS20): `Copy` added — every variant is already `Copy`, and
// `TtyError::Parse(ParseError)` (the Windows `nextEvent` error path) needs
// `TtyError` to stay `Copy` as it is on POSIX.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseError {
    InvalidUtf8,                 // parseGround
    Color(cell::ColorSpecError), // rgbFromSpec (OSC 4/10/11/12)
    Base64,                      // std.base64.standard.Decoder failure (OSC 52)
}

// PORT NOTE: `mouse_bits` is a Zig struct-of-consts (`motion`/`buttons`/`shift`/`alt`/
// `ctrl` are `u8`, `leave` is `u16`); ported as a private const module of the same types.
mod mouse_bits {
    pub const MOTION: u8 = 0b0010_0000;
    pub const BUTTONS: u8 = 0b1100_0011;
    pub const SHIFT: u8 = 0b0000_0100;
    pub const ALT: u8 = 0b0000_1000;
    pub const CTRL: u8 = 0b0001_0000;
    pub const LEAVE: u16 = 0b1_0000_0000;
}

// the state of the parser
//
// PORT NOTE: `State` is declared in Zig but used only as documentation — the dispatch is
// a direct `input[1]` switch, no stored state. Kept `#[allow(dead_code)]` for parity.
#[allow(dead_code)]
enum State {
    Ground,
    Escape,
    Csi,
    Osc,
    Dcs,
    Sos,
    Pm,
    Apc,
    Ss2,
    Ss3,
}

/// The ANSI input parser.
pub struct Parser {
    // a buffer to temporarily store text in. We need this to encode
    // text-as-codepoints
    buf: [u8; 128],
}

impl Default for Parser {
    fn default() -> Self {
        // PORT NOTE: Zig `buf: [128]u8 = undefined` → zeroed here (never read before
        // written — the text-encode arms fill it before slicing).
        Parser { buf: [0u8; 128] }
    }
}

impl Parser {
    /// Parse the first event from the input buffer. If a completion event is not
    /// present, Result.event will be null and Result.n will be 0
    ///
    /// If an unknown event is found, Result.event will be null and Result.n will be
    /// greater than 0
    pub fn parse<'a>(&'a mut self, input: &'a [u8]) -> Result<ParseResult<'a>, ParseError> {
        debug_assert!(!input.is_empty());

        // We gate this for len > 1 so we can detect singular escape key presses
        if input[0] == 0x1b && input.len() > 1 {
            match input[1] {
                0x4F => Ok(parse_ss3(input)),
                0x50 => Ok(skip_until_st(input)),            // DCS
                0x58 => Ok(skip_until_st(input)),            // SOS
                0x5B => Ok(parse_csi(input, &mut self.buf)), // CSI
                0x5D => parse_osc(input),
                0x5E => Ok(skip_until_st(input)), // PM
                0x5F => Ok(parse_apc(input)),
                _ => {
                    // Anything else is an "alt + <char>" keypress
                    let key = Key {
                        mods: Modifiers {
                            alt: true,
                            ..Default::default()
                        },
                        ..key_with_codepoint(input[1] as u32)
                    };
                    Ok(ParseResult {
                        event: Some(Event::KeyPress(key)),
                        n: 2,
                    })
                }
            }
        } else {
            parse_ground(input)
        }
    }
}

/// Parse ground state
fn parse_ground(input: &[u8]) -> Result<ParseResult<'_>, ParseError> {
    debug_assert!(!input.is_empty());

    let b = input[0];
    let mut n: usize = 1;
    // ground state generates keypresses when parsing input. We
    // generally get ascii characters, but anything less than
    // 0x20 is a Ctrl+<c> keypress. We map these to lowercase
    // ascii characters when we can
    let key: Key = match b {
        0x00 => Key {
            mods: Modifiers {
                ctrl: true,
                ..Default::default()
            },
            ..key_with_codepoint(b'@' as u32)
        },
        0x08 => key_with_codepoint(key::backspace),
        0x09 => key_with_codepoint(key::tab),
        0x0A => Key {
            mods: Modifiers {
                ctrl: true,
                ..Default::default()
            },
            ..key_with_codepoint(b'j' as u32)
        },
        0x0D => key_with_codepoint(key::enter),
        0x01..=0x07 | 0x0B..=0x0C | 0x0E..=0x1A => Key {
            mods: Modifiers {
                ctrl: true,
                ..Default::default()
            },
            ..key_with_codepoint((b + 0x60) as u32)
        },
        0x1B => {
            debug_assert!(input.len() == 1); // parseGround expects len == 1 with 0x1b
            key_with_codepoint(key::escape)
        }
        0x7F => key_with_codepoint(key::backspace),
        _ => {
            let mut iter = uucode::utf8::Iterator::init(input);
            // return null if we don't have a valid codepoint
            let first_cp = match iter.next() {
                Some(cp) => cp,
                None => return Err(ParseError::InvalidUtf8),
            };

            n = match utf8_codepoint_sequence_length_checked(first_cp) {
                Some(len) => len,
                None => return Err(ParseError::InvalidUtf8),
            };

            // Check if we have a multi-codepoint grapheme
            let mut code = first_cp;
            let mut grapheme_iter =
                uucode::grapheme::Iterator::init(uucode::utf8::Iterator::init(input));
            let mut grapheme_len: usize = 0;
            let mut cp_count: usize = 0;

            while let Some(result) = grapheme_iter.next_code_point() {
                cp_count += 1;
                if result.is_break {
                    // Found the first grapheme boundary
                    grapheme_len = grapheme_iter.i;
                    break;
                }
            }

            if grapheme_len > 0 {
                n = grapheme_len;
                if cp_count > 1 {
                    code = key::multicodepoint;
                }
            }

            Key {
                text: Some(&input[0..n]),
                ..key_with_codepoint(code)
            }
        }
    };

    Ok(ParseResult {
        event: Some(Event::KeyPress(key)),
        n,
    })
}

fn parse_ss3(input: &[u8]) -> ParseResult<'static> {
    if input.len() < 3 {
        return ParseResult { event: None, n: 0 };
    }
    let key = match input[2] {
        0x1B => return ParseResult { event: None, n: 2 },
        b'A' => key_with_codepoint(key::up),
        b'B' => key_with_codepoint(key::down),
        b'C' => key_with_codepoint(key::right),
        b'D' => key_with_codepoint(key::left),
        b'E' => key_with_codepoint(key::kp_begin),
        b'F' => key_with_codepoint(key::end),
        b'H' => key_with_codepoint(key::home),
        b'P' => key_with_codepoint(key::f1),
        b'Q' => key_with_codepoint(key::f2),
        b'R' => key_with_codepoint(key::f3),
        b'S' => key_with_codepoint(key::f4),
        _ => {
            // log.warn("unhandled ss3: {x}", .{input[2]}) — dropped (crate precedent)
            return ParseResult { event: None, n: 3 };
        }
    };
    ParseResult {
        event: Some(Event::KeyPress(key)),
        n: 3,
    }
}

fn parse_apc(input: &[u8]) -> ParseResult<'static> {
    if input.len() < 3 {
        return ParseResult { event: None, n: 0 };
    }
    let end = match index_of_scalar_pos(input, 2, 0x1b) {
        Some(e) => e,
        None => return ParseResult { event: None, n: 0 },
    };
    let sequence = &input[0..end + 1 + 1];

    match input[2] {
        b'G' => ParseResult {
            event: Some(Event::CapKittyGraphics),
            n: sequence.len(),
        },
        _ => ParseResult {
            event: None,
            n: sequence.len(),
        },
    }
}

/// Skips sequences until we see an ST (String Terminator, ESC \)
fn skip_until_st(input: &[u8]) -> ParseResult<'static> {
    if input.len() < 3 {
        return ParseResult { event: None, n: 0 };
    }
    let end = match index_of_scalar_pos(input, 2, 0x1b) {
        Some(e) => e,
        None => return ParseResult { event: None, n: 0 },
    };
    if input.len() < end + 1 + 1 {
        return ParseResult { event: None, n: 0 };
    }
    let sequence = &input[0..end + 1 + 1];
    ParseResult {
        event: None,
        n: sequence.len(),
    }
}

/// Parses an OSC sequence
fn parse_osc(input: &[u8]) -> Result<ParseResult<'static>, ParseError> {
    if input.len() < 3 {
        return Ok(ParseResult { event: None, n: 0 });
    }
    let mut bel_terminated = false;
    // end is the index of the terminating byte(s) (either the last byte of an
    // ST or BEL)
    let end: usize = {
        let esc_result = skip_until_st(input);
        if esc_result.n > 0 {
            esc_result.n
        } else {
            // No escape, could be BEL terminated
            let bel = match index_of_scalar_pos(input, 2, 0x07) {
                Some(b) => b,
                None => return Ok(ParseResult { event: None, n: 0 }),
            };
            bel_terminated = true;
            bel + 1
        }
    };

    // The complete OSC sequence
    let sequence = &input[0..end];

    let null_event = ParseResult {
        event: None,
        n: sequence.len(),
    };

    let semicolon_idx = match index_of_scalar_pos(input, 2, b';') {
        Some(i) => i,
        None => return Ok(null_event),
    };
    let ps = match parse_unsigned(&input[2..semicolon_idx], U8) {
        Some(v) => v as u8,
        None => return Ok(null_event),
    };

    match ps {
        4 => {
            let color_idx_delim = match index_of_scalar_pos(input, semicolon_idx + 1, b';') {
                Some(i) => i,
                None => return Ok(null_event),
            };
            let ps_idx = match parse_unsigned(&input[semicolon_idx + 1..color_idx_delim], U8) {
                Some(v) => v as u8,
                None => return Ok(null_event),
            };
            let color_spec = if bel_terminated {
                &input[color_idx_delim + 1..sequence.len() - 1]
            } else {
                &input[color_idx_delim + 1..sequence.len() - 2]
            };

            let color = cell::Color::rgb_from_spec(color_spec).map_err(ParseError::Color)?;
            let event = cell::Report {
                kind: cell::Kind::Index(ps_idx),
                value: rgb_of(color),
            };
            Ok(ParseResult {
                event: Some(Event::ColorReport(event)),
                n: sequence.len(),
            })
        }
        // PORT NOTE (R1): Zig's explicit `10, 11, 12 =>` list is kept verbatim (not
        // collapsed to a range) — the inner `match ps` maps each to fg/bg/cursor.
        #[allow(clippy::manual_range_patterns)]
        10 | 11 | 12 => {
            let color_spec = if bel_terminated {
                &input[semicolon_idx + 1..sequence.len() - 1]
            } else {
                &input[semicolon_idx + 1..sequence.len() - 2]
            };

            let color = cell::Color::rgb_from_spec(color_spec).map_err(ParseError::Color)?;
            let event = cell::Report {
                kind: match ps {
                    10 => cell::Kind::Fg,
                    11 => cell::Kind::Bg,
                    12 => cell::Kind::Cursor,
                    _ => unreachable!(),
                },
                value: rgb_of(color),
            };
            Ok(ParseResult {
                event: Some(Event::ColorReport(event)),
                n: sequence.len(),
            })
        }
        52 => {
            if input[semicolon_idx + 1] != b'c' {
                return Ok(null_event);
            }
            let payload = if bel_terminated {
                &input[semicolon_idx + 3..sequence.len() - 1]
            } else {
                &input[semicolon_idx + 3..sequence.len() - 2]
            };
            let text = base64_standard_decode(payload)?;
            // log.debug("decoded paste: {s}", .{text}) — dropped (crate precedent)
            Ok(ParseResult {
                event: Some(Event::Paste(text)),
                n: sequence.len(),
            })
        }
        _ => Ok(null_event),
    }
}

fn parse_csi<'b>(input: &[u8], text_buf: &'b mut [u8]) -> ParseResult<'b> {
    if input.len() < 3 {
        return ParseResult { event: None, n: 0 };
    }
    // We start iterating at index 2 to get past the '['
    let sequence = {
        let mut found: Option<&[u8]> = None;
        for (offset, &b) in input[2..].iter().enumerate() {
            let i = offset + 2;
            match b {
                0x40..=0xFF => {
                    found = Some(&input[0..i + 1]);
                    break;
                }
                _ => continue,
            }
        }
        match found {
            Some(s) => s,
            None => return ParseResult { event: None, n: 0 },
        }
    };
    let null_event = ParseResult {
        event: None,
        n: sequence.len(),
    };

    let final_byte = sequence[sequence.len() - 1];
    match final_byte {
        b'A' | b'B' | b'C' | b'D' | b'E' | b'F' | b'H' | b'P' | b'Q' | b'R' | b'S' => {
            // Legacy keys
            // CSI {ABCDEFHPQS}
            // CSI 1 ; modifier:event_type {ABCDEFHPQS}

            // Split first into fields delimited by ';'
            let mut field_iter = sequence[2..sequence.len() - 1].split(|&c| c == b';');

            // skip the first field
            let _ = field_iter.next();

            let mut is_release = false;
            let mut key: Key<'b> = key_with_codepoint(match final_byte {
                b'A' => key::up,
                b'B' => key::down,
                b'C' => key::right,
                b'D' => key::left,
                b'E' => key::kp_begin,
                b'F' => key::end,
                b'H' => key::home,
                b'P' => key::f1,
                b'Q' => key::f2,
                b'R' => key::f3,
                b'S' => key::f4,
                _ => return null_event,
            });

            let mut text_len: Option<usize> = None;

            // field2: modifier_mask:event_type
            if let Some(field_buf) = field_iter.next() {
                let mut param_iter = field_buf.split(|&c| c == b':');
                let modifier_buf = param_iter.next().unwrap();
                let modifier_mask = match parse_param(modifier_buf, Some(1), U8) {
                    Some(v) => v,
                    None => return null_event,
                };
                key.mods = Modifiers::from_u8(modifier_mask.saturating_sub(1) as u8);

                if let Some(event_type_buf) = param_iter.next() {
                    is_release = event_type_buf == b"3";
                }
            }

            // field3: text_as_codepoint[:text_as_codepoint]
            if let Some(field_buf) = field_iter.next() {
                let mut param_iter = field_buf.split(|&c| c == b':');
                let mut total: usize = 0;
                for cp_buf in param_iter.by_ref() {
                    let cp = match parse_param(cp_buf, None, U21) {
                        Some(v) => v,
                        None => return null_event,
                    };
                    total += match utf8_encode(cp, &mut text_buf[total..]) {
                        Some(len) => len,
                        None => return null_event,
                    };
                }
                text_len = Some(total);
            }

            if let Some(total) = text_len {
                key.text = Some(&text_buf[0..total]);
            }

            let event = if is_release {
                Event::KeyRelease(key)
            } else {
                Event::KeyPress(key)
            };
            ParseResult {
                event: Some(event),
                n: sequence.len(),
            }
        }
        b'Z' => {
            // Legacy keys
            // CSI Z
            ParseResult {
                event: Some(Event::KeyPress(Key {
                    mods: Modifiers {
                        shift: true,
                        ..Default::default()
                    },
                    ..key_with_codepoint(key::tab)
                })),
                n: sequence.len(),
            }
        }
        b'~' => {
            // Legacy keys
            // CSI number ~
            // CSI number ; modifier ~
            // CSI number ; modifier:event_type ; text_as_codepoint ~
            let mut field_iter = sequence[2..sequence.len() - 1].split(|&c| c == b';');
            let number_buf = field_iter.next().unwrap(); // always will have one field
            let number = match parse_param(number_buf, None, U16) {
                Some(v) => v,
                None => return null_event,
            };

            let mut key: Key<'b> = key_with_codepoint(match number {
                2 => key::insert,
                3 => key::delete,
                5 => key::page_up,
                6 => key::page_down,
                7 => key::home,
                8 => key::end,
                11 => key::f1,
                12 => key::f2,
                13 => key::f3,
                14 => key::f4,
                15 => key::f5,
                17 => key::f6,
                18 => key::f7,
                19 => key::f8,
                20 => key::f9,
                21 => key::f10,
                23 => key::f11,
                24 => key::f12,
                200 => {
                    return ParseResult {
                        event: Some(Event::PasteStart),
                        n: sequence.len(),
                    }
                }
                201 => {
                    return ParseResult {
                        event: Some(Event::PasteEnd),
                        n: sequence.len(),
                    }
                }
                57427 => key::kp_begin,
                _ => return null_event,
            });

            let mut is_release = false;
            let mut text_len: Option<usize> = None;

            // field2: modifier_mask:event_type
            if let Some(field_buf) = field_iter.next() {
                let mut param_iter = field_buf.split(|&c| c == b':');
                let modifier_buf = param_iter.next().unwrap();
                let modifier_mask = match parse_param(modifier_buf, Some(1), U8) {
                    Some(v) => v,
                    None => return null_event,
                };
                key.mods = Modifiers::from_u8(modifier_mask.saturating_sub(1) as u8);

                if let Some(event_type_buf) = param_iter.next() {
                    is_release = event_type_buf == b"3";
                }
            }

            // field3: text_as_codepoint[:text_as_codepoint]
            if let Some(field_buf) = field_iter.next() {
                let mut param_iter = field_buf.split(|&c| c == b':');
                let mut total: usize = 0;
                for cp_buf in param_iter.by_ref() {
                    let cp = match parse_param(cp_buf, None, U21) {
                        Some(v) => v,
                        None => return null_event,
                    };
                    total += match utf8_encode(cp, &mut text_buf[total..]) {
                        Some(len) => len,
                        None => return null_event,
                    };
                }
                text_len = Some(total);
            }

            if let Some(total) = text_len {
                key.text = Some(&text_buf[0..total]);
            }

            let event = if is_release {
                Event::KeyRelease(key)
            } else {
                Event::KeyPress(key)
            };
            ParseResult {
                event: Some(event),
                n: sequence.len(),
            }
        }

        b'I' => ParseResult {
            event: Some(Event::FocusIn),
            n: sequence.len(),
        },
        b'O' => ParseResult {
            event: Some(Event::FocusOut),
            n: sequence.len(),
        },
        b'M' | b'm' => parse_mouse(sequence, input),
        b'c' => {
            // Primary DA (CSI ? Pm c)
            debug_assert!(sequence.len() >= 4); // ESC [ ? c == 4 bytes
            match input[2] {
                b'?' => ParseResult {
                    event: Some(Event::CapDa1),
                    n: sequence.len(),
                },
                _ => null_event,
            }
        }
        b'n' => {
            // Device Status Report
            // CSI Ps n
            // CSI ? Ps n
            debug_assert!(sequence.len() >= 3);
            match sequence[2] {
                b'?' => {
                    let delim_idx = match index_of_scalar_pos(input, 3, b';') {
                        Some(i) => i,
                        None => return null_event,
                    };
                    let ps = match parse_unsigned(&input[3..delim_idx], U16) {
                        Some(v) => v,
                        None => return null_event,
                    };
                    match ps {
                        997 => {
                            // Color scheme update (CSI 997 ; Ps n)
                            // See https://github.com/contour-terminal/contour/blob/master/docs/vt-extensions/color-palette-update-notifications.md
                            match sequence[delim_idx + 1] {
                                b'1' => ParseResult {
                                    event: Some(Event::ColorScheme(cell::Scheme::Dark)),
                                    n: sequence.len(),
                                },
                                b'2' => ParseResult {
                                    event: Some(Event::ColorScheme(cell::Scheme::Light)),
                                    n: sequence.len(),
                                },
                                _ => null_event,
                            }
                        }
                        _ => null_event,
                    }
                }
                _ => null_event,
            }
        }
        b't' => {
            // XTWINOPS
            // Split first into fields delimited by ';'
            let mut iter = sequence[2..sequence.len() - 1].split(|&c| c == b';');
            let ps = iter.next().unwrap();
            if b"48" == ps {
                // in band window resize
                // CSI 48 ; height ; width ; height_pix ; width_pix t
                let height_char = match iter.next() {
                    Some(v) => v,
                    None => return null_event,
                };
                let width_char = match iter.next() {
                    Some(v) => v,
                    None => return null_event,
                };
                let height_pix = iter.next().unwrap_or(b"0");
                let width_pix = iter.next().unwrap_or(b"0");

                let winsize = Winsize {
                    rows: match parse_unsigned(height_char, U16) {
                        Some(v) => v as u16,
                        None => return null_event,
                    },
                    cols: match parse_unsigned(width_char, U16) {
                        Some(v) => v as u16,
                        None => return null_event,
                    },
                    x_pixel: match parse_unsigned(width_pix, U16) {
                        Some(v) => v as u16,
                        None => return null_event,
                    },
                    y_pixel: match parse_unsigned(height_pix, U16) {
                        Some(v) => v as u16,
                        None => return null_event,
                    },
                };
                return ParseResult {
                    event: Some(Event::Winsize(winsize)),
                    n: sequence.len(),
                };
            }
            null_event
        }
        b'u' => {
            // Kitty keyboard
            // CSI unicode-key-code:alternate-key-codes ; modifiers:event-type ; text-as-codepoints u
            // Not all fields will be present. Only unicode-key-code is
            // mandatory

            if sequence.len() > 2 && sequence[2] == b'?' {
                return ParseResult {
                    event: Some(Event::CapKittyKeyboard),
                    n: sequence.len(),
                };
            }

            // PORT NOTE: Zig `key.codepoint = undefined` → 0 placeholder (overwritten in
            // field1 before any read).
            let mut key: Key<'b> = key_with_codepoint(0);
            // Split first into fields delimited by ';'
            let mut field_iter = sequence[2..sequence.len() - 1].split(|&c| c == b';');

            {
                // field 1
                // unicode-key-code:shifted_codepoint:base_layout_codepoint
                let field_buf = field_iter.next().unwrap(); // There will always be at least one field
                let mut param_iter = field_buf.split(|&c| c == b':');
                let codepoint_buf = param_iter.next().unwrap();
                key.codepoint = match parse_param(codepoint_buf, None, U21) {
                    Some(v) => v,
                    None => return null_event,
                };

                if let Some(shifted_cp_buf) = param_iter.next() {
                    key.shifted_codepoint = parse_param(shifted_cp_buf, None, U21);
                }
                if let Some(base_layout_buf) = param_iter.next() {
                    key.base_layout_codepoint = parse_param(base_layout_buf, None, U21);
                }
            }

            let mut is_release = false;
            let mut text_len: Option<usize> = None;

            // field2: modifier_mask:event_type
            if let Some(field_buf) = field_iter.next() {
                let mut param_iter = field_buf.split(|&c| c == b':');
                let modifier_buf = param_iter.next().unwrap();
                let modifier_mask = match parse_param(modifier_buf, Some(1), U8) {
                    Some(v) => v,
                    None => return null_event,
                };
                key.mods = Modifiers::from_u8(modifier_mask.saturating_sub(1) as u8);

                if let Some(event_type_buf) = param_iter.next() {
                    is_release = event_type_buf == b"3";
                }
            }

            // field3: text_as_codepoint[:text_as_codepoint]
            if let Some(field_buf) = field_iter.next() {
                let mut param_iter = field_buf.split(|&c| c == b':');
                let mut total: usize = 0;
                for cp_buf in param_iter.by_ref() {
                    let cp = match parse_param(cp_buf, None, U21) {
                        Some(v) => v,
                        None => return null_event,
                    };
                    total += match utf8_encode(cp, &mut text_buf[total..]) {
                        Some(len) => len,
                        None => return null_event,
                    };
                }
                text_len = Some(total);
            }

            {
                // We check if we have *only* shift, no text, and a printable character. This can
                // happen when we have disambiguate on and a key is pressed and encoded as CSI u,
                // for example shift + space can produce CSI 32 ; 2 u
                let mod_test = Modifiers {
                    shift: true,
                    caps_lock: key.mods.caps_lock,
                    num_lock: key.mods.num_lock,
                    ..Default::default()
                };
                if text_len.is_none()
                    && key.mods.eql(mod_test)
                    && key.codepoint <= u8::MAX as u32
                    && (0x20..=0x7e).contains(&(key.codepoint as u8))
                {
                    // Encode the codepoint as upper
                    let upper = (key.codepoint as u8).to_ascii_uppercase();
                    let n = utf8_encode(upper as u32, text_buf).unwrap();
                    text_len = Some(n);
                    key.shifted_codepoint = Some(upper as u32);
                }
            }

            if let Some(total) = text_len {
                key.text = Some(&text_buf[0..total]);
            }

            let event = if is_release {
                Event::KeyRelease(key)
            } else {
                Event::KeyPress(key)
            };

            ParseResult {
                event: Some(event),
                n: sequence.len(),
            }
        }
        b'y' => {
            // DECRPM (CSI ? Ps ; Pm $ y)
            let delim_idx = match index_of_scalar_pos(input, 3, b';') {
                Some(i) => i,
                None => return null_event,
            };
            let ps = match parse_unsigned(&input[3..delim_idx], U16) {
                Some(v) => v,
                None => return null_event,
            };
            let pm = match parse_unsigned(&input[delim_idx + 1..sequence.len() - 2], U8) {
                Some(v) => v,
                None => return null_event,
            };
            match ps {
                // Mouse Pixel reporting
                1016 => match pm {
                    0 | 4 => null_event,
                    _ => ParseResult {
                        event: Some(Event::CapSgrPixels),
                        n: sequence.len(),
                    },
                },
                // Unicode Core, see https://github.com/contour-terminal/terminal-unicode-core
                2027 => match pm {
                    0 | 4 => null_event,
                    _ => ParseResult {
                        event: Some(Event::CapUnicode),
                        n: sequence.len(),
                    },
                },
                // Color scheme reportnig, see https://github.com/contour-terminal/contour/blob/master/docs/vt-extensions/color-palette-update-notifications.md
                2031 => match pm {
                    0 | 4 => null_event,
                    _ => ParseResult {
                        event: Some(Event::CapColorSchemeUpdates),
                        n: sequence.len(),
                    },
                },
                _ => null_event,
            }
        }
        b'q' => {
            // kitty multi cursor cap (CSI > 1;2;3;29;30;40;100;101 TRAILER) (TRAILER is " q")
            let second_final = sequence[sequence.len() - 2];
            if second_final != b' ' {
                return null_event;
            }
            // check for any digits. we're not too picky about checking the supported cursor types here
            for &c in &sequence[0..sequence.len() - 2] {
                match c {
                    b'0'..=b'9' => {
                        return ParseResult {
                            event: Some(Event::CapMultiCursor),
                            n: sequence.len(),
                        }
                    }
                    _ => continue,
                }
            }
            null_event
        }
        _ => null_event,
    }
}

/// Parse a param buffer, returning a default value if the param was empty
///
/// PORT NOTE (N206): libvaxis 0.6.0 (the pinned vaxis) reads a param with
/// `std.fmt.parseInt(T, buf, 10)`, 0.5.1 with `parseUnsigned`; for the unsigned
/// types here the difference is that `parseInt` takes a leading `+` and reads
/// `-0` as zero. The pin's `parseInt` is followed.
///
/// The Zig function is generic over `comptime T`; its instantiations are the
/// unsigned ones (this function, returning the value widened to `u32`) and the
/// mouse coordinates' `i16` ([`parse_param_i16`], parity tail S-10b).
fn parse_param(buf: &[u8], default: Option<u32>, t: IntType) -> Option<u32> {
    // if (buf.len == 0) return default;
    if buf.is_empty() {
        return default;
    }
    // return std.fmt.parseInt(T, buf, 10) catch return null;
    parse_int_type(t, buf, 10).ok().map(|v| v as u32)
}

/// `parseParam(i16, buf, default)` — the SGR mouse coordinates (libvaxis 0.6.0
/// `Parser.zig:705-706`; `Mouse.col`/`row` are `i16` since 0.6.0).
fn parse_param_i16(buf: &[u8], default: Option<i16>) -> Option<i16> {
    // if (buf.len == 0) return default;
    if buf.is_empty() {
        return default;
    }
    // return std.fmt.parseInt(T, buf, 10) catch return null;
    rustty_zigstd::fmt::parse_int::<i16>(buf, 10).ok()
}

/// Parse a mouse event
fn parse_mouse(input: &[u8], full_input: &[u8]) -> ParseResult<'static> {
    let null_event = ParseResult {
        event: None,
        n: input.len(),
    };

    let button_mask: u16;
    let px: i16;
    let py: i16;
    let xterm: bool;
    if input.len() == 3 && input[2] == b'M' && full_input.len() >= 6 {
        xterm = true;
        // PORT NOTE: reference defect (R13), UD72. The reference subtracts 32
        // from each raw byte in `u8` (`full_input[3] - 32`), which traps for a
        // byte below 32 (typed or pasted after `ESC [ M`) in safe builds and
        // wraps in ReleaseFast. Such an event is skipped whole (all six bytes),
        // as for an invalid button below.
        let (Some(b), Some(x), Some(y)) = (
            full_input[3].checked_sub(32),
            full_input[4].checked_sub(32),
            full_input[5].checked_sub(32),
        ) else {
            return ParseResult { event: None, n: 6 };
        };
        button_mask = b as u16;
        // `px = full_input[4] - 32`: a `u8` coerced to the `i16` (always fits).
        px = i16::from(x);
        py = i16::from(y);
    } else if input.len() >= 4 && input[2] == b'<' {
        xterm = false;
        let delim1 = match index_of_scalar_pos(input, 3, b';') {
            Some(i) => i,
            None => return null_event,
        };
        button_mask = match parse_param(&input[3..delim1], None, U16) {
            Some(v) => v as u16,
            None => return null_event,
        };
        let delim2 = match index_of_scalar_pos(input, delim1 + 1, b';') {
            Some(i) => i,
            None => return null_event,
        };
        px = match parse_param_i16(&input[delim1 + 1..delim2], Some(1)) {
            Some(v) => v,
            None => return null_event,
        };
        py = match parse_param_i16(&input[delim2 + 1..input.len() - 1], Some(1)) {
            Some(v) => v,
            None => return null_event,
        };
    } else {
        return null_event;
    }

    if button_mask & mouse_bits::LEAVE > 0 {
        return ParseResult {
            event: Some(Event::MouseLeave),
            n: if xterm { 6 } else { input.len() },
        };
    }

    // const button: Mouse.Button = @enumFromInt(button_mask & mouse_bits.buttons);
    //
    // PORT NOTE: reference defect (R13), UD72 — a masked value that names no
    // button (192..=195) skips the event, consuming exactly what the event
    // would have (the reference's own `n` for a mouse event), so the bytes are
    // not re-read as keys. See `Button::from_u8`.
    let Some(button) = mouse::Button::from_u8((button_mask & mouse_bits::BUTTONS as u16) as u8)
    else {
        return ParseResult {
            event: None,
            n: if xterm { 6 } else { input.len() },
        };
    };
    let motion = button_mask & mouse_bits::MOTION as u16 > 0;
    let shift = button_mask & mouse_bits::SHIFT as u16 > 0;
    let alt = button_mask & mouse_bits::ALT as u16 > 0;
    let ctrl = button_mask & mouse_bits::CTRL as u16 > 0;

    let r#type = if motion && button != mouse::Button::None {
        mouse::Type::Drag
    } else if motion && button == mouse::Button::None {
        mouse::Type::Motion
    } else if xterm {
        if button == mouse::Button::None {
            mouse::Type::Release
        } else {
            mouse::Type::Press
        }
    } else if input[input.len() - 1] == b'm' {
        mouse::Type::Release
    } else {
        mouse::Type::Press
    };

    // PORT NOTE: Zig field defaults `xoffset`/`yoffset = 0` set explicitly (Mouse has no
    // `Default`).
    let mouse_ev = Mouse {
        button,
        mods: mouse::Modifiers { shift, alt, ctrl },
        col: px.saturating_sub(1),
        row: py.saturating_sub(1),
        xoffset: 0,
        yoffset: 0,
        r#type,
    };
    ParseResult {
        event: Some(Event::Mouse(mouse_ev)),
        n: if xterm { 6 } else { input.len() },
    }
}

// ---- the pure primitives (std-shim ports, VAXIS_C3A_DESIGN §5) ----

/// The Zig integer types the reply numbers are parsed into (`u8`, `u16`, and
/// `u21` for a codepoint).
const U8: IntType = IntType::unsigned(8);
const U16: IntType = IntType::unsigned(16);
const U21: IntType = IntType::unsigned(21);

/// Build a `Key` with just `codepoint` set (Zig `.{ .codepoint = X }`); the rest default.
fn key_with_codepoint<'a>(codepoint: u32) -> Key<'a> {
    Key {
        codepoint,
        text: None,
        shifted_codepoint: None,
        base_layout_codepoint: None,
        mods: Modifiers::default(),
    }
}

/// `Color.rgb` — extract the `[3]u8` from a `Color.rgb_from_spec` result (always `Rgb`).
fn rgb_of(color: cell::Color) -> [u8; 3] {
    match color {
        cell::Color::Rgb(v) => v,
        _ => unreachable!(),
    }
}

/// `std.mem.indexOfScalarPos(u8, s, start, needle)`.
fn index_of_scalar_pos(s: &[u8], start: usize, needle: u8) -> Option<usize> {
    s[start..]
        .iter()
        .position(|&b| b == needle)
        .map(|off| start + off)
}

/// `std.fmt.parseUnsigned(T, buf, 10)` for the Zig type `t`; `None` is any of
/// its errors (every caller does `catch return null_event`).
///
/// PORT NOTE (N206): this was a digits-only parser, which refused the `_`
/// separator Zig skips (`ESC ] 4 ; 1_0 ; ...` is palette index 10 in the
/// reference). It is `rustty_zigstd::fmt`, the port's one copy of Zig's
/// parser. Faithful: `_` is the standard library's grammar and libvaxis has no
/// position of its own on it (compare UD141, where Ghostty's own OSC 99 parser
/// refuses `_`). In a CSI a `_` (0x5F) is a final byte, so it only reaches the
/// OSC numbers.
fn parse_unsigned(buf: &[u8], t: IntType) -> Option<u32> {
    parse_unsigned_type(t, buf, 10).ok().map(|v| v as u32)
}

/// `std.unicode.utf8Encode(cp: u21, buf) !usize` — `None` for surrogates + `>0x10FFFF`
/// (exactly Zig's error via `char::from_u32`).
fn utf8_encode(cp: u32, buf: &mut [u8]) -> Option<usize> {
    let c = char::from_u32(cp)?;
    Some(c.encode_utf8(buf).len())
}

/// `std.unicode.utf8CodepointSequenceLength(cp) !usize` — the erroring variant (`None`
/// for `cp > 0x10FFFF`, i.e. `catch return error.InvalidUTF8`).
fn utf8_codepoint_sequence_length_checked(cp: u32) -> Option<usize> {
    if cp < 0x80 {
        Some(1)
    } else if cp < 0x800 {
        Some(2)
    } else if cp < 0x10000 {
        Some(3)
    } else if cp <= 0x10FFFF {
        Some(4)
    } else {
        None
    }
}

/// Faithful port of `std.base64.standard.Decoder` (`calcSizeForSlice` + `decode`): the
/// standard alphabet (`A–Za–z0–9+/`) with `=` padding. Invalid length/char → `Base64`.
fn base64_standard_decode(payload: &[u8]) -> Result<Vec<u8>, ParseError> {
    fn char_index(c: u8) -> Option<u8> {
        match c {
            b'A'..=b'Z' => Some(c - b'A'),
            b'a'..=b'z' => Some(c - b'a' + 26),
            b'0'..=b'9' => Some(c - b'0' + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }

    let len = payload.len();
    // calcSizeForSlice: source length must be a multiple of 4 (padding included).
    if !len.is_multiple_of(4) {
        return Err(ParseError::Base64);
    }
    if len == 0 {
        return Ok(Vec::new());
    }

    let mut padding = 0usize;
    if payload[len - 1] == b'=' {
        padding += 1;
    }
    if payload[len - 2] == b'=' {
        padding += 1;
    }
    let out_len = len / 4 * 3 - padding;
    let mut out = Vec::with_capacity(out_len);

    let mut i = 0;
    while i < len {
        let c0 = char_index(payload[i]).ok_or(ParseError::Base64)?;
        let c1 = char_index(payload[i + 1]).ok_or(ParseError::Base64)?;
        out.push((c0 << 2) | (c1 >> 4));
        if payload[i + 2] != b'=' {
            let c2 = char_index(payload[i + 2]).ok_or(ParseError::Base64)?;
            out.push((c1 << 4) | (c2 >> 2));
            if payload[i + 3] != b'=' {
                let c3 = char_index(payload[i + 3]).ok_or(ParseError::Base64)?;
                out.push((c2 << 6) | c3);
            }
        }
        i += 4;
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_xterm_keypress() {
        let input = b"a";
        let mut parser = Parser::default();
        let result = parser.parse(input).unwrap();
        let expected_key = Key {
            text: Some(b"a"),
            ..key_with_codepoint(b'a' as u32)
        };
        let expected_event = Event::KeyPress(expected_key);

        assert_eq!(1, result.n);
        assert_eq!(Some(expected_event), result.event);
    }

    #[test]
    fn single_xterm_keypress_backspace() {
        let input = b"\x08";
        let mut parser = Parser::default();
        let result = parser.parse(input).unwrap();
        let expected_key = key_with_codepoint(key::backspace);
        let expected_event = Event::KeyPress(expected_key);

        assert_eq!(1, result.n);
        assert_eq!(Some(expected_event), result.event);
    }

    #[test]
    fn single_xterm_keypress_with_more_buffer() {
        let input = b"ab";
        let mut parser = Parser::default();
        let result = parser.parse(input).unwrap();
        let expected_key = Key {
            text: Some(b"a"),
            ..key_with_codepoint(b'a' as u32)
        };
        let expected_event = Event::KeyPress(expected_key);

        assert_eq!(1, result.n);
        let actual_text = match result.event.as_ref().unwrap() {
            Event::KeyPress(k) => k.text.unwrap(),
            _ => panic!("expected key_press"),
        };
        assert_eq!(expected_key.text.unwrap(), actual_text);
        assert_eq!(Some(expected_event), result.event);
    }

    #[test]
    fn xterm_escape_keypress() {
        let input = b"\x1b";
        let mut parser = Parser::default();
        let result = parser.parse(input).unwrap();
        let expected_key = key_with_codepoint(key::escape);
        let expected_event = Event::KeyPress(expected_key);

        assert_eq!(1, result.n);
        assert_eq!(Some(expected_event), result.event);
    }

    #[test]
    fn xterm_ctrl_a() {
        let input = b"\x01";
        let mut parser = Parser::default();
        let result = parser.parse(input).unwrap();
        let expected_key = Key {
            mods: Modifiers {
                ctrl: true,
                ..Default::default()
            },
            ..key_with_codepoint(b'a' as u32)
        };
        let expected_event = Event::KeyPress(expected_key);

        assert_eq!(1, result.n);
        assert_eq!(Some(expected_event), result.event);
    }

    #[test]
    fn xterm_alt_a() {
        let input = b"\x1ba";
        let mut parser = Parser::default();
        let result = parser.parse(input).unwrap();
        let expected_key = Key {
            mods: Modifiers {
                alt: true,
                ..Default::default()
            },
            ..key_with_codepoint(b'a' as u32)
        };
        let expected_event = Event::KeyPress(expected_key);

        assert_eq!(2, result.n);
        assert_eq!(Some(expected_event), result.event);
    }

    #[test]
    fn xterm_key_up() {
        {
            // normal version
            let input = b"\x1b[A";
            let mut parser = Parser::default();
            let result = parser.parse(input).unwrap();
            let expected_key = key_with_codepoint(key::up);
            let expected_event = Event::KeyPress(expected_key);

            assert_eq!(3, result.n);
            assert_eq!(Some(expected_event), result.event);
        }

        {
            // application keys version
            let input = b"\x1bOA";
            let mut parser = Parser::default();
            let result = parser.parse(input).unwrap();
            let expected_key = key_with_codepoint(key::up);
            let expected_event = Event::KeyPress(expected_key);

            assert_eq!(3, result.n);
            assert_eq!(Some(expected_event), result.event);
        }
    }

    #[test]
    fn xterm_shift_up() {
        let input = b"\x1b[1;2A";
        let mut parser = Parser::default();
        let result = parser.parse(input).unwrap();
        let expected_key = Key {
            mods: Modifiers {
                shift: true,
                ..Default::default()
            },
            ..key_with_codepoint(key::up)
        };
        let expected_event = Event::KeyPress(expected_key);

        assert_eq!(6, result.n);
        assert_eq!(Some(expected_event), result.event);
    }

    #[test]
    fn xterm_shift_tab() {
        let input = b"\x1b[Z";
        let mut parser = Parser::default();
        let result = parser.parse(input).unwrap();
        let expected_key = Key {
            mods: Modifiers {
                shift: true,
                ..Default::default()
            },
            ..key_with_codepoint(key::tab)
        };
        let expected_event = Event::KeyPress(expected_key);

        assert_eq!(3, result.n);
        assert_eq!(Some(expected_event), result.event);
    }

    #[test]
    fn xterm_insert() {
        let input = b"\x1b[2~";
        let mut parser = Parser::default();
        let result = parser.parse(input).unwrap();
        let expected_key = key_with_codepoint(key::insert);
        let expected_event = Event::KeyPress(expected_key);

        assert_eq!(input.len(), result.n);
        assert_eq!(Some(expected_event), result.event);
    }

    #[test]
    fn paste_start() {
        let input = b"\x1b[200~";
        let mut parser = Parser::default();
        let result = parser.parse(input).unwrap();
        let expected_event = Event::PasteStart;

        assert_eq!(6, result.n);
        assert_eq!(Some(expected_event), result.event);
    }

    #[test]
    fn paste_end() {
        let input = b"\x1b[201~";
        let mut parser = Parser::default();
        let result = parser.parse(input).unwrap();
        let expected_event = Event::PasteEnd;

        assert_eq!(6, result.n);
        assert_eq!(Some(expected_event), result.event);
    }

    #[test]
    fn osc52_paste() {
        let input = b"\x1b]52;c;b3NjNTIgcGFzdGU=\x1b\\";
        let expected_text = b"osc52 paste";
        let mut parser = Parser::default();
        let result = parser.parse(input).unwrap();

        assert_eq!(25, result.n);
        match result.event.unwrap() {
            Event::Paste(text) => assert_eq!(expected_text.as_slice(), text.as_slice()),
            _ => panic!("expected paste"),
        }
    }

    #[test]
    fn focus_in() {
        let input = b"\x1b[I";
        let mut parser = Parser::default();
        let result = parser.parse(input).unwrap();
        let expected_event = Event::FocusIn;

        assert_eq!(3, result.n);
        assert_eq!(Some(expected_event), result.event);
    }

    #[test]
    fn focus_out() {
        let input = b"\x1b[O";
        let mut parser = Parser::default();
        let result = parser.parse(input).unwrap();
        let expected_event = Event::FocusOut;

        assert_eq!(3, result.n);
        assert_eq!(Some(expected_event), result.event);
    }

    #[test]
    fn kitty_shift_a_without_text_reporting() {
        let input = b"\x1b[97:65;2u";
        let mut parser = Parser::default();
        let result = parser.parse(input).unwrap();
        let expected_key = Key {
            shifted_codepoint: Some(b'A' as u32),
            mods: Modifiers {
                shift: true,
                ..Default::default()
            },
            text: Some(b"A"),
            ..key_with_codepoint(b'a' as u32)
        };
        let expected_event = Event::KeyPress(expected_key);

        assert_eq!(10, result.n);
        assert_eq!(Some(expected_event), result.event);
    }

    #[test]
    fn kitty_alt_shift_a_without_text_reporting() {
        let input = b"\x1b[97:65;4u";
        let mut parser = Parser::default();
        let result = parser.parse(input).unwrap();
        let expected_key = Key {
            shifted_codepoint: Some(b'A' as u32),
            mods: Modifiers {
                shift: true,
                alt: true,
                ..Default::default()
            },
            ..key_with_codepoint(b'a' as u32)
        };
        let expected_event = Event::KeyPress(expected_key);

        assert_eq!(10, result.n);
        assert_eq!(Some(expected_event), result.event);
    }

    #[test]
    fn kitty_a_without_text_reporting() {
        let input = b"\x1b[97u";
        let mut parser = Parser::default();
        let result = parser.parse(input).unwrap();
        let expected_key = key_with_codepoint(b'a' as u32);
        let expected_event = Event::KeyPress(expected_key);

        assert_eq!(5, result.n);
        assert_eq!(Some(expected_event), result.event);
    }

    #[test]
    fn kitty_release_event() {
        let input = b"\x1b[97;1:3u";
        let mut parser = Parser::default();
        let result = parser.parse(input).unwrap();
        let expected_key = key_with_codepoint(b'a' as u32);
        let expected_event = Event::KeyRelease(expected_key);

        assert_eq!(9, result.n);
        assert_eq!(Some(expected_event), result.event);
    }

    #[test]
    fn single_codepoint() {
        let input = "🙂".as_bytes();
        let mut parser = Parser::default();
        let result = parser.parse(input).unwrap();
        let expected_key = Key {
            text: Some(input),
            ..key_with_codepoint(0x1F642)
        };
        let expected_event = Event::KeyPress(expected_key);

        assert_eq!(4, result.n);
        assert_eq!(Some(expected_event), result.event);
    }

    #[test]
    fn single_codepoint_with_more_in_buffer() {
        let input = "🙂a".as_bytes();
        let mut parser = Parser::default();
        let result = parser.parse(input).unwrap();
        let expected_key = Key {
            text: Some("🙂".as_bytes()),
            ..key_with_codepoint(0x1F642)
        };
        let expected_event = Event::KeyPress(expected_key);

        assert_eq!(4, result.n);
        assert_eq!(Some(expected_event), result.event);
    }

    #[test]
    fn multiple_codepoint_grapheme() {
        let input = "👩‍🚀".as_bytes();
        let mut parser = Parser::default();
        let result = parser.parse(input).unwrap();
        let expected_key = Key {
            text: Some(input),
            ..key_with_codepoint(key::multicodepoint)
        };
        let expected_event = Event::KeyPress(expected_key);

        assert_eq!(input.len(), result.n);
        assert_eq!(Some(expected_event), result.event);
    }

    #[test]
    fn multiple_codepoint_grapheme_with_more_after() {
        let input = "👩‍🚀abc".as_bytes();
        let mut parser = Parser::default();
        let result = parser.parse(input).unwrap();
        let expected_key = Key {
            text: Some("👩‍🚀".as_bytes()),
            ..key_with_codepoint(key::multicodepoint)
        };

        assert_eq!(expected_key.text.unwrap().len(), result.n);
        let actual = match result.event.unwrap() {
            Event::KeyPress(k) => k,
            _ => panic!("expected key_press"),
        };
        assert_eq!(expected_key.text.unwrap(), actual.text.unwrap());
        assert_eq!(expected_key.codepoint, actual.codepoint);
    }

    #[test]
    fn flag_emoji() {
        let input = "🇺🇸".as_bytes();
        let mut parser = Parser::default();
        let result = parser.parse(input).unwrap();
        let expected_key = Key {
            text: Some(input),
            ..key_with_codepoint(key::multicodepoint)
        };
        let expected_event = Event::KeyPress(expected_key);

        assert_eq!(input.len(), result.n);
        assert_eq!(Some(expected_event), result.event);
    }

    #[test]
    fn combining_mark() {
        // a with combining acute accent (NFD form)
        let input = "a\u{0301}".as_bytes();
        let mut parser = Parser::default();
        let result = parser.parse(input).unwrap();
        let expected_key = Key {
            text: Some(input),
            ..key_with_codepoint(key::multicodepoint)
        };
        let expected_event = Event::KeyPress(expected_key);

        assert_eq!(input.len(), result.n);
        assert_eq!(Some(expected_event), result.event);
    }

    #[test]
    fn skin_tone_emoji() {
        let input = "👋🏿".as_bytes();
        let mut parser = Parser::default();
        let result = parser.parse(input).unwrap();
        let expected_key = Key {
            text: Some(input),
            ..key_with_codepoint(key::multicodepoint)
        };
        let expected_event = Event::KeyPress(expected_key);

        assert_eq!(input.len(), result.n);
        assert_eq!(Some(expected_event), result.event);
    }

    #[test]
    fn text_variation_selector() {
        // Heavy black heart with text variation selector
        let input = "❤︎".as_bytes();
        let mut parser = Parser::default();
        let result = parser.parse(input).unwrap();
        let expected_key = Key {
            text: Some(input),
            ..key_with_codepoint(key::multicodepoint)
        };
        let expected_event = Event::KeyPress(expected_key);

        assert_eq!(input.len(), result.n);
        assert_eq!(Some(expected_event), result.event);
    }

    #[test]
    fn keycap_sequence() {
        let input = "1️⃣".as_bytes();
        let mut parser = Parser::default();
        let result = parser.parse(input).unwrap();
        let expected_key = Key {
            text: Some(input),
            ..key_with_codepoint(key::multicodepoint)
        };
        let expected_event = Event::KeyPress(expected_key);

        assert_eq!(input.len(), result.n);
        assert_eq!(Some(expected_event), result.event);
    }

    #[test]
    fn csi_kitty_multi_cursor() {
        let mut buf = [0u8; 1];
        {
            let input = b"\x1b[>1;2;3;29;30;40;100;101 q";
            let result = parse_csi(input, &mut buf);

            assert_eq!(input.len(), result.n);
            assert_eq!(Some(Event::CapMultiCursor), result.event);
        }
        {
            let input = b"\x1b[> q";
            let result = parse_csi(input, &mut buf);

            assert_eq!(input.len(), result.n);
            assert_eq!(None, result.event);
        }
    }

    #[test]
    fn csi_decrpm() {
        let mut buf = [0u8; 1];
        {
            let input = b"\x1b[?1016;1$y";
            let result = parse_csi(input, &mut buf);

            assert_eq!(input.len(), result.n);
            assert_eq!(Some(Event::CapSgrPixels), result.event);
        }
        {
            let input = b"\x1b[?1016;0$y";
            let result = parse_csi(input, &mut buf);

            assert_eq!(input.len(), result.n);
            assert_eq!(None, result.event);
        }
    }

    #[test]
    fn csi_primary_da() {
        let mut buf = [0u8; 1];
        let input = b"\x1b[?c";
        let result = parse_csi(input, &mut buf);

        assert_eq!(input.len(), result.n);
        assert_eq!(Some(Event::CapDa1), result.event);
    }

    #[test]
    fn csi_dsr() {
        let mut buf = [0u8; 1];
        {
            let input = b"\x1b[?997;1n";
            let result = parse_csi(input, &mut buf);

            assert_eq!(input.len(), result.n);
            assert_eq!(Some(Event::ColorScheme(cell::Scheme::Dark)), result.event);
        }
        {
            let input = b"\x1b[?997;2n";
            let result = parse_csi(input, &mut buf);

            assert_eq!(input.len(), result.n);
            assert_eq!(Some(Event::ColorScheme(cell::Scheme::Light)), result.event);
        }
        {
            let input = b"\x1b[0n";
            let result = parse_csi(input, &mut buf);

            assert_eq!(input.len(), result.n);
            assert_eq!(None, result.event);
        }
    }

    #[test]
    fn csi_mouse() {
        let mut buf = [0u8; 1];
        let input = b"\x1b[<35;1;1m";
        let result = parse_csi(input, &mut buf);
        let expected = Event::Mouse(Mouse {
            col: 0,
            row: 0,
            xoffset: 0,
            yoffset: 0,
            button: mouse::Button::None,
            r#type: mouse::Type::Motion,
            mods: mouse::Modifiers::default(),
        });

        assert_eq!(input.len(), result.n);
        assert_eq!(Some(expected), result.event);
    }

    /// UD72 (R13): a button mask with both wheel bits set (192..=195) names no
    /// `Button`; libvaxis's `@enumFromInt` panics. The event is skipped, and the
    /// whole sequence consumed. Also X10 bytes below 32, which underflow the
    /// reference's `- 32`.
    #[test]
    fn a_mouse_event_with_an_invalid_button_or_byte_is_skipped() {
        let mut buf = [0u8; 1];

        // Positive controls: the neighbours of the invalid range still decode.
        for (input, button) in [
            (&b"\x1b[<131;1;1M"[..], mouse::Button::Button11),
            (&b"\x1b[<67;1;1M"[..], mouse::Button::WheelLeft),
            (&b"\x1b[M\xa3\x21\x21"[..], mouse::Button::Button11),
        ] {
            let result = parse_csi(input, &mut buf);
            assert_eq!(input.len(), result.n);
            match result.event {
                Some(Event::Mouse(m)) => assert_eq!(button, m.button),
                other => panic!("{input:?} decoded to {other:?}"),
            }
        }

        // SGR: 192..=195, plus modifier/motion bits on top (they are masked off).
        for input in [
            &b"\x1b[<192;1;1M"[..],
            &b"\x1b[<195;5;7m"[..],
            &b"\x1b[<228;1;1M"[..],
        ] {
            let result = parse_csi(input, &mut buf);
            assert_eq!(None, result.event, "{input:?} is skipped");
            assert_eq!(input.len(), result.n, "{input:?} is consumed whole");
        }

        // X10 (`ESC [ M` + three raw bytes): button byte 192 + 32 = 0xE0, and a
        // byte below 32 in each of the three positions.
        for input in [
            &b"\x1b[M\xe0\x21\x21"[..],
            &b"\x1b[M\x10\x21\x21"[..],
            &b"\x1b[M\x20\x05\x21"[..],
            &b"\x1b[M\x20\x21\x00"[..],
        ] {
            let result = parse_csi(input, &mut buf);
            assert_eq!(None, result.event, "{input:?} is skipped");
            assert_eq!(6, result.n, "{input:?} is consumed whole");
        }
    }

    /// libvaxis 0.6.0 `test "parse(csi): mouse (negative)"`: `Mouse.col`/`row` are
    /// `i16`, read with `parseParam(i16, ..)`, so a negative coordinate is kept.
    #[test]
    fn csi_mouse_negative() {
        let mut buf = [0u8; 1];
        let input = b"\x1b[<35;-50;-100m";
        let result = parse_csi(input, &mut buf);
        let expected = Event::Mouse(Mouse {
            col: -51,
            row: -101,
            xoffset: 0,
            yoffset: 0,
            button: mouse::Button::None,
            r#type: mouse::Type::Motion,
            mods: mouse::Modifiers::default(),
        });

        assert_eq!(input.len(), result.n);
        assert_eq!(Some(expected), result.event);
    }

    /// S-10b (the 36 SGR mouse results N206 left open): the coordinates are
    /// `parseParam(i16, ..) -| 1`, so `0` is column -1, `-5` is -6, `-0` is -1, the
    /// i16 floor saturates, and a value past `i16` is refused (null event, sequence
    /// consumed). Expected values MEASURED with libvaxis 0.6.0 built with Zig 0.16.0
    /// (S-10b probe `mouse` mode).
    #[test]
    fn sgr_mouse_coordinates_are_i16() {
        let at = |input: &[u8]| {
            let mut buf = [0u8; 1];
            let r = parse_csi(input, &mut buf);
            assert_eq!(input.len(), r.n, "{input:?} is consumed whole");
            match r.event {
                Some(Event::Mouse(m)) => Some((m.col, m.row)),
                None => None,
                other => panic!("{input:?} decoded to {other:?}"),
            }
        };
        assert_eq!(Some((-1, -1)), at(b"\x1b[<0;0;0M"));
        assert_eq!(Some((-6, 4)), at(b"\x1b[<0;-5;5M"));
        assert_eq!(Some((-1, -1)), at(b"\x1b[<0;-0;-0M"));
        assert_eq!(Some((32766, -32768)), at(b"\x1b[<0;32767;-32768M"));
        assert_eq!(None, at(b"\x1b[<0;40000;1M"));
        assert_eq!(None, at(b"\x1b[<0;1;32768M"));
        assert_eq!(None, at(b"\x1b[<0;-32769;1M"));
        // Empty coordinates take the default 1, i.e. column / row 0.
        assert_eq!(Some((0, 0)), at(b"\x1b[<0;;M"));
    }

    #[test]
    fn csi_xterm_mouse() {
        let mut buf = [0u8; 1];
        let input = b"\x1b[M\x20\x21\x21";
        let result = parse_csi(input, &mut buf);
        let expected = Event::Mouse(Mouse {
            col: 0,
            row: 0,
            xoffset: 0,
            yoffset: 0,
            button: mouse::Button::Left,
            r#type: mouse::Type::Press,
            mods: mouse::Modifiers::default(),
        });

        assert_eq!(input.len(), result.n);
        assert_eq!(Some(expected), result.event);
    }

    #[test]
    fn disambiguate_shift_space() {
        let input = b"\x1b[32;2u";
        let mut parser = Parser::default();
        let result = parser.parse(input).unwrap();
        let expected_key = Key {
            shifted_codepoint: Some(b' ' as u32),
            mods: Modifiers {
                shift: true,
                ..Default::default()
            },
            text: Some(b" "),
            ..key_with_codepoint(b' ' as u32)
        };
        let expected_event = Event::KeyPress(expected_key);

        assert_eq!(7, result.n);
        assert_eq!(Some(expected_event), result.event);
    }

    // N206: the numbers in the replies vaxis parses go through Zig 0.16.0
    // `std.fmt`, so they take its grammar. The expected values were measured
    // with libvaxis 0.6.0 (the pinned vaxis) built with Zig 0.16.0.

    /// OSC numbers are `std.fmt.parseUnsigned(u8, .., 10)`, which skips a `_`
    /// between digits (`1_0` is 10) and refuses a sign.
    #[test]
    fn osc_numbers_take_zig_digit_separators() {
        let mut parser = Parser::default();
        let r = parser
            .parse(b"\x1b]4;1_0;rgb:1111/2222/3333\x1b\\")
            .unwrap();
        assert_eq!(28, r.n);
        assert_eq!(
            Some(Event::ColorReport(cell::Report {
                kind: cell::Kind::Index(10),
                value: [0x11, 0x22, 0x33],
            })),
            r.event
        );

        let mut parser = Parser::default();
        let r = parser.parse(b"\x1b]1_1;rgb:1111/2222/3333\x1b\\").unwrap();
        assert_eq!(26, r.n);
        assert_eq!(
            Some(Event::ColorReport(cell::Report {
                kind: cell::Kind::Bg,
                value: [0x11, 0x22, 0x33],
            })),
            r.event
        );

        for input in [
            &b"\x1b]4;+10;rgb:1111/2222/3333\x1b\\"[..],
            &b"\x1b]4;-0;rgb:1111/2222/3333\x1b\\"[..],
            &b"\x1b]4;_10;rgb:1111/2222/3333\x1b\\"[..],
            &b"\x1b]4;10_;rgb:1111/2222/3333\x1b\\"[..],
            &b"\x1b]4;256;rgb:1111/2222/3333\x1b\\"[..],
        ] {
            let mut parser = Parser::default();
            let r = parser.parse(input).unwrap();
            assert_eq!(None, r.event, "{input:?}");
            assert_eq!(input.len(), r.n, "{input:?}");
        }
    }

    /// CSI parameters read by `parseParam` are `std.fmt.parseInt(T, .., 10)`
    /// in libvaxis 0.6.0, which takes a leading `+` and reads `-0` as zero.
    #[test]
    fn csi_params_take_zig_parse_int_signs() {
        let parse = |input: &[u8]| {
            let mut parser = Parser::default();
            let r = parser.parse(input).unwrap();
            let key = match r.event {
                Some(Event::KeyPress(k)) => Some((k.codepoint, k.mods, k.shifted_codepoint)),
                None => None,
                other => panic!("{input:?}: {other:?}"),
            };
            (key, r.n)
        };
        let ctrl = Modifiers {
            ctrl: true,
            ..Default::default()
        };
        assert_eq!(
            (Some((97, Modifiers::default(), None)), 6),
            parse(b"\x1b[+97u")
        );
        assert_eq!(
            (Some((0, Modifiers::default(), None)), 5),
            parse(b"\x1b[-0u")
        );
        assert_eq!((Some((97, ctrl, None)), 8), parse(b"\x1b[97;+5u"));
        assert_eq!(
            (Some((97, Modifiers::default(), Some(5))), 8),
            parse(b"\x1b[97:+5u")
        );
        assert_eq!((Some((key::end, ctrl, None)), 7), parse(b"\x1b[1;+5F"));
        assert_eq!(
            (Some((key::insert, Modifiers::default(), None)), 5),
            parse(b"\x1b[+2~")
        );
        // `-1` is `error.Overflow` for an unsigned type; a second sign is refused.
        assert_eq!((None, 5), parse(b"\x1b[-1u"));
        assert_eq!((None, 6), parse(b"\x1b[+-5u"));

        let mut parser = Parser::default();
        let r = parser.parse(b"\x1b[<+0;+5;+6M").unwrap();
        assert_eq!(12, r.n);
        assert_eq!(
            Some(Event::Mouse(Mouse {
                col: 4,
                row: 5,
                xoffset: 0,
                yoffset: 0,
                button: mouse::Button::Left,
                r#type: mouse::Type::Press,
                mods: mouse::Modifiers::default(),
            })),
            r.event
        );
    }

    /// The CSI numbers libvaxis reads with `std.fmt.parseUnsigned` (window
    /// size, DECRPM, DSR) still refuse a sign.
    #[test]
    fn csi_unsigned_numbers_refuse_a_sign() {
        let mut parser = Parser::default();
        let r = parser.parse(b"\x1b[48;97;2;3;4t").unwrap();
        assert_eq!(
            Some(Event::Winsize(Winsize {
                rows: 97,
                cols: 2,
                x_pixel: 4,
                y_pixel: 3,
            })),
            r.event
        );
        for input in [
            &b"\x1b[48;+97;2;3;4t"[..],
            &b"\x1b[48;-0;2;3;4t"[..],
            &b"\x1b[?+2026;1$y"[..],
            &b"\x1b[?2026;+1$y"[..],
            &b"\x1b[?+997;1n"[..],
        ] {
            let mut parser = Parser::default();
            let r = parser.parse(input).unwrap();
            assert_eq!(None, r.event, "{input:?}");
            assert_eq!(input.len(), r.n, "{input:?}");
        }
    }
}
