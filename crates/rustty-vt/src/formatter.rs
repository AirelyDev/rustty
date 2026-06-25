//! rustty-vt formatter
//!
//! PORTS: ghostty src/terminal/formatter.zig
//!
//! PORT STATUS: production formatters FILLED (vt-11b) — `PageFormatter` (incl.
//! the `formatWithState` core loop + `TrailingState`), `PageListFormatter`, and
//! `ScreenFormatter` + all cell/writer/style helpers are ported and exercised
//! via Screen dump tests. `TerminalFormatter` remains a compiling stub (needs
//! `Terminal`, vt-12); the ~100 formatter `test{}` blocks defer to vt-12 (they
//! build fixtures via `Terminal::vtStream`). See DEFERRALS.md.
// PORT NOTE: the remaining `clippy::todo`/`dead_code`/`unused_*` allows now cover
// ONLY the `TerminalFormatter::{init,format}` stub bodies (`todo!()`) and the
// args/imports they will consume once `Terminal` lands in vt-12. The production
// formatters below are fully ported. `should_implement_trait` is for the inherent
// `format` methods (faithful to the reference's `format` name, not `std::fmt`).
#![allow(clippy::todo, dead_code, unused_variables, unused_imports)]
#![allow(clippy::should_implement_trait)]

// const std = @import("std");
// const assert = @import("../quirks.zig").inlineAssert;
// const lib = @import("lib.zig");
// const Allocator = std.mem.Allocator;
// const color = @import("color.zig");
// const size = @import("size.zig");
// const charsets = @import("charsets.zig");
// const hyperlink = @import("hyperlink.zig");
// const kitty = @import("kitty.zig");
// const modespkg = @import("modes.zig");
// const Screen = @import("Screen.zig");
// const Terminal = @import("Terminal.zig");
// const Cell = @import("page.zig").Cell;
// const Coordinate = @import("point.zig").Coordinate;
// const Page = @import("page.zig").Page;
// const PageList = @import("PageList.zig");
// const Pin = PageList.Pin;
// const Row = @import("page.zig").Row;
// const Selection = @import("Selection.zig");
// const Style = @import("style.zig").Style;
use crate::color;
use crate::page as pagepkg;
use crate::page::u21;
use crate::page::{Cell, Page, Row};
use crate::pagelist::{PageList, Pin};
use crate::point;
use crate::point::Coordinate;
use crate::screen::Screen;
use crate::selection::Selection;
use crate::size;

// PORT NOTE: the reference allocator maps to `allocator_api2::alloc::Global`
// (the crate's allocator seam).
use allocator_api2::alloc::Global as Allocator;

// PORT NOTE: the reference writer is `*std.Io.Writer` (a byte sink). Per the
// Screen-SCC plan seam, this ports to `&mut dyn std::io::Write`.
use std::io;
use std::io::Write as _;

// PORT NOTE (writer seam bridge): the reference passes a `*std.Io.Writer` and
// uses Zig formatting (`writer.print("{f}", .{style.formatterVt()})`) to render
// styles. In rustty, `style.rs`'s `VtFormatter`/`HtmlFormatter` already-ported
// `format` methods target `core::fmt::Write` (the idiomatic Rust `{}` sink),
// while this formatter's public seam is `&mut dyn std::io::Write`. To delegate
// style emission to `style.rs` without re-porting it, we wrap the `io::Write`
// sink in a tiny adapter that implements `core::fmt::Write` by forwarding the
// UTF-8 bytes. Style/escape output is always valid UTF-8, so this is lossless.
struct IoToFmt<'w> {
    inner: &'w mut dyn io::Write,
    err: Option<io::Error>,
}

impl<'w> IoToFmt<'w> {
    fn new(inner: &'w mut dyn io::Write) -> IoToFmt<'w> {
        IoToFmt { inner, err: None }
    }
}

impl core::fmt::Write for IoToFmt<'_> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        match self.inner.write_all(s.as_bytes()) {
            Ok(()) => Ok(()),
            Err(e) => {
                self.err = Some(e);
                Err(core::fmt::Error)
            }
        }
    }
}

// PORT NOTE: the reference `std.Io.Writer.Discarding` is a sink that counts the
// bytes written without storing them (used to pre-compute how many bytes an
// emission will produce so each one can be mapped to the same pin/coordinate).
// This is the faithful equivalent: an `io::Write` that only tracks a count.
struct Discarding {
    count: usize,
}

impl Discarding {
    fn new() -> Discarding {
        Discarding { count: 0 }
    }
}

impl io::Write for Discarding {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.count += buf.len();
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

// PORT NOTE: helper to encode a Unicode codepoint as UTF-8 to an `io::Write`,
// mirroring the reference `writer.print("{u}", .{codepoint})`. Invalid scalar
// values are passed through via `char::from_u32`; the reference assumes valid
// codepoints (they come from validated page contents).
fn write_codepoint_utf8(writer: &mut dyn io::Write, codepoint: u21) -> io::Result<()> {
    let c = char::from_u32(codepoint).unwrap_or('\u{FFFD}');
    let mut buf = [0u8; 4];
    writer.write_all(c.encode_utf8(&mut buf).as_bytes())
}

/// Formats available.
// PORT NOTE: `Format = lib.Enum(lib.target, &.{...})`; plain enum for the `.zig`
// target with the keys in order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    // Plain text.
    Plain,

    // Include VT sequences to preserve colors, styles, URLs, etc.
    // This is predominantly SGR sequences but may contain others as needed.
    //
    // Note that for reference colors, like palette indices, this will
    // vary based on the formatter and you should see the docs. For example,
    // PageFormatter with VT will emit SGR sequences with palette indices,
    // not the color itself.
    //
    // For VT, newlines will be emitted as `\r\n` so that the cursor properly
    // moves back to the beginning prior emitting follow-up lines.
    Vt,

    // HTML output.
    //
    // This will emit inline styles for as much styling as possible,
    // in the interest of simplicity and ease of editing. This isn't meant
    // to build the most beautiful or efficient HTML, but rather to be
    // stylistically correct.
    //
    // For colors, RGB values are emitted as inline CSS (#RRGGBB) while palette
    // indices use CSS variables (var(--vt-palette-N)). The palette colors are
    // emitted by TerminalFormatter.Extra.palette as a <style> block if you
    // want to also include that. But if you only format a screen or lower,
    // the formatter doesn't have access to the current palette to render it.
    //
    // Newlines are emitted as actual '\n' characters. Consumers should use
    // CSS white-space: pre or pre-wrap to preserve spacing and alignment.
    Html,
}

/// Returns true if the format emits styled output (not plaintext).
pub fn format_styled(fmt: Format) -> bool {
    match fmt {
        Format::Plain => false,
        Format::Html | Format::Vt => true,
    }
}

pub struct CodepointMap {
    /// Unicode codepoint range to replace.
    /// Asserts: range[0] <= range[1]
    pub range: [u21; 2],

    /// Replacement value for this range.
    pub replacement: Replacement,
}

pub enum Replacement {
    /// A single replacement codepoint.
    Codepoint(u21),

    /// A UTF-8 encoded string to replace with. Asserts the
    /// UTF-8 encoding (must be valid).
    // PORT NOTE: `[]const u8` -> `Vec<u8>` (never a lossy `String`).
    String(Vec<u8>),
}

/// Common encoding options regardless of what exact formatter is used.
pub struct Options {
    /// The format to emit.
    pub emit: Format,

    /// Whether to unwrap soft-wrapped lines. If false, this will emit the
    /// screen contents as it is rendered on the page in the given size.
    pub unwrap: bool,

    /// Trim trailing whitespace on lines with other text. Trailing blank
    /// lines are always trimmed. This only affects trailing whitespace
    /// on rows that have at least one other cell with text. Whitespace
    /// is currently only space characters (0x20).
    pub trim: bool,

    /// Replace matching Unicode codepoints with some other values.
    /// This will use the last matching range found in the list.
    // PORT NOTE: the reference `std.MultiArrayList(CodepointMap)` ports to a
    // `Vec<CodepointMap>` (the SoA optimization is not part of the parity
    // contract for this small map).
    pub codepoint_map: Option<Vec<CodepointMap>>,

    /// Set a background and foreground color to use for the "screen".
    /// For styled formats, this will emit the proper sequences or styles.
    pub background: Option<color::RGB>,
    pub foreground: Option<color::RGB>,

    /// If set, then styled formats in `emit` will use this palette to
    /// emit colors directly as RGB. If this is null, styled formats will
    /// still work but will use deferred palette styling (e.g. CSS variables
    /// for HTML or the actual palette indexes for VT).
    pub palette: Option<*const color::Palette>,
}

impl Options {
    // PORT NOTE: the reference `Options` struct uses field defaults
    // (`unwrap = false`, `trim = true`, `codepoint_map = .{}`, etc.). Rust has
    // no struct-literal defaults, so the named consts below carry the same
    // defaults and the per-format constructors `plain`/`vt`/`html` mirror the
    // reference `pub const plain/vt/html` declarations.
    fn base(emit: Format) -> Options {
        Options {
            emit,
            unwrap: false,
            trim: true,
            // PORT NOTE: the reference default is an empty `MultiArrayList`
            // (`.{}`), i.e. a present-but-empty list, NOT null. We mirror that
            // with `Some(vec![])` so codepoint-map lookups iterate an empty set
            // rather than skipping the search.
            codepoint_map: Some(Vec::new()),
            background: None,
            foreground: None,
            palette: None,
        }
    }

    pub fn plain() -> Options {
        Options::base(Format::Plain)
    }
    pub fn vt() -> Options {
        Options::base(Format::Vt)
    }
    pub fn html() -> Options {
        Options::base(Format::Html)
    }
}

/// Maps byte positions in formatted output to PageList pins.
///
/// Used by formatters that operate on PageLists to track the source position
/// of each byte written. The caller is responsible for freeing the map.
pub struct PinMap<'a> {
    pub alloc: Allocator,
    pub map: &'a mut Vec<Pin>,
}

/// Terminal formatter formats the active terminal screen.
///
/// This will always only emit data related to the currently active screen.
/// If you want to emit data for a specific screen (e.g. primary vs alt), then
/// switch to that screen in the terminal prior to using this.
///
/// If you want to emit data for all screens (a less common operation), then
/// you must create a no-content TerminalFormatter followed by multiple
/// explicit ScreenFormatter calls. This isn't a common operation so this
/// little extra work should be acceptable.
///
/// For styled formatting, this will emit the palette colors at the
/// beginning so that the output can be rendered properly according to
/// the current terminal state.
//
// PORT NOTE: `TerminalFormatter` is a COMPILING STUB. It depends on `Terminal`
// (`terminal: *const Terminal`), which is ported in vt-12. The struct and its
// `Extra` packed-struct are defined here verbatim (with comments preserved) but
// the `terminal` field is held back until `Terminal` exists. `init`/`format`
// are signature stubs with `todo!()` bodies.
pub struct TerminalFormatter {
    // PORT NOTE: `terminal: *const Terminal` — DEFERRED to vt-12 (needs the
    // ported `Terminal` type). Restored once `terminal.rs` lands.
    /// The common options
    pub opts: Options,

    /// The content to include.
    pub content: Content,

    /// Extra stuff to emit, such as terminal modes, palette, cursor, etc.
    /// This information is ONLY emitted when the format is "vt".
    pub extra: TerminalExtra,

    /// If non-null, then `map` will contain the Pin of every byte
    /// byte written to the writer offset by the byte index. It is the
    /// caller's responsibility to free the map.
    ///
    /// Note that some emitted bytes may not correspond to any Pin, such as
    /// the extra data around terminal state (palette, modes, etc.). For these,
    /// we'll map it to the most previous pin so there is some continuity but
    /// its an arbitrary choice.
    ///
    /// Warning: there is a significant performance hit to track this
    pub pin_map: Option<PinMap<'static>>,
}

// PORT NOTE: nested `TerminalFormatter.Extra` packed struct, hoisted to a
// top-level name to avoid the inherent-associated-type limitation in Rust.
#[derive(Clone, Copy)]
pub struct TerminalExtra {
    /// Emit the palette using OSC 4 sequences.
    pub palette: bool,

    /// Emit terminal modes that differ from their defaults using CSI h/l
    /// sequences. Defaults are according to the Ghostty defaults which
    /// are generally match most terminal defaults. This will include
    /// things like current screen, bracketed mode, mouse event reporting,
    /// etc.
    pub modes: bool,

    /// Emit scrolling region state using DECSTBM and DECSLRM sequences.
    pub scrolling_region: bool,

    /// Emit tabstop positions by clearing all tabs (CSI 3 g) and setting
    /// each configured tabstop with HTS.
    pub tabstops: bool,

    /// Emit the present working directory using OSC 7.
    pub pwd: bool,

    /// Emit keyboard modes such as ModifyOtherKeys using CSI > 4 m
    /// sequences.
    pub keyboard: bool,

    /// The screen extras to emit. TerminalFormatter always only
    /// emits data for the currently active screen. If you want to emit
    /// data for all screens, you should manually construct a no-content
    /// terminal formatter, followed by screen formatters.
    pub screen: ScreenExtra,
}

impl TerminalExtra {
    /// Emit nothing.
    pub const NONE: TerminalExtra = TerminalExtra {
        palette: false,
        modes: false,
        scrolling_region: false,
        tabstops: false,
        pwd: false,
        keyboard: false,
        screen: ScreenExtra::NONE,
    };

    /// Emit style-relevant information only such as palettes.
    pub const STYLES: TerminalExtra = TerminalExtra {
        palette: true,
        modes: false,
        scrolling_region: false,
        tabstops: false,
        pwd: false,
        keyboard: false,
        screen: ScreenExtra::STYLES,
    };

    /// Emit everything. This reconstructs the terminal state as closely
    /// as possible.
    pub const ALL: TerminalExtra = TerminalExtra {
        palette: true,
        modes: true,
        scrolling_region: true,
        tabstops: true,
        pwd: true,
        keyboard: true,
        screen: ScreenExtra::ALL,
    };
}

impl TerminalFormatter {
    // PORT NOTE: `init` takes `terminal: *const Terminal` in the reference;
    // DEFERRED to vt-12. The stub signature omits the terminal argument until
    // `Terminal` is ported.
    pub fn init(opts: Options) -> TerminalFormatter {
        todo!()
    }

    pub fn format(self, writer: &mut dyn io::Write) -> io::Result<()> {
        todo!()
    }
}

/// Screen formatter formats a single terminal screen (e.g. primary vs alt).
pub struct ScreenFormatter<'a> {
    /// The screen to format.
    pub screen: *const Screen,

    /// The common options
    pub opts: Options,

    /// The content to include.
    pub content: Content,

    /// Extra stuff to emit, such as cursor, style, hyperlinks, etc.
    /// This information is ONLY emitted when the format is "vt".
    pub extra: ScreenExtra,

    /// If non-null, then `map` will contain the Pin of every byte
    /// byte written to the writer offset by the byte index. It is the
    /// caller's responsibility to free the map.
    ///
    /// Note that some emitted bytes may not correspond to any Pin, such as
    /// the extra data around screen state. For these, we'll map it to the
    /// most previous pin so there is some continuity but its an arbitrary
    /// choice.
    ///
    /// Warning: there is a significant performance hit to track this
    pub pin_map: Option<PinMap<'a>>,
}

// PORT NOTE: nested `ScreenFormatter.Content` union(enum), hoisted to a
// top-level name. Shared by `TerminalFormatter` (which references
// `ScreenFormatter.Content`).
pub enum Content {
    /// Emit no content, only terminal state such as modes, palette, etc.
    /// via extra.
    None,

    /// Emit the content specified by the selection. Null for all.
    /// The selection is inclusive on both ends.
    Selection(Option<Selection>),
}

// PORT NOTE: nested `ScreenFormatter.Extra` packed struct, hoisted to a
// top-level name. Referenced by `TerminalExtra.screen`.
#[derive(Clone, Copy)]
pub struct ScreenExtra {
    /// Emit cursor position using CUP (CSI H).
    pub cursor: bool,

    /// Emit current SGR style state based on the cursor's active style_id.
    /// This reconstructs the SGR attributes (bold, italic, colors, etc.) at
    /// the cursor position.
    pub style: bool,

    /// Emit current hyperlink state using OSC 8 sequences.
    /// This sets the active hyperlink based on cursor.hyperlink_id.
    pub hyperlink: bool,

    /// Emit character protection mode using DECSCA.
    pub protection: bool,

    /// Emit Kitty keyboard protocol state using CSI > u and CSI = sequences.
    pub kitty_keyboard: bool,

    /// Emit character set designations and invocations.
    /// This includes G0-G3 designations (ESC ( ) * +) and GL/GR invocations.
    pub charsets: bool,
}

impl ScreenExtra {
    /// Emit nothing.
    pub const NONE: ScreenExtra = ScreenExtra {
        cursor: false,
        style: false,
        hyperlink: false,
        protection: false,
        kitty_keyboard: false,
        charsets: false,
    };

    /// Emit style-relevant information only.
    pub const STYLES: ScreenExtra = ScreenExtra {
        cursor: false,
        style: true,
        hyperlink: true,
        protection: false,
        kitty_keyboard: false,
        charsets: false,
    };

    /// Emit everything. This reconstructs the screen state as closely
    /// as possible.
    pub const ALL: ScreenExtra = ScreenExtra {
        cursor: true,
        style: true,
        hyperlink: true,
        protection: true,
        kitty_keyboard: true,
        charsets: true,
    };

    // PORT NOTE: the reference bit-casts the packed struct to its backing
    // integer and checks `!= 0`. Rust's `ScreenExtra` is a plain bool struct, so
    // the faithful equivalent is "any field set".
    fn is_set(self) -> bool {
        self.cursor
            || self.style
            || self.hyperlink
            || self.protection
            || self.kitty_keyboard
            || self.charsets
    }
}

impl<'a> ScreenFormatter<'a> {
    pub fn init(screen: *const Screen, opts: Options) -> ScreenFormatter<'a> {
        ScreenFormatter {
            screen,
            opts,
            content: Content::Selection(None),
            extra: ScreenExtra::NONE,
            pin_map: None,
        }
    }

    pub fn format(mut self, writer: &mut dyn io::Write) -> io::Result<()> {
        // PORT NOTE: `self.screen` is a `*const Screen`. The reference accesses
        // fields directly; we deref the raw pointer (the caller guarantees the
        // screen outlives the formatter).
        let screen: &Screen = unsafe { &*self.screen };

        match self.content {
            Content::None => {}

            Content::Selection(selection_) => {
                // Emit our pagelist contents according to our selection.
                let mut list_formatter: PageListFormatter =
                    PageListFormatter::init(&screen.pages, copy_options(&self.opts));
                // PORT NOTE: the reference copies `self.pin_map` (a struct whose
                // `map` field is a pointer to a shared `ArrayList`) into the list
                // formatter. In Rust the equivalent is a *reborrow* of the
                // `&mut Vec<Pin>` so the same list is appended to; the borrow ends
                // when `format` consumes the list formatter, leaving `self.pin_map`
                // usable again for the extras pass below.
                if let Some(m) = self.pin_map.as_mut() {
                    list_formatter.pin_map = Some(PinMap {
                        alloc: m.alloc,
                        map: &mut *m.map,
                    });
                }
                if let Some(sel) = selection_ {
                    list_formatter.top_left = Some(sel.top_left(screen));
                    list_formatter.bottom_right = Some(sel.bottom_right(screen));
                    list_formatter.rectangle = sel.rectangle;
                }
                list_formatter.format(writer)?;
            }
        }

        // Emit extra screen state after content if we care. The state has
        // to be emitted after since some state such as cursor position and
        // style are impacted by content rendering.
        match self.opts.emit {
            Format::Plain => return Ok(()),
            Format::Vt => {
                if !self.extra.is_set() {
                    return Ok(());
                }
            }

            // HTML doesn't preserve any screen state because it has
            // nothing to do with rendering.
            Format::Html => return Ok(()),
        }

        // Emit current SGR style state
        if self.extra.style {
            let cursor = &screen.cursor;
            let formatter = cursor.style.formatter_vt();
            let mut adapter = IoToFmt::new(writer);
            write_fmt_result(&mut adapter, |w| formatter.format(w))?;
        }

        // Emit current hyperlink state using OSC 8
        if self.extra.hyperlink {
            let cursor = &screen.cursor;
            if let Some(link_ptr) = cursor.hyperlink {
                // PORT NOTE: `cursor.hyperlink` is `Option<*mut Hyperlink>`;
                // deref the raw pointer to borrow the live hyperlink.
                let link = unsafe { &*link_ptr };
                // Start hyperlink with uri (and explicit id if present)
                match &link.id {
                    crate::hyperlink::HyperlinkId::Explicit(id) => {
                        writer.write_all(b"\x1b]8;id=")?;
                        writer.write_all(id)?;
                        writer.write_all(b";")?;
                        writer.write_all(&link.uri)?;
                        writer.write_all(b"\x1b\\")?;
                    }
                    crate::hyperlink::HyperlinkId::Implicit(_) => {
                        writer.write_all(b"\x1b]8;;")?;
                        writer.write_all(&link.uri)?;
                        writer.write_all(b"\x1b\\")?;
                    }
                }
            }
        }

        // Emit character protection mode using DECSCA
        if self.extra.protection {
            let cursor = &screen.cursor;
            if cursor.protected {
                // DEC protected mode
                writer.write_all(b"\x1b[1\"q")?;
            }
        }

        // Emit Kitty keyboard protocol state using CSI = u
        if self.extra.kitty_keyboard {
            let current_flags = screen.kitty_keyboard.current();
            if current_flags.int() != crate::kitty::KeyFlags::DISABLED.int() {
                let flags = current_flags.int();
                write!(writer, "\x1b[={};1u", flags)?;
            }
        }

        // Emit character set designations and invocations
        if self.extra.charsets {
            let charset = &screen.charset;

            // Emit G0-G3 designations
            // PORT NOTE: `std.enums.values(charsets.Slots)` — the ordered list
            // of all enum tags; inlined here.
            for slot in [
                crate::charsets::Slots::G0,
                crate::charsets::Slots::G1,
                crate::charsets::Slots::G2,
                crate::charsets::Slots::G3,
            ] {
                let cs = charset.charsets.get(slot);
                if cs != crate::charsets::Charset::Utf8 {
                    // Only emit non-default charsets
                    let intermediate: u8 = match slot {
                        crate::charsets::Slots::G0 => b'(',
                        crate::charsets::Slots::G1 => b')',
                        crate::charsets::Slots::G2 => b'*',
                        crate::charsets::Slots::G3 => b'+',
                    };
                    let final_byte: u8 = match cs {
                        crate::charsets::Charset::Ascii => b'B',
                        crate::charsets::Charset::British => b'A',
                        crate::charsets::Charset::DecSpecial => b'0',
                        _ => continue,
                    };
                    writer.write_all(&[0x1b, intermediate, final_byte])?;
                }
            }

            // Emit GL invocation if not G0
            if charset.gl != crate::charsets::Slots::G0 {
                let seq: &[u8] = match charset.gl {
                    crate::charsets::Slots::G0 => unreachable!(),
                    crate::charsets::Slots::G1 => b"\x0e", // SO - Shift Out
                    crate::charsets::Slots::G2 => b"\x1bn", // LS2
                    crate::charsets::Slots::G3 => b"\x1bo", // LS3
                };
                writer.write_all(seq)?;
            }

            // Emit GR invocation if not G2
            if charset.gr != crate::charsets::Slots::G2 {
                let seq: &[u8] = match charset.gr {
                    crate::charsets::Slots::G0 => unreachable!(), // GR can't be G0
                    crate::charsets::Slots::G1 => b"\x1b~",       // LS1R
                    crate::charsets::Slots::G2 => unreachable!(),
                    crate::charsets::Slots::G3 => b"\x1b|", // LS3R
                };
                writer.write_all(seq)?;
            }
        }

        // Emit cursor position using CUP (CSI H)
        if self.extra.cursor {
            let cursor = &screen.cursor;
            // CUP is 1-indexed
            write!(writer, "\x1b[{};{}H", cursor.y + 1, cursor.x + 1)?;
        }

        // If we have a pin_map, we need to count how many bytes the extras
        // will emit so we can map them all to the same pin. We do this by
        // formatting to a discarding writer with content=none.
        if let Some(m) = self.pin_map.as_mut() {
            let mut discarding = Discarding::new();
            let mut extra_formatter: ScreenFormatter = ScreenFormatter {
                screen: self.screen,
                opts: copy_options(&self.opts),
                content: Content::None,
                extra: self.extra,
                pin_map: None,
            };
            // PORT NOTE: `extra_formatter.content = .none; .pin_map = null;` —
            // already set above; everything else is a copy of `self`.
            let _ = &mut extra_formatter;
            extra_formatter.format(&mut discarding)?;

            // Map all those bytes to the same pin. Use the first page node
            // to ensure the node pointer is always properly initialized.
            let pin = if !m.map.is_empty() {
                // There is a weird Zig miscompilation here on 0.15.2.
                // If I return the m.map.items value directly then we
                // get undefined memory (even though we're copying a
                // Pin struct). If we duplicate here like this we do
                // not.
                let last = m.map[m.map.len() - 1];
                Pin {
                    node: last.node,
                    x: last.x,
                    y: last.y,
                    garbage: last.garbage,
                }
            } else {
                screen.pages.get_top_left(point::Tag::Screen)
            };
            for _ in 0..discarding.count {
                m.map.push(pin);
            }
        }

        Ok(())
    }
}

// PORT NOTE: small helper to bridge the `core::fmt::Write` style formatters
// (style.rs) onto the `io::Write` seam and to surface their `io::Error`.
// Runs a closure that writes through the `IoToFmt` adapter and converts a
// `core::fmt::Error` back into the captured `io::Error`.
fn write_fmt_result(
    adapter: &mut IoToFmt<'_>,
    f: impl FnOnce(&mut IoToFmt<'_>) -> core::fmt::Result,
) -> io::Result<()> {
    match f(adapter) {
        Ok(()) => Ok(()),
        Err(_) => Err(adapter
            .err
            .take()
            .unwrap_or_else(|| io::Error::other("fmt error"))),
    }
}

// PORT NOTE: `Options` holds a `Vec<CodepointMap>` and is therefore not `Copy`.
// The reference copies the `Options` struct freely (it is a Zig value type). For
// the rare "extra formatter on a discarding writer" path we need a shallow copy
// with the same fields; `codepoint_map` does not affect extras emission, so we
// clone the simple fields and carry an empty map.
fn copy_options(opts: &Options) -> Options {
    Options {
        emit: opts.emit,
        unwrap: opts.unwrap,
        trim: opts.trim,
        codepoint_map: Some(Vec::new()),
        background: opts.background,
        foreground: opts.foreground,
        palette: opts.palette,
    }
}

/// PageList formatter formats multiple pages as represented by a PageList.
pub struct PageListFormatter<'a> {
    /// The pagelist to format.
    pub list: *const PageList,

    /// The common options
    pub opts: Options,

    /// The bounds of the PageList to format. The top left and bottom right
    /// MUST be ordered properly.
    pub top_left: Option<Pin>,
    pub bottom_right: Option<Pin>,

    /// If true, the boundaries define a rectangle selection where start_x
    /// and end_x apply to every row, not just the first and last.
    pub rectangle: bool,

    /// If non-null, then `map` will contain the Pin of every byte
    /// byte written to the writer offset by the byte index. It is the
    /// caller's responsibility to free the map.
    ///
    /// Warning: there is a significant performance hit to track this
    pub pin_map: Option<PinMap<'a>>,
}

impl<'a> PageListFormatter<'a> {
    pub fn init(list: *const PageList, opts: Options) -> PageListFormatter<'a> {
        PageListFormatter {
            list,
            opts,
            top_left: None,
            bottom_right: None,
            rectangle: false,
            pin_map: None,
        }
    }

    pub fn format(mut self, writer: &mut dyn io::Write) -> io::Result<()> {
        // PORT NOTE: `self.list` is a `*const PageList`; deref to access (the
        // caller guarantees it outlives the formatter).
        let list: &PageList = unsafe { &*self.list };

        let tl: Pin = self
            .top_left
            .unwrap_or_else(|| list.get_top_left(point::Tag::Screen));
        let br: Pin = self
            .bottom_right
            .unwrap_or_else(|| list.get_bottom_right(point::Tag::Screen).unwrap());

        // If we keep track of pins, we'll need this.
        let mut point_map: Vec<Coordinate> = Vec::new();

        let mut page_state: Option<TrailingState> = None;
        let mut iter = tl.page_iterator(crate::pagelist::iter::Direction::RightDown, Some(br));
        while let Some(chunk) = iter.next() {
            debug_assert!(chunk.start < chunk.end);
            debug_assert!(chunk.end > 0);

            // PORT NOTE: `&chunk.node.data` — the page lives in the node; deref
            // the raw `*mut Node` to take a `*const Page`.
            let page: *const Page = unsafe { &(*chunk.node).data };
            let mut formatter: PageFormatter = PageFormatter::init(page, copy_options(&self.opts));
            formatter.start_y = chunk.start;
            formatter.end_y = Some(chunk.end - 1);
            formatter.trailing_state = page_state;
            formatter.rectangle = self.rectangle;

            // For rectangle selection, apply start_x and end_x to all chunks
            if self.rectangle {
                formatter.start_x = tl.x;
                formatter.end_x = Some(br.x);
            } else {
                // Otherwise only on the first/last, respectively.
                if core::ptr::eq(chunk.node, tl.node) {
                    formatter.start_x = tl.x;
                }
                if core::ptr::eq(chunk.node, br.node) {
                    formatter.end_x = Some(br.x);
                }
            }

            // If we're tracking pins, then we setup a point map for the
            // page formatter (cause it can't track pins). And then we convert
            // this to pins later.
            if self.pin_map.is_some() {
                point_map.clear();
                formatter.point_map = Some(PointMap {
                    alloc: Allocator,
                    map: &mut point_map,
                });
            }

            page_state = Some(formatter.format_with_state(writer)?);

            // If we're tracking pins then grab our points and write them
            // to our pin map.
            if let Some(m) = self.pin_map.as_mut() {
                for coord in point_map.iter() {
                    m.map.push(Pin {
                        node: chunk.node,
                        x: coord.x,
                        y: coord.y as size::CellCountInt,
                        garbage: false,
                    });
                }
            }
        }

        Ok(())
    }
}

/// Page formatter.
///
/// For styled formatting such as VT, this will emit references for palette
/// colors. If you want to capture the palette as-is at the type of formatting,
/// you'll have to emit the sequences for setting up the palette prior to
/// this formatting. (TODO: A function to do this)
pub struct PageFormatter<'a> {
    /// The page to format.
    pub page: *const Page,

    /// The common options
    pub opts: Options,

    /// Start and end points within the page to format. If end x is not given
    /// then it will be the full width. If end y is not given then it will be
    /// the full height.
    ///
    /// The start and end are both inclusive, so equal values will still
    /// return a non-empty result (i.e. a single cell or row).
    ///
    /// The start x is considered the X in the first row and end X is
    /// X in the final row. This isn't a rectangle selection by default.
    ///
    /// If start X falls on the second column of a wide character, then
    /// the entire character will be included (as if you specified the
    /// previous column).
    pub start_x: size::CellCountInt,
    pub start_y: size::CellCountInt,
    pub end_x: Option<size::CellCountInt>,
    pub end_y: Option<size::CellCountInt>,

    /// If true, the start x/y and end x/y define a rectangle selection.
    /// In this case, the boundaries will apply to every row, not just
    /// the first and last.
    pub rectangle: bool,

    /// If non-null, then `map` will contain the x/y coordinate of every
    /// byte written to the writer offset by the byte index. It is the
    /// caller's responsibility to free the map.
    ///
    /// The x/y coordinate will be the coordinates within the page.
    ///
    /// Warning: there is a significant performance hit to track this
    pub point_map: Option<PointMap<'a>>,

    /// The previous trailing state from the prior page. If you're iterating
    /// over multiple pages this helps ensure that unwrapping and other
    /// accounting works properly.
    pub trailing_state: Option<TrailingState>,
}

// PORT NOTE: inline anonymous param struct for `PageFormatter.point_map`.
pub struct PointMap<'a> {
    pub alloc: Allocator,
    pub map: &'a mut Vec<Coordinate>,
}

/// Trailing state. This is used to ensure that rows wrapped across
/// multiple pages are unwrapped properly, as well as other accounting
/// we may do in the future.
#[derive(Clone, Copy)]
pub struct TrailingState {
    pub rows: usize,
    pub cells: usize,
}

impl TrailingState {
    pub const EMPTY: TrailingState = TrailingState { rows: 0, cells: 0 };
}

impl<'a> PageFormatter<'a> {
    /// Initializes a page formatter. Other options can be set directly on the
    /// struct after initialization and before calling `format()`.
    pub fn init(page: *const Page, opts: Options) -> PageFormatter<'a> {
        PageFormatter {
            page,
            opts,
            start_x: 0,
            start_y: 0,
            end_x: None,
            end_y: None,
            rectangle: false,
            point_map: None,
            trailing_state: None,
        }
    }

    pub fn format(self, writer: &mut dyn io::Write) -> io::Result<()> {
        let _ = self.format_with_state(writer)?;
        Ok(())
    }

    pub fn format_with_state(mut self, writer: &mut dyn io::Write) -> io::Result<TrailingState> {
        // PORT NOTE: `self.page` is a `*const Page`; deref to access (the caller
        // guarantees the page outlives the formatter).
        let page: &Page = unsafe { &*self.page };

        let mut blank_rows: usize = 0;
        let mut blank_cells: usize = 0;

        // Continue our prior trailing state if we have it, but only if we're
        // starting from the beginning (start_y and start_x are both 0).
        // If a non-zero start position is specified, ignore trailing state.
        if let Some(state) = self.trailing_state {
            if self.start_y == 0 && self.start_x == 0 {
                blank_rows = state.rows;
                blank_cells = state.cells;
            }
        }

        // Setup our starting column and perform some validation for overflows.
        // Note: start_x only applies to the first row, end_x only applies to the last row.
        let start_x: size::CellCountInt = self.start_x;
        if start_x >= page.size.cols {
            return Ok(TrailingState {
                rows: blank_rows,
                cells: blank_cells,
            });
        }
        let end_x_unclamped: size::CellCountInt = self.end_x.unwrap_or(page.size.cols - 1);
        let mut end_x = core::cmp::min(end_x_unclamped, page.size.cols - 1);

        // Setup our starting row and perform some validation for overflows.
        let start_y: size::CellCountInt = self.start_y;
        if start_y >= page.size.rows {
            return Ok(TrailingState {
                rows: blank_rows,
                cells: blank_cells,
            });
        }
        let end_y_unclamped: size::CellCountInt = self.end_y.unwrap_or(page.size.rows - 1);
        if start_y > end_y_unclamped {
            return Ok(TrailingState {
                rows: blank_rows,
                cells: blank_cells,
            });
        }
        let mut end_y = core::cmp::min(end_y_unclamped, page.size.rows - 1);

        // Edge case: if our end x/y falls on a spacer head AND we're unwrapping,
        // then we move the x/y to the start of the next row (if available).
        if self.opts.unwrap && !self.rectangle {
            let final_row = page.get_row(end_y as usize);
            let cells = unsafe { &*page.get_cells(final_row) };
            if let pagepkg::Wide::SpacerHead = cells[end_x as usize].wide() {
                // Move to next row if available
                //
                // TODO: if unavailable, we should add to our trailing state
                //
                // so the pagelist formatter can be aware and maybe add
                // another page
                if end_y < page.size.rows - 1 {
                    end_y += 1;
                    end_x = 0;
                }
            }
        }

        // If we only have a single row, validate that start_x <= end_x
        if start_y == end_y && start_x > end_x {
            return Ok(TrailingState {
                rows: blank_rows,
                cells: blank_cells,
            });
        }

        // Wrap HTML output in monospace font styling
        match self.opts.emit {
            Format::Plain => {}

            Format::Html => {
                // Setup our div. We use a buffer here that should always
                // fit the stuff we need, in order to make counting bytes easier.
                // PORT NOTE: the reference uses a fixed 1024-byte buffer to make
                // counting easy; a `Vec<u8>` is the faithful sink and we measure
                // its length.
                let mut header: Vec<u8> = Vec::new();
                // Monospace and whitespace preserving
                header.extend_from_slice(b"<div style=\"font-family: monospace; white-space: pre;");

                // Background/foreground colors
                if let Some(bg) = self.opts.background {
                    write!(
                        header,
                        "background-color: #{:02x}{:02x}{:02x};",
                        bg.r, bg.g, bg.b
                    )?;
                }
                if let Some(fg) = self.opts.foreground {
                    write!(header, "color: #{:02x}{:02x}{:02x};", fg.r, fg.g, fg.b)?;
                }

                header.extend_from_slice(b"\">");

                writer.write_all(&header)?;
                if let Some(map) = self.point_map.as_mut() {
                    for _ in 0..header.len() {
                        map.map.push(Coordinate { x: 0, y: 0 });
                    }
                }
            }

            Format::Vt => {
                // OSC 10 sets foreground color, OSC 11 sets background color
                let mut header: Vec<u8> = Vec::new();
                if let Some(fg) = self.opts.foreground {
                    write!(
                        header,
                        "\x1b]10;rgb:{:02x}/{:02x}/{:02x}\x1b\\",
                        fg.r, fg.g, fg.b
                    )?;
                }
                if let Some(bg) = self.opts.background {
                    write!(
                        header,
                        "\x1b]11;rgb:{:02x}/{:02x}/{:02x}\x1b\\",
                        bg.r, bg.g, bg.b
                    )?;
                }

                writer.write_all(&header)?;
                if let Some(map) = self.point_map.as_mut() {
                    for _ in 0..header.len() {
                        map.map.push(Coordinate { x: 0, y: 0 });
                    }
                }
            }
        }

        // Our style for non-plain formats
        let mut style: crate::style::Style = crate::style::Style::default();

        // Track hyperlink state for HTML output. We need to close </a> tags
        // when the hyperlink changes or ends.
        let mut current_hyperlink_id: Option<crate::hyperlink::Id> = None;

        for y_usize in (start_y as usize)..(end_y as usize + 1) {
            let y: size::CellCountInt = y_usize as size::CellCountInt;
            let row: *mut Row = page.get_row(y as usize);
            let cells: &[Cell] = unsafe { &*page.get_cells(row) };

            // Determine the x range for this row
            // - First row: start_x to end of row (or end_x if single row)
            // - Last row: start of row to end_x
            // - Middle rows: full width
            // The end is always straightforward
            let row_end_x: size::CellCountInt = if self.rectangle || y == end_y {
                end_x + 1
            } else {
                page.size.cols
            };

            // The first we have to check if our start X falls on the
            // tail of a wide character.
            let row_start_x: size::CellCountInt = if start_x > 0 && (self.rectangle || y == start_y)
            {
                match cells[start_x as usize].wide() {
                    // Include the prior cell to get the full wide char
                    pagepkg::Wide::SpacerTail => start_x - 1,

                    // If we're a spacer head on our first row then we
                    // skip this whole row.
                    pagepkg::Wide::SpacerHead => continue,

                    pagepkg::Wide::Narrow | pagepkg::Wide::Wide => start_x,
                }
            } else {
                0
            };

            let cells_subset: &[Cell] = &cells[(row_start_x as usize)..(row_end_x as usize)];

            // If this row is blank, accumulate to avoid a bunch of extra
            // work later. If it isn't blank, make sure we dump all our
            // blanks.
            if !Cell::has_text_any(cells_subset) {
                blank_rows += 1;
                continue;
            }

            if blank_rows > 0 {
                // Reset style before emitting newlines to prevent background
                // colors from bleeding into the next line's leading cells.
                if !style.default_style() {
                    self.format_style_close(writer)?;
                    style = crate::style::Style::default();
                }

                let sequence: &[u8] = match self.opts.emit {
                    // Plaintext just uses standard newlines because newlines
                    // on their own usually move the cursor back in anywhere
                    // you type plaintext.
                    Format::Plain => b"\n",

                    // VT uses \r\n because in a raw pty, \n alone doesn't
                    // guarantee moving the cursor back to column 0. \r
                    // makes it work for sure.
                    Format::Vt => b"\r\n",

                    // HTML uses just \n because HTML rendering will move
                    // the cursor back.
                    Format::Html => b"\n",
                };

                for _ in 0..blank_rows {
                    writer.write_all(sequence)?;
                }

                // \r and \n map to the row that ends with this newline.
                // If we're continuing (trailing state) then this will be
                // in a prior page, so we just map to the first row of this
                // page.
                if let Some(map) = self.point_map.as_mut() {
                    let start: Coordinate = if !map.map.is_empty() {
                        map.map[map.map.len() - 1]
                    } else {
                        Coordinate { x: 0, y: 0 }
                    };

                    // The first one inherits the x value.
                    for _ in 0..sequence.len() {
                        map.map.push(Coordinate {
                            x: start.x,
                            y: start.y,
                        });
                    }

                    // All others have x = 0 since they reference their prior
                    // blank line.
                    for y_offset_usize in 1..blank_rows {
                        let y_offset: u32 = y_offset_usize as u32;
                        for _ in 0..sequence.len() {
                            map.map.push(Coordinate {
                                x: 0,
                                y: start.y + y_offset,
                            });
                        }
                    }
                }

                blank_rows = 0;
            }

            // If we're not wrapped, we always add a newline so after
            // the row is printed we can add a newline.
            if !unsafe { (*row).wrap() } || !self.opts.unwrap {
                blank_rows += 1;
            }

            // If the row doesn't continue a wrap then we need to reset
            // our blank cell count.
            if !unsafe { (*row).wrap_continuation() } || !self.opts.unwrap {
                blank_cells = 0;
            }

            // Go through each cell and print it
            'cells: for (i, cell) in cells_subset.iter().enumerate() {
                let x: size::CellCountInt = row_start_x + i as size::CellCountInt;

                // Skip spacers. These happen naturally when wide characters
                // are printed again on the screen (for well-behaved terminals!)
                match cell.wide() {
                    pagepkg::Wide::Narrow | pagepkg::Wide::Wide => {}
                    pagepkg::Wide::SpacerHead | pagepkg::Wide::SpacerTail => continue,
                }

                // If we have a zero value, then we accumulate a counter. We
                // only want to turn zero values into spaces if we have a non-zero
                // char sometime later.
                'blank: {
                    // If we're emitting styled output (not plaintext) and
                    // the cell has some kind of styling or is not empty
                    // then this isn't blank.
                    if format_styled(self.opts.emit) && (!cell.is_empty() || cell.has_styling()) {
                        break 'blank;
                    }

                    // Cells with no text are blank
                    if !cell.has_text() {
                        blank_cells += 1;
                        continue 'cells;
                    }

                    // Trailing spaces are blank. We know it is trailing
                    // because if we get a non-empty cell later we'll
                    // fill the blanks.
                    if cell.codepoint() == ' ' as u21 && self.opts.trim {
                        blank_cells += 1;
                        continue 'cells;
                    }
                }

                // This cell is not blank. If we have accumulated blank cells
                // then we want to emit them now.
                if blank_cells > 0 {
                    for _ in 0..blank_cells {
                        writer.write_all(b" ")?;
                    }

                    if let Some(map) = self.point_map.as_mut() {
                        // Map each blank cell to its coordinate. Blank cells can span
                        // multiple rows if they carry over from wrap continuation.
                        let mut remaining_blanks = blank_cells;
                        let mut blank_x = x;
                        let mut blank_y = y;
                        while remaining_blanks > 0 {
                            if blank_x > 0 {
                                // We have space in this row
                                blank_x -= 1;
                            } else if blank_y > 0 {
                                // Wrap to previous row
                                blank_y -= 1;
                                blank_x = page.size.cols - 1;
                            } else {
                                // Can't go back further, just use (0, 0)
                                blank_x = 0;
                                blank_y = 0;
                            }

                            map.map.push(Coordinate {
                                x: blank_x,
                                y: blank_y as u32,
                            });
                            remaining_blanks -= 1;
                        }
                    }

                    blank_cells = 0;
                }

                'style: {
                    // If we aren't emitting styled output then we don't
                    // have to worry about styles.
                    if !format_styled(self.opts.emit) {
                        break 'style;
                    }

                    // Get our cell style.
                    let cell_style = self.cell_style(page, cell);

                    // If the style hasn't changed, don't bloat output.
                    if cell_style.eql(&style) {
                        break 'style;
                    }

                    // If we had a previous style, we need to close it,
                    // because we've confirmed we have some new style
                    // (which is maybe default).
                    if !style.default_style() {
                        match self.opts.emit {
                            Format::Html => self.format_style_close(writer)?,

                            // For VT, we only close if we're switching to a default
                            // style because any non-default style will emit
                            // a \x1b[0m as the start of a VT coloring sequence.
                            Format::Vt => {
                                if cell_style.default_style() {
                                    self.format_style_close(writer)?;
                                }
                            }

                            // Unreachable because of the styled() check at the
                            // top of this block.
                            Format::Plain => unreachable!(),
                        }
                    }

                    // At this point, we can copy our style over
                    style = cell_style;

                    // If we're just the default style now, we're done.
                    if cell_style.default_style() {
                        break 'style;
                    }

                    // New style, emit it.
                    self.format_style_open(writer, &style)?;

                    // If we have a point map, we map the style to
                    // this cell.
                    if self.point_map.is_some() {
                        let mut discarding = Discarding::new();
                        self.format_style_open(&mut discarding, &style)?;
                        let count = discarding.count;
                        // PORT NOTE: `count` is computed above (needs `&self`),
                        // then the map borrow is taken separately to satisfy the
                        // borrow checker (cannot hold `&mut self.point_map` across
                        // the `&self` `format_style_open` call).
                        if let Some(map) = self.point_map.as_mut() {
                            for _ in 0..count {
                                map.map.push(Coordinate { x, y: y as u32 });
                            }
                        }
                    }
                }

                // Hyperlink state
                'hyperlink: {
                    // We currently only emit hyperlinks for HTML. In the
                    // future we can support emitting OSC 8 hyperlinks for
                    // VT output as well.
                    if self.opts.emit != Format::Html {
                        break 'hyperlink;
                    }

                    // Get the hyperlink ID. This ID is our internal ID,
                    // not necessarily the OSC8 ID.
                    let link_id_: Option<u16> = if cell.hyperlink() {
                        page.lookup_hyperlink(cell as *const Cell)
                    } else {
                        None
                    };

                    // If our hyperlink IDs match (even null) then we have
                    // identical hyperlink state and we do nothing.
                    if current_hyperlink_id == link_id_ {
                        break 'hyperlink;
                    }

                    // If our prior hyperlink ID was non-null, we need to
                    // close it because the ID has changed.
                    if current_hyperlink_id.is_some() {
                        self.format_hyperlink_close(writer)?;
                        current_hyperlink_id = None;
                    }

                    // Set our current hyperlink ID
                    let link_id = match link_id_ {
                        Some(v) => v,
                        None => break 'hyperlink,
                    };
                    current_hyperlink_id = Some(link_id);

                    // Emit the opening hyperlink tag
                    let uri: Vec<u8> = {
                        let link = unsafe { &*page.hyperlink_set.get(page.memory_ptr(), link_id) };
                        let base = page.memory_ptr();
                        unsafe { link.uri.slice(base) }.to_vec()
                    };
                    self.format_hyperlink_open(writer, &uri)?;

                    // If we have a point map, we map the hyperlink to
                    // this cell.
                    if self.point_map.is_some() {
                        let mut discarding = Discarding::new();
                        self.format_hyperlink_open(&mut discarding, &uri)?;
                        let count = discarding.count;
                        if let Some(map) = self.point_map.as_mut() {
                            for _ in 0..count {
                                map.map.push(Coordinate { x, y: y as u32 });
                            }
                        }
                    }
                }

                match cell.content_tag() {
                    // We combine codepoint and graphemes because both have
                    // shared style handling. We use comptime to dup it.
                    tag @ (pagepkg::ContentTag::Codepoint
                    | pagepkg::ContentTag::CodepointGrapheme) => {
                        self.write_cell(tag, writer, page, cell)?;

                        // If we have a point map, all codepoints map to this
                        // cell.
                        if self.point_map.is_some() {
                            let mut discarding = Discarding::new();
                            self.write_cell(tag, &mut discarding, page, cell)?;
                            let count = discarding.count;
                            if let Some(map) = self.point_map.as_mut() {
                                for _ in 0..count {
                                    map.map.push(Coordinate { x, y: y as u32 });
                                }
                            }
                        }
                    }

                    // Cells with only background color (no text). Emit a space
                    // with the appropriate background color SGR sequence.
                    pagepkg::ContentTag::BgColorPalette | pagepkg::ContentTag::BgColorRgb => {
                        writer.write_all(b" ")?;
                        if let Some(map) = self.point_map.as_mut() {
                            map.map.push(Coordinate { x, y: y as u32 });
                        }
                    }
                }
            }
        }

        // If the style is non-default, we need to close our style tag.
        if !style.default_style() {
            self.format_style_close(writer)?;
        }

        // Close any open hyperlink for HTML output
        if current_hyperlink_id.is_some() {
            self.format_hyperlink_close(writer)?;
        }

        // Close the monospace wrapper for HTML output
        if self.opts.emit == Format::Html {
            let closing = b"</div>";
            writer.write_all(closing)?;
            if let Some(map) = self.point_map.as_mut() {
                let last = map.map[map.map.len() - 1];
                for _ in 0..closing.len() {
                    map.map.push(last);
                }
            }
        }

        Ok(TrailingState {
            rows: blank_rows,
            cells: blank_cells,
        })
    }

    fn write_cell(
        &self,
        tag: pagepkg::ContentTag,
        writer: &mut dyn io::Write,
        page: &Page,
        cell: &Cell,
    ) -> io::Result<()> {
        // Blank cells get an empty space that isn't replaced by anything
        // because it isn't really a space. We do this so that formatting
        // is preserved if we're emitting styles.
        if !cell.has_text() {
            writer.write_all(b" ")?;
            return Ok(());
        }

        self.write_codepoint_with_replacement(writer, cell.content_codepoint())?;
        if tag == pagepkg::ContentTag::CodepointGrapheme {
            for &cp in page.lookup_grapheme(cell as *const Cell).unwrap() {
                self.write_codepoint_with_replacement(writer, cp as u21)?;
            }
        }
        Ok(())
    }

    fn write_codepoint_with_replacement(
        &self,
        writer: &mut dyn io::Write,
        codepoint: u21,
    ) -> io::Result<()> {
        // Search for our replacement
        let r_: Option<&Replacement> = 'replacement: {
            let map = match self.opts.codepoint_map.as_ref() {
                Some(m) => m,
                None => break 'replacement None,
            };
            let len = map.len();
            for forward_i in 0..len {
                let i = len - forward_i - 1;
                let range = map[i].range;
                if range[0] <= codepoint && codepoint <= range[1] {
                    break 'replacement Some(&map[i].replacement);
                }
            }

            None
        };

        // If no replacement, write it directly.
        let r = match r_ {
            Some(r) => r,
            None => return self.write_codepoint(writer, codepoint),
        };

        match r {
            Replacement::Codepoint(v) => self.write_codepoint(writer, *v)?,

            Replacement::String(s) => {
                // PORT NOTE: the reference validates UTF-8 (`Utf8View.init(s)
                // catch unreachable`) then iterates codepoints. The string is an
                // asserted-valid UTF-8 byte slice; we iterate its `char`s.
                let view = core::str::from_utf8(s).unwrap();
                for cp in view.chars() {
                    self.write_codepoint(writer, cp as u21)?;
                }
            }
        }
        Ok(())
    }

    fn write_codepoint(&self, writer: &mut dyn io::Write, codepoint: u21) -> io::Result<()> {
        match self.opts.emit {
            Format::Plain | Format::Vt => write_codepoint_utf8(writer, codepoint),
            Format::Html => {
                match codepoint {
                    cp if cp == '<' as u21 => writer.write_all(b"&lt;"),
                    cp if cp == '>' as u21 => writer.write_all(b"&gt;"),
                    cp if cp == '&' as u21 => writer.write_all(b"&amp;"),
                    cp if cp == '"' as u21 => writer.write_all(b"&quot;"),
                    cp if cp == '\'' as u21 => writer.write_all(b"&#39;"),
                    _ => {
                        // For HTML, emit ASCII (< 0x80) directly, but encode
                        // all non-ASCII as numeric entities to avoid encoding
                        // detection issues (fixes #9426). We can't set the
                        // meta tag because we emit partial HTML so this ensures
                        // proper unicode handling.
                        if codepoint < 0x80 {
                            write_codepoint_utf8(writer, codepoint)
                        } else {
                            write!(writer, "&#{};", codepoint)
                        }
                    }
                }
            }
        }
    }

    /// Returns the style for the given cell. If there is no styling this
    /// will return the default style.
    fn cell_style(&self, page: &Page, cell: &Cell) -> crate::style::Style {
        match cell.content_tag() {
            pagepkg::ContentTag::Codepoint | pagepkg::ContentTag::CodepointGrapheme => {
                if !cell.has_styling() {
                    crate::style::Style::default()
                } else {
                    unsafe { *page.styles.get(page.memory_ptr(), cell.style_id()) }
                }
            }

            pagepkg::ContentTag::BgColorPalette => crate::style::Style {
                bg_color: crate::style::Color::Palette(cell.content_color_palette()),
                ..crate::style::Style::default()
            },

            pagepkg::ContentTag::BgColorRgb => {
                let rgb = cell.content_color_rgb();
                crate::style::Style {
                    bg_color: crate::style::Color::Rgb(color::RGB {
                        r: rgb.r,
                        g: rgb.g,
                        b: rgb.b,
                    }),
                    ..crate::style::Style::default()
                }
            }
        }
    }

    /// Write a string with HTML escaping. Used for escaping href attributes
    /// and other HTML attribute values.
    fn format_style_open(
        &self,
        writer: &mut dyn io::Write,
        style: &crate::style::Style,
    ) -> io::Result<()> {
        match self.opts.emit {
            Format::Plain => unreachable!(),

            Format::Vt => {
                let mut formatter = style.formatter_vt();
                // PORT NOTE: `self.opts.palette` is `Option<*const Palette>`;
                // deref to the `Option<&Palette>` the style formatter expects.
                formatter.palette = self.opts.palette.map(|p| unsafe { &*p });
                let mut adapter = IoToFmt::new(writer);
                write_fmt_result(&mut adapter, |w| {
                    use core::fmt::Write as _;
                    formatter.format(w)
                })
            }

            // We use `display: inline` so that the div doesn't impact
            // layout since we're primarily using it as a CSS wrapper.
            Format::Html => {
                let mut formatter = style.formatter_html();
                formatter.palette = self.opts.palette.map(|p| unsafe { &*p });
                let mut adapter = IoToFmt::new(writer);
                write_fmt_result(&mut adapter, |w| {
                    use core::fmt::Write as _;
                    w.write_str("<div style=\"display: inline;")?;
                    formatter.format(w)?;
                    w.write_str("\">")
                })
            }
        }
    }

    // PORT NOTE: takes `&mut self` (the reference takes `self` by value but its
    // `point_map` field is a pointer to a heap `ArrayList`, so the in-place
    // append below aliases the real list). In Rust we express that aliasing via
    // `&mut self`.
    fn format_style_close(&mut self, writer: &mut dyn io::Write) -> io::Result<()> {
        let s: &[u8] = match self.opts.emit {
            Format::Plain => return Ok(()),
            Format::Vt => b"\x1b[0m",
            Format::Html => b"</div>",
        };

        writer.write_all(s)?;
        if let Some(m) = self.point_map.as_mut() {
            debug_assert!(!m.map.is_empty());
            let last = m.map[m.map.len() - 1];
            for _ in 0..s.len() {
                m.map.push(last);
            }
        }
        Ok(())
    }

    fn format_hyperlink_open(&self, writer: &mut dyn io::Write, uri: &[u8]) -> io::Result<()> {
        match self.opts.emit {
            Format::Plain | Format::Vt => unreachable!(),

            // layout since we're primarily using it as a CSS wrapper.
            Format::Html => {
                writer.write_all(b"<a href=\"")?;
                for &byte in uri {
                    self.write_codepoint(writer, byte as u21)?;
                }
                writer.write_all(b"\">")
            }
        }
    }

    // PORT NOTE: takes `&mut self` for the same reason as `format_style_close`.
    fn format_hyperlink_close(&mut self, writer: &mut dyn io::Write) -> io::Result<()> {
        let s: &[u8] = match self.opts.emit {
            Format::Html => b"</a>",
            Format::Plain | Format::Vt => return Ok(()),
        };

        writer.write_all(s)?;
        if let Some(m) = self.point_map.as_mut() {
            debug_assert!(!m.map.is_empty());
            let last = m.map[m.map.len() - 1];
            for _ in 0..s.len() {
                m.map.push(last);
            }
        }
        Ok(())
    }
}
