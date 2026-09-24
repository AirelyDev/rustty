//! Port of `libvaxis` `ctlseqs.zig` (0.6.0: 161 LOC; the three multi-cursor sequences are new) — the control-sequence string
//! constants (queries, mouse/paste/sync/unicode/color-scheme sets, cursor/erase/alt
//! screen, SGR colors/underlines/attributes, OSC sequences, kitty graphics, and the
//! color control sequences).
//!
//! PORT NOTE (byte-slices, R1): every Zig `[]const u8` string literal → a Rust
//! `pub const … : &[u8] = b"…"` byte constant, NEVER a lossy `String` — these are raw
//! control bytes. The exact Zig snake_case identifiers are preserved.
//!
//! PORT NOTE (format templates): the `{d}`/`{s}`/`{f}`/`{x:0>2}` placeholders are Zig
//! `std.fmt` templates that the later tty/renderer chunks fill; chunk 3a lands them
//! verbatim as byte constants (the format-application seam is a later chunk).
#![allow(non_upper_case_globals)]

// Queries
pub const primary_device_attrs: &[u8] = b"\x1b[c";
pub const tertiary_device_attrs: &[u8] = b"\x1b[=c";
pub const device_status_report: &[u8] = b"\x1b[5n";
pub const xtversion: &[u8] = b"\x1b[>0q";
pub const decrqm_focus: &[u8] = b"\x1b[?1004$p";
pub const decrqm_sgr_pixels: &[u8] = b"\x1b[?1016$p";
pub const decrqm_sync: &[u8] = b"\x1b[?2026$p";
pub const decrqm_unicode: &[u8] = b"\x1b[?2027$p";
pub const decrqm_color_scheme: &[u8] = b"\x1b[?2031$p";
pub const csi_u_query: &[u8] = b"\x1b[?u";
pub const kitty_graphics_query: &[u8] = b"\x1b_Gi=1,a=q\x1b\\";
pub const sixel_geometry_query: &[u8] = b"\x1b[?2;1;0S";
pub const cursor_position_request: &[u8] = b"\x1b[6n";
pub const explicit_width_query: &[u8] = b"\x1b]66;w=1; \x1b\\";
pub const scaled_text_query: &[u8] = b"\x1b]66;s=2; \x1b\\";
pub const multi_cursor_query: &[u8] = b"\x1b[> q";

// mouse. We try for button motion and any motion. terminals will enable the
// last one we tried (any motion). This was added because zellij doesn't
// support any motion currently
// See: https://github.com/zellij-org/zellij/issues/1679
pub const mouse_set: &[u8] = b"\x1b[?1002;1003;1004;1006h";
pub const mouse_set_pixels: &[u8] = b"\x1b[?1002;1003;1004;1016h";
pub const mouse_reset: &[u8] = b"\x1b[?1002;1003;1004;1006;1016l";

// in-band window size reports
pub const in_band_resize_set: &[u8] = b"\x1b[?2048h";
pub const in_band_resize_reset: &[u8] = b"\x1b[?2048l";

// sync
pub const sync_set: &[u8] = b"\x1b[?2026h";
pub const sync_reset: &[u8] = b"\x1b[?2026l";

// unicode
pub const unicode_set: &[u8] = b"\x1b[?2027h";
pub const unicode_reset: &[u8] = b"\x1b[?2027l";
pub const explicit_width: &[u8] = b"\x1b]66;w={d};{s}\x1b\\";

// text sizing
pub const scaled_text: &[u8] = b"\x1b]66;s={d}:w={d};{s}\x1b\\";
pub const scaled_text_with_fractions: &[u8] = b"\x1b]66;s={d}:w={d}:n={d}:d={d}:v={d};{s}\x1b\\";

// bracketed paste
pub const bp_set: &[u8] = b"\x1b[?2004h";
pub const bp_reset: &[u8] = b"\x1b[?2004l";

// color scheme updates
pub const color_scheme_request: &[u8] = b"\x1b[?996n";
pub const color_scheme_set: &[u8] = b"\x1b[?2031h";
pub const color_scheme_reset: &[u8] = b"\x1b[?2031l";

// Key encoding
pub const csi_u_push: &[u8] = b"\x1b[>{d}u";
pub const csi_u_pop: &[u8] = b"\x1b[<u";

// Cursor
pub const home: &[u8] = b"\x1b[H";
pub const cup: &[u8] = b"\x1b[{d};{d}H";
pub const hide_cursor: &[u8] = b"\x1b[?25l";
pub const show_cursor: &[u8] = b"\x1b[?25h";
pub const cursor_shape: &[u8] = b"\x1b[{d} q";
pub const ri: &[u8] = b"\x1bM";
pub const ind: &[u8] = b"\n";
pub const cuf: &[u8] = b"\x1b[{d}C";
pub const cub: &[u8] = b"\x1b[{d}D";

// Multi Cursor
pub const secondary_cursors_rgb: &[u8] = b"\x1b[>40;2:{d}:{d}:{d} q";
pub const reset_secondary_cursors: &[u8] = b"\x1b[>0;4 q";
pub const show_secondary_cursor: &[u8] = b"\x1b[>29;2:{d}:{d} q";

// Erase
pub const erase_below_cursor: &[u8] = b"\x1b[J";

// alt screen
pub const smcup: &[u8] = b"\x1b[?1049h";
pub const rmcup: &[u8] = b"\x1b[?1049l";

// sgr reset all
pub const sgr_reset: &[u8] = b"\x1b[m";

// colors
pub const fg_base: &[u8] = b"\x1b[3{d}m";
pub const fg_bright: &[u8] = b"\x1b[9{d}m";
pub const bg_base: &[u8] = b"\x1b[4{d}m";
pub const bg_bright: &[u8] = b"\x1b[10{d}m";

pub const fg_reset: &[u8] = b"\x1b[39m";
pub const bg_reset: &[u8] = b"\x1b[49m";
pub const ul_reset: &[u8] = b"\x1b[59m";
pub const fg_indexed: &[u8] = b"\x1b[38:5:{d}m";
pub const bg_indexed: &[u8] = b"\x1b[48:5:{d}m";
pub const ul_indexed: &[u8] = b"\x1b[58:5:{d}m";
pub const fg_rgb: &[u8] = b"\x1b[38:2:{d}:{d}:{d}m";
pub const bg_rgb: &[u8] = b"\x1b[48:2:{d}:{d}:{d}m";
pub const ul_rgb: &[u8] = b"\x1b[58:2:{d}:{d}:{d}m";
pub const fg_indexed_legacy: &[u8] = b"\x1b[38;5;{d}m";
pub const bg_indexed_legacy: &[u8] = b"\x1b[48;5;{d}m";
pub const ul_indexed_legacy: &[u8] = b"\x1b[58;5;{d}m";
pub const fg_rgb_legacy: &[u8] = b"\x1b[38;2;{d};{d};{d}m";
pub const bg_rgb_legacy: &[u8] = b"\x1b[48;2;{d};{d};{d}m";
pub const ul_rgb_legacy: &[u8] = b"\x1b[58;2;{d};{d};{d}m";
pub const ul_rgb_conpty: &[u8] = b"\x1b[58:2::{d}:{d}:{d}m";

// Underlines
pub const ul_off: &[u8] = b"\x1b[24m"; // NOTE: this could be \x1b[4:0m but is not as widely supported
pub const ul_single: &[u8] = b"\x1b[4m";
pub const ul_double: &[u8] = b"\x1b[4:2m";
pub const ul_curly: &[u8] = b"\x1b[4:3m";
pub const ul_dotted: &[u8] = b"\x1b[4:4m";
pub const ul_dashed: &[u8] = b"\x1b[4:5m";

// Attributes
pub const bold_set: &[u8] = b"\x1b[1m";
pub const dim_set: &[u8] = b"\x1b[2m";
pub const italic_set: &[u8] = b"\x1b[3m";
pub const blink_set: &[u8] = b"\x1b[5m";
pub const reverse_set: &[u8] = b"\x1b[7m";
pub const invisible_set: &[u8] = b"\x1b[8m";
pub const strikethrough_set: &[u8] = b"\x1b[9m";
pub const bold_dim_reset: &[u8] = b"\x1b[22m";
pub const italic_reset: &[u8] = b"\x1b[23m";
pub const blink_reset: &[u8] = b"\x1b[25m";
pub const reverse_reset: &[u8] = b"\x1b[27m";
pub const invisible_reset: &[u8] = b"\x1b[28m";
pub const strikethrough_reset: &[u8] = b"\x1b[29m";

// OSC sequences
pub const osc2_set_title: &[u8] = b"\x1b]2;{s}\x1b\\";
pub const osc7: &[u8] = b"\x1b]7;{f}\x1b\\";
pub const osc8: &[u8] = b"\x1b]8;{s};{s}\x1b\\";
pub const osc8_clear: &[u8] = b"\x1b]8;;\x1b\\";
pub const osc9_notify: &[u8] = b"\x1b]9;{s}\x1b\\";
pub const osc777_notify: &[u8] = b"\x1b]777;notify;{s};{s}\x1b\\";
pub const osc22_mouse_shape: &[u8] = b"\x1b]22;{s}\x1b\\";
pub const osc52_clipboard_copy: &[u8] = b"\x1b]52;c;{s}\x1b\\";
pub const osc52_clipboard_request: &[u8] = b"\x1b]52;c;?\x1b\\";

// Kitty graphics
pub const kitty_graphics_clear: &[u8] = b"\x1b_Ga=d\x1b\\";
pub const kitty_graphics_preamble: &[u8] = b"\x1b_Ga=p,i={d}";
pub const kitty_graphics_closing: &[u8] = b",C=1\x1b\\";

// Color control sequences
pub const osc4_query: &[u8] = b"\x1b]4;{d};?\x1b\\"; // color index {d}
pub const osc4_reset: &[u8] = b"\x1b]104\x1b\\"; // this resets _all_ color indexes
pub const osc10_query: &[u8] = b"\x1b]10;?\x1b\\"; // fg
pub const osc10_set: &[u8] = b"\x1b]10;rgb:{x:0>2}{x:0>2}/{x:0>2}{x:0>2}/{x:0>2}{x:0>2}\x1b\\"; // set default terminal fg
pub const osc10_reset: &[u8] = b"\x1b]110\x1b\\"; // reset fg to terminal default
pub const osc11_query: &[u8] = b"\x1b]11;?\x1b\\"; // bg
pub const osc11_set: &[u8] = b"\x1b]11;rgb:{x:0>2}{x:0>2}/{x:0>2}{x:0>2}/{x:0>2}{x:0>2}\x1b\\"; // set default terminal bg
pub const osc11_reset: &[u8] = b"\x1b]111\x1b\\"; // reset bg to terminal default
pub const osc12_query: &[u8] = b"\x1b]12;?\x1b\\"; // cursor color
pub const osc12_set: &[u8] = b"\x1b]12;rgb:{x:0>2}{x:0>2}/{x:0>2}{x:0>2}/{x:0>2}{x:0>2}\x1b\\"; // set terminal cursor color
pub const osc12_reset: &[u8] = b"\x1b]112\x1b\\"; // reset cursor to terminal default
