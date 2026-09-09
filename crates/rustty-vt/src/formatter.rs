//! rustty-vt formatter
//!
//! PORTS: ghostty src/terminal/formatter.zig
//!
//! PORT STATUS: FILLED (vt-12c-fmt) — `PageFormatter` (incl. the
//! `formatWithState` core loop + `TrailingState`), `PageListFormatter`,
//! `ScreenFormatter`, and `TerminalFormatter` + all cell/writer/style helpers are
//! ported. `TerminalFormatter::{init,format}` compose the production formatters
//! and emit palette/modes/scrolling_region/tabstops/pwd/kitty_keyboard Extra. All
//! ~100 formatter `test{}` blocks are ported (they build fixtures via
//! `Terminal::vt_stream`). See DEFERRALS.md.
// PORT NOTE: `should_implement_trait` is for the inherent `format` methods
// (faithful to the reference's `format` name, not `std::fmt`).
#![allow(dead_code)]
#![allow(clippy::should_implement_trait)]

// const std = @import("std");
// const assert = @import("../quirks.zig").inlineAssert;
// const fastprint = @import("../fastprint.zig");
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
use crate::fastprint;
use crate::page as pagepkg;
use crate::page::u21;
use crate::page::{Cell, Page, Row};
use crate::pagelist::{PageList, Pin};
use crate::point;
use crate::point::Coordinate;
use crate::screen::Screen;
use crate::selection::Selection;
use crate::size;
use crate::terminal::Terminal;

// PORT NOTE: the reference allocator maps to `allocator_api2::alloc::Global`
// (the crate's allocator seam).
use allocator_api2::alloc::Global as Allocator;

// PORT NOTE: the reference writer is `*std.Io.Writer` (a byte sink). Per the
// Screen-SCC plan seam, this ports to `&mut dyn std::io::Write`.
use std::io;
use std::io::Write as _;

// PORT NOTE (writer seam bridge): the reference passes a `*std.Io.Writer` and
// uses Zig formatting (`writer.print("{f}", .{style.formatterVt()})`) to render
// styles. In Rustty, `style.rs`'s `VtFormatter`/`HtmlFormatter` already-ported
// `format` methods target `core::fmt::Write` (the idiomatic Rust `{}` sink),
// while this formatter's public seam is `&mut dyn std::io::Write`. To delegate
// style emission to `style.rs` without re-porting it, we wrap the `io::Write`
// sink in a tiny adapter that implements `core::fmt::Write` by forwarding the
// UTF-8 bytes. Style/escape output is always valid UTF-8, so this is lossless.
// PERF(port): a minimal buffered adapter installed by `format_with_state_emit`
// around its sink for the duration of one format — the Rust stand-in for the
// reference's `*std.Io.Writer`, whose `writeAll` appends to an inline buffer
// and only takes the vtable when draining. See the note at the install site.
struct BufOut<'w> {
    inner: &'w mut dyn io::Write,
    buf: [u8; 4096],
    len: usize,
}

impl BufOut<'_> {
    fn flush_buf(&mut self) -> io::Result<()> {
        if self.len > 0 {
            let n = self.len;
            self.len = 0;
            self.inner.write_all(&self.buf[..n])?;
        }
        Ok(())
    }
}

impl io::Write for BufOut<'_> {
    #[inline]
    fn write(&mut self, s: &[u8]) -> io::Result<usize> {
        self.write_all(s)?;
        Ok(s.len())
    }

    #[inline]
    fn write_all(&mut self, s: &[u8]) -> io::Result<()> {
        if self.len + s.len() > self.buf.len() {
            self.flush_buf()?;
            // Oversized writes go straight through (buffer is empty now).
            if s.len() > self.buf.len() {
                return self.inner.write_all(s);
            }
        }
        self.buf[self.len..self.len + s.len()].copy_from_slice(s);
        self.len += s.len();
        Ok(())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.flush_buf()?;
        self.inner.flush()
    }
}

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

// PERF(port): the reference passes `emit` as a COMPTIME parameter down the
// emission chain (`comptime emit: Format` on `writeCellRun` and
// `encodeCodepoint`, dispatched by `inline else` at the top), monomorphizing
// the hot loop per format so every `emit` branch and the unused formats' code
// disappear from each instantiation. The const-generic `EMIT` discriminant is
// that comptime parameter's Rust spelling: the hot helpers derive a
// compile-time-known `Format` from it and their bodies are otherwise
// unchanged. A single runtime-`emit` instantiation kept all three format arms
// (and the HTML arm's hyperlink lookup) live inside the per-cell loop.
#[inline(always)]
const fn emit_from_const<const EMIT: u8>() -> Format {
    match EMIT {
        0 => Format::Plain,
        1 => Format::Vt,
        _ => Format::Html,
    }
}

// PORT NOTE (Rust-only glue): `Clone` realizes the Zig value-copies of
// `Options` (and hence its map entries) at the reference's copy sites
// (`var copy = opts;` — e.g. core `copySelectionToClipboards`); Zig copies
// the MultiArrayList header shallowly, Rust clones the owned entries (same
// observable content).
#[derive(Clone)]
pub struct CodepointMap {
    /// Unicode codepoint range to replace.
    /// Asserts: range[0] <= range[1]
    pub range: [u21; 2],

    /// Replacement value for this range.
    pub replacement: Replacement,
}

#[derive(Clone)]
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
///
/// The mapping is stored in two parts: a per-byte x/y coordinate (8
/// bytes per output byte, half the size of a Pin) and a tiny table of
/// page nodes covering byte ranges (there are only ever a handful of
/// pages). This also lets page formatters write coordinates directly
/// into the map without a separate coordinate-to-pin conversion pass.
pub struct PinMap<'a> {
    pub alloc: Allocator,
    pub map: &'a mut PinMapMap,
}

/// The type of the page node referenced by pins.
// PORT NOTE: nested `PinMap.Node` (`@FieldType(Pin, "node")`), hoisted to a
// top-level name per the `TerminalExtra`/`ScreenExtra` precedent.
pub type PinMapNode = *mut crate::pagelist::Node;

/// A page node covering output bytes starting at `offset`
/// (inclusive) until the next entry's offset (or the end of the
/// output).
// PORT NOTE: nested `PinMap.NodeRun`, hoisted to a top-level name.
#[derive(Clone, Copy)]
pub struct NodeRun {
    pub offset: usize,
    pub node: PinMapNode,
}

// PORT NOTE: nested `PinMap.Map`, hoisted to the parent-prefixed top-level
// name `PinMapMap` (the `TerminalExtra`/`ScreenExtra` hoisting precedent).
#[derive(Clone)]
pub struct PinMapMap {
    /// The x/y coordinate within its page for every output byte.
    // PORT NOTE: `std.ArrayList(Coordinate) = .empty` -> `Vec<Coordinate>`.
    pub points: Vec<Coordinate>,

    /// The page node for ranges of output bytes, ordered by offset.
    pub nodes: Vec<NodeRun>,
}

impl PinMapMap {
    // pub const empty: Map = .{};
    pub const EMPTY: PinMapMap = PinMapMap {
        points: Vec::new(),
        nodes: Vec::new(),
    };

    pub fn deinit(&mut self, alloc: Allocator) {
        // PORT NOTE: the reference frees both `alloc`-owned lists. The owning
        // `Vec`s free on drop; replacing them here is the faithful
        // deallocation, and the `alloc` parameter is kept for signature parity.
        let _ = alloc;
        self.points = Vec::new();
        self.nodes = Vec::new();
    }

    pub fn clear_retaining_capacity(&mut self) {
        self.points.clear();
        self.nodes.clear();
    }

    /// The total number of bytes mapped.
    pub fn count(&self) -> usize {
        self.points.len()
    }

    /// Set the page node for all bytes appended from here on,
    /// until the next call. No-op if the node is unchanged.
    // PORT NOTE: returns `Allocator.Error!void` in the reference; `Vec` growth
    // aborts on OOM (the file-wide convention that dropped the
    // `catch return error.WriteFailed` arms), so this is infallible here.
    pub fn set_node(&mut self, alloc: Allocator, node: PinMapNode) {
        let _ = alloc;
        if let Some(last) = self.nodes.last() {
            if core::ptr::eq(last.node, node) {
                return;
            }
        }

        self.nodes.push(NodeRun {
            offset: self.points.len(),
            node,
        });
    }

    /// Append `n` bytes that map to `pin`.
    pub fn append(&mut self, alloc: Allocator, pin: Pin, n: usize) {
        if n == 0 {
            return;
        }
        self.set_node(alloc, pin.node);
        // try self.points.appendNTimes(alloc, .{ .x = pin.x, .y = pin.y }, n);
        self.points.extend(core::iter::repeat_n(
            Coordinate {
                x: pin.x,
                y: pin.y as u32,
            },
            n,
        ));
    }

    /// Returns the pin that the byte at the given offset maps to,
    /// or null if the offset is out of range.
    pub fn get(&self, offset: usize) -> Option<Pin> {
        if offset >= self.points.len() {
            return None;
        }
        let coord = self.points[offset];
        // PORT NOTE: `.y = @intCast(coord.y)` — the safety-checked narrowing
        // maps to the debug_assert + `as` pairing (R7).
        debug_assert!(coord.y <= size::CellCountInt::MAX as u32);
        Some(Pin {
            node: PinMap::find_node(&self.nodes, offset)?,
            x: coord.x,
            y: coord.y as size::CellCountInt,
            // PORT NOTE: the reference Pin field default (`garbage = false`).
            garbage: false,
        })
    }

    /// Returns the last pin in the map, if any.
    pub fn get_last_or_null(&self) -> Option<Pin> {
        let len = self.points.len();
        if len == 0 {
            return None;
        }
        self.get(len - 1)
    }
}

impl<'a> PinMap<'a> {
    /// Binary search for the node covering `offset` in a slice of node
    /// runs sorted by offset. Returns null only if the slice is empty
    /// or the offset precedes the first run.
    pub fn find_node(runs: &[NodeRun], offset: usize) -> Option<PinMapNode> {
        if runs.is_empty() || offset < runs[0].offset {
            return None;
        }
        let mut lo: usize = 0;
        let mut hi: usize = runs.len();
        while lo + 1 < hi {
            let mid = lo + (hi - lo) / 2;
            if runs[mid].offset <= offset {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        Some(runs[lo].node)
    }
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
pub struct TerminalFormatter<'a> {
    /// The terminal to format.
    // PORT NOTE: `terminal: *const Terminal`; deref on access (the caller
    // guarantees the terminal outlives the formatter).
    pub terminal: *const Terminal,

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
    pub pin_map: Option<PinMap<'a>>,
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

impl<'a> TerminalFormatter<'a> {
    pub fn init(terminal: *const Terminal, opts: Options) -> TerminalFormatter<'a> {
        TerminalFormatter {
            terminal,
            opts,
            content: Content::Selection(None),
            extra: TerminalExtra::STYLES,
            pin_map: None,
        }
    }

    pub fn format(mut self, writer: &mut dyn io::Write) -> io::Result<()> {
        // PORT NOTE: `self.terminal` is a `*const Terminal`. The reference
        // accesses fields directly; we deref the raw pointer (the caller
        // guarantees the terminal outlives the formatter).
        let terminal: &Terminal = unsafe { &*self.terminal };

        // Emit palette before screen content if using VT format. Technically
        // we could do this after but this way if replay is slow for whatever
        // reason the colors will be right right away.
        'palette: {
            if self.extra.palette {
                match self.opts.emit {
                    Format::Plain => break 'palette,

                    Format::Vt => {
                        for (i, rgb) in terminal.colors.palette.current.iter().enumerate() {
                            write!(
                                writer,
                                "\x1b]4;{};rgb:{:02x}/{:02x}/{:02x}\x1b\\",
                                i, rgb.r, rgb.g, rgb.b,
                            )?;
                        }
                    }

                    // For HTML, we emit CSS to setup our palette variables.
                    Format::Html => {
                        writer.write_all(b"<style>:root{")?;
                        for (i, rgb) in terminal.colors.palette.current.iter().enumerate() {
                            write!(
                                writer,
                                "--vt-palette-{}: #{:02x}{:02x}{:02x};",
                                i, rgb.r, rgb.g, rgb.b,
                            )?;
                        }
                        writer.write_all(b"}</style>")?;
                    }
                }

                // If we have a pin_map, add the bytes we wrote to map.
                if let Some(m) = self.pin_map.as_mut() {
                    let mut discarding = Discarding::new();
                    let mut extra_formatter: TerminalFormatter = TerminalFormatter {
                        terminal: self.terminal,
                        opts: copy_options(&self.opts),
                        content: Content::None,
                        extra: TerminalExtra::NONE,
                        pin_map: None,
                    };
                    extra_formatter.extra.palette = true;
                    extra_formatter.format(&mut discarding)?;

                    // Map all those bytes to the same pin. Use the top left to ensure
                    // the node pointer is always properly initialized.
                    m.map.append(
                        m.alloc,
                        unsafe { &*terminal.screens.active }
                            .pages
                            .get_top_left(point::Tag::Screen),
                        discarding.count,
                    );
                }
            }
        }

        // Emit terminal modes that differ from defaults. We probably have
        // some modes we want to emit before and some after, but for now for
        // simplicity we just emit them all before. If we make this more complex
        // later we should add test cases for it.
        if self.opts.emit == Format::Vt && self.extra.modes {
            // PORT NOTE: the reference `inline for (@typeInfo(modespkg.Mode)
            // .@"enum".fields)` iterates every mode in declaration order. We
            // iterate the bit-indexed `Mode` table; for each mode the current
            // value is `modes.get(mode)`, the default is the per-mode default
            // bit (`@field(modes.default, field.name)`), and the tag is the
            // bitcast `ModeTag` (giving `ansi`/`value`).
            for mode in crate::modes::Mode::all() {
                let current = terminal.modes.get(mode);
                let default_val = terminal.modes.default.get_mode(mode);

                if current != default_val {
                    let tag = crate::modes::ModeTag::from_mode(mode);
                    let prefix: &str = if tag.ansi { "" } else { "?" };
                    let suffix: &str = if current { "h" } else { "l" };
                    write!(writer, "\x1b[{}{}{}", prefix, tag.value, suffix)?;
                }
            }

            // If we have a pin_map, add the bytes we wrote to map.
            if let Some(m) = self.pin_map.as_mut() {
                let mut discarding = Discarding::new();
                let mut extra_formatter: TerminalFormatter = TerminalFormatter {
                    terminal: self.terminal,
                    opts: copy_options(&self.opts),
                    content: Content::None,
                    extra: TerminalExtra::NONE,
                    pin_map: None,
                };
                extra_formatter.extra.modes = true;
                extra_formatter.format(&mut discarding)?;

                // Map all those bytes to the same pin. Use the top left to ensure
                // the node pointer is always properly initialized.
                m.map.append(
                    m.alloc,
                    unsafe { &*terminal.screens.active }
                        .pages
                        .get_top_left(point::Tag::Screen),
                    discarding.count,
                );
            }
        }

        // Emit tabstop positions before the screen contents because setting
        // them moves the cursor. Screen formatting will restore the requested
        // cursor position afterwards.
        if self.opts.emit == Format::Vt && self.extra.tabstops {
            // Clear all tabs (CSI 3 g)
            write!(writer, "\x1b[3g")?;

            // Set each configured tabstop by moving cursor and using HTS
            for col in 0..terminal.cols {
                if terminal.tabstops.get(col as usize) {
                    // Move cursor to the column (1-indexed)
                    write!(writer, "\x1b[{}G", col + 1)?;
                    // Set tab (HTS)
                    write!(writer, "\x1bH")?;
                }
            }

            // Screen contents are formatted relative to the top-left.
            writer.write_all(b"\x1b[H")?;

            // If we have a pin_map, add the bytes we wrote to map.
            if let Some(m) = self.pin_map.as_mut() {
                let mut discarding = Discarding::new();
                let mut extra_formatter: TerminalFormatter = TerminalFormatter {
                    terminal: self.terminal,
                    opts: copy_options(&self.opts),
                    content: Content::None,
                    extra: TerminalExtra::NONE,
                    pin_map: None,
                };
                extra_formatter.extra.tabstops = true;
                extra_formatter.format(&mut discarding)?;

                // Map all those bytes to the same pin. Use the top left to ensure
                // the node pointer is always properly initialized.
                m.map.append(
                    m.alloc,
                    unsafe { &*terminal.screens.active }
                        .pages
                        .get_top_left(point::Tag::Screen),
                    discarding.count,
                );
            }
        }

        let mut screen_formatter: ScreenFormatter =
            ScreenFormatter::init(terminal.screens.active, clone_options(&self.opts));
        screen_formatter.content = match &self.content {
            Content::None => Content::None,
            Content::Selection(sel) => Content::Selection(*sel),
        };
        if let Some(m) = self.pin_map.as_mut() {
            screen_formatter.pin_map = Some(PinMap {
                alloc: m.alloc,
                map: &mut *m.map,
            });
        }
        screen_formatter.format(writer)?;

        // Extra terminal state to emit after the screen contents so that
        // it doesn't impact the emitted contents.
        if self.opts.emit == Format::Vt {
            // Emit scrolling region using DECSTBM and DECSLRM
            if self.extra.scrolling_region {
                let region = &terminal.scrolling_region;

                // DECSTBM: top and bottom margins (1-indexed)
                // Only emit if not the full screen
                if region.top != 0 || region.bottom != terminal.rows - 1 {
                    write!(writer, "\x1b[{};{}r", region.top + 1, region.bottom + 1)?;
                }

                // DECSLRM: left and right margins (1-indexed)
                // Only emit if not the full width
                if region.left != 0 || region.right != terminal.cols - 1 {
                    write!(writer, "\x1b[{};{}s", region.left + 1, region.right + 1)?;
                }
            }

            // Emit keyboard modes such as ModifyOtherKeys
            if self.extra.keyboard {
                // Only emit if modify_other_keys_2 is true
                if terminal.flags.modify_other_keys_2 {
                    write!(writer, "\x1b[>4;2m")?;
                }
            }

            // Emit present working directory using OSC 7
            if self.extra.pwd {
                let pwd = terminal.pwd.as_slice();
                if !pwd.is_empty() {
                    writer.write_all(b"\x1b]7;")?;
                    writer.write_all(pwd)?;
                    writer.write_all(b"\x1b\\")?;
                }
            }

            // If we have a pin_map, add the bytes we wrote to map.
            if let Some(m) = self.pin_map.as_mut() {
                let mut discarding = Discarding::new();
                let mut extra_formatter: TerminalFormatter = TerminalFormatter {
                    terminal: self.terminal,
                    opts: copy_options(&self.opts),
                    content: Content::None,
                    extra: TerminalExtra::NONE,
                    pin_map: None,
                };
                extra_formatter.extra.scrolling_region = self.extra.scrolling_region;
                extra_formatter.extra.keyboard = self.extra.keyboard;
                extra_formatter.extra.pwd = self.extra.pwd;
                extra_formatter.format(&mut discarding)?;

                let pin = m.map.get_last_or_null().unwrap_or_else(|| {
                    unsafe { &*terminal.screens.active }
                        .pages
                        .get_top_left(point::Tag::Screen)
                });
                m.map.append(m.alloc, pin, discarding.count);
            }
        }

        // Emit extra screen state last because terminal state such
        // as scrolling regions can move the cursor, so we have to set
        // cursor last.
        // PORT NOTE: the reference reuses the same by-value `screen_formatter`
        // (its `format` copies `self`); the Rust `format` consumes `self`, so
        // the trailing pass reconstructs an identical formatter, carrying the
        // same pin_map reborrow.
        let mut screen_formatter: ScreenFormatter =
            ScreenFormatter::init(terminal.screens.active, copy_options(&self.opts));
        screen_formatter.content = Content::None;
        screen_formatter.extra = self.extra.screen;
        if let Some(m) = self.pin_map.as_mut() {
            screen_formatter.pin_map = Some(PinMap {
                alloc: m.alloc,
                map: &mut *m.map,
            });
        }
        screen_formatter.format(writer)?;

        Ok(())
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
                    PageListFormatter::init(&screen.pages, clone_options(&self.opts));
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

        // Emit cursor position before the other extras because restoring a
        // pending wrap requires reprinting the cell at the right edge. That
        // print uses and changes active screen state, so the requested style,
        // hyperlink, protection, and charset must be restored afterwards.
        if self.extra.cursor {
            'cursor: {
                let cursor = &screen.cursor;

                // If we don't have pending wrap, then we can just use CUP.
                if !cursor.pending_wrap || cursor.x != screen.pages.cols - 1 {
                    write!(writer, "\x1b[{};{}H", cursor.y + 1, cursor.x + 1)?;
                    break 'cursor;
                }

                // Pending wrap, we can't use CUP because it resets pending wrap.
                let start_x: size::CellCountInt = match unsafe { (*cursor.page_cell).wide() } {
                    pagepkg::Wide::SpacerTail => cursor.x - 1,
                    pagepkg::Wide::Narrow | pagepkg::Wide::Wide | pagepkg::Wide::SpacerHead => {
                        cursor.x
                    }
                };

                // Move cursor to the edge.
                write!(writer, "\x1b[{};{}H", cursor.y + 1, start_x + 1)?;

                // Reformat the cell which sets the proper pending wrap state.
                // PORT NOTE (SB): the 1-cell PageFormatter's page alias is
                // derived from the pin's OWN node pointer
                // (`cursor.page_pin.node.page()`), never a fresh `&mut` —
                // both the screen alias and this page alias are read-only.
                let page_pin = unsafe { *cursor.page_pin };
                let mut cell_formatter: PageFormatter = PageFormatter::init(
                    unsafe { crate::pagelist::Node::page(page_pin.node) } as *const Page,
                    clone_options(&self.opts),
                );
                cell_formatter.start_x = cursor.x;
                cell_formatter.end_x = Some(cursor.x);
                cell_formatter.start_y = page_pin.y;
                cell_formatter.end_y = Some(page_pin.y);
                cell_formatter.format(writer)?;
            }
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
            let pin = m
                .map
                .get_last_or_null()
                .unwrap_or_else(|| screen.pages.get_top_left(point::Tag::Screen));
            m.map.append(m.alloc, pin, discarding.count);
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
// The reference copies the `Options` struct freely (it is a Zig value type),
// including `codepoint_map`, at EVERY delegation site
// (src/terminal/formatter.zig:486, 675, 726, 897). Rust `Options` owning the
// map means a blanket "shallow copy" helper is unsafe to use everywhere: it
// must be reserved for the passes below that provably never reach
// PageFormatter's content-emitting path, and `clone_options` (below) used
// everywhere else.
//
// `copy_options` drops the map and MUST ONLY be used for a pass whose
// `content` is `Content::None` — i.e. a pass that emits ONLY "extras" state
// (palette/modes/tabstops/scrolling-region/keyboard/pwd/cursor-position/
// style/hyperlink/protection/kitty-keyboard/charsets) and never calls into
// PageListFormatter/PageFormatter, so no codepoint lookup ever runs against
// it. Concretely: the `pin_map` byte-counting "discarding writer" replay
// passes in `TerminalFormatter::format` and `ScreenFormatter::format`, and
// the trailing "extra screen state" `ScreenFormatter` built in
// `TerminalFormatter::format` (content = `Content::None`).
//
// Every other delegation hop — anything that reaches PageListFormatter or
// PageFormatter, including the single-cell cursor reformat in
// `ScreenFormatter::format` — MUST use `clone_options` instead, or the
// `codepoint_map` a caller set (e.g. the `clipboard-codepoint-map` config
// option via `Surface::copySelectionToClipboards`) silently stops applying.
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

/// Options for a CONTENT delegation hop: the codepoint map must reach
/// `PageFormatter`, which is the only place it is applied (oracle
/// src/terminal/formatter.zig:486,675,726,897 pass `self.opts` by value).
/// `copy_options` above deliberately drops it and is for the extras-only
/// passes documented on its own doc comment — never for a hop that reaches
/// PageListFormatter/PageFormatter.
fn clone_options(opts: &Options) -> Options {
    Options {
        codepoint_map: opts.codepoint_map.clone(),
        ..copy_options(opts)
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

        let mut page_state: Option<TrailingState> = None;
        let mut iter = tl.page_iterator(crate::pagelist::iter::Direction::RightDown, Some(br));
        while let Some(chunk) = iter.next() {
            debug_assert!(chunk.start < chunk.end);
            debug_assert!(chunk.end > 0);

            // PORT NOTE: `&chunk.node.data` — the page lives in the node; deref
            // the raw `*mut Node` to take a `*const Page`.
            let page: *const Page = unsafe { &(*crate::pagelist::Node::page(chunk.node)) };
            let mut formatter: PageFormatter = PageFormatter::init(page, clone_options(&self.opts));
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

            // If we're tracking pins, the page formatter writes its
            // per-byte coordinates directly into our map's point list
            // and we record which page node covers those bytes.
            if let Some(m) = self.pin_map.as_mut() {
                m.map.set_node(m.alloc, chunk.node);
                let base = m.map.points.len();
                formatter.point_map = Some(PointMap {
                    alloc: m.alloc,
                    map: &mut m.map.points,
                    base,
                });
            }

            page_state = Some(formatter.format_with_state(writer)?);
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

/// See point_map.
// PORT NOTE: nested `PageFormatter.PointMap`, hoisted to a top-level name.
pub struct PointMap<'a> {
    pub alloc: Allocator,
    pub map: &'a mut Vec<Coordinate>,

    /// The index in `map` at which this formatter's output begins.
    /// Entries before this index belong to a caller (e.g. previous
    /// pages of a PageListFormatter) and aren't inspected. This
    /// exists so that callers can share one list across pages.
    // PORT NOTE: the reference field default (`base: usize = 0`) is supplied
    // explicitly at each construction site (Rust struct literals have no
    // field defaults).
    pub base: usize,
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

    pub fn format_with_state(self, writer: &mut dyn io::Write) -> io::Result<TrailingState> {
        // Specialize the hot path on the emitted format so that the
        // per-cell loop contains no per-cell format dispatch.
        // PORT NOTE: the reference `switch (self.opts.emit) { inline else =>
        // |emit| ... }` comptime-specializes per format. Per this file's
        // comptime-parameter convention (`writeCell`'s `comptime tag` ported
        // as a runtime param), `emit` is a hoisted runtime parameter — the
        // dispatch still happens exactly once, here.
        let emit = self.opts.emit;
        self.format_with_state_emit(writer, emit)
    }

    fn format_with_state_emit(
        self,
        writer: &mut dyn io::Write,
        emit: Format,
    ) -> io::Result<TrailingState> {
        // PERF(port): the reference's `*std.Io.Writer` buffers every
        // `writeAll` inline and only takes the vtable when draining; the
        // port's `&mut dyn io::Write` seam paid one virtual call PLUS the
        // sink's own machinery (`Vec::reserve` + memcpy) per emission —
        // ~230k calls per format on styled content (per-run flushes, SGR
        // opens/closes, newlines). Wrapping the sink in a small buffered
        // adapter for the duration of one format restores the reference's
        // batching: the byte stream is unchanged (same bytes, same order),
        // only the call count changes (~bytes/4096). Flushed on BOTH exits —
        // an unbuffered writer would also have delivered all bytes produced
        // before an error surfaced.
        let mut out = BufOut {
            inner: writer,
            buf: [0; 4096],
            len: 0,
        };
        let result = self.format_with_state_emit_inner(&mut out, emit);
        let flushed = out.flush_buf();
        let ts = result?;
        flushed?;
        Ok(ts)
    }

    fn format_with_state_emit_inner(
        mut self,
        writer: &mut dyn io::Write,
        emit: Format,
    ) -> io::Result<TrailingState> {
        // PORT NOTE: `self.page` is a `*const Page`; deref to access (the caller
        // guarantees the page outlives the formatter).
        let page: &Page = unsafe { &*self.page };

        let mut blank_rows: usize = 0;
        let mut blank_cells: usize = 0;

        // PERF(port): the cell-run emit buffer is declared once here and
        // threaded into `write_cell_run` — the reference declares it
        // `undefined` (never zeroed) inside `writeCellRun`; a zeroed
        // in-function declaration cost two 512-byte memsets per style
        // boundary on styled content.
        let mut run_buf: [u8; 512] = [0; 512];
        // PERF(port): same treatment for the SGR scratch `format_style_open`'s
        // Vt arm assembles into (the reference's is `undefined` in-frame): a
        // zeroed in-function declaration was a 128-byte memset per style
        // change.
        let mut style_buf: [u8; 128] = [0; 128];

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
        match emit {
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

        // Our style for non-plain formats. Alongside the style itself we
        // track the page-local interned style id it corresponds to (styles
        // are interned per-page so id equality implies style equality).
        // The id is only a fast-path hint: it is set to `invalid_style_id`
        // whenever the current style didn't come from an interned id
        // (e.g. bg-color-only cells which synthesize styles).
        let invalid_style_id: u32 = u32::MAX;
        let mut style: crate::style::Style = crate::style::Style::default();
        let mut style_id: u32 = 0;

        // Whether the codepoint map has any entries. Hoisted out of the
        // per-codepoint path so that the common no-map case can use the
        // fast cell run path below.
        let cp_map_empty: bool = match self.opts.codepoint_map.as_ref() {
            Some(m) => m.is_empty(),
            None => true,
        };

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
                    self.format_style_close(emit, writer)?;
                    style = crate::style::Style::default();
                    style_id = 0;
                }

                let sequence: &[u8] = match emit {
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
                    let start: Coordinate = if map.map.len() > map.base {
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
            // PORT NOTE: the reference `while (...) : (cell_i += 1)` continue
            // expression runs on every `continue`; this port increments at
            // each continue site and at the loop tail.
            let mut cell_i: usize = 0;
            'cells: while cell_i < cells_subset.len() {
                let cell: &Cell = &cells_subset[cell_i];
                let x: size::CellCountInt = (row_start_x as usize + cell_i) as size::CellCountInt;

                // Fast path: runs of simple cells (single codepoint, no
                // style/hyperlink transitions) are encoded in batches,
                // avoiding all of the per-cell bookkeeping below. This is
                // only valid when we have no codepoint map and when our
                // current style/hyperlink state is known-stable.
                if cp_map_empty {
                    'fast: {
                        if format_styled(emit) && style_id == invalid_style_id {
                            break 'fast;
                        }

                        let track_points = self.point_map.is_some();
                        // `inline else` — one dispatch per run start; see
                        // `emit_from_const`.
                        let consumed = match emit {
                            Format::Plain => self.write_cell_run::<{ Format::Plain as u8 }>(
                                track_points,
                                writer,
                                &cells_subset[cell_i..],
                                x,
                                y,
                                style_id,
                                current_hyperlink_id,
                                &mut blank_cells,
                                &mut run_buf,
                            )?,
                            Format::Vt => self.write_cell_run::<{ Format::Vt as u8 }>(
                                track_points,
                                writer,
                                &cells_subset[cell_i..],
                                x,
                                y,
                                style_id,
                                current_hyperlink_id,
                                &mut blank_cells,
                                &mut run_buf,
                            )?,
                            Format::Html => self.write_cell_run::<{ Format::Html as u8 }>(
                                track_points,
                                writer,
                                &cells_subset[cell_i..],
                                x,
                                y,
                                style_id,
                                current_hyperlink_id,
                                &mut blank_cells,
                                &mut run_buf,
                            )?,
                        };

                        // Zero cells consumed means the first cell isn't
                        // eligible for the fast path; handle it below.
                        if consumed == 0 {
                            break 'fast;
                        }

                        // The continue expression adds the final one.
                        cell_i += consumed - 1;
                        cell_i += 1;
                        continue 'cells;
                    }
                }

                // Skip spacers. These happen naturally when wide characters
                // are printed again on the screen (for well-behaved terminals!)
                match cell.wide() {
                    pagepkg::Wide::Narrow | pagepkg::Wide::Wide => {}
                    pagepkg::Wide::SpacerHead | pagepkg::Wide::SpacerTail => {
                        cell_i += 1;
                        continue 'cells;
                    }
                }

                // If we have a zero value, then we accumulate a counter. We
                // only want to turn zero values into spaces if we have a non-zero
                // char sometime later.
                'blank: {
                    // If we're emitting styled output (not plaintext) and
                    // the cell has some kind of styling or is not empty
                    // then this isn't blank.
                    if format_styled(emit) && (!cell.is_empty() || cell.has_styling()) {
                        break 'blank;
                    }

                    // Cells with no text are blank
                    if !cell.has_text() {
                        blank_cells += 1;
                        cell_i += 1;
                        continue 'cells;
                    }

                    // Trailing spaces are blank. We know it is trailing
                    // because if we get a non-empty cell later we'll
                    // fill the blanks.
                    if cell.codepoint() == ' ' as u21 && self.opts.trim {
                        blank_cells += 1;
                        cell_i += 1;
                        continue 'cells;
                    }
                }

                // This cell is not blank. If we have accumulated blank cells
                // then we want to emit them now.
                if blank_cells > 0 {
                    for _ in 0..blank_cells {
                        writer.write_all(b" ")?;
                    }

                    if self.point_map.is_some() {
                        self.append_blank_points(blank_cells, x, y);
                    }

                    blank_cells = 0;
                }

                'style: {
                    // If we aren't emitting styled output then we don't
                    // have to worry about styles.
                    if !format_styled(emit) {
                        break 'style;
                    }

                    // Fast path: styles are interned per-page, so if this
                    // cell's style id matches the id of our current style
                    // then the style is unchanged.
                    let cell_style_id: u32 =
                        match cell.content_tag() {
                            pagepkg::ContentTag::Codepoint
                            | pagepkg::ContentTag::CodepointGrapheme => cell.style_id() as u32,
                            pagepkg::ContentTag::BgColorPalette
                            | pagepkg::ContentTag::BgColorRgb => invalid_style_id,
                        };
                    if cell_style_id == style_id && cell_style_id != invalid_style_id {
                        break 'style;
                    }

                    // Get our cell style.
                    let cell_style = self.cell_style(page, cell);

                    // If the style hasn't changed, don't bloat output.
                    // When both ids are interned (and thus different, since
                    // equal ids broke out above), interning guarantees the
                    // styles differ so we can skip the comparison entirely.
                    // PORT NOTE: the reference nests these two ifs; the allow
                    // keeps that 1:1 structure past `collapsible_if`.
                    #[allow(clippy::collapsible_if)]
                    if cell_style_id == invalid_style_id || style_id == invalid_style_id {
                        if cell_style.eql(&style) {
                            style_id = cell_style_id;
                            break 'style;
                        }
                    }

                    // If we had a previous style, we need to close it,
                    // because we've confirmed we have some new style
                    // (which is maybe default).
                    if !style.default_style() {
                        match emit {
                            Format::Html => self.format_style_close(emit, writer)?,

                            // For VT, we only close if we're switching to a default
                            // style because any non-default style will emit
                            // a \x1b[0m as the start of a VT coloring sequence.
                            Format::Vt => {
                                if cell_style.default_style() {
                                    self.format_style_close(emit, writer)?;
                                }
                            }

                            // Unreachable because of the styled() check at the
                            // top of this block.
                            Format::Plain => unreachable!(),
                        }
                    }

                    // At this point, we can copy our style over
                    style = cell_style;
                    style_id = cell_style_id;

                    // If we're just the default style now, we're done.
                    if cell_style.default_style() {
                        break 'style;
                    }

                    // New style, emit it.
                    self.format_style_open(emit, writer, &style, &mut style_buf)?;

                    // If we have a point map, we map the style to
                    // this cell.
                    if self.point_map.is_some() {
                        let mut discarding = Discarding::new();
                        self.format_style_open(emit, &mut discarding, &style, &mut style_buf)?;
                        let count = discarding.count;
                        // PORT NOTE: `count` is computed above (needs `&self`),
                        // then the map borrow is taken separately to satisfy the
                        // borrow checker (cannot hold `&mut self.point_map` across
                        // the `&self` `format_style_open` call).
                        if let Some(map) = self.point_map.as_mut() {
                            map.map
                                .extend(core::iter::repeat_n(Coordinate { x, y: y as u32 }, count));
                        }
                    }
                }

                // Hyperlink state
                'hyperlink: {
                    // We currently only emit hyperlinks for HTML. In the
                    // future we can support emitting OSC 8 hyperlinks for
                    // VT output as well.
                    if emit != Format::Html {
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
                        self.format_hyperlink_close(emit, writer)?;
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
                    self.format_hyperlink_open(emit, writer, &uri)?;

                    // If we have a point map, we map the hyperlink to
                    // this cell.
                    if self.point_map.is_some() {
                        let mut discarding = Discarding::new();
                        self.format_hyperlink_open(emit, &mut discarding, &uri)?;
                        let count = discarding.count;
                        if let Some(map) = self.point_map.as_mut() {
                            map.map
                                .extend(core::iter::repeat_n(Coordinate { x, y: y as u32 }, count));
                        }
                    }
                }

                match cell.content_tag() {
                    // We combine codepoint and graphemes because both have
                    // shared style handling. We use comptime to dup it.
                    tag @ (pagepkg::ContentTag::Codepoint
                    | pagepkg::ContentTag::CodepointGrapheme) => {
                        self.write_cell(tag, emit, writer, page, cell)?;

                        // If we have a point map, all codepoints map to this
                        // cell.
                        if self.point_map.is_some() {
                            let mut discarding = Discarding::new();
                            self.write_cell(tag, emit, &mut discarding, page, cell)?;
                            let count = discarding.count;
                            if let Some(map) = self.point_map.as_mut() {
                                map.map.extend(core::iter::repeat_n(
                                    Coordinate { x, y: y as u32 },
                                    count,
                                ));
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

                cell_i += 1;
            }
        }

        // If the style is non-default, we need to close our style tag.
        if !style.default_style() {
            self.format_style_close(emit, writer)?;
        }

        // Close any open hyperlink for HTML output
        if current_hyperlink_id.is_some() {
            self.format_hyperlink_close(emit, writer)?;
        }

        // Close the monospace wrapper for HTML output
        if emit == Format::Html {
            let closing = b"</div>";
            writer.write_all(closing)?;
            if let Some(map) = self.point_map.as_mut() {
                // PORT NOTE: `ensureUnusedCapacity` + `appendNTimesAssumeCapacity`
                // -> `reserve` + `extend(repeat_n(..))`.
                map.map.reserve(closing.len());
                let last = map.map[map.map.len() - 1];
                map.map.extend(core::iter::repeat_n(last, closing.len()));
            }
            // Closing the div creates a newline in the output
            // so make sure to create one less newline
            if blank_rows >= 1 {
                blank_rows -= 1;
            }
        }

        Ok(TrailingState {
            rows: blank_rows,
            cells: blank_cells,
        })
    }

    /// Fast path for writing runs of simple cells: single-codepoint cells
    /// that require no style or hyperlink handling. Output bytes are
    /// batched into a stack buffer to avoid per-cell writer dispatch.
    /// Returns the number of cells consumed, which may be zero if the
    /// first cell isn't eligible for the fast path (in which case the
    /// caller must handle it via the slow path).
    ///
    /// Requirements (asserted by the caller, not here):
    ///
    ///   - The codepoint map is empty.
    ///   - For styled formats, `run_style_id` is the valid interned
    ///     page-local id of the currently active style.
    ///   - For HTML, no hyperlink is currently open.
    ///
    /// `run_x`/`run_y` are the page coordinates of `cells[0]`, used for
    /// point map tracking.
    ///
    /// Blank cell accounting matches the slow path: accumulated blanks
    /// are only materialized once a non-blank cell is found, and any
    /// remainder is written back to `blank_cells`.
    // Deliberately not inlined: this is instantiated per emit format and
    // inlining every copy into formatWithStateEmit's row loop bloats the
    // binary. track_points is a runtime bool for the same reason: a
    // comptime bool doubles the instantiation count for one predictable
    // branch per emitted cell.
    // PORT NOTE: `emit` is a runtime param here (the file's comptime-param
    // convention), so there is a single instantiation; `noinline` maps to
    // `#[inline(never)]` to keep the reference's row-loop code-size intent.
    #[allow(clippy::too_many_arguments)]
    #[inline(never)]
    fn write_cell_run<const EMIT: u8>(
        &mut self,
        track_points: bool,
        writer: &mut dyn io::Write,
        cells: &[Cell],
        run_x: size::CellCountInt,
        run_y: size::CellCountInt,
        run_style_id: u32,
        run_hyperlink_id: Option<crate::hyperlink::Id>,
        blank_cells: &mut usize,
        // PERF(port): caller-provided scratch — see `format_with_state_emit`.
        // The reference's is `var buf: [512]u8 = undefined;` in this frame.
        run_buf: &mut [u8; 512],
    ) -> io::Result<usize> {
        // `comptime emit: Format` — see `emit_from_const`.
        let emit: Format = emit_from_const::<EMIT>();
        debug_assert!(track_points == self.point_map.is_some());

        let page: &Page = unsafe { &*self.page };

        // The largest single-cell encoding must fit after a flush: the
        // HTML entity for the maximum codepoint ("&#2097151;") is 10
        // bytes, escapes are up to 6.
        const MAX_ENCODING_LEN: usize = 16;
        let buf: &mut [u8; 512] = run_buf;
        let mut len: usize = 0;
        let mut pending: usize = *blank_cells;

        // PORT NOTE: `while (i < cells.len) : (i += 1)` — the continue
        // expression maps to increments at each continue site and the tail.
        let mut i: usize = 0;
        while i < cells.len() {
            let cell = &cells[i];

            // Spacers produce no output, matching the slow path which
            // skips them before any blank/style handling.
            match cell.wide() {
                pagepkg::Wide::Narrow | pagepkg::Wide::Wide => {}
                pagepkg::Wide::SpacerHead | pagepkg::Wide::SpacerTail => {
                    i += 1;
                    continue;
                }
            }

            // Only text cells: bg-color cells synthesize styles and take
            // the slow path.
            match cell.content_tag() {
                pagepkg::ContentTag::Codepoint | pagepkg::ContentTag::CodepointGrapheme => {}
                pagepkg::ContentTag::BgColorPalette | pagepkg::ContentTag::BgColorRgb => break,
            }

            if format_styled(emit) {
                // Style transition, take the slow path.
                if cell.style_id() as u32 != run_style_id {
                    break;
                }
            }

            let cp: u21 = cell.content_codepoint();

            // Blank cell accounting, matching the slow path blank block.
            if format_styled(emit) {
                // Styled formats only treat unstyled empty cells as
                // blank; anything else (including spaces) is written
                // so that styling is preserved.
                if cp == 0 && cell.wide() == pagepkg::Wide::Narrow && run_style_id == 0 {
                    pending += 1;
                    i += 1;
                    continue;
                }
            } else {
                // Cells with no text are blank.
                if cp == 0 {
                    pending += 1;
                    i += 1;
                    continue;
                }

                // Trailing spaces are blank.
                if cp == ' ' as u21 && self.opts.trim {
                    pending += 1;
                    i += 1;
                    continue;
                }
            }

            // Hyperlink state must be stable within a run: any non-blank
            // cell must belong to the currently open hyperlink (or none).
            // Transitions take the slow path. This is checked after blank
            // accounting because blank cells never touch hyperlink state.
            if emit == Format::Html {
                if cell.hyperlink() {
                    let run_id = match run_hyperlink_id {
                        Some(v) => v,
                        None => break,
                    };
                    let cell_id = match page.lookup_hyperlink(cell as *const Cell) {
                        Some(v) => v,
                        None => break,
                    };
                    if cell_id != run_id {
                        break;
                    }
                } else if run_hyperlink_id.is_some() {
                    break;
                }
            }

            // The page coordinate of this cell, for point tracking.
            let x: size::CellCountInt = (run_x as usize + i) as size::CellCountInt;

            // This cell produces output: materialize accumulated blanks.
            if pending > 0 {
                if track_points {
                    self.append_blank_points(pending, x, run_y);
                }

                while pending > 0 {
                    if len == buf.len() {
                        writer.write_all(&buf[..len])?;
                        len = 0;
                    }
                    let n = core::cmp::min(pending, buf.len() - len);
                    buf[len..len + n].fill(b' ');
                    len += n;
                    pending -= n;
                }
            }

            // Flush if the largest possible encoding may not fit.
            if len + MAX_ENCODING_LEN > buf.len() {
                writer.write_all(&buf[..len])?;
                len = 0;
            }

            let mut cell_bytes: usize;

            // Empty (but styled or wide) cells emit a space, matching
            // writeCell.
            if cp == 0 {
                buf[len] = b' ';
                len += 1;
                cell_bytes = 1;
            } else {
                let new_len = encode_codepoint::<EMIT>(buf, len, cp);
                cell_bytes = new_len - len;
                len = new_len;

                // Multi-codepoint graphemes emit their extra codepoints,
                // matching writeCell. This is out-of-line to keep the
                // hot loop for the common single-codepoint case small.
                if cell.content_tag() == pagepkg::ContentTag::CodepointGrapheme {
                    cell_bytes += self.write_grapheme_cps(emit, writer, cell, buf, &mut len)?;
                }
            }

            // All of the cell's bytes map to the cell's coordinate.
            if track_points {
                let map = self.point_map.as_mut().unwrap();
                map.map.extend(core::iter::repeat_n(
                    Coordinate { x, y: run_y as u32 },
                    cell_bytes,
                ));
            }

            i += 1;
        }

        if len > 0 {
            writer.write_all(&buf[..len])?;
        }
        *blank_cells = pending;
        Ok(i)
    }

    /// Encode the extra codepoints of a multi-codepoint grapheme into
    /// buf, flushing to the writer as needed. Returns the number of
    /// bytes written. This is deliberately not inlined so that the
    /// (rare) grapheme case doesn't bloat the writeCellRun hot loop.
    #[inline(never)]
    fn write_grapheme_cps(
        &self,
        emit: Format,
        writer: &mut dyn io::Write,
        cell: &Cell,
        buf: &mut [u8; 512],
        len: &mut usize,
    ) -> io::Result<usize> {
        let page: &Page = unsafe { &*self.page };
        const MAX_ENCODING_LEN: usize = 16;
        let mut bytes: usize = 0;
        for &gcp in page.lookup_grapheme(cell as *const Cell).unwrap() {
            if *len + MAX_ENCODING_LEN > buf.len() {
                writer.write_all(&buf[..*len])?;
                *len = 0;
            }
            // Cold path (see `#[inline(never)]` above): one runtime dispatch
            // into the monomorphized encoder per grapheme codepoint.
            let new_len = match emit {
                Format::Plain => encode_codepoint::<{ Format::Plain as u8 }>(buf, *len, gcp as u21),
                Format::Vt => encode_codepoint::<{ Format::Vt as u8 }>(buf, *len, gcp as u21),
                Format::Html => encode_codepoint::<{ Format::Html as u8 }>(buf, *len, gcp as u21),
            };
            bytes += new_len - *len;
            *len = new_len;
        }
        Ok(bytes)
    }

    /// Append the point map entries for a run of `count` blank cells
    /// that are materialized as spaces just before the cell at (x, y).
    /// Blank cells can span multiple rows if they carry over from wrap
    /// continuation, so this walks backwards from (x, y).
    // PORT NOTE: the reference passes `map: *const PointMap` (whose inner list
    // pointer is mutable through the const handle); the Rust expression of
    // that aliasing is `&mut self` mutating `self.point_map` directly.
    fn append_blank_points(&mut self, count: usize, x: size::CellCountInt, y: size::CellCountInt) {
        let cols = unsafe { &*self.page }.size.cols;
        let map = self.point_map.as_mut().unwrap();
        // PORT NOTE: `ensureUnusedCapacity` + `appendAssumeCapacity` ->
        // `reserve` + `push` (Vec growth aborts on OOM; the reference's
        // `catch return error.WriteFailed` arm has no Rust analogue).
        map.map.reserve(count);

        let mut remaining = count;
        let mut blank_x = x;
        let mut blank_y = y;
        while remaining > 0 {
            if blank_x > 0 {
                // We have space in this row
                blank_x -= 1;
            } else if blank_y > 0 {
                // Wrap to previous row
                blank_y -= 1;
                blank_x = cols - 1;
            } else {
                // Can't go back further, just use (0, 0)
                blank_x = 0;
                blank_y = 0;
            }

            map.map.push(Coordinate {
                x: blank_x,
                y: blank_y as u32,
            });
            remaining -= 1;
        }
    }

    fn write_cell(
        &self,
        tag: pagepkg::ContentTag,
        emit: Format,
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

        self.write_codepoint_with_replacement(emit, writer, cell.content_codepoint())?;
        if tag == pagepkg::ContentTag::CodepointGrapheme {
            for &cp in page.lookup_grapheme(cell as *const Cell).unwrap() {
                self.write_codepoint_with_replacement(emit, writer, cp as u21)?;
            }
        }
        Ok(())
    }

    fn write_codepoint_with_replacement(
        &self,
        emit: Format,
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
            None => return self.write_codepoint(emit, writer, codepoint),
        };

        match r {
            Replacement::Codepoint(v) => self.write_codepoint(emit, writer, *v)?,

            Replacement::String(s) => {
                // PORT NOTE: the reference validates UTF-8 (`Utf8View.init(s)
                // catch unreachable`) then iterates codepoints. The string is an
                // asserted-valid UTF-8 byte slice; we iterate its `char`s.
                let view = core::str::from_utf8(s).unwrap();
                for cp in view.chars() {
                    self.write_codepoint(emit, writer, cp as u21)?;
                }
            }
        }
        Ok(())
    }

    // PORT NOTE: the reference discards `self` (`_ = self;`) — the fn keeps
    // its method shape but no longer reads any field.
    fn write_codepoint(
        &self,
        emit: Format,
        writer: &mut dyn io::Write,
        codepoint: u21,
    ) -> io::Result<()> {
        let _ = self;
        match emit {
            // PORT NOTE: `writer.printUnicodeCodepoint(codepoint)` — the
            // UTF-8 encode with replacement-character fallback below.
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
                            writer.write_all(&[codepoint as u8])
                        } else {
                            let mut buf: [u8; 16] = [0; 16];
                            buf[0..2].copy_from_slice(b"&#");
                            let mut len: usize =
                                2 + fastprint::print_decimal(&mut buf[2..], codepoint);
                            buf[len] = b';';
                            len += 1;
                            writer.write_all(&buf[..len])
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
        emit: Format,
        writer: &mut dyn io::Write,
        style: &crate::style::Style,
        // PERF(port): caller-provided SGR scratch — see `format_with_state_emit`.
        style_buf: &mut [u8; 128],
    ) -> io::Result<()> {
        match emit {
            Format::Plain => unreachable!(),

            Format::Vt => {
                let mut formatter = style.formatter_vt();
                // PORT NOTE: `self.opts.palette` is `Option<*const Palette>`;
                // deref to the `Option<&Palette>` the style formatter expects.
                formatter.palette = self.opts.palette.map(|p| unsafe { &*p });
                // PERF(port): the reference emits the SGR bytes directly
                // (`writer.print("{f}", ...)` bottoms out in `writeAll`);
                // going through the `core::fmt::Write` adapter added a UTF-8
                // validation scan and a non-inlinable hop per style change.
                let len = formatter.format_into(style_buf);
                writer.write_all(&style_buf[..len])
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
    fn format_style_close(&mut self, emit: Format, writer: &mut dyn io::Write) -> io::Result<()> {
        let s: &[u8] = match emit {
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

    fn format_hyperlink_open(
        &self,
        emit: Format,
        writer: &mut dyn io::Write,
        uri: &[u8],
    ) -> io::Result<()> {
        match emit {
            Format::Plain | Format::Vt => unreachable!(),

            // layout since we're primarily using it as a CSS wrapper.
            Format::Html => {
                writer.write_all(b"<a href=\"")?;
                for &byte in uri {
                    self.write_codepoint(emit, writer, byte as u21)?;
                }
                writer.write_all(b"\">")
            }
        }
    }

    // PORT NOTE: takes `&mut self` for the same reason as `format_style_close`.
    fn format_hyperlink_close(
        &mut self,
        emit: Format,
        writer: &mut dyn io::Write,
    ) -> io::Result<()> {
        let s: &[u8] = match emit {
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

/// Encode a single codepoint into buf at len, returning the new
/// length. The caller must guarantee enough remaining buffer space
/// for the largest possible encoding.
// PORT NOTE: the reference nests this self-less `inline fn` inside
// `PageFormatter`; Rust impl blocks only hold associated fns, so it is a
// module-level fn. `comptime emit` is the const-generic `EMIT` (see
// `emit_from_const`), which prunes the unused format arms per instantiation;
// `inline` -> `#[inline(always)]` (plain `#[inline]` lost to the cost model,
// leaving an out-of-line call per emitted codepoint where the reference's
// comptime-specialized form is ~4 instructions).
#[inline(always)]
// PERF(port): `len` travels BY VALUE (returning the new length) — the
// `&mut usize` form address-took it, pinning it to a stack slot per
// iteration and blocking the range fact from the caller's flush guard, which
// left a bounds check per emitted byte.
fn encode_codepoint<const EMIT: u8>(buf: &mut [u8; 512], len: usize, cp: u21) -> usize {
    let emit: Format = emit_from_const::<EMIT>();
    let mut len = len;
    let start = len;
    match emit {
        Format::Plain | Format::Vt => {
            if cp < 0x80 {
                buf[start] = cp as u8;
                len += 1;
            } else {
                // PORT NOTE: `std.unicode.utf8Encode(cp, ..) catch { .. }` —
                // `char::from_u32` rejects the same domain (surrogates and
                // out-of-range values).
                match char::from_u32(cp) {
                    Some(c) => len += c.encode_utf8(&mut buf[start..start + 4]).len(),
                    None => {
                        // Matches Writer.printUnicodeCodepoint: invalid
                        // codepoints become the replacement character.
                        buf[start..start + 3].copy_from_slice(&[0xEF, 0xBF, 0xBD]);
                        len += 3;
                    }
                }
            }
        }

        Format::Html => 'html: {
            let esc: Option<&[u8]> = match cp {
                cp if cp == '<' as u21 => Some(b"&lt;"),
                cp if cp == '>' as u21 => Some(b"&gt;"),
                cp if cp == '&' as u21 => Some(b"&amp;"),
                cp if cp == '"' as u21 => Some(b"&quot;"),
                cp if cp == '\'' as u21 => Some(b"&#39;"),
                _ => None,
            };
            if let Some(s) = esc {
                buf[start..start + s.len()].copy_from_slice(s);
                len += s.len();
                break 'html;
            }

            // ASCII is emitted directly, everything else as a
            // numeric entity. See writeCodepoint.
            if cp < 0x80 {
                buf[start] = cp as u8;
                len += 1;
                break 'html;
            }

            buf[start..start + 2].copy_from_slice(b"&#");
            len += 2;
            len += fastprint::print_decimal(&mut buf[len..], cp);
            buf[len] = b';';
            len += 1;
        }
    }

    len
}

#[cfg(test)]
mod tests {
    // PORT NOTE: the reference test bodies use index loops (`for (0..N) |i|`) and
    // index `point_map.items[offset + i]`. We keep the faithful indexed form
    // rather than the iterator form clippy would prefer.
    #![allow(clippy::needless_range_loop)]

    use super::*;
    use crate::terminal::{Colors, Options as TermOptions, Terminal};
    use allocator_api2::alloc::Global;

    // PORT NOTE: the reference tests call `Terminal.init(alloc, .{ .cols, .rows })`
    // relying on `Options` field defaults. Rust `Options` has no field defaults,
    // so this helper supplies them verbatim (mirroring terminal.rs's test `opts`).
    fn opts(cols: size::CellCountInt, rows: size::CellCountInt) -> TermOptions {
        TermOptions {
            cols,
            rows,
            max_scrollback_bytes: Some(10_000),
            max_scrollback_lines: None,
            colors: Colors {
                background: crate::color::DynamicRGB::UNSET,
                foreground: crate::color::DynamicRGB::UNSET,
                cursor: crate::color::DynamicRGB::UNSET,
                palette: crate::color::DynamicPalette::default_palette(),
            },
            default_modes: crate::modes::ModePacked::default(),
            default_cursor_style: crate::screen::CursorStyle::Block,
            default_cursor_blink: Some(false),
            kitty_image_storage_limit: 320 * 1000 * 1000,
            kitty_image_loading_limits: crate::kitty::graphics_image::Limits::DIRECT,
        }
    }

    // PORT NOTE: `&pages.pages.last.?.data` — deref the raw last node to a
    // `*const Page`. The caller guarantees the node outlives the formatter.
    unsafe fn last_page(pages: &PageList) -> *const Page {
        unsafe { &(*crate::pagelist::Node::page(pages.pages.last)) }
    }

    // PORT NOTE: `std.mem.trimStart(u8, s, pat)` — strip leading bytes that are
    // any of the bytes in `pat` (Zig trims the *set* of bytes).
    fn trim_start<'a>(s: &'a [u8], pat: &[u8]) -> &'a [u8] {
        let mut i = 0;
        while i < s.len() && pat.contains(&s[i]) {
            i += 1;
        }
        &s[i..]
    }

    // PORT NOTE: `std.mem.indexOf(u8, haystack, needle)`.
    fn index_of(haystack: &[u8], needle: &[u8]) -> Option<usize> {
        if needle.is_empty() {
            return Some(0);
        }
        haystack.windows(needle.len()).position(|w| w == needle)
    }

    // PORT NOTE: `Coordinate{ .x, .y }` helper for assertions.
    fn coord(x: size::CellCountInt, y: u32) -> Coordinate {
        Coordinate { x, y }
    }

    // PORT NOTE: `Pin{ .node, .x, .y }` helper; the reference `Pin` has no
    // `garbage` field, so we default it to false (untracked pins are never
    // garbage).
    fn pin(node: *mut crate::pagelist::Node, x: size::CellCountInt, y: size::CellCountInt) -> Pin {
        Pin {
            node,
            x,
            y,
            garbage: false,
        }
    }

    // test "Page plain single line"
    #[test]
    fn page_plain_single_line() {
        let mut builder: Vec<u8> = Vec::new();
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        {
            let mut s = t.vt_stream();
            s.next_slice(b"hello, world");
        }

        // Verify we have only a single page
        let pages = unsafe { &(*t.screens.active).pages };
        assert!(!pages.pages.first.is_null());
        assert!(core::ptr::eq(pages.pages.first, pages.pages.last));

        // Create the formatter
        let page = unsafe { last_page(pages) };
        let mut formatter: PageFormatter = PageFormatter::init(page, Options::plain());

        // Test our point map.
        let mut point_map: Vec<Coordinate> = Vec::new();
        formatter.point_map = Some(PointMap {
            alloc: Global,
            map: &mut point_map,
            base: 0,
        });

        // Verify output
        let state = formatter.format_with_state(&mut builder).unwrap();
        let output = builder.as_slice();
        assert_eq!(output, b"hello, world");
        let page_ref = unsafe { &*page };
        assert_eq!(page_ref.size.rows as usize, state.rows);
        assert_eq!(page_ref.size.cols as usize - 12, state.cells);

        // Verify our point map
        assert_eq!(output.len(), point_map.len());
        for i in 0..output.len() {
            assert_eq!(coord(i as size::CellCountInt, 0), point_map[i]);
        }
        t.deinit(Global);
    }

    // test "Page plain single line soft-wrapped unwrapped"
    #[test]
    fn page_plain_single_line_soft_wrapped_unwrapped() {
        let mut builder: Vec<u8> = Vec::new();
        let mut t = Terminal::init(Global, opts(3, 5)).unwrap();
        {
            let mut s = t.vt_stream();
            s.next_slice(b"hello!");
        }

        // Verify we have only a single page
        let pages = unsafe { &(*t.screens.active).pages };
        assert!(!pages.pages.first.is_null());
        assert!(core::ptr::eq(pages.pages.first, pages.pages.last));

        // Create the formatter
        let page = unsafe { last_page(pages) };
        let mut formatter: PageFormatter = PageFormatter::init(
            page,
            Options {
                emit: Format::Plain,
                unwrap: true,
                ..Options::plain()
            },
        );

        // Test our point map.
        let mut point_map: Vec<Coordinate> = Vec::new();
        formatter.point_map = Some(PointMap {
            alloc: Global,
            map: &mut point_map,
            base: 0,
        });

        // Verify output
        // Note: we don't test the trailing state, which may have bugs
        // with unwrap...
        let _ = formatter.format_with_state(&mut builder).unwrap();
        let output = builder.as_slice();
        assert_eq!(output, b"hello!");

        // Verify our point map
        assert_eq!(output.len(), point_map.len());
        assert_eq!(coord(0, 0), point_map[0]);
        assert_eq!(coord(1, 0), point_map[1]);
        assert_eq!(coord(2, 0), point_map[2]);
        assert_eq!(coord(0, 1), point_map[3]);
        assert_eq!(coord(1, 1), point_map[4]);
        assert_eq!(coord(2, 1), point_map[5]);
        t.deinit(Global);
    }

    // test "Page plain single wide char"
    #[test]
    fn page_plain_single_wide_char() {
        let mut builder: Vec<u8> = Vec::new();
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        {
            let mut s = t.vt_stream();
            s.next_slice("1A⚡".as_bytes());
        }

        // Verify we have only a single page
        let pages = unsafe { &(*t.screens.active).pages };
        assert!(!pages.pages.first.is_null());
        assert!(core::ptr::eq(pages.pages.first, pages.pages.last));

        // Create the formatter
        let page = unsafe { last_page(pages) };
        let page_ref = unsafe { &*page };

        // Full string
        {
            builder.clear();
            let mut point_map: Vec<Coordinate> = Vec::new();
            let mut formatter: PageFormatter = PageFormatter::init(page, Options::plain());
            formatter.point_map = Some(PointMap {
                alloc: Global,
                map: &mut point_map,
                base: 0,
            });
            let state = formatter.format_with_state(&mut builder).unwrap();
            let output = builder.as_slice();
            assert_eq!(output, "1A⚡".as_bytes());
            assert_eq!(page_ref.size.rows as usize, state.rows);
            assert_eq!(page_ref.size.cols as usize - 4, state.cells);

            // Verify our point map
            assert_eq!(output.len(), point_map.len());
            for i in 2..output.len() {
                assert_eq!(coord(2, 0), point_map[i]);
            }
        }

        // Wide only (from start)
        {
            builder.clear();
            let mut point_map: Vec<Coordinate> = Vec::new();
            let mut formatter: PageFormatter = PageFormatter::init(page, Options::plain());
            formatter.point_map = Some(PointMap {
                alloc: Global,
                map: &mut point_map,
                base: 0,
            });
            formatter.start_x = 2;
            let state = formatter.format_with_state(&mut builder).unwrap();
            let output = builder.as_slice();
            assert_eq!(output, "⚡".as_bytes());
            assert_eq!(page_ref.size.rows as usize, state.rows);
            assert_eq!(page_ref.size.cols as usize - 4, state.cells);

            // Verify our point map
            assert_eq!(output.len(), point_map.len());
            for i in 0..output.len() {
                assert_eq!(coord(2, 0), point_map[i]);
            }
        }

        // Wide only (from tail)
        {
            builder.clear();
            let mut point_map: Vec<Coordinate> = Vec::new();
            let mut formatter: PageFormatter = PageFormatter::init(page, Options::plain());
            formatter.point_map = Some(PointMap {
                alloc: Global,
                map: &mut point_map,
                base: 0,
            });
            formatter.start_x = 3;
            let state = formatter.format_with_state(&mut builder).unwrap();
            let output = builder.as_slice();
            assert_eq!(output, "⚡".as_bytes());
            assert_eq!(page_ref.size.rows as usize, state.rows);
            assert_eq!(page_ref.size.cols as usize - 4, state.cells);

            // Verify our point map
            assert_eq!(output.len(), point_map.len());
            for i in 0..output.len() {
                assert_eq!(coord(2, 0), point_map[i]);
            }
        }
        t.deinit(Global);
    }

    // test "Page plain single wide char soft-wrapped unwrapped"
    #[test]
    fn page_plain_single_wide_char_soft_wrapped_unwrapped() {
        let mut builder: Vec<u8> = Vec::new();
        let mut t = Terminal::init(Global, opts(3, 24)).unwrap();
        {
            let mut s = t.vt_stream();
            s.next_slice("1A⚡".as_bytes());
        }

        // Verify we have only a single page
        let pages = unsafe { &(*t.screens.active).pages };
        assert!(!pages.pages.first.is_null());
        assert!(core::ptr::eq(pages.pages.first, pages.pages.last));

        // Create the formatter
        let page = unsafe { last_page(pages) };
        let page_ref = unsafe { &*page };

        // Full string
        {
            builder.clear();
            let mut point_map: Vec<Coordinate> = Vec::new();
            let mut formatter: PageFormatter = PageFormatter::init(page, Options::plain());
            formatter.opts.unwrap = true;
            formatter.point_map = Some(PointMap {
                alloc: Global,
                map: &mut point_map,
                base: 0,
            });
            let state = formatter.format_with_state(&mut builder).unwrap();
            let output = builder.as_slice();
            assert_eq!(output, "1A⚡".as_bytes());
            assert_eq!(page_ref.size.rows as usize - 1, state.rows);
            assert_eq!(page_ref.size.cols as usize - 2, state.cells);

            // Verify our point map
            assert_eq!(output.len(), point_map.len());
            for i in 2..output.len() {
                assert_eq!(coord(0, 1), point_map[i]);
            }
        }

        // Full string (ending on spacer head)
        {
            builder.clear();
            let mut point_map: Vec<Coordinate> = Vec::new();
            let mut formatter: PageFormatter = PageFormatter::init(page, Options::plain());
            formatter.opts.unwrap = true;
            formatter.point_map = Some(PointMap {
                alloc: Global,
                map: &mut point_map,
                base: 0,
            });
            formatter.end_x = Some(2);
            formatter.end_y = Some(0);
            let _ = formatter.format_with_state(&mut builder).unwrap();
            let output = builder.as_slice();
            assert_eq!(output, "1A⚡".as_bytes());

            // Verify our point map
            assert_eq!(output.len(), point_map.len());
            for i in 2..output.len() {
                assert_eq!(coord(0, 1), point_map[i]);
            }
        }

        // Wide only (from start)
        {
            builder.clear();
            let mut point_map: Vec<Coordinate> = Vec::new();
            let mut formatter: PageFormatter = PageFormatter::init(page, Options::plain());
            formatter.opts.unwrap = true;
            formatter.point_map = Some(PointMap {
                alloc: Global,
                map: &mut point_map,
                base: 0,
            });
            formatter.start_x = 2;
            let state = formatter.format_with_state(&mut builder).unwrap();
            let output = builder.as_slice();
            assert_eq!(output, "⚡".as_bytes());
            assert_eq!(page_ref.size.rows as usize - 1, state.rows);
            assert_eq!(page_ref.size.cols as usize - 2, state.cells);

            // Verify our point map
            assert_eq!(output.len(), point_map.len());
            for i in 0..output.len() {
                assert_eq!(coord(0, 1), point_map[i]);
            }
        }

        // Wide only (from tail)
        {
            builder.clear();
            let mut point_map: Vec<Coordinate> = Vec::new();
            let mut formatter: PageFormatter = PageFormatter::init(page, Options::plain());
            formatter.opts.unwrap = true;
            formatter.point_map = Some(PointMap {
                alloc: Global,
                map: &mut point_map,
                base: 0,
            });
            formatter.start_y = 1;
            formatter.start_x = 1;
            let state = formatter.format_with_state(&mut builder).unwrap();
            let output = builder.as_slice();
            assert_eq!(output, "⚡".as_bytes());
            assert_eq!(page_ref.size.rows as usize - 1, state.rows);
            assert_eq!(page_ref.size.cols as usize - 2, state.cells);

            // Verify our point map
            assert_eq!(output.len(), point_map.len());
            for i in 0..output.len() {
                assert_eq!(coord(0, 1), point_map[i]);
            }
        }
        t.deinit(Global);
    }

    // test "Page plain multiline"
    #[test]
    fn page_plain_multiline() {
        let mut builder: Vec<u8> = Vec::new();
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        {
            let mut s = t.vt_stream();
            s.next_slice(b"hello\r\nworld");
        }

        // Verify we have only a single page
        let pages = unsafe { &(*t.screens.active).pages };
        assert!(!pages.pages.first.is_null());
        assert!(core::ptr::eq(pages.pages.first, pages.pages.last));

        // Create the formatter
        let page = unsafe { last_page(pages) };
        let page_ref = unsafe { &*page };
        let mut formatter: PageFormatter = PageFormatter::init(page, Options::plain());
        let mut point_map: Vec<Coordinate> = Vec::new();
        formatter.point_map = Some(PointMap {
            alloc: Global,
            map: &mut point_map,
            base: 0,
        });

        // Verify output
        let state = formatter.format_with_state(&mut builder).unwrap();
        let output = builder.as_slice();
        assert_eq!(output, b"hello\nworld");
        assert_eq!(page_ref.size.rows as usize - 1, state.rows);
        assert_eq!(page_ref.size.cols as usize - 5, state.cells);

        // Verify point map
        assert_eq!(output.len(), point_map.len());
        for i in 0..5 {
            assert_eq!(coord(i as size::CellCountInt, 0), point_map[i]);
        }
        assert_eq!(coord(4, 0), point_map[5]); // \n
        for i in 0..5 {
            assert_eq!(coord(i as size::CellCountInt, 1), point_map[6 + i]);
        }
        t.deinit(Global);
    }

    // test "Page plain multiline rectangle"
    #[test]
    fn page_plain_multiline_rectangle() {
        let mut builder: Vec<u8> = Vec::new();
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        {
            let mut s = t.vt_stream();
            s.next_slice(b"hello\r\nworld");
        }

        // Verify we have only a single page
        let pages = unsafe { &(*t.screens.active).pages };
        assert!(!pages.pages.first.is_null());
        assert!(core::ptr::eq(pages.pages.first, pages.pages.last));

        // Create the formatter
        let page = unsafe { last_page(pages) };
        let page_ref = unsafe { &*page };
        let mut formatter: PageFormatter = PageFormatter::init(page, Options::plain());
        formatter.start_x = 1;
        formatter.end_x = Some(3);
        formatter.rectangle = true;
        let mut point_map: Vec<Coordinate> = Vec::new();
        formatter.point_map = Some(PointMap {
            alloc: Global,
            map: &mut point_map,
            base: 0,
        });

        // Verify output
        let state = formatter.format_with_state(&mut builder).unwrap();
        let output = builder.as_slice();
        assert_eq!(output, b"ell\norl");
        assert_eq!(page_ref.size.rows as usize - 1, state.rows);
        assert_eq!(0, state.cells);

        // Verify point map
        assert_eq!(output.len(), point_map.len());
        for i in 0..3 {
            assert_eq!(coord((i + 1) as size::CellCountInt, 0), point_map[i]);
        }
        assert_eq!(coord(3, 0), point_map[3]); // \n
        for i in 0..3 {
            assert_eq!(coord((i + 1) as size::CellCountInt, 1), point_map[4 + i]);
        }
        t.deinit(Global);
    }

    // test "Page plain multi blank lines"
    #[test]
    fn page_plain_multi_blank_lines() {
        let mut builder: Vec<u8> = Vec::new();
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        {
            let mut s = t.vt_stream();
            s.next_slice(b"hello\r\n\r\n\r\nworld");
        }

        // Verify we have only a single page
        let pages = unsafe { &(*t.screens.active).pages };
        assert!(!pages.pages.first.is_null());
        assert!(core::ptr::eq(pages.pages.first, pages.pages.last));

        // Create the formatter
        let page = unsafe { last_page(pages) };
        let page_ref = unsafe { &*page };
        let mut formatter: PageFormatter = PageFormatter::init(page, Options::plain());
        let mut point_map: Vec<Coordinate> = Vec::new();
        formatter.point_map = Some(PointMap {
            alloc: Global,
            map: &mut point_map,
            base: 0,
        });

        // Verify output
        let state = formatter.format_with_state(&mut builder).unwrap();
        let output = builder.as_slice();
        assert_eq!(output, b"hello\n\n\nworld");
        assert_eq!(page_ref.size.rows as usize - 3, state.rows);
        assert_eq!(page_ref.size.cols as usize - 5, state.cells);

        // Verify point map
        assert_eq!(output.len(), point_map.len());
        for i in 0..5 {
            assert_eq!(coord(i as size::CellCountInt, 0), point_map[i]);
        }
        assert_eq!(coord(4, 0), point_map[5]); // \n after row 0
        assert_eq!(coord(0, 1), point_map[6]); // \n after blank row 1
        assert_eq!(coord(0, 2), point_map[7]); // \n after blank row 2
        for i in 0..5 {
            assert_eq!(coord(i as size::CellCountInt, 3), point_map[8 + i]);
        }
        t.deinit(Global);
    }

    // test "Page plain trailing blank lines"
    #[test]
    fn page_plain_trailing_blank_lines() {
        let mut builder: Vec<u8> = Vec::new();
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        {
            let mut s = t.vt_stream();
            s.next_slice(b"hello\r\nworld\r\n\r\n");
        }

        // Verify we have only a single page
        let pages = unsafe { &(*t.screens.active).pages };
        assert!(!pages.pages.first.is_null());
        assert!(core::ptr::eq(pages.pages.first, pages.pages.last));

        // Create the formatter
        let page = unsafe { last_page(pages) };
        let page_ref = unsafe { &*page };
        let mut formatter: PageFormatter = PageFormatter::init(page, Options::plain());
        let mut point_map: Vec<Coordinate> = Vec::new();
        formatter.point_map = Some(PointMap {
            alloc: Global,
            map: &mut point_map,
            base: 0,
        });

        // Verify output. We expect there to be no trailing newlines because
        // we can't differentiate trailing blank lines as being meaningful because
        // the page formatter can't see the cursor position.
        let state = formatter.format_with_state(&mut builder).unwrap();
        let output = builder.as_slice();
        assert_eq!(output, b"hello\nworld");
        assert_eq!(page_ref.size.rows as usize - 1, state.rows);
        assert_eq!(page_ref.size.cols as usize - 5, state.cells);

        // Verify point map
        assert_eq!(output.len(), point_map.len());
        for i in 0..5 {
            assert_eq!(coord(i as size::CellCountInt, 0), point_map[i]);
        }
        assert_eq!(coord(4, 0), point_map[5]); // \n
        for i in 0..5 {
            assert_eq!(coord(i as size::CellCountInt, 1), point_map[6 + i]);
        }
        t.deinit(Global);
    }

    // test "Page plain trailing whitespace"
    #[test]
    fn page_plain_trailing_whitespace() {
        let mut builder: Vec<u8> = Vec::new();
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        {
            let mut s = t.vt_stream();
            s.next_slice(b"hello   \r\nworld   ");
        }

        // Verify we have only a single page
        let pages = unsafe { &(*t.screens.active).pages };
        assert!(!pages.pages.first.is_null());
        assert!(core::ptr::eq(pages.pages.first, pages.pages.last));

        // Create the formatter
        let page = unsafe { last_page(pages) };
        let page_ref = unsafe { &*page };
        let mut formatter: PageFormatter = PageFormatter::init(page, Options::plain());
        let mut point_map: Vec<Coordinate> = Vec::new();
        formatter.point_map = Some(PointMap {
            alloc: Global,
            map: &mut point_map,
            base: 0,
        });

        // Verify output. We expect there to be no trailing newlines because
        // we can't differentiate trailing blank lines as being meaningful because
        // the page formatter can't see the cursor position.
        let state = formatter.format_with_state(&mut builder).unwrap();
        let output = builder.as_slice();
        assert_eq!(output, b"hello\nworld");
        assert_eq!(page_ref.size.rows as usize - 1, state.rows);
        assert_eq!(page_ref.size.cols as usize - 5, state.cells);

        // Verify point map
        assert_eq!(output.len(), point_map.len());
        for i in 0..5 {
            assert_eq!(coord(i as size::CellCountInt, 0), point_map[i]);
        }
        assert_eq!(coord(4, 0), point_map[5]); // \n
        for i in 0..5 {
            assert_eq!(coord(i as size::CellCountInt, 1), point_map[6 + i]);
        }
        t.deinit(Global);
    }

    // test "Page plain trailing whitespace no trim"
    #[test]
    fn page_plain_trailing_whitespace_no_trim() {
        let mut builder: Vec<u8> = Vec::new();
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        {
            let mut s = t.vt_stream();
            s.next_slice(b"hello   \r\nworld  ");
        }

        // Verify we have only a single page
        let pages = unsafe { &(*t.screens.active).pages };
        assert!(!pages.pages.first.is_null());
        assert!(core::ptr::eq(pages.pages.first, pages.pages.last));

        // Create the formatter
        let page = unsafe { last_page(pages) };
        let page_ref = unsafe { &*page };
        let mut formatter: PageFormatter = PageFormatter::init(
            page,
            Options {
                emit: Format::Plain,
                trim: false,
                ..Options::plain()
            },
        );
        let mut point_map: Vec<Coordinate> = Vec::new();
        formatter.point_map = Some(PointMap {
            alloc: Global,
            map: &mut point_map,
            base: 0,
        });

        // Verify output. We expect there to be no trailing newlines because
        // we can't differentiate trailing blank lines as being meaningful because
        // the page formatter can't see the cursor position.
        let state = formatter.format_with_state(&mut builder).unwrap();
        let output = builder.as_slice();
        assert_eq!(output, b"hello   \nworld  ");
        assert_eq!(page_ref.size.rows as usize - 1, state.rows);
        assert_eq!(page_ref.size.cols as usize - 7, state.cells);

        // Verify point map
        assert_eq!(output.len(), point_map.len());
        for i in 0..8 {
            assert_eq!(coord(i as size::CellCountInt, 0), point_map[i]);
        }
        assert_eq!(coord(7, 0), point_map[8]); // \n
        for i in 0..7 {
            assert_eq!(coord(i as size::CellCountInt, 1), point_map[9 + i]);
        }
        t.deinit(Global);
    }

    // test "Page plain with prior trailing state rows"
    #[test]
    fn page_plain_with_prior_trailing_state_rows() {
        let mut builder: Vec<u8> = Vec::new();
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        {
            let mut s = t.vt_stream();
            s.next_slice(b"hello");
        }

        let pages = unsafe { &(*t.screens.active).pages };
        assert!(!pages.pages.first.is_null());
        assert!(core::ptr::eq(pages.pages.first, pages.pages.last));

        let page = unsafe { last_page(pages) };
        let page_ref = unsafe { &*page };
        let mut formatter: PageFormatter = PageFormatter::init(page, Options::plain());
        formatter.trailing_state = Some(TrailingState { rows: 2, cells: 0 });
        let mut point_map: Vec<Coordinate> = Vec::new();
        formatter.point_map = Some(PointMap {
            alloc: Global,
            map: &mut point_map,
            base: 0,
        });

        let state = formatter.format_with_state(&mut builder).unwrap();
        let output = builder.as_slice();
        assert_eq!(output, b"\n\nhello");
        assert_eq!(page_ref.size.rows as usize, state.rows);
        assert_eq!(page_ref.size.cols as usize - 5, state.cells);

        // Verify point map
        assert_eq!(output.len(), point_map.len());
        assert_eq!(coord(0, 0), point_map[0]); // \n first blank row
        assert_eq!(coord(0, 1), point_map[1]); // \n second blank row
        for i in 0..5 {
            assert_eq!(coord(i as size::CellCountInt, 0), point_map[2 + i]);
        }
        t.deinit(Global);
    }

    // test "Page plain with prior trailing state cells no wrapped line"
    #[test]
    fn page_plain_with_prior_trailing_state_cells_no_wrapped_line() {
        let mut builder: Vec<u8> = Vec::new();
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        {
            let mut s = t.vt_stream();
            s.next_slice(b"hello");
        }

        let pages = unsafe { &(*t.screens.active).pages };
        assert!(!pages.pages.first.is_null());
        assert!(core::ptr::eq(pages.pages.first, pages.pages.last));

        let page = unsafe { last_page(pages) };
        let page_ref = unsafe { &*page };
        let mut formatter: PageFormatter = PageFormatter::init(page, Options::plain());
        formatter.trailing_state = Some(TrailingState { rows: 0, cells: 3 });
        let mut point_map: Vec<Coordinate> = Vec::new();
        formatter.point_map = Some(PointMap {
            alloc: Global,
            map: &mut point_map,
            base: 0,
        });

        let state = formatter.format_with_state(&mut builder).unwrap();
        let output = builder.as_slice();
        // Blank cells are reset when row is not a wrap continuation
        assert_eq!(output, b"hello");
        assert_eq!(page_ref.size.rows as usize, state.rows);
        assert_eq!(page_ref.size.cols as usize - 5, state.cells);

        // Verify point map
        assert_eq!(output.len(), point_map.len());
        for i in 0..5 {
            assert_eq!(coord(i as size::CellCountInt, 0), point_map[i]);
        }
        t.deinit(Global);
    }

    // test "Page plain with prior trailing state cells with wrap continuation"
    #[test]
    fn page_plain_with_prior_trailing_state_cells_with_wrap_continuation() {
        let mut builder: Vec<u8> = Vec::new();
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        {
            let mut s = t.vt_stream();
            s.next_slice(b"world");
        }

        let pages = unsafe { &(*t.screens.active).pages };
        assert!(!pages.pages.first.is_null());
        assert!(core::ptr::eq(pages.pages.first, pages.pages.last));

        let page = unsafe { last_page(pages) };
        let page_ref = unsafe { &*page };

        // Surgically modify the first row to be a wrap continuation
        let row = page_ref.get_row(0);
        unsafe { (*row).set_wrap_continuation(true) };

        let mut formatter: PageFormatter = PageFormatter::init(
            page,
            Options {
                emit: Format::Plain,
                unwrap: true,
                ..Options::plain()
            },
        );
        formatter.trailing_state = Some(TrailingState { rows: 0, cells: 3 });
        let mut point_map: Vec<Coordinate> = Vec::new();
        formatter.point_map = Some(PointMap {
            alloc: Global,
            map: &mut point_map,
            base: 0,
        });

        let state = formatter.format_with_state(&mut builder).unwrap();
        let output = builder.as_slice();
        // Blank cells are preserved when row is a wrap continuation with unwrap enabled
        assert_eq!(output, b"   world");
        assert_eq!(page_ref.size.rows as usize, state.rows);
        assert_eq!(page_ref.size.cols as usize - 5, state.cells);

        // Verify point map - 3 spaces from prior trailing state + "world"
        assert_eq!(output.len(), point_map.len());
        // The 3 blank cells can't go back beyond (0,0) so they all map to (0,0)
        assert_eq!(coord(0, 0), point_map[0]); // space
        assert_eq!(coord(0, 0), point_map[1]); // space
        assert_eq!(coord(0, 0), point_map[2]); // space
        for i in 0..5 {
            assert_eq!(coord(i as size::CellCountInt, 0), point_map[3 + i]);
        }
        t.deinit(Global);
    }

    // test "Page plain soft-wrapped without unwrap"
    #[test]
    fn page_plain_soft_wrapped_without_unwrap() {
        let mut builder: Vec<u8> = Vec::new();
        let mut t = Terminal::init(Global, opts(10, 24)).unwrap();
        {
            let mut s = t.vt_stream();
            s.next_slice(b"hello world test");
        }

        let pages = unsafe { &(*t.screens.active).pages };
        assert!(!pages.pages.first.is_null());
        assert!(core::ptr::eq(pages.pages.first, pages.pages.last));

        let page = unsafe { last_page(pages) };
        let page_ref = unsafe { &*page };
        let mut formatter: PageFormatter = PageFormatter::init(page, Options::plain());
        let mut point_map: Vec<Coordinate> = Vec::new();
        formatter.point_map = Some(PointMap {
            alloc: Global,
            map: &mut point_map,
            base: 0,
        });

        let state = formatter.format_with_state(&mut builder).unwrap();
        let output = builder.as_slice();
        // Without unwrap, wrapped lines show as separate lines
        assert_eq!(output, b"hello worl\nd test");
        assert_eq!(page_ref.size.rows as usize - 1, state.rows);
        assert_eq!(page_ref.size.cols as usize - 6, state.cells);

        // Verify point map
        assert_eq!(output.len(), point_map.len());
        for i in 0..10 {
            assert_eq!(coord(i as size::CellCountInt, 0), point_map[i]);
        }
        assert_eq!(coord(9, 0), point_map[10]); // \n
        for i in 0..6 {
            assert_eq!(coord(i as size::CellCountInt, 1), point_map[11 + i]);
        }
        t.deinit(Global);
    }

    // test "Page plain soft-wrapped with unwrap"
    #[test]
    fn page_plain_soft_wrapped_with_unwrap() {
        let mut builder: Vec<u8> = Vec::new();
        let mut t = Terminal::init(Global, opts(10, 24)).unwrap();
        {
            let mut s = t.vt_stream();
            s.next_slice(b"hello world test");
        }

        let pages = unsafe { &(*t.screens.active).pages };
        assert!(!pages.pages.first.is_null());
        assert!(core::ptr::eq(pages.pages.first, pages.pages.last));

        let page = unsafe { last_page(pages) };
        let page_ref = unsafe { &*page };
        let mut formatter: PageFormatter = PageFormatter::init(
            page,
            Options {
                emit: Format::Plain,
                unwrap: true,
                ..Options::plain()
            },
        );
        let mut point_map: Vec<Coordinate> = Vec::new();
        formatter.point_map = Some(PointMap {
            alloc: Global,
            map: &mut point_map,
            base: 0,
        });

        let state = formatter.format_with_state(&mut builder).unwrap();
        let output = builder.as_slice();
        // With unwrap, wrapped lines are joined together
        assert_eq!(output, b"hello world test");
        assert_eq!(page_ref.size.rows as usize - 1, state.rows);
        assert_eq!(page_ref.size.cols as usize - 6, state.cells);

        // Verify point map
        assert_eq!(output.len(), point_map.len());
        for i in 0..10 {
            assert_eq!(coord(i as size::CellCountInt, 0), point_map[i]);
        }
        for i in 0..6 {
            assert_eq!(coord(i as size::CellCountInt, 1), point_map[10 + i]);
        }
        t.deinit(Global);
    }

    // test "Page plain soft-wrapped 3 lines without unwrap"
    #[test]
    fn page_plain_soft_wrapped_3_lines_without_unwrap() {
        let mut builder: Vec<u8> = Vec::new();
        let mut t = Terminal::init(Global, opts(10, 24)).unwrap();
        {
            let mut s = t.vt_stream();
            s.next_slice(b"hello world this is a test");
        }

        let pages = unsafe { &(*t.screens.active).pages };
        assert!(!pages.pages.first.is_null());
        assert!(core::ptr::eq(pages.pages.first, pages.pages.last));

        let page = unsafe { last_page(pages) };
        let page_ref = unsafe { &*page };
        let mut formatter: PageFormatter = PageFormatter::init(page, Options::plain());
        let mut point_map: Vec<Coordinate> = Vec::new();
        formatter.point_map = Some(PointMap {
            alloc: Global,
            map: &mut point_map,
            base: 0,
        });

        let state = formatter.format_with_state(&mut builder).unwrap();
        let output = builder.as_slice();
        // Without unwrap, wrapped lines show as separate lines
        assert_eq!(output, b"hello worl\nd this is\na test");
        assert_eq!(page_ref.size.rows as usize - 2, state.rows);
        assert_eq!(page_ref.size.cols as usize - 6, state.cells);

        // Verify point map
        assert_eq!(output.len(), point_map.len());
        for i in 0..10 {
            assert_eq!(coord(i as size::CellCountInt, 0), point_map[i]);
        }
        assert_eq!(coord(9, 0), point_map[10]); // \n
        for i in 0..9 {
            assert_eq!(coord(i as size::CellCountInt, 1), point_map[11 + i]);
        }
        assert_eq!(coord(8, 1), point_map[20]); // \n
        for i in 0..6 {
            assert_eq!(coord(i as size::CellCountInt, 2), point_map[21 + i]);
        }
        t.deinit(Global);
    }

    // test "Page plain soft-wrapped 3 lines with unwrap"
    #[test]
    fn page_plain_soft_wrapped_3_lines_with_unwrap() {
        let mut builder: Vec<u8> = Vec::new();
        let mut t = Terminal::init(Global, opts(10, 24)).unwrap();
        {
            let mut s = t.vt_stream();
            s.next_slice(b"hello world this is a test");
        }

        let pages = unsafe { &(*t.screens.active).pages };
        assert!(!pages.pages.first.is_null());
        assert!(core::ptr::eq(pages.pages.first, pages.pages.last));

        let page = unsafe { last_page(pages) };
        let page_ref = unsafe { &*page };
        let mut formatter: PageFormatter = PageFormatter::init(
            page,
            Options {
                emit: Format::Plain,
                unwrap: true,
                ..Options::plain()
            },
        );
        let mut point_map: Vec<Coordinate> = Vec::new();
        formatter.point_map = Some(PointMap {
            alloc: Global,
            map: &mut point_map,
            base: 0,
        });

        let state = formatter.format_with_state(&mut builder).unwrap();
        let output = builder.as_slice();
        // With unwrap, wrapped lines are joined together
        assert_eq!(output, b"hello world this is a test");
        assert_eq!(page_ref.size.rows as usize - 2, state.rows);
        assert_eq!(page_ref.size.cols as usize - 6, state.cells);

        // Verify point map - unwrapped text spans 3 rows
        assert_eq!(output.len(), point_map.len());
        for i in 0..10 {
            assert_eq!(coord(i as size::CellCountInt, 0), point_map[i]);
        }
        for i in 0..10 {
            assert_eq!(coord(i as size::CellCountInt, 1), point_map[10 + i]);
        }
        for i in 0..6 {
            assert_eq!(coord(i as size::CellCountInt, 2), point_map[20 + i]);
        }
        t.deinit(Global);
    }

    // test "Page plain start_y subset"
    #[test]
    fn page_plain_start_y_subset() {
        let mut builder: Vec<u8> = Vec::new();
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        {
            let mut s = t.vt_stream();
            s.next_slice(b"hello\r\nworld\r\ntest");
        }
        let pages = unsafe { &(*t.screens.active).pages };
        let page = unsafe { last_page(pages) };
        let page_ref = unsafe { &*page };
        let mut formatter: PageFormatter = PageFormatter::init(page, Options::plain());
        formatter.start_y = 1;
        let mut point_map: Vec<Coordinate> = Vec::new();
        formatter.point_map = Some(PointMap {
            alloc: Global,
            map: &mut point_map,
            base: 0,
        });
        let state = formatter.format_with_state(&mut builder).unwrap();
        let output = builder.as_slice();
        assert_eq!(output, b"world\ntest");
        assert_eq!(page_ref.size.rows as usize - 2, state.rows);
        assert_eq!(page_ref.size.cols as usize - 4, state.cells);

        // Verify point map
        assert_eq!(output.len(), point_map.len());
        for i in 0..5 {
            assert_eq!(coord(i as size::CellCountInt, 1), point_map[i]);
        }
        assert_eq!(coord(4, 1), point_map[5]); // \n
        for i in 0..4 {
            assert_eq!(coord(i as size::CellCountInt, 2), point_map[6 + i]);
        }
        t.deinit(Global);
    }

    // test "Page plain end_y subset"
    #[test]
    fn page_plain_end_y_subset() {
        let mut builder: Vec<u8> = Vec::new();
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        {
            let mut s = t.vt_stream();
            s.next_slice(b"hello\r\nworld\r\ntest");
        }
        let pages = unsafe { &(*t.screens.active).pages };
        let page = unsafe { last_page(pages) };
        let page_ref = unsafe { &*page };
        let mut formatter: PageFormatter = PageFormatter::init(page, Options::plain());
        formatter.end_y = Some(1);
        let mut point_map: Vec<Coordinate> = Vec::new();
        formatter.point_map = Some(PointMap {
            alloc: Global,
            map: &mut point_map,
            base: 0,
        });
        let state = formatter.format_with_state(&mut builder).unwrap();
        let output = builder.as_slice();
        assert_eq!(output, b"hello\nworld");
        assert_eq!(1, state.rows);
        assert_eq!(page_ref.size.cols as usize - 5, state.cells);

        // Verify point map
        assert_eq!(output.len(), point_map.len());
        for i in 0..5 {
            assert_eq!(coord(i as size::CellCountInt, 0), point_map[i]);
        }
        assert_eq!(coord(4, 0), point_map[5]); // \n
        for i in 0..5 {
            assert_eq!(coord(i as size::CellCountInt, 1), point_map[6 + i]);
        }
        t.deinit(Global);
    }

    // test "Page plain start_y and end_y range"
    #[test]
    fn page_plain_start_y_and_end_y_range() {
        let mut builder: Vec<u8> = Vec::new();
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        {
            let mut s = t.vt_stream();
            s.next_slice(b"hello\r\nworld\r\ntest\r\nfoo");
        }
        let pages = unsafe { &(*t.screens.active).pages };
        let page = unsafe { last_page(pages) };
        let page_ref = unsafe { &*page };
        let mut formatter: PageFormatter = PageFormatter::init(page, Options::plain());
        formatter.start_y = 1;
        formatter.end_y = Some(2);
        let mut point_map: Vec<Coordinate> = Vec::new();
        formatter.point_map = Some(PointMap {
            alloc: Global,
            map: &mut point_map,
            base: 0,
        });
        let state = formatter.format_with_state(&mut builder).unwrap();
        let output = builder.as_slice();
        assert_eq!(output, b"world\ntest");
        assert_eq!(1, state.rows);
        assert_eq!(page_ref.size.cols as usize - 4, state.cells);

        // Verify point map
        assert_eq!(output.len(), point_map.len());
        for i in 0..5 {
            assert_eq!(coord(i as size::CellCountInt, 1), point_map[i]);
        }
        assert_eq!(coord(4, 1), point_map[5]); // \n
        for i in 0..4 {
            assert_eq!(coord(i as size::CellCountInt, 2), point_map[6 + i]);
        }
        t.deinit(Global);
    }

    // test "Page plain start_y out of bounds"
    #[test]
    fn page_plain_start_y_out_of_bounds() {
        let mut builder: Vec<u8> = Vec::new();
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        {
            let mut s = t.vt_stream();
            s.next_slice(b"hello");
        }
        let pages = unsafe { &(*t.screens.active).pages };
        let page = unsafe { last_page(pages) };
        let mut formatter: PageFormatter = PageFormatter::init(page, Options::plain());
        formatter.start_y = 30;
        let mut point_map: Vec<Coordinate> = Vec::new();
        formatter.point_map = Some(PointMap {
            alloc: Global,
            map: &mut point_map,
            base: 0,
        });
        let state = formatter.format_with_state(&mut builder).unwrap();
        let output = builder.as_slice();
        assert_eq!(output, b"");
        assert_eq!(0, state.rows);
        assert_eq!(0, state.cells);

        // Verify point map is empty
        assert_eq!(0, point_map.len());
        t.deinit(Global);
    }

    // test "Page plain end_y greater than rows"
    #[test]
    fn page_plain_end_y_greater_than_rows() {
        let mut builder: Vec<u8> = Vec::new();
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        {
            let mut s = t.vt_stream();
            s.next_slice(b"hello");
        }
        let pages = unsafe { &(*t.screens.active).pages };
        let page = unsafe { last_page(pages) };
        let page_ref = unsafe { &*page };
        let mut formatter: PageFormatter = PageFormatter::init(page, Options::plain());
        formatter.end_y = Some(30);
        let mut point_map: Vec<Coordinate> = Vec::new();
        formatter.point_map = Some(PointMap {
            alloc: Global,
            map: &mut point_map,
            base: 0,
        });
        let state = formatter.format_with_state(&mut builder).unwrap();
        let output = builder.as_slice();
        // Should clamp to page.size.rows and work normally
        assert_eq!(output, b"hello");
        assert_eq!(page_ref.size.rows as usize, state.rows);
        assert_eq!(page_ref.size.cols as usize - 5, state.cells);

        // Verify point map
        assert_eq!(output.len(), point_map.len());
        for i in 0..5 {
            assert_eq!(coord(i as size::CellCountInt, 0), point_map[i]);
        }
        t.deinit(Global);
    }

    // test "Page plain end_y less than start_y"
    #[test]
    fn page_plain_end_y_less_than_start_y() {
        let mut builder: Vec<u8> = Vec::new();
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        {
            let mut s = t.vt_stream();
            s.next_slice(b"hello");
        }
        let pages = unsafe { &(*t.screens.active).pages };
        let page = unsafe { last_page(pages) };
        let mut formatter: PageFormatter = PageFormatter::init(page, Options::plain());
        formatter.start_y = 5;
        formatter.end_y = Some(2);
        let mut point_map: Vec<Coordinate> = Vec::new();
        formatter.point_map = Some(PointMap {
            alloc: Global,
            map: &mut point_map,
            base: 0,
        });
        let state = formatter.format_with_state(&mut builder).unwrap();
        let output = builder.as_slice();
        assert_eq!(output, b"");
        assert_eq!(0, state.rows);
        assert_eq!(0, state.cells);

        // Verify point map is empty
        assert_eq!(0, point_map.len());
        t.deinit(Global);
    }

    // test "Page plain start_x on first row only"
    #[test]
    fn page_plain_start_x_on_first_row_only() {
        let mut builder: Vec<u8> = Vec::new();
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        {
            let mut s = t.vt_stream();
            s.next_slice(b"hello world");
        }
        let pages = unsafe { &(*t.screens.active).pages };
        let page = unsafe { last_page(pages) };
        let page_ref = unsafe { &*page };
        let mut formatter: PageFormatter = PageFormatter::init(page, Options::plain());
        formatter.start_x = 6;
        let mut point_map: Vec<Coordinate> = Vec::new();
        formatter.point_map = Some(PointMap {
            alloc: Global,
            map: &mut point_map,
            base: 0,
        });
        let state = formatter.format_with_state(&mut builder).unwrap();
        let output = builder.as_slice();
        assert_eq!(output, b"world");
        assert_eq!(page_ref.size.rows as usize, state.rows);
        assert_eq!(page_ref.size.cols as usize - 11, state.cells);

        // Verify point map
        assert_eq!(output.len(), point_map.len());
        for i in 0..5 {
            assert_eq!(coord((i + 6) as size::CellCountInt, 0), point_map[i]);
        }
        t.deinit(Global);
    }

    // test "Page plain end_x on last row only"
    #[test]
    fn page_plain_end_x_on_last_row_only() {
        let mut builder: Vec<u8> = Vec::new();
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        {
            let mut s = t.vt_stream();
            s.next_slice(b"first line\r\nsecond line\r\nthird line");
        }
        let pages = unsafe { &(*t.screens.active).pages };
        let page = unsafe { last_page(pages) };
        let mut formatter: PageFormatter = PageFormatter::init(page, Options::plain());
        formatter.end_y = Some(2);
        formatter.end_x = Some(4);
        let mut point_map: Vec<Coordinate> = Vec::new();
        formatter.point_map = Some(PointMap {
            alloc: Global,
            map: &mut point_map,
            base: 0,
        });
        let state = formatter.format_with_state(&mut builder).unwrap();
        let output = builder.as_slice();
        assert_eq!(output, b"first line\nsecond line\nthird");
        assert_eq!(1, state.rows);
        assert_eq!(0, state.cells);

        // Verify point map
        assert_eq!(output.len(), point_map.len());
        for i in 0..10 {
            assert_eq!(coord(i as size::CellCountInt, 0), point_map[i]);
        }
        assert_eq!(coord(9, 0), point_map[10]); // \n
        for i in 0..11 {
            assert_eq!(coord(i as size::CellCountInt, 1), point_map[11 + i]);
        }
        assert_eq!(coord(10, 1), point_map[22]); // \n
        for i in 0..5 {
            assert_eq!(coord(i as size::CellCountInt, 2), point_map[23 + i]);
        }
        t.deinit(Global);
    }

    // test "Page plain start_x and end_x multiline"
    #[test]
    fn page_plain_start_x_and_end_x_multiline() {
        let mut builder: Vec<u8> = Vec::new();
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        {
            let mut s = t.vt_stream();
            s.next_slice(b"hello world\r\ntest case\r\nfoo bar");
        }
        let pages = unsafe { &(*t.screens.active).pages };
        let page = unsafe { last_page(pages) };
        let mut formatter: PageFormatter = PageFormatter::init(page, Options::plain());
        formatter.start_x = 6;
        formatter.end_y = Some(2);
        formatter.end_x = Some(2);
        let mut point_map: Vec<Coordinate> = Vec::new();
        formatter.point_map = Some(PointMap {
            alloc: Global,
            map: &mut point_map,
            base: 0,
        });
        let state = formatter.format_with_state(&mut builder).unwrap();
        let output = builder.as_slice();
        // First row: "world" (start_x=6 to end of row)
        // Second row: "test case" (full row)
        // Third row: "foo" (start to end_x=2, inclusive)
        assert_eq!(output, b"world\ntest case\nfoo");
        assert_eq!(1, state.rows);
        assert_eq!(0, state.cells);

        // Verify point map
        assert_eq!(output.len(), point_map.len());
        for i in 0..5 {
            assert_eq!(coord((i + 6) as size::CellCountInt, 0), point_map[i]);
        }
        assert_eq!(coord(10, 0), point_map[5]); // \n
        for i in 0..9 {
            assert_eq!(coord(i as size::CellCountInt, 1), point_map[6 + i]);
        }
        assert_eq!(coord(8, 1), point_map[15]); // \n
        for i in 0..3 {
            assert_eq!(coord(i as size::CellCountInt, 2), point_map[16 + i]);
        }
        t.deinit(Global);
    }

    // test "Page plain start_x out of bounds"
    #[test]
    fn page_plain_start_x_out_of_bounds() {
        let mut builder: Vec<u8> = Vec::new();
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        {
            let mut s = t.vt_stream();
            s.next_slice(b"hello");
        }
        let pages = unsafe { &(*t.screens.active).pages };
        let page = unsafe { last_page(pages) };
        let mut formatter: PageFormatter = PageFormatter::init(page, Options::plain());
        formatter.start_x = 100;
        let mut point_map: Vec<Coordinate> = Vec::new();
        formatter.point_map = Some(PointMap {
            alloc: Global,
            map: &mut point_map,
            base: 0,
        });
        let state = formatter.format_with_state(&mut builder).unwrap();
        let output = builder.as_slice();
        assert_eq!(output, b"");
        assert_eq!(0, state.rows);
        assert_eq!(0, state.cells);

        // Verify point map is empty
        assert_eq!(0, point_map.len());
        t.deinit(Global);
    }

    // test "Page plain end_x greater than cols"
    #[test]
    fn page_plain_end_x_greater_than_cols() {
        let mut builder: Vec<u8> = Vec::new();
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        {
            let mut s = t.vt_stream();
            s.next_slice(b"hello");
        }
        let pages = unsafe { &(*t.screens.active).pages };
        let page = unsafe { last_page(pages) };
        let page_ref = unsafe { &*page };
        let mut formatter: PageFormatter = PageFormatter::init(page, Options::plain());
        formatter.end_x = Some(100);
        let mut point_map: Vec<Coordinate> = Vec::new();
        formatter.point_map = Some(PointMap {
            alloc: Global,
            map: &mut point_map,
            base: 0,
        });
        let state = formatter.format_with_state(&mut builder).unwrap();
        let output = builder.as_slice();
        assert_eq!(output, b"hello");
        assert_eq!(page_ref.size.rows as usize, state.rows);
        assert_eq!(page_ref.size.cols as usize - 5, state.cells);

        // Verify point map
        assert_eq!(output.len(), point_map.len());
        for i in 0..5 {
            assert_eq!(coord(i as size::CellCountInt, 0), point_map[i]);
        }
        t.deinit(Global);
    }

    // test "Page plain end_x less than start_x single row"
    #[test]
    fn page_plain_end_x_less_than_start_x_single_row() {
        let mut builder: Vec<u8> = Vec::new();
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        {
            let mut s = t.vt_stream();
            s.next_slice(b"hello");
        }
        let pages = unsafe { &(*t.screens.active).pages };
        let page = unsafe { last_page(pages) };
        let mut formatter: PageFormatter = PageFormatter::init(page, Options::plain());
        formatter.start_x = 10;
        formatter.end_y = Some(0);
        formatter.end_x = Some(5);
        let mut point_map: Vec<Coordinate> = Vec::new();
        formatter.point_map = Some(PointMap {
            alloc: Global,
            map: &mut point_map,
            base: 0,
        });
        let state = formatter.format_with_state(&mut builder).unwrap();
        let output = builder.as_slice();
        assert_eq!(output, b"");
        assert_eq!(0, state.rows);
        assert_eq!(0, state.cells);

        // Verify point map is empty
        assert_eq!(0, point_map.len());
        t.deinit(Global);
    }

    // test "Page plain start_y non-zero ignores trailing state"
    #[test]
    fn page_plain_start_y_non_zero_ignores_trailing_state() {
        let mut builder: Vec<u8> = Vec::new();
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        {
            let mut s = t.vt_stream();
            s.next_slice(b"hello\r\nworld");
        }
        let pages = unsafe { &(*t.screens.active).pages };
        let page = unsafe { last_page(pages) };
        let page_ref = unsafe { &*page };
        let mut formatter: PageFormatter = PageFormatter::init(page, Options::plain());
        formatter.start_y = 1;
        formatter.trailing_state = Some(TrailingState { rows: 5, cells: 10 });
        let mut point_map: Vec<Coordinate> = Vec::new();
        formatter.point_map = Some(PointMap {
            alloc: Global,
            map: &mut point_map,
            base: 0,
        });
        let state = formatter.format_with_state(&mut builder).unwrap();
        let output = builder.as_slice();
        // Should NOT output the 5 newlines from trailing_state because start_y is non-zero
        assert_eq!(output, b"world");
        assert_eq!(page_ref.size.rows as usize - 1, state.rows);
        assert_eq!(page_ref.size.cols as usize - 5, state.cells);

        // Verify point map
        assert_eq!(output.len(), point_map.len());
        for i in 0..5 {
            assert_eq!(coord(i as size::CellCountInt, 1), point_map[i]);
        }
        t.deinit(Global);
    }

    // test "Page plain start_x non-zero ignores trailing state"
    #[test]
    fn page_plain_start_x_non_zero_ignores_trailing_state() {
        let mut builder: Vec<u8> = Vec::new();
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        {
            let mut s = t.vt_stream();
            s.next_slice(b"hello world");
        }
        let pages = unsafe { &(*t.screens.active).pages };
        let page = unsafe { last_page(pages) };
        let page_ref = unsafe { &*page };
        let mut formatter: PageFormatter = PageFormatter::init(page, Options::plain());
        formatter.start_x = 6;
        formatter.trailing_state = Some(TrailingState { rows: 2, cells: 8 });
        let mut point_map: Vec<Coordinate> = Vec::new();
        formatter.point_map = Some(PointMap {
            alloc: Global,
            map: &mut point_map,
            base: 0,
        });
        let state = formatter.format_with_state(&mut builder).unwrap();
        let output = builder.as_slice();
        // Should NOT output the 2 newlines or 8 spaces from trailing_state because start_x is non-zero
        assert_eq!(output, b"world");
        assert_eq!(page_ref.size.rows as usize, state.rows);
        assert_eq!(page_ref.size.cols as usize - 11, state.cells);

        // Verify point map
        assert_eq!(output.len(), point_map.len());
        for i in 0..5 {
            assert_eq!(coord((i + 6) as size::CellCountInt, 0), point_map[i]);
        }
        t.deinit(Global);
    }

    // test "Page plain start_y and start_x zero uses trailing state"
    #[test]
    fn page_plain_start_y_and_start_x_zero_uses_trailing_state() {
        let mut builder: Vec<u8> = Vec::new();
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        {
            let mut s = t.vt_stream();
            s.next_slice(b"hello");
        }
        let pages = unsafe { &(*t.screens.active).pages };
        let page = unsafe { last_page(pages) };
        let page_ref = unsafe { &*page };
        let mut formatter: PageFormatter = PageFormatter::init(page, Options::plain());
        formatter.start_y = 0;
        formatter.start_x = 0;
        formatter.trailing_state = Some(TrailingState { rows: 2, cells: 0 });
        let mut point_map: Vec<Coordinate> = Vec::new();
        formatter.point_map = Some(PointMap {
            alloc: Global,
            map: &mut point_map,
            base: 0,
        });
        let state = formatter.format_with_state(&mut builder).unwrap();
        let output = builder.as_slice();
        // SHOULD output the 2 newlines from trailing_state because both start_y and start_x are 0
        assert_eq!(output, b"\n\nhello");
        assert_eq!(page_ref.size.rows as usize, state.rows);
        assert_eq!(page_ref.size.cols as usize - 5, state.cells);

        // Verify point map
        assert_eq!(output.len(), point_map.len());
        assert_eq!(coord(0, 0), point_map[0]); // \n first blank row
        assert_eq!(coord(0, 1), point_map[1]); // \n second blank row
        for i in 0..5 {
            assert_eq!(coord(i as size::CellCountInt, 0), point_map[2 + i]);
        }
        t.deinit(Global);
    }

    // test "Page plain single line with styling"
    #[test]
    fn page_plain_single_line_with_styling() {
        let mut builder: Vec<u8> = Vec::new();
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        {
            let mut s = t.vt_stream();
            s.next_slice(b"hello, \x1b[1mworld\x1b[0m");
        }

        // Verify we have only a single page
        let pages = unsafe { &(*t.screens.active).pages };
        assert!(!pages.pages.first.is_null());
        assert!(core::ptr::eq(pages.pages.first, pages.pages.last));

        // Create the formatter
        let page = unsafe { last_page(pages) };
        let page_ref = unsafe { &*page };
        let mut formatter: PageFormatter = PageFormatter::init(page, Options::plain());
        let mut point_map: Vec<Coordinate> = Vec::new();
        formatter.point_map = Some(PointMap {
            alloc: Global,
            map: &mut point_map,
            base: 0,
        });

        // Verify output
        let state = formatter.format_with_state(&mut builder).unwrap();
        let output = builder.as_slice();
        assert_eq!(output, b"hello, world");
        assert_eq!(page_ref.size.rows as usize, state.rows);
        assert_eq!(page_ref.size.cols as usize - 12, state.cells);

        // Verify point map
        assert_eq!(output.len(), point_map.len());
        for i in 0..12 {
            assert_eq!(coord(i as size::CellCountInt, 0), point_map[i]);
        }
        t.deinit(Global);
    }

    // test "Page VT single line plain text"
    #[test]
    fn page_vt_single_line_plain_text() {
        let mut builder: Vec<u8> = Vec::new();
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        {
            let mut s = t.vt_stream();
            s.next_slice(b"hello");
        }
        let pages = unsafe { &(*t.screens.active).pages };
        let page = unsafe { last_page(pages) };
        let mut formatter: PageFormatter = PageFormatter::init(page, Options::vt());
        let mut point_map: Vec<Coordinate> = Vec::new();
        formatter.point_map = Some(PointMap {
            alloc: Global,
            map: &mut point_map,
            base: 0,
        });
        formatter.format(&mut builder).unwrap();
        let output = builder.as_slice();
        assert_eq!(output, b"hello");

        // Verify point map
        assert_eq!(output.len(), point_map.len());
        for i in 0..5 {
            assert_eq!(coord(i as size::CellCountInt, 0), point_map[i]);
        }
        t.deinit(Global);
    }

    // test "Page VT single line with bold"
    #[test]
    fn page_vt_single_line_with_bold() {
        let mut builder: Vec<u8> = Vec::new();
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        {
            let mut s = t.vt_stream();
            s.next_slice(b"\x1b[1mhello\x1b[0m");
        }
        let pages = unsafe { &(*t.screens.active).pages };
        let page = unsafe { last_page(pages) };
        let mut formatter: PageFormatter = PageFormatter::init(page, Options::vt());
        let mut point_map: Vec<Coordinate> = Vec::new();
        formatter.point_map = Some(PointMap {
            alloc: Global,
            map: &mut point_map,
            base: 0,
        });
        formatter.format(&mut builder).unwrap();
        let output = builder.as_slice();
        assert_eq!(output, b"\x1b[0m\x1b[1mhello\x1b[0m");

        // Verify point map - style sequences should point to first character they style
        assert_eq!(output.len(), point_map.len());
        // \x1b[0m = 4 bytes, \x1b[1m = 4 bytes, total 8 bytes of style sequences
        // All style bytes should map to the first styled character at (0, 0)
        for i in 0..8 {
            assert_eq!(coord(0, 0), point_map[i]);
        }
        // Then "hello" maps to its respective positions
        for i in 0..5 {
            assert_eq!(coord(i as size::CellCountInt, 0), point_map[8 + i]);
        }
        t.deinit(Global);
    }

    // test "Page VT multiple styles"
    #[test]
    fn page_vt_multiple_styles() {
        let mut builder: Vec<u8> = Vec::new();
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        {
            let mut s = t.vt_stream();
            s.next_slice(b"\x1b[1mhello \x1b[3mworld\x1b[0m");
        }
        let pages = unsafe { &(*t.screens.active).pages };
        let page = unsafe { last_page(pages) };
        let mut formatter: PageFormatter = PageFormatter::init(page, Options::vt());
        let mut point_map: Vec<Coordinate> = Vec::new();
        formatter.point_map = Some(PointMap {
            alloc: Global,
            map: &mut point_map,
            base: 0,
        });
        formatter.format(&mut builder).unwrap();
        let output = builder.as_slice();
        assert_eq!(
            output,
            b"\x1b[0m\x1b[1mhello \x1b[0m\x1b[1m\x1b[3mworld\x1b[0m"
        );

        // Verify point map matches output length
        assert_eq!(output.len(), point_map.len());
        t.deinit(Global);
    }

    // test "Page VT with foreground color"
    #[test]
    fn page_vt_with_foreground_color() {
        let mut builder: Vec<u8> = Vec::new();
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        {
            let mut s = t.vt_stream();
            s.next_slice(b"\x1b[31mred\x1b[0m");
        }
        let pages = unsafe { &(*t.screens.active).pages };
        let page = unsafe { last_page(pages) };
        let mut formatter: PageFormatter = PageFormatter::init(page, Options::vt());
        let mut point_map: Vec<Coordinate> = Vec::new();
        formatter.point_map = Some(PointMap {
            alloc: Global,
            map: &mut point_map,
            base: 0,
        });
        formatter.format(&mut builder).unwrap();
        let output = builder.as_slice();
        assert_eq!(output, b"\x1b[0m\x1b[38;5;1mred\x1b[0m");

        // Verify point map - style sequences should point to first character they style
        assert_eq!(output.len(), point_map.len());
        // \x1b[0m = 4 bytes, \x1b[38;5;1m = 9 bytes, total 13 bytes of style sequences
        // All style bytes should map to the first styled character at (0, 0)
        for i in 0..13 {
            assert_eq!(coord(0, 0), point_map[i]);
        }
        // Then "red" maps to its respective positions
        for i in 0..3 {
            assert_eq!(coord(i as size::CellCountInt, 0), point_map[13 + i]);
        }
        t.deinit(Global);
    }

    // test "Page VT with background and foreground colors"
    #[test]
    fn page_vt_with_background_and_foreground_colors() {
        let mut builder: Vec<u8> = Vec::new();
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        {
            let mut s = t.vt_stream();
            s.next_slice(b"hello");
        }
        let pages = unsafe { &(*t.screens.active).pages };
        let page = unsafe { last_page(pages) };
        let formatter: PageFormatter = PageFormatter::init(
            page,
            Options {
                emit: Format::Vt,
                background: Some(crate::color::RGB {
                    r: 0x12,
                    g: 0x34,
                    b: 0x56,
                }),
                foreground: Some(crate::color::RGB {
                    r: 0xab,
                    g: 0xcd,
                    b: 0xef,
                }),
                ..Options::vt()
            },
        );
        formatter.format(&mut builder).unwrap();
        let output = builder.as_slice();

        // Should emit OSC 10 for foreground, OSC 11 for background, then the text
        assert_eq!(
            output,
            b"\x1b]10;rgb:ab/cd/ef\x1b\\\x1b]11;rgb:12/34/56\x1b\\hello"
        );
        t.deinit(Global);
    }

    // test "Page VT multi-line with styles"
    #[test]
    fn page_vt_multi_line_with_styles() {
        let mut builder: Vec<u8> = Vec::new();
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        {
            let mut s = t.vt_stream();
            s.next_slice(b"\x1b[1mfirst\x1b[0m\r\n\x1b[3msecond\x1b[0m");
        }
        let pages = unsafe { &(*t.screens.active).pages };
        let page = unsafe { last_page(pages) };
        let mut formatter: PageFormatter = PageFormatter::init(page, Options::vt());
        let mut point_map: Vec<Coordinate> = Vec::new();
        formatter.point_map = Some(PointMap {
            alloc: Global,
            map: &mut point_map,
            base: 0,
        });
        formatter.format(&mut builder).unwrap();
        let output = builder.as_slice();
        // Note: style is reset before newline to prevent background colors from
        // bleeding to the next line's leading cells.
        assert_eq!(
            output,
            b"\x1b[0m\x1b[1mfirst\x1b[0m\r\n\x1b[0m\x1b[3msecond\x1b[0m"
        );

        // Verify point map matches output length
        assert_eq!(output.len(), point_map.len());
        t.deinit(Global);
    }

    // test "Page VT duplicate style not emitted twice"
    #[test]
    fn page_vt_duplicate_style_not_emitted_twice() {
        let mut builder: Vec<u8> = Vec::new();
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        {
            let mut s = t.vt_stream();
            s.next_slice(b"\x1b[1mhel\x1b[1mlo\x1b[0m");
        }
        let pages = unsafe { &(*t.screens.active).pages };
        let page = unsafe { last_page(pages) };
        let mut formatter: PageFormatter = PageFormatter::init(page, Options::vt());
        let mut point_map: Vec<Coordinate> = Vec::new();
        formatter.point_map = Some(PointMap {
            alloc: Global,
            map: &mut point_map,
            base: 0,
        });
        formatter.format(&mut builder).unwrap();
        let output = builder.as_slice();
        assert_eq!(output, b"\x1b[0m\x1b[1mhello\x1b[0m");

        // Verify point map matches output length
        assert_eq!(output.len(), point_map.len());
        t.deinit(Global);
    }

    // test "PageList plain single line"
    #[test]
    fn pagelist_plain_single_line() {
        let mut builder: Vec<u8> = Vec::new();
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        {
            let mut s = t.vt_stream();
            s.next_slice(b"hello, world");
        }
        let mut pin_map: PinMapMap = PinMapMap::EMPTY;
        let pages_ptr: *const PageList = unsafe { &(*t.screens.active).pages };
        let mut formatter: PageListFormatter = PageListFormatter::init(pages_ptr, Options::plain());
        formatter.pin_map = Some(PinMap {
            alloc: Global,
            map: &mut pin_map,
        });
        formatter.format(&mut builder).unwrap();
        let output = builder.as_slice();
        assert_eq!(output, b"hello, world");

        // Verify pin map
        assert_eq!(output.len(), pin_map.count());
        let node = unsafe { (*t.screens.active).pages.pages.first };
        for i in 0..output.len() {
            assert_eq!(
                pin(node, i as size::CellCountInt, 0),
                pin_map.get(i).unwrap()
            );
        }
        t.deinit(Global);
    }

    // test "PageList plain spanning two pages"
    #[cfg_attr(
        all(miri, debug_assertions),
        ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): `Page::assert_integrity` is O(page) and runs on nearly every PageList mutation under `debug_assertions`, which puts this test out of Miri's budget in the debug-assertions-ON G6 lane. It RUNS there — UB-clean and leak-clean — in the G6c lane (`check_leaks.sh --debug-assertions-off`), measured per test in single-worker isolation; see the WS19 T6 per-test table. Do NOT widen this to a plain `miri` ignore: that would silently drop the test from BOTH lanes."
    )]
    #[test]
    fn pagelist_plain_spanning_two_pages() {
        let mut builder: Vec<u8> = Vec::new();
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        let first_page_rows =
            unsafe { crate::pagelist::Node::capacity((*t.screens.active).pages.pages.first).rows };
        {
            let mut s = t.vt_stream();
            // Fill the first page almost completely
            for _ in 0..first_page_rows - 1 {
                s.next_slice(b"\r\n");
            }
            s.next_slice(b"page one");
        }
        // PORT NOTE (WS19 T6) — RE-DERIVE, NEVER CACHE. `Terminal::active_mut()`
        // (`&mut *self.screens.active`) is a Unique retag over the whole `Screen`
        // allocation, so it pops any raw pointer derived from that allocation
        // earlier; reading through a cached one afterwards is Stacked-Borrows UB
        // (memory: rust-mut-args-strongly-protected). This test used to cache it
        // across its `vt_stream()`/search writes. The violation was real and
        // deterministic, and had never been observed because a blanket
        // "multi-page PageList intractable" annotation kept the test out of Miri
        // entirely — found the moment the G6c (`-C debug-assertions=off`) lane
        // made it runnable. Test-code only: no production path caches this.
        let active_ptr = t.screens.active;
        let pages_ptr = || -> *const PageList { unsafe { &raw const (*active_ptr).pages } };
        // Verify we're still on one page
        assert!(unsafe { core::ptr::eq((*pages_ptr()).pages.first, (*pages_ptr()).pages.last) });
        // Add one more newline to push content to a second page
        {
            let mut s = t.vt_stream();
            s.next_slice(b"\r\n");
        }
        assert!(unsafe { !core::ptr::eq((*pages_ptr()).pages.first, (*pages_ptr()).pages.last) });
        // Write content on the second page
        {
            let mut s = t.vt_stream();
            s.next_slice(b"page two");
        }

        // Format the entire PageList
        let mut pin_map: PinMapMap = PinMapMap::EMPTY;
        let mut formatter: PageListFormatter =
            PageListFormatter::init(pages_ptr(), Options::plain());
        formatter.pin_map = Some(PinMap {
            alloc: Global,
            map: &mut pin_map,
        });
        formatter.format(&mut builder).unwrap();
        let full_output = builder.as_slice();
        let output = trim_start(full_output, b"\n");
        assert_eq!(output, b"page one\npage two");

        // Verify pin map
        assert_eq!(full_output.len(), pin_map.count());
        let first_node = unsafe { (*pages_ptr()).pages.first };
        let last_node = unsafe { (*pages_ptr()).pages.last };
        let trimmed_count = full_output.len() - output.len();

        // First part (trimmed blank lines) maps to first node
        for i in 0..trimmed_count {
            assert_eq!(first_node, pin_map.get(i).unwrap().node);
        }

        // "page one" (8 chars) maps to first node
        for i in 0..8 {
            let idx = trimmed_count + i;
            assert_eq!(first_node, pin_map.get(idx).unwrap().node);
            assert_eq!(i as size::CellCountInt, pin_map.get(idx).unwrap().x);
        }

        // \n - maps to last node as it represents the transition to new page
        assert_eq!(last_node, pin_map.get(trimmed_count + 8).unwrap().node);

        // "page two" (8 chars) maps to last node
        for i in 0..8 {
            let idx = trimmed_count + 9 + i;
            assert_eq!(last_node, pin_map.get(idx).unwrap().node);
            assert_eq!(i as size::CellCountInt, pin_map.get(idx).unwrap().x);
        }
        t.deinit(Global);
    }

    // test "PageList soft-wrapped line spanning two pages without unwrap"
    #[cfg_attr(
        all(miri, debug_assertions),
        ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): `Page::assert_integrity` is O(page) and runs on nearly every PageList mutation under `debug_assertions`, which puts this test out of Miri's budget in the debug-assertions-ON G6 lane. It RUNS there — UB-clean and leak-clean — in the G6c lane (`check_leaks.sh --debug-assertions-off`), measured per test in single-worker isolation; see the WS19 T6 per-test table. Do NOT widen this to a plain `miri` ignore: that would silently drop the test from BOTH lanes."
    )]
    #[test]
    fn pagelist_soft_wrapped_line_spanning_two_pages_without_unwrap() {
        let mut builder: Vec<u8> = Vec::new();
        let mut t = Terminal::init(Global, opts(10, 3)).unwrap();
        let first_page_rows =
            unsafe { crate::pagelist::Node::capacity((*t.screens.active).pages.pages.first).rows };
        {
            let mut s = t.vt_stream();
            // Fill the first page with soft-wrapped content
            for _ in 0..first_page_rows - 1 {
                s.next_slice(b"\r\n");
            }
            s.next_slice(b"hello world test");
        }
        let pages_ptr: *const PageList = unsafe { &(*t.screens.active).pages };
        // Verify we're on two pages due to wrapping
        assert!(unsafe { !core::ptr::eq((*pages_ptr).pages.first, (*pages_ptr).pages.last) });

        // Format without unwrap - should show line breaks
        let mut pin_map: PinMapMap = PinMapMap::EMPTY;
        let mut formatter: PageListFormatter = PageListFormatter::init(pages_ptr, Options::plain());
        formatter.pin_map = Some(PinMap {
            alloc: Global,
            map: &mut pin_map,
        });
        formatter.format(&mut builder).unwrap();
        let full_output = builder.as_slice();
        let output = trim_start(full_output, b"\n");
        assert_eq!(output, b"hello worl\nd test");

        // Verify pin map
        assert_eq!(full_output.len(), pin_map.count());
        let first_node = unsafe { (*pages_ptr).pages.first };
        let last_node = unsafe { (*pages_ptr).pages.last };
        let trimmed_count = full_output.len() - output.len();

        // First part (trimmed blank lines) maps to first node
        for i in 0..trimmed_count {
            assert_eq!(first_node, pin_map.get(i).unwrap().node);
        }

        // First line maps to first node
        for i in 0..10 {
            let idx = trimmed_count + i;
            assert_eq!(first_node, pin_map.get(idx).unwrap().node);
        }

        // \n - maps to last node as it represents the transition to new page
        assert_eq!(last_node, pin_map.get(trimmed_count + 10).unwrap().node);

        // "d test" (6 chars) maps to last node
        for i in 0..6 {
            let idx = trimmed_count + 11 + i;
            assert_eq!(last_node, pin_map.get(idx).unwrap().node);
        }
        t.deinit(Global);
    }

    // test "PageList soft-wrapped line spanning two pages with unwrap"
    #[cfg_attr(
        all(miri, debug_assertions),
        ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): `Page::assert_integrity` is O(page) and runs on nearly every PageList mutation under `debug_assertions`, which puts this test out of Miri's budget in the debug-assertions-ON G6 lane. It RUNS there — UB-clean and leak-clean — in the G6c lane (`check_leaks.sh --debug-assertions-off`), measured per test in single-worker isolation; see the WS19 T6 per-test table. Do NOT widen this to a plain `miri` ignore: that would silently drop the test from BOTH lanes."
    )]
    #[test]
    fn pagelist_soft_wrapped_line_spanning_two_pages_with_unwrap() {
        let mut builder: Vec<u8> = Vec::new();
        let mut t = Terminal::init(Global, opts(10, 3)).unwrap();
        let first_page_rows =
            unsafe { crate::pagelist::Node::capacity((*t.screens.active).pages.pages.first).rows };
        {
            let mut s = t.vt_stream();
            // Fill the first page with soft-wrapped content
            for _ in 0..first_page_rows - 1 {
                s.next_slice(b"\r\n");
            }
            s.next_slice(b"hello world test");
        }
        let pages_ptr: *const PageList = unsafe { &(*t.screens.active).pages };
        // Verify we're on two pages due to wrapping
        assert!(unsafe { !core::ptr::eq((*pages_ptr).pages.first, (*pages_ptr).pages.last) });

        // Format with unwrap - should join the wrapped lines
        let mut pin_map: PinMapMap = PinMapMap::EMPTY;
        let mut formatter: PageListFormatter = PageListFormatter::init(
            pages_ptr,
            Options {
                emit: Format::Plain,
                unwrap: true,
                ..Options::plain()
            },
        );
        formatter.pin_map = Some(PinMap {
            alloc: Global,
            map: &mut pin_map,
        });
        formatter.format(&mut builder).unwrap();
        let full_output = builder.as_slice();
        let output = trim_start(full_output, b"\r\n");
        assert_eq!(output, b"hello world test");

        // Verify pin map
        assert_eq!(full_output.len(), pin_map.count());
        let first_node = unsafe { (*pages_ptr).pages.first };
        let last_node = unsafe { (*pages_ptr).pages.last };
        let trimmed_count = full_output.len() - output.len();

        // First part (trimmed blank lines) maps to first node
        for i in 0..trimmed_count {
            assert_eq!(first_node, pin_map.get(i).unwrap().node);
        }

        // First line from first page
        for i in 0..10 {
            let idx = trimmed_count + i;
            assert_eq!(first_node, pin_map.get(idx).unwrap().node);
        }

        // "d test" (6 chars) from last page
        for i in 0..6 {
            let idx = trimmed_count + 10 + i;
            assert_eq!(last_node, pin_map.get(idx).unwrap().node);
        }
        t.deinit(Global);
    }

    // test "PageList VT spanning two pages"
    #[cfg_attr(
        all(miri, debug_assertions),
        ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): `Page::assert_integrity` is O(page) and runs on nearly every PageList mutation under `debug_assertions`, which puts this test out of Miri's budget in the debug-assertions-ON G6 lane. It RUNS there — UB-clean and leak-clean — in the G6c lane (`check_leaks.sh --debug-assertions-off`), measured per test in single-worker isolation; see the WS19 T6 per-test table. Do NOT widen this to a plain `miri` ignore: that would silently drop the test from BOTH lanes."
    )]
    #[test]
    fn pagelist_vt_spanning_two_pages() {
        let mut builder: Vec<u8> = Vec::new();
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        let first_page_rows =
            unsafe { crate::pagelist::Node::capacity((*t.screens.active).pages.pages.first).rows };
        {
            let mut s = t.vt_stream();
            // Fill the first page almost completely
            for _ in 0..first_page_rows - 1 {
                s.next_slice(b"\r\n");
            }
            s.next_slice(b"\x1b[1mpage one");
        }
        // PORT NOTE (WS19 T6) — RE-DERIVE, NEVER CACHE. `Terminal::active_mut()`
        // (`&mut *self.screens.active`) is a Unique retag over the whole `Screen`
        // allocation, so it pops any raw pointer derived from that allocation
        // earlier; reading through a cached one afterwards is Stacked-Borrows UB
        // (memory: rust-mut-args-strongly-protected). This test used to cache it
        // across its `vt_stream()`/search writes. The violation was real and
        // deterministic, and had never been observed because a blanket
        // "multi-page PageList intractable" annotation kept the test out of Miri
        // entirely — found the moment the G6c (`-C debug-assertions=off`) lane
        // made it runnable. Test-code only: no production path caches this.
        let active_ptr = t.screens.active;
        let pages_ptr = || -> *const PageList { unsafe { &raw const (*active_ptr).pages } };
        // Verify we're still on one page
        assert!(unsafe { core::ptr::eq((*pages_ptr()).pages.first, (*pages_ptr()).pages.last) });
        // Add one more newline to push content to a second page
        {
            let mut s = t.vt_stream();
            s.next_slice(b"\r\n");
        }
        assert!(unsafe { !core::ptr::eq((*pages_ptr()).pages.first, (*pages_ptr()).pages.last) });
        // New content is still styled
        {
            let mut s = t.vt_stream();
            s.next_slice(b"page two");
        }

        // Format the entire PageList with VT
        let mut pin_map: PinMapMap = PinMapMap::EMPTY;
        let mut formatter: PageListFormatter = PageListFormatter::init(pages_ptr(), Options::vt());
        formatter.pin_map = Some(PinMap {
            alloc: Global,
            map: &mut pin_map,
        });
        formatter.format(&mut builder).unwrap();
        let full_output = builder.as_slice();
        let output = trim_start(full_output, b"\r\n");
        assert_eq!(
            output,
            b"\x1b[0m\x1b[1mpage one\x1b[0m\r\n\x1b[0m\x1b[1mpage two\x1b[0m"
        );

        // Verify pin map
        assert_eq!(full_output.len(), pin_map.count());
        let first_node = unsafe { (*pages_ptr()).pages.first };
        let last_node = unsafe { (*pages_ptr()).pages.last };

        // Just verify we have entries for both pages in the pin map
        let mut first_count: usize = 0;
        let mut last_count: usize = 0;
        for byte_i in 0..pin_map.count() {
            let p = pin_map.get(byte_i).unwrap();
            if p.node == first_node {
                first_count += 1;
            }
            if p.node == last_node {
                last_count += 1;
            }
        }
        assert!(first_count > 0);
        assert!(last_count > 0);
        t.deinit(Global);
    }

    // test "PageList plain with x offset on single page"
    #[test]
    fn pagelist_plain_with_x_offset_on_single_page() {
        let mut builder: Vec<u8> = Vec::new();
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        {
            let mut s = t.vt_stream();
            s.next_slice(b"hello world\r\ntest case\r\nfoo bar");
        }
        let pages_ptr: *const PageList = unsafe { &(*t.screens.active).pages };
        let node = unsafe { (*pages_ptr).pages.first };
        let mut pin_map: PinMapMap = PinMapMap::EMPTY;
        let mut formatter: PageListFormatter = PageListFormatter::init(pages_ptr, Options::plain());
        formatter.top_left = Some(pin(node, 6, 0));
        formatter.bottom_right = Some(pin(node, 2, 2));
        formatter.pin_map = Some(PinMap {
            alloc: Global,
            map: &mut pin_map,
        });
        formatter.format(&mut builder).unwrap();
        let output = builder.as_slice();
        assert_eq!(output, b"world\ntest case\nfoo");

        // Verify pin map
        assert_eq!(output.len(), pin_map.count());
        for byte_i in 0..pin_map.count() {
            let p = pin_map.get(byte_i).unwrap();
            assert_eq!(node, p.node);
        }

        // "world" starts at x=6, y=0
        for i in 0..5 {
            assert_eq!((6 + i) as size::CellCountInt, pin_map.get(i).unwrap().x);
            assert_eq!(0, pin_map.get(i).unwrap().y);
        }
        t.deinit(Global);
    }

    // test "PageList plain with x offset spanning two pages"
    #[cfg_attr(
        all(miri, debug_assertions),
        ignore = "MIRI EXCEPTION — G6 LANE ONLY (.port/phase-2/MIRI_EXCEPTIONS.md, 'WS19 T6 — the two-lane re-measurement'): `Page::assert_integrity` is O(page) and runs on nearly every PageList mutation under `debug_assertions`, which puts this test out of Miri's budget in the debug-assertions-ON G6 lane. It RUNS there — UB-clean and leak-clean — in the G6c lane (`check_leaks.sh --debug-assertions-off`), measured per test in single-worker isolation; see the WS19 T6 per-test table. Do NOT widen this to a plain `miri` ignore: that would silently drop the test from BOTH lanes."
    )]
    #[test]
    fn pagelist_plain_with_x_offset_spanning_two_pages() {
        let mut builder: Vec<u8> = Vec::new();
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        let first_page_rows =
            unsafe { crate::pagelist::Node::capacity((*t.screens.active).pages.pages.first).rows };
        {
            let mut s = t.vt_stream();
            // Fill first page almost completely
            for _ in 0..first_page_rows - 1 {
                s.next_slice(b"\r\n");
            }
            s.next_slice(b"hello world");
        }
        // PORT NOTE (WS19 T6) — RE-DERIVE, NEVER CACHE. `Terminal::active_mut()`
        // (`&mut *self.screens.active`) is a Unique retag over the whole `Screen`
        // allocation, so it pops any raw pointer derived from that allocation
        // earlier; reading through a cached one afterwards is Stacked-Borrows UB
        // (memory: rust-mut-args-strongly-protected). This test used to cache it
        // across its `vt_stream()`/search writes. The violation was real and
        // deterministic, and had never been observed because a blanket
        // "multi-page PageList intractable" annotation kept the test out of Miri
        // entirely — found the moment the G6c (`-C debug-assertions=off`) lane
        // made it runnable. Test-code only: no production path caches this.
        let active_ptr = t.screens.active;
        let pages_ptr = || -> *const PageList { unsafe { &raw const (*active_ptr).pages } };
        // Verify we're still on one page
        assert!(unsafe { core::ptr::eq((*pages_ptr()).pages.first, (*pages_ptr()).pages.last) });
        // Push to second page
        {
            let mut s = t.vt_stream();
            s.next_slice(b"\r\n");
        }
        assert!(unsafe { !core::ptr::eq((*pages_ptr()).pages.first, (*pages_ptr()).pages.last) });
        {
            let mut s = t.vt_stream();
            s.next_slice(b"foo bar test");
        }
        let first_node = unsafe { (*pages_ptr()).pages.first };
        let last_node = unsafe { (*pages_ptr()).pages.last };
        let first_node_size_rows = unsafe { crate::pagelist::Node::rows(first_node) };

        let mut pin_map: PinMapMap = PinMapMap::EMPTY;
        let mut formatter: PageListFormatter =
            PageListFormatter::init(pages_ptr(), Options::plain());
        formatter.top_left = Some(pin(first_node, 6, first_node_size_rows - 1));
        formatter.bottom_right = Some(pin(last_node, 2, 1));
        formatter.pin_map = Some(PinMap {
            alloc: Global,
            map: &mut pin_map,
        });
        formatter.format(&mut builder).unwrap();
        let full_output = builder.as_slice();
        let output = trim_start(full_output, b"\n");
        assert_eq!(output, b"world\nfoo");

        // Verify pin map
        assert_eq!(full_output.len(), pin_map.count());
        let trimmed_count = full_output.len() - output.len();

        // "world" (5 chars) from first page
        for i in 0..5 {
            let idx = trimmed_count + i;
            assert_eq!(first_node, pin_map.get(idx).unwrap().node);
            assert_eq!((6 + i) as size::CellCountInt, pin_map.get(idx).unwrap().x);
        }

        // \n - maps to last node as it represents the transition to new page
        assert_eq!(last_node, pin_map.get(trimmed_count + 5).unwrap().node);

        // "foo" (3 chars) from last page
        for i in 0..3 {
            let idx = trimmed_count + 6 + i;
            assert_eq!(last_node, pin_map.get(idx).unwrap().node);
            assert_eq!(i as size::CellCountInt, pin_map.get(idx).unwrap().x);
        }
        t.deinit(Global);
    }

    // test "PageList plain with start_x only"
    #[test]
    fn pagelist_plain_with_start_x_only() {
        let mut builder: Vec<u8> = Vec::new();
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        {
            let mut s = t.vt_stream();
            s.next_slice(b"hello world");
        }
        let pages_ptr: *const PageList = unsafe { &(*t.screens.active).pages };
        let node = unsafe { (*pages_ptr).pages.first };
        let mut pin_map: PinMapMap = PinMapMap::EMPTY;
        let mut formatter: PageListFormatter = PageListFormatter::init(pages_ptr, Options::plain());
        formatter.top_left = Some(pin(node, 6, 0));
        formatter.pin_map = Some(PinMap {
            alloc: Global,
            map: &mut pin_map,
        });
        formatter.format(&mut builder).unwrap();
        let output = builder.as_slice();
        assert_eq!(output, b"world");

        // Verify pin map
        assert_eq!(output.len(), pin_map.count());
        for i in 0..5 {
            assert_eq!(node, pin_map.get(i).unwrap().node);
            assert_eq!((6 + i) as size::CellCountInt, pin_map.get(i).unwrap().x);
            assert_eq!(0, pin_map.get(i).unwrap().y);
        }
        t.deinit(Global);
    }

    // test "PageList plain with end_x only"
    #[test]
    fn pagelist_plain_with_end_x_only() {
        let mut builder: Vec<u8> = Vec::new();
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        {
            let mut s = t.vt_stream();
            s.next_slice(b"hello world\r\ntest");
        }
        let pages_ptr: *const PageList = unsafe { &(*t.screens.active).pages };
        let node = unsafe { (*pages_ptr).pages.first };
        let mut pin_map: PinMapMap = PinMapMap::EMPTY;
        let mut formatter: PageListFormatter = PageListFormatter::init(pages_ptr, Options::plain());
        formatter.bottom_right = Some(pin(node, 2, 1));
        formatter.pin_map = Some(PinMap {
            alloc: Global,
            map: &mut pin_map,
        });
        formatter.format(&mut builder).unwrap();
        let output = builder.as_slice();
        assert_eq!(output, b"hello world\ntes");

        // Verify pin map
        assert_eq!(output.len(), pin_map.count());

        // "hello world" (11 chars) on y=0
        for i in 0..11 {
            assert_eq!(node, pin_map.get(i).unwrap().node);
            assert_eq!(i as size::CellCountInt, pin_map.get(i).unwrap().x);
            assert_eq!(0, pin_map.get(i).unwrap().y);
        }

        // \n
        assert_eq!(node, pin_map.get(11).unwrap().node);

        // "tes" (3 chars) on y=1
        for i in 0..3 {
            assert_eq!(node, pin_map.get(12 + i).unwrap().node);
            assert_eq!(i as size::CellCountInt, pin_map.get(12 + i).unwrap().x);
            assert_eq!(1, pin_map.get(12 + i).unwrap().y);
        }
        t.deinit(Global);
    }

    // test "PageList plain rectangle basic"
    #[test]
    fn pagelist_plain_rectangle_basic() {
        let mut builder: Vec<u8> = Vec::new();
        let mut t = Terminal::init(Global, opts(30, 5)).unwrap();
        {
            let mut s = t.vt_stream();
            s.next_slice(b"Lorem ipsum dolor\r\n");
            s.next_slice(b"sit amet, consectetur\r\n");
            s.next_slice(b"adipiscing elit, sed do\r\n");
            s.next_slice(b"eiusmod tempor incididunt\r\n");
            s.next_slice(b"ut labore et dolore");
        }
        let pages_ptr: *const PageList = unsafe { &(*t.screens.active).pages };
        let mut formatter: PageListFormatter = PageListFormatter::init(pages_ptr, Options::plain());
        formatter.top_left = unsafe { (*pages_ptr).pin(crate::point::Point::Screen(coord(2, 1))) };
        formatter.bottom_right =
            unsafe { (*pages_ptr).pin(crate::point::Point::Screen(coord(6, 3))) };
        formatter.rectangle = true;
        formatter.format(&mut builder).unwrap();
        let output = builder.as_slice();
        let expected = "t ame\nipisc\nusmod";
        assert_eq!(output, expected.as_bytes());
        t.deinit(Global);
    }

    // test "PageList plain rectangle with EOL"
    #[test]
    fn pagelist_plain_rectangle_with_eol() {
        let mut builder: Vec<u8> = Vec::new();
        let mut t = Terminal::init(Global, opts(30, 5)).unwrap();
        {
            let mut s = t.vt_stream();
            s.next_slice(b"Lorem ipsum dolor\r\n");
            s.next_slice(b"sit amet, consectetur\r\n");
            s.next_slice(b"adipiscing elit, sed do\r\n");
            s.next_slice(b"eiusmod tempor incididunt\r\n");
            s.next_slice(b"ut labore et dolore");
        }
        let pages_ptr: *const PageList = unsafe { &(*t.screens.active).pages };
        let mut formatter: PageListFormatter = PageListFormatter::init(pages_ptr, Options::plain());
        formatter.top_left = unsafe { (*pages_ptr).pin(crate::point::Point::Screen(coord(12, 0))) };
        formatter.bottom_right =
            unsafe { (*pages_ptr).pin(crate::point::Point::Screen(coord(26, 4))) };
        formatter.rectangle = true;
        formatter.format(&mut builder).unwrap();
        let output = builder.as_slice();
        let expected = "dolor\nnsectetur\nlit, sed do\nor incididunt\n dolore";
        assert_eq!(output, expected.as_bytes());
        t.deinit(Global);
    }

    // test "PageList plain rectangle more complex with breaks"
    #[test]
    fn pagelist_plain_rectangle_more_complex_with_breaks() {
        let mut builder: Vec<u8> = Vec::new();
        let mut t = Terminal::init(Global, opts(30, 8)).unwrap();
        {
            let mut s = t.vt_stream();
            s.next_slice(b"Lorem ipsum dolor\r\n");
            s.next_slice(b"sit amet, consectetur\r\n");
            s.next_slice(b"adipiscing elit, sed do\r\n");
            s.next_slice(b"eiusmod tempor incididunt\r\n");
            s.next_slice(b"ut labore et dolore\r\n");
            s.next_slice(b"\r\n");
            s.next_slice(b"magna aliqua. Ut enim\r\n");
            s.next_slice(b"ad minim veniam, quis");
        }
        let pages_ptr: *const PageList = unsafe { &(*t.screens.active).pages };
        let mut formatter: PageListFormatter = PageListFormatter::init(pages_ptr, Options::plain());
        formatter.top_left = unsafe { (*pages_ptr).pin(crate::point::Point::Screen(coord(11, 2))) };
        formatter.bottom_right =
            unsafe { (*pages_ptr).pin(crate::point::Point::Screen(coord(26, 7))) };
        formatter.rectangle = true;
        formatter.format(&mut builder).unwrap();
        let output = builder.as_slice();
        let expected = "elit, sed do\npor incididunt\nt dolore\n\na. Ut enim\nniam, quis";
        assert_eq!(output, expected.as_bytes());
        t.deinit(Global);
    }

    // test "TerminalFormatter plain no selection"
    #[test]
    fn terminalformatter_plain_no_selection() {
        let mut builder: Vec<u8> = Vec::new();
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        {
            let mut s = t.vt_stream();
            s.next_slice(b"hello\r\nworld");
        }
        let t_ptr: *const Terminal = &t;
        let formatter: TerminalFormatter = TerminalFormatter::init(t_ptr, Options::plain());
        formatter.format(&mut builder).unwrap();
        assert_eq!(builder.as_slice(), b"hello\nworld");
        t.deinit(Global);
    }

    // test "TerminalFormatter vt with palette"
    #[test]
    fn terminalformatter_vt_with_palette() {
        let mut builder: Vec<u8> = Vec::new();
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        {
            let mut s = t.vt_stream();
            // Modify some palette colors using VT sequences
            s.next_slice(b"\x1b]4;0;rgb:12/34/56\x1b\\");
            s.next_slice(b"\x1b]4;1;rgb:ab/cd/ef\x1b\\");
            s.next_slice(b"\x1b]4;255;rgb:ff/00/ff\x1b\\");
            s.next_slice(b"test");
        }
        let t_ptr: *const Terminal = &t;
        let formatter: TerminalFormatter = TerminalFormatter::init(t_ptr, Options::vt());
        formatter.format(&mut builder).unwrap();
        let output = builder.clone();

        // Create a second terminal and apply the output
        let mut t2 = Terminal::init(Global, opts(80, 24)).unwrap();
        {
            let mut s2 = t2.vt_stream();
            s2.next_slice(output.as_slice());
        }

        // Verify the palettes match
        assert_eq!(t.colors.palette.current[0], t2.colors.palette.current[0]);
        assert_eq!(t.colors.palette.current[1], t2.colors.palette.current[1]);
        assert_eq!(
            t.colors.palette.current[255],
            t2.colors.palette.current[255]
        );
        t.deinit(Global);
        t2.deinit(Global);
    }

    // test "TerminalFormatter with selection"
    #[test]
    fn terminalformatter_with_selection() {
        let mut builder: Vec<u8> = Vec::new();
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        {
            let mut s = t.vt_stream();
            s.next_slice(b"line1\r\nline2\r\nline3");
        }
        let pages = unsafe { &(*t.screens.active).pages };
        let tl = pages.pin(crate::point::Point::Active(coord(0, 1))).unwrap();
        let br = pages.pin(crate::point::Point::Active(coord(4, 1))).unwrap();
        let t_ptr: *const Terminal = &t;
        let mut formatter: TerminalFormatter = TerminalFormatter::init(t_ptr, Options::plain());
        formatter.content = Content::Selection(Some(Selection::init(tl, br, false)));
        formatter.format(&mut builder).unwrap();
        assert_eq!(builder.as_slice(), b"line2");
        t.deinit(Global);
    }

    // test "TerminalFormatter plain with pin_map"
    #[test]
    fn terminalformatter_plain_with_pin_map() {
        let mut builder: Vec<u8> = Vec::new();
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        {
            let mut s = t.vt_stream();
            s.next_slice(b"hello, world");
        }
        let mut pin_map: PinMapMap = PinMapMap::EMPTY;
        let t_ptr: *const Terminal = &t;
        let mut formatter: TerminalFormatter = TerminalFormatter::init(t_ptr, Options::plain());
        formatter.pin_map = Some(PinMap {
            alloc: Global,
            map: &mut pin_map,
        });
        formatter.format(&mut builder).unwrap();
        let output = builder.as_slice();
        assert_eq!(output, b"hello, world");

        // Verify pin map
        assert_eq!(output.len(), pin_map.count());
        let node = unsafe { (*t.screens.active).pages.pages.first };
        for i in 0..output.len() {
            assert_eq!(
                pin(node, i as size::CellCountInt, 0),
                pin_map.get(i).unwrap()
            );
        }
        t.deinit(Global);
    }

    // test "TerminalFormatter plain multiline with pin_map"
    #[test]
    fn terminalformatter_plain_multiline_with_pin_map() {
        let mut builder: Vec<u8> = Vec::new();
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        {
            let mut s = t.vt_stream();
            s.next_slice(b"hello\r\nworld");
        }
        let mut pin_map: PinMapMap = PinMapMap::EMPTY;
        let t_ptr: *const Terminal = &t;
        let mut formatter: TerminalFormatter = TerminalFormatter::init(t_ptr, Options::plain());
        formatter.pin_map = Some(PinMap {
            alloc: Global,
            map: &mut pin_map,
        });
        formatter.format(&mut builder).unwrap();
        let output = builder.as_slice();
        assert_eq!(output, b"hello\nworld");

        // Verify pin map
        assert_eq!(output.len(), pin_map.count());
        let node = unsafe { (*t.screens.active).pages.pages.first };
        // "hello" (5 chars)
        for i in 0..5 {
            assert_eq!(node, pin_map.get(i).unwrap().node);
            assert_eq!(i as size::CellCountInt, pin_map.get(i).unwrap().x);
            assert_eq!(0, pin_map.get(i).unwrap().y);
        }
        // "\n" maps to end of first line
        assert_eq!(node, pin_map.get(5).unwrap().node);
        // "world" (5 chars)
        for i in 0..5 {
            let idx = 6 + i;
            assert_eq!(node, pin_map.get(idx).unwrap().node);
            assert_eq!(i as size::CellCountInt, pin_map.get(idx).unwrap().x);
            assert_eq!(1, pin_map.get(idx).unwrap().y);
        }
        t.deinit(Global);
    }

    // test "TerminalFormatter vt with palette and pin_map"
    #[test]
    fn terminalformatter_vt_with_palette_and_pin_map() {
        let mut builder: Vec<u8> = Vec::new();
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        {
            let mut s = t.vt_stream();
            // Modify some palette colors using VT sequences
            s.next_slice(b"\x1b]4;0;rgb:12/34/56\x1b\\");
            s.next_slice(b"test");
        }
        let mut pin_map: PinMapMap = PinMapMap::EMPTY;
        let t_ptr: *const Terminal = &t;
        let mut formatter: TerminalFormatter = TerminalFormatter::init(t_ptr, Options::vt());
        formatter.pin_map = Some(PinMap {
            alloc: Global,
            map: &mut pin_map,
        });
        formatter.format(&mut builder).unwrap();
        let output = builder.as_slice();

        // Verify pin map - palette bytes should be mapped to top left
        assert_eq!(output.len(), pin_map.count());
        let node = unsafe { (*t.screens.active).pages.pages.first };
        for i in 0..output.len() {
            assert_eq!(node, pin_map.get(i).unwrap().node);
        }
        t.deinit(Global);
    }

    // test "TerminalFormatter with selection and pin_map"
    #[test]
    fn terminalformatter_with_selection_and_pin_map() {
        let mut builder: Vec<u8> = Vec::new();
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        {
            let mut s = t.vt_stream();
            s.next_slice(b"line1\r\nline2\r\nline3");
        }
        let mut pin_map: PinMapMap = PinMapMap::EMPTY;
        let pages = unsafe { &(*t.screens.active).pages };
        let tl = pages.pin(crate::point::Point::Active(coord(0, 1))).unwrap();
        let br = pages.pin(crate::point::Point::Active(coord(4, 1))).unwrap();
        let t_ptr: *const Terminal = &t;
        let mut formatter: TerminalFormatter = TerminalFormatter::init(t_ptr, Options::plain());
        formatter.content = Content::Selection(Some(Selection::init(tl, br, false)));
        formatter.pin_map = Some(PinMap {
            alloc: Global,
            map: &mut pin_map,
        });
        formatter.format(&mut builder).unwrap();
        let output = builder.as_slice();
        assert_eq!(output, b"line2");

        // Verify pin map
        assert_eq!(output.len(), pin_map.count());
        let node = unsafe { (*t.screens.active).pages.pages.first };
        // "line2" (5 chars) from row 1
        for i in 0..5 {
            assert_eq!(node, pin_map.get(i).unwrap().node);
            assert_eq!(i as size::CellCountInt, pin_map.get(i).unwrap().x);
            assert_eq!(1, pin_map.get(i).unwrap().y);
        }
        t.deinit(Global);
    }

    // test "Screen plain single line"
    #[test]
    fn screen_plain_single_line() {
        let mut builder: Vec<u8> = Vec::new();
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        {
            let mut s = t.vt_stream();
            s.next_slice(b"hello, world");
        }
        let mut pin_map: PinMapMap = PinMapMap::EMPTY;
        let screen_ptr: *const Screen = t.screens.active;
        let mut formatter: ScreenFormatter = ScreenFormatter::init(screen_ptr, Options::plain());
        formatter.pin_map = Some(PinMap {
            alloc: Global,
            map: &mut pin_map,
        });
        formatter.format(&mut builder).unwrap();
        let output = builder.as_slice();
        assert_eq!(output, b"hello, world");

        // Verify pin map
        assert_eq!(output.len(), pin_map.count());
        let node = unsafe { (*t.screens.active).pages.pages.first };
        for i in 0..output.len() {
            assert_eq!(
                pin(node, i as size::CellCountInt, 0),
                pin_map.get(i).unwrap()
            );
        }
        t.deinit(Global);
    }

    // test "Screen plain multiline"
    #[test]
    fn screen_plain_multiline() {
        let mut builder: Vec<u8> = Vec::new();
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        {
            let mut s = t.vt_stream();
            s.next_slice(b"hello\r\nworld");
        }
        let mut pin_map: PinMapMap = PinMapMap::EMPTY;
        let screen_ptr: *const Screen = t.screens.active;
        let mut formatter: ScreenFormatter = ScreenFormatter::init(screen_ptr, Options::plain());
        formatter.pin_map = Some(PinMap {
            alloc: Global,
            map: &mut pin_map,
        });
        formatter.format(&mut builder).unwrap();
        let output = builder.as_slice();
        assert_eq!(output, b"hello\nworld");

        // Verify pin map
        assert_eq!(output.len(), pin_map.count());
        let node = unsafe { (*t.screens.active).pages.pages.first };
        // "hello" (5 chars)
        for i in 0..5 {
            assert_eq!(node, pin_map.get(i).unwrap().node);
            assert_eq!(i as size::CellCountInt, pin_map.get(i).unwrap().x);
            assert_eq!(0, pin_map.get(i).unwrap().y);
        }
        // "\n" maps to end of first line
        assert_eq!(node, pin_map.get(5).unwrap().node);
        // "world" (5 chars)
        for i in 0..5 {
            let idx = 6 + i;
            assert_eq!(node, pin_map.get(idx).unwrap().node);
            assert_eq!(i as size::CellCountInt, pin_map.get(idx).unwrap().x);
            assert_eq!(1, pin_map.get(idx).unwrap().y);
        }
        t.deinit(Global);
    }

    // test "Screen plain with selection"
    #[test]
    fn screen_plain_with_selection() {
        let mut builder: Vec<u8> = Vec::new();
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        {
            let mut s = t.vt_stream();
            s.next_slice(b"line1\r\nline2\r\nline3");
        }
        let mut pin_map: PinMapMap = PinMapMap::EMPTY;
        let pages = unsafe { &(*t.screens.active).pages };
        let tl = pages.pin(crate::point::Point::Active(coord(0, 1))).unwrap();
        let br = pages.pin(crate::point::Point::Active(coord(4, 1))).unwrap();
        let screen_ptr: *const Screen = t.screens.active;
        let mut formatter: ScreenFormatter = ScreenFormatter::init(screen_ptr, Options::plain());
        formatter.content = Content::Selection(Some(Selection::init(tl, br, false)));
        formatter.pin_map = Some(PinMap {
            alloc: Global,
            map: &mut pin_map,
        });
        formatter.format(&mut builder).unwrap();
        let output = builder.as_slice();
        assert_eq!(output, b"line2");

        // Verify pin map
        assert_eq!(output.len(), pin_map.count());
        let node = unsafe { (*t.screens.active).pages.pages.first };
        // "line2" (5 chars) from row 1
        for i in 0..5 {
            assert_eq!(node, pin_map.get(i).unwrap().node);
            assert_eq!(i as size::CellCountInt, pin_map.get(i).unwrap().x);
            assert_eq!(1, pin_map.get(i).unwrap().y);
        }
        t.deinit(Global);
    }

    // test "Screen vt with cursor position"
    #[test]
    fn screen_vt_with_cursor_position() {
        let mut builder: Vec<u8> = Vec::new();
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        // Position cursor at a specific location
        {
            let mut s = t.vt_stream();
            s.next_slice(b"hello\r\nworld");
        }
        let mut pin_map: PinMapMap = PinMapMap::EMPTY;
        let screen_ptr: *const Screen = t.screens.active;
        let mut formatter: ScreenFormatter = ScreenFormatter::init(screen_ptr, Options::vt());
        formatter.extra.cursor = true;
        formatter.pin_map = Some(PinMap {
            alloc: Global,
            map: &mut pin_map,
        });
        formatter.format(&mut builder).unwrap();
        let output = builder.clone();

        // Create a second terminal and apply the output
        let mut t2 = Terminal::init(Global, opts(80, 24)).unwrap();
        {
            let mut s2 = t2.vt_stream();
            s2.next_slice(output.as_slice());
        }

        // Verify cursor positions match
        assert_eq!(unsafe { (*t.screens.active).cursor.x }, unsafe {
            (*t2.screens.active).cursor.x
        });
        assert_eq!(unsafe { (*t.screens.active).cursor.y }, unsafe {
            (*t2.screens.active).cursor.y
        });

        // Verify pin map - the extras should be mapped to the last pin
        assert_eq!(output.len(), pin_map.count());
        let node = unsafe { (*t.screens.active).pages.pages.first };
        let content_len = b"hello\r\nworld".len();
        // Content bytes map to their positions
        for i in 0..content_len {
            assert_eq!(node, pin_map.get(i).unwrap().node);
        }
        // Extra bytes (cursor position) map to last content pin
        for i in content_len..output.len() {
            assert_eq!(node, pin_map.get(i).unwrap().node);
        }
        t.deinit(Global);
        t2.deinit(Global);
    }

    // test "Terminal vt cursor preserves pending wrap"
    #[test]
    fn terminal_vt_cursor_preserves_pending_wrap() {
        let mut builder: Vec<u8> = Vec::new();

        let mut source = Terminal::init(Global, opts(4, 2)).unwrap();
        {
            let mut source_stream = source.vt_stream();
            source_stream.next_slice(b"abcd");
        }
        assert!(unsafe { (*source.screens.active).cursor.pending_wrap });

        let mut pin_map: PinMapMap = PinMapMap::EMPTY;

        let source_ptr: *const Terminal = &source;
        let mut formatter: TerminalFormatter = TerminalFormatter::init(source_ptr, Options::vt());
        formatter.extra = TerminalExtra::NONE;
        formatter.extra.screen.cursor = true;
        formatter.pin_map = Some(PinMap {
            alloc: Global,
            map: &mut pin_map,
        });
        formatter.format(&mut builder).unwrap();
        assert_eq!(builder.len(), pin_map.count());

        let mut target = Terminal::init(Global, opts(4, 2)).unwrap();
        {
            let mut target_stream = target.vt_stream();
            target_stream.next_slice(builder.as_slice());
        }

        assert_eq!(
            unsafe { (*source.screens.active).cursor.pending_wrap },
            unsafe { (*target.screens.active).cursor.pending_wrap },
        );

        {
            let mut source_stream = source.vt_stream();
            source_stream.next_slice(b"X");
        }
        {
            let mut target_stream = target.vt_stream();
            target_stream.next_slice(b"X");
        }
        let source_contents = unsafe { &(*source.screens.active) }
            .dump_string_alloc(
                Global,
                crate::point::Point::Screen(crate::point::Coordinate::default()),
            )
            .unwrap();
        let target_contents = unsafe { &(*target.screens.active) }
            .dump_string_alloc(
                Global,
                crate::point::Point::Screen(crate::point::Coordinate::default()),
            )
            .unwrap();
        assert_eq!(source_contents, target_contents);
        source.deinit(Global);
        target.deinit(Global);
    }

    // test "Screen vt with style"
    #[test]
    fn screen_vt_with_style() {
        let mut builder: Vec<u8> = Vec::new();
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        // Set some style attributes
        {
            let mut s = t.vt_stream();
            s.next_slice(b"\x1b[1;31mhello");
        }
        let mut pin_map: PinMapMap = PinMapMap::EMPTY;
        let screen_ptr: *const Screen = t.screens.active;
        let mut formatter: ScreenFormatter = ScreenFormatter::init(screen_ptr, Options::vt());
        formatter.extra.style = true;
        formatter.pin_map = Some(PinMap {
            alloc: Global,
            map: &mut pin_map,
        });
        formatter.format(&mut builder).unwrap();
        let output = builder.clone();

        // Create a second terminal and apply the output
        let mut t2 = Terminal::init(Global, opts(80, 24)).unwrap();
        {
            let mut s2 = t2.vt_stream();
            s2.next_slice(output.as_slice());
        }

        // Verify styles match
        assert!(unsafe { (*t.screens.active).cursor.style }
            .eql(&unsafe { (*t2.screens.active).cursor.style }));

        // Verify pin map
        assert_eq!(output.len(), pin_map.count());
        let node = unsafe { (*t.screens.active).pages.pages.first };
        for i in 0..output.len() {
            assert_eq!(node, pin_map.get(i).unwrap().node);
        }
        t.deinit(Global);
        t2.deinit(Global);
    }

    // test "Screen vt with hyperlink"
    #[test]
    fn screen_vt_with_hyperlink() {
        let mut builder: Vec<u8> = Vec::new();
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        // Set a hyperlink
        {
            let mut s = t.vt_stream();
            s.next_slice(b"\x1b]8;;http://example.com\x1b\\hello");
        }
        let mut pin_map: PinMapMap = PinMapMap::EMPTY;
        let screen_ptr: *const Screen = t.screens.active;
        let mut formatter: ScreenFormatter = ScreenFormatter::init(screen_ptr, Options::vt());
        formatter.extra.hyperlink = true;
        formatter.pin_map = Some(PinMap {
            alloc: Global,
            map: &mut pin_map,
        });
        formatter.format(&mut builder).unwrap();
        let output = builder.clone();

        // Create a second terminal and apply the output
        let mut t2 = Terminal::init(Global, opts(80, 24)).unwrap();
        {
            let mut s2 = t2.vt_stream();
            s2.next_slice(output.as_slice());
        }

        // Verify hyperlinks match
        let has_link1 = unsafe { (*t.screens.active).cursor.hyperlink }.is_some();
        let has_link2 = unsafe { (*t2.screens.active).cursor.hyperlink }.is_some();
        assert_eq!(has_link1, has_link2);

        if has_link1 {
            let link1 = unsafe { &*(*t.screens.active).cursor.hyperlink.unwrap() };
            let link2 = unsafe { &*(*t2.screens.active).cursor.hyperlink.unwrap() };
            assert_eq!(link1.uri, link2.uri);
        }

        // Verify pin map
        assert_eq!(output.len(), pin_map.count());
        let node = unsafe { (*t.screens.active).pages.pages.first };
        for i in 0..output.len() {
            assert_eq!(node, pin_map.get(i).unwrap().node);
        }
        t.deinit(Global);
        t2.deinit(Global);
    }

    // test "Screen vt with protection"
    #[test]
    fn screen_vt_with_protection() {
        let mut builder: Vec<u8> = Vec::new();
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        // Enable protection mode
        {
            let mut s = t.vt_stream();
            s.next_slice(b"\x1b[1\"qhello");
        }
        let mut pin_map: PinMapMap = PinMapMap::EMPTY;
        let screen_ptr: *const Screen = t.screens.active;
        let mut formatter: ScreenFormatter = ScreenFormatter::init(screen_ptr, Options::vt());
        formatter.extra.protection = true;
        formatter.pin_map = Some(PinMap {
            alloc: Global,
            map: &mut pin_map,
        });
        formatter.format(&mut builder).unwrap();
        let output = builder.clone();

        // Create a second terminal and apply the output
        let mut t2 = Terminal::init(Global, opts(80, 24)).unwrap();
        {
            let mut s2 = t2.vt_stream();
            s2.next_slice(output.as_slice());
        }

        // Verify protection state matches
        assert_eq!(unsafe { (*t.screens.active).cursor.protected }, unsafe {
            (*t2.screens.active).cursor.protected
        });

        // Verify pin map
        assert_eq!(output.len(), pin_map.count());
        let node = unsafe { (*t.screens.active).pages.pages.first };
        for i in 0..output.len() {
            assert_eq!(node, pin_map.get(i).unwrap().node);
        }
        t.deinit(Global);
        t2.deinit(Global);
    }

    // test "Screen vt with kitty keyboard"
    #[test]
    fn screen_vt_with_kitty_keyboard() {
        let mut builder: Vec<u8> = Vec::new();
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        // Set kitty keyboard flags (disambiguate + report_events = 3)
        {
            let mut s = t.vt_stream();
            s.next_slice(b"\x1b[=3;1uhello");
        }
        let mut pin_map: PinMapMap = PinMapMap::EMPTY;
        let screen_ptr: *const Screen = t.screens.active;
        let mut formatter: ScreenFormatter = ScreenFormatter::init(screen_ptr, Options::vt());
        formatter.extra.kitty_keyboard = true;
        formatter.pin_map = Some(PinMap {
            alloc: Global,
            map: &mut pin_map,
        });
        formatter.format(&mut builder).unwrap();
        let output = builder.clone();

        // Create a second terminal and apply the output
        let mut t2 = Terminal::init(Global, opts(80, 24)).unwrap();
        {
            let mut s2 = t2.vt_stream();
            s2.next_slice(output.as_slice());
        }

        // Verify kitty keyboard state matches
        let flags1 = unsafe { (*t.screens.active).kitty_keyboard.current() }.int();
        let flags2 = unsafe { (*t2.screens.active).kitty_keyboard.current() }.int();
        assert_eq!(flags1, flags2);

        // Verify pin map
        assert_eq!(output.len(), pin_map.count());
        let node = unsafe { (*t.screens.active).pages.pages.first };
        for i in 0..output.len() {
            assert_eq!(node, pin_map.get(i).unwrap().node);
        }
        t.deinit(Global);
        t2.deinit(Global);
    }

    // test "Screen vt with charsets"
    #[test]
    fn screen_vt_with_charsets() {
        let mut builder: Vec<u8> = Vec::new();
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        // Set G0 to DEC special and shift to G1
        {
            let mut s = t.vt_stream();
            s.next_slice(b"\x1b(0\x0ehello");
        }
        let mut pin_map: PinMapMap = PinMapMap::EMPTY;
        let screen_ptr: *const Screen = t.screens.active;
        let mut formatter: ScreenFormatter = ScreenFormatter::init(screen_ptr, Options::vt());
        formatter.extra.charsets = true;
        formatter.pin_map = Some(PinMap {
            alloc: Global,
            map: &mut pin_map,
        });
        formatter.format(&mut builder).unwrap();
        let output = builder.clone();

        // Create a second terminal and apply the output
        let mut t2 = Terminal::init(Global, opts(80, 24)).unwrap();
        {
            let mut s2 = t2.vt_stream();
            s2.next_slice(output.as_slice());
        }

        // Verify charset state matches
        assert_eq!(unsafe { (*t.screens.active).charset.gl }, unsafe {
            (*t2.screens.active).charset.gl
        });
        assert_eq!(unsafe { (*t.screens.active).charset.gr }, unsafe {
            (*t2.screens.active).charset.gr
        });
        assert_eq!(
            unsafe {
                (*t.screens.active)
                    .charset
                    .charsets
                    .get(crate::charsets::Slots::G0)
            },
            unsafe {
                (*t2.screens.active)
                    .charset
                    .charsets
                    .get(crate::charsets::Slots::G0)
            }
        );

        // Verify pin map
        assert_eq!(output.len(), pin_map.count());
        let node = unsafe { (*t.screens.active).pages.pages.first };
        for i in 0..output.len() {
            assert_eq!(node, pin_map.get(i).unwrap().node);
        }
        t.deinit(Global);
        t2.deinit(Global);
    }

    // test "Terminal vt with scrolling region"
    #[test]
    fn terminal_vt_with_scrolling_region() {
        let mut builder: Vec<u8> = Vec::new();
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        // Set scrolling region: top=5, bottom=20
        {
            let mut s = t.vt_stream();
            s.next_slice(b"\x1b[6;21rhello");
        }
        let t_ptr: *const Terminal = &t;
        let mut formatter: TerminalFormatter = TerminalFormatter::init(t_ptr, Options::vt());
        formatter.extra.scrolling_region = true;
        formatter.format(&mut builder).unwrap();
        let output = builder.clone();

        // Create a second terminal and apply the output
        let mut t2 = Terminal::init(Global, opts(80, 24)).unwrap();
        {
            let mut s2 = t2.vt_stream();
            s2.next_slice(output.as_slice());
        }

        // Verify scrolling regions match
        assert_eq!(t.scrolling_region.top, t2.scrolling_region.top);
        assert_eq!(t.scrolling_region.bottom, t2.scrolling_region.bottom);
        assert_eq!(t.scrolling_region.left, t2.scrolling_region.left);
        assert_eq!(t.scrolling_region.right, t2.scrolling_region.right);
        t.deinit(Global);
        t2.deinit(Global);
    }

    // test "Terminal vt with modes"
    #[test]
    fn terminal_vt_with_modes() {
        let mut builder: Vec<u8> = Vec::new();
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        {
            let mut s = t.vt_stream();
            // Enable some modes that differ from defaults
            s.next_slice(b"\x1b[?2004h"); // Bracketed paste
            s.next_slice(b"\x1b[?1000h"); // Mouse event normal
            s.next_slice(b"\x1b[?7l"); // Disable wraparound (default is true)
            s.next_slice(b"hello");
        }
        let t_ptr: *const Terminal = &t;
        let mut formatter: TerminalFormatter = TerminalFormatter::init(t_ptr, Options::vt());
        formatter.extra.modes = true;
        formatter.format(&mut builder).unwrap();
        let output = builder.clone();

        // Create a second terminal and apply the output
        let mut t2 = Terminal::init(Global, opts(80, 24)).unwrap();
        {
            let mut s2 = t2.vt_stream();
            s2.next_slice(output.as_slice());
        }

        // Verify modes match
        assert_eq!(
            t.modes.get(crate::modes::Mode::BracketedPaste),
            t2.modes.get(crate::modes::Mode::BracketedPaste)
        );
        assert_eq!(
            t.modes.get(crate::modes::Mode::MouseEventNormal),
            t2.modes.get(crate::modes::Mode::MouseEventNormal)
        );
        assert_eq!(
            t.modes.get(crate::modes::Mode::Wraparound),
            t2.modes.get(crate::modes::Mode::Wraparound)
        );
        t.deinit(Global);
        t2.deinit(Global);
    }

    // test "Terminal vt with tabstops"
    #[test]
    fn terminal_vt_with_tabstops() {
        let mut builder: Vec<u8> = Vec::new();
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        {
            let mut s = t.vt_stream();
            // Clear all tabs and set custom tabstops
            s.next_slice(b"\x1b[3g"); // Clear all tabs
            s.next_slice(b"\x1b[5G\x1bH"); // Set tab at column 5
            s.next_slice(b"\x1b[15G\x1bH"); // Set tab at column 15
            s.next_slice(b"\x1b[30G\x1bH"); // Set tab at column 30
            s.next_slice(b"\x1b[Hhello");
        }
        let mut pin_map: PinMapMap = PinMapMap::EMPTY;
        let t_ptr: *const Terminal = &t;
        let mut formatter: TerminalFormatter = TerminalFormatter::init(t_ptr, Options::vt());
        formatter.extra.tabstops = true;
        formatter.extra.screen.cursor = true;
        formatter.pin_map = Some(PinMap {
            alloc: Global,
            map: &mut pin_map,
        });
        formatter.format(&mut builder).unwrap();
        let output = builder.clone();

        // Create a second terminal and apply the output
        let mut t2 = Terminal::init(Global, opts(80, 24)).unwrap();
        {
            let mut s2 = t2.vt_stream();
            s2.next_slice(output.as_slice());
        }

        // Verify tabstops match (columns are 0-indexed in the API)
        assert_eq!(t.tabstops.get(4), t2.tabstops.get(4));
        assert_eq!(t.tabstops.get(14), t2.tabstops.get(14));
        assert_eq!(t.tabstops.get(29), t2.tabstops.get(29));
        assert!(t2.tabstops.get(4)); // Column 5 (1-indexed)
        assert!(t2.tabstops.get(14)); // Column 15 (1-indexed)
        assert!(t2.tabstops.get(29)); // Column 30 (1-indexed)
        assert!(!t2.tabstops.get(8)); // Not a tab

        // Tabstop serialization must not offset the screen contents.
        for (col, expected) in b"hello".iter().enumerate() {
            let cell = unsafe { &(*t2.screens.active).pages }
                .get_cell(crate::point::Point::Screen(coord(
                    col as size::CellCountInt,
                    0,
                )))
                .unwrap();
            assert_eq!(*expected as u21, unsafe { (*cell.cell).codepoint() });
        }

        // Emitting tabstops moves the cursor to each configured column. When
        // cursor state is included, it must be restored afterwards.
        assert_eq!(unsafe { (*t.screens.active).cursor.x }, unsafe {
            (*t2.screens.active).cursor.x
        });
        assert_eq!(unsafe { (*t.screens.active).cursor.y }, unsafe {
            (*t2.screens.active).cursor.y
        });

        // Verify the reordered terminal state is still represented in the map.
        assert_eq!(output.len(), pin_map.count());
        t.deinit(Global);
        t2.deinit(Global);
    }

    // test "Terminal vt with keyboard modes"
    #[test]
    fn terminal_vt_with_keyboard_modes() {
        let mut builder: Vec<u8> = Vec::new();
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        {
            let mut s = t.vt_stream();
            // Set modify other keys mode 2
            s.next_slice(b"\x1b[>4;2m");
            s.next_slice(b"hello");
        }
        let t_ptr: *const Terminal = &t;
        let mut formatter: TerminalFormatter = TerminalFormatter::init(t_ptr, Options::vt());
        formatter.extra.keyboard = true;
        formatter.format(&mut builder).unwrap();
        let output = builder.clone();

        // Create a second terminal and apply the output
        let mut t2 = Terminal::init(Global, opts(80, 24)).unwrap();
        {
            let mut s2 = t2.vt_stream();
            s2.next_slice(output.as_slice());
        }

        // Verify keyboard mode matches
        assert_eq!(t.flags.modify_other_keys_2, t2.flags.modify_other_keys_2);
        assert!(t2.flags.modify_other_keys_2);
        t.deinit(Global);
        t2.deinit(Global);
    }

    // test "Terminal vt with pwd"
    #[test]
    fn terminal_vt_with_pwd() {
        let mut builder: Vec<u8> = Vec::new();
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        // Set pwd using OSC 7
        {
            let mut s = t.vt_stream();
            s.next_slice(b"\x1b]7;file://host/home/user\x1b\\hello");
        }
        let t_ptr: *const Terminal = &t;
        let mut formatter: TerminalFormatter = TerminalFormatter::init(t_ptr, Options::vt());
        formatter.extra.pwd = true;
        formatter.format(&mut builder).unwrap();
        let output = builder.clone();

        // Create a second terminal and apply the output
        let mut t2 = Terminal::init(Global, opts(80, 24)).unwrap();
        {
            let mut s2 = t2.vt_stream();
            s2.next_slice(output.as_slice());
        }

        // Verify pwd matches
        assert_eq!(t.pwd.as_slice(), t2.pwd.as_slice());
        t.deinit(Global);
        t2.deinit(Global);
    }

    // test "Page html with multiple styles"
    #[test]
    fn page_html_with_multiple_styles() {
        let mut builder: Vec<u8> = Vec::new();
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        // Set bold, then italic, then reset
        {
            let mut s = t.vt_stream();
            s.next_slice(b"\x1b[1mbold\x1b[3mitalic\x1b[0mnormal");
        }
        let pages = unsafe { &(*t.screens.active).pages };
        let page = unsafe { last_page(pages) };
        let formatter: PageFormatter = PageFormatter::init(page, Options::html());
        formatter.format(&mut builder).unwrap();
        let output = builder.as_slice();
        assert_eq!(
            output,
            concat!(
                "<div style=\"font-family: monospace; white-space: pre;\">",
                "<div style=\"display: inline;font-weight: bold;\">bold</div>",
                "<div style=\"display: inline;font-weight: bold;font-style: italic;\">italic</div>",
                "normal",
                "</div>"
            )
            .as_bytes()
        );
        t.deinit(Global);
    }

    // test "Page html plain text"
    #[test]
    fn page_html_plain_text() {
        let mut builder: Vec<u8> = Vec::new();
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        {
            let mut s = t.vt_stream();
            s.next_slice(b"hello, world");
        }
        let pages = unsafe { &(*t.screens.active).pages };
        let page = unsafe { last_page(pages) };
        let formatter: PageFormatter = PageFormatter::init(page, Options::html());
        formatter.format(&mut builder).unwrap();
        let output = builder.as_slice();
        // Plain text without styles should be wrapped in monospace div
        assert_eq!(
            output,
            b"<div style=\"font-family: monospace; white-space: pre;\">hello, world</div>"
        );
        t.deinit(Global);
    }

    // test "Page html with colors"
    #[test]
    fn page_html_with_colors() {
        let mut builder: Vec<u8> = Vec::new();
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        // Set red foreground, blue background
        {
            let mut s = t.vt_stream();
            s.next_slice(b"\x1b[31;44mcolored");
        }
        let pages = unsafe { &(*t.screens.active).pages };
        let page = unsafe { last_page(pages) };
        let formatter: PageFormatter = PageFormatter::init(page, Options::html());
        formatter.format(&mut builder).unwrap();
        let output = builder.as_slice();
        assert_eq!(
            output,
            concat!(
                "<div style=\"font-family: monospace; white-space: pre;\">",
                "<div style=\"display: inline;color: var(--vt-palette-1);background-color: var(--vt-palette-4);\">colored</div>",
                "</div>"
            ).as_bytes()
        );
        t.deinit(Global);
    }

    // test "TerminalFormatter html with palette"
    #[test]
    fn terminalformatter_html_with_palette() {
        let mut builder: Vec<u8> = Vec::new();
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        {
            let mut s = t.vt_stream();
            // Modify some palette colors
            s.next_slice(b"\x1b]4;0;rgb:12/34/56\x1b\\");
            s.next_slice(b"\x1b]4;1;rgb:ab/cd/ef\x1b\\");
            s.next_slice(b"\x1b]4;255;rgb:ff/00/ff\x1b\\");
            s.next_slice(b"test");
        }
        let t_ptr: *const Terminal = &t;
        let mut formatter: TerminalFormatter = TerminalFormatter::init(t_ptr, Options::html());
        formatter.extra.palette = true;
        formatter.format(&mut builder).unwrap();
        let output = builder.as_slice();

        // Verify palette CSS variables are emitted
        assert!(index_of(output, b"<style>:root{").is_some());
        assert!(index_of(output, b"--vt-palette-0: #123456;").is_some());
        assert!(index_of(output, b"--vt-palette-1: #abcdef;").is_some());
        assert!(index_of(output, b"--vt-palette-255: #ff00ff;").is_some());
        assert!(index_of(output, b"}</style>").is_some());
        assert!(index_of(output, b"test").is_some());
        t.deinit(Global);
    }

    // test "Page html with background and foreground colors"
    #[test]
    fn page_html_with_background_and_foreground_colors() {
        let mut builder: Vec<u8> = Vec::new();
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        {
            let mut s = t.vt_stream();
            s.next_slice(b"hello");
        }
        let pages = unsafe { &(*t.screens.active).pages };
        let page = unsafe { last_page(pages) };
        let formatter: PageFormatter = PageFormatter::init(
            page,
            Options {
                emit: Format::Html,
                background: Some(crate::color::RGB {
                    r: 0x12,
                    g: 0x34,
                    b: 0x56,
                }),
                foreground: Some(crate::color::RGB {
                    r: 0xab,
                    g: 0xcd,
                    b: 0xef,
                }),
                ..Options::html()
            },
        );
        formatter.format(&mut builder).unwrap();
        let output = builder.as_slice();
        assert_eq!(
            output,
            b"<div style=\"font-family: monospace; white-space: pre;background-color: #123456;color: #abcdef;\">hello</div>"
        );
        t.deinit(Global);
    }

    // test "Page html with escaping"
    #[test]
    fn page_html_with_escaping() {
        let mut builder: Vec<u8> = Vec::new();
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        {
            let mut s = t.vt_stream();
            s.next_slice(b"<tag>&\"'text");
        }
        let pages = unsafe { &(*t.screens.active).pages };
        let page = unsafe { last_page(pages) };
        let mut formatter: PageFormatter = PageFormatter::init(page, Options::html());
        let mut point_map: Vec<Coordinate> = Vec::new();
        formatter.point_map = Some(PointMap {
            alloc: Global,
            map: &mut point_map,
            base: 0,
        });
        formatter.format(&mut builder).unwrap();
        let output = builder.as_slice();
        assert_eq!(
            output,
            b"<div style=\"font-family: monospace; white-space: pre;\">&lt;tag&gt;&amp;&quot;&#39;text</div>"
        );

        // Verify point map length matches output
        assert_eq!(output.len(), point_map.len());

        // Opening wrapper div
        let wrapper_start = "<div style=\"font-family: monospace; white-space: pre;\">";
        let wrapper_start_len = wrapper_start.len();
        for i in 0..wrapper_start_len {
            assert_eq!(coord(0, 0), point_map[i]);
        }

        // Verify each character maps correctly, accounting for escaping
        let offset = wrapper_start_len;
        // < (4 bytes: &lt;) -> x=0
        for i in 0..4 {
            assert_eq!(coord(0, 0), point_map[offset + i]);
        }
        // t (1 byte) -> x=1
        assert_eq!(coord(1, 0), point_map[offset + 4]);
        // a (1 byte) -> x=2
        assert_eq!(coord(2, 0), point_map[offset + 5]);
        // g (1 byte) -> x=3
        assert_eq!(coord(3, 0), point_map[offset + 6]);
        // > (4 bytes: &gt;) -> x=4
        for i in 0..4 {
            assert_eq!(coord(4, 0), point_map[offset + 7 + i]);
        }
        // & (5 bytes: &amp;) -> x=5
        for i in 0..5 {
            assert_eq!(coord(5, 0), point_map[offset + 11 + i]);
        }
        // " (6 bytes: &quot;) -> x=6
        for i in 0..6 {
            assert_eq!(coord(6, 0), point_map[offset + 16 + i]);
        }
        // ' (5 bytes: &#39;) -> x=7
        for i in 0..5 {
            assert_eq!(coord(7, 0), point_map[offset + 22 + i]);
        }
        // t (1 byte) -> x=8
        assert_eq!(coord(8, 0), point_map[offset + 27]);
        // e (1 byte) -> x=9
        assert_eq!(coord(9, 0), point_map[offset + 28]);
        // x (1 byte) -> x=10
        assert_eq!(coord(10, 0), point_map[offset + 29]);
        // t (1 byte) -> x=11
        assert_eq!(coord(11, 0), point_map[offset + 30]);
        t.deinit(Global);
    }

    // test "Page html with unicode as numeric entities"
    #[test]
    fn page_html_with_unicode_as_numeric_entities() {
        let mut builder: Vec<u8> = Vec::new();
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        // Box drawing characters that caused issue #9426
        {
            let mut s = t.vt_stream();
            s.next_slice("╰─ ❯".as_bytes());
        }
        let pages = unsafe { &(*t.screens.active).pages };
        let page = unsafe { last_page(pages) };
        let formatter: PageFormatter = PageFormatter::init(page, Options::html());
        formatter.format(&mut builder).unwrap();
        let output = builder.as_slice();
        // Expected: box drawing chars as numeric entities
        // ╰ = U+2570 = 9584, ─ = U+2500 = 9472, ❯ = U+276F = 10095
        assert_eq!(
            output,
            b"<div style=\"font-family: monospace; white-space: pre;\">&#9584;&#9472; &#10095;</div>"
        );
        t.deinit(Global);
    }

    // test "Page html trailing blank lines"
    #[test]
    fn page_html_trailing_blank_lines() {
        let mut builder: Vec<u8> = Vec::new();
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        {
            let mut s = t.vt_stream();
            s.next_slice(b"hello\r\nworld\r\n\r\n");
        }

        let pages = unsafe { &(*t.screens.active).pages };
        assert!(!pages.pages.first.is_null());
        assert!(core::ptr::eq(pages.pages.first, pages.pages.last));

        let page = unsafe { last_page(pages) };
        let formatter: PageFormatter = PageFormatter::init(page, Options::html());

        let state = formatter.format_with_state(&mut builder).unwrap();
        let output = builder.as_slice();

        // The closing div behaves as a newline
        assert_eq!((unsafe { &*page }.size.rows - 2) as usize, state.rows);
        assert_eq!(
            b"<div style=\"font-family: monospace; white-space: pre;\">hello\nworld</div>"
                .as_slice(),
            output,
        );
        t.deinit(Global);
    }

    // test "Page html ascii characters unchanged"
    #[test]
    fn page_html_ascii_characters_unchanged() {
        let mut builder: Vec<u8> = Vec::new();
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        {
            let mut s = t.vt_stream();
            s.next_slice(b"hello world");
        }
        let pages = unsafe { &(*t.screens.active).pages };
        let page = unsafe { last_page(pages) };
        let formatter: PageFormatter = PageFormatter::init(page, Options::html());
        formatter.format(&mut builder).unwrap();
        let output = builder.as_slice();
        // ASCII should be emitted directly
        assert_eq!(
            output,
            b"<div style=\"font-family: monospace; white-space: pre;\">hello world</div>"
        );
        t.deinit(Global);
    }

    // test "Page html mixed ascii and unicode"
    #[test]
    fn page_html_mixed_ascii_and_unicode() {
        let mut builder: Vec<u8> = Vec::new();
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        {
            let mut s = t.vt_stream();
            s.next_slice("test ╰─❯ ok".as_bytes());
        }
        let pages = unsafe { &(*t.screens.active).pages };
        let page = unsafe { last_page(pages) };
        let formatter: PageFormatter = PageFormatter::init(page, Options::html());
        formatter.format(&mut builder).unwrap();
        let output = builder.as_slice();
        // Mix of ASCII and Unicode entities
        assert_eq!(output, b"<div style=\"font-family: monospace; white-space: pre;\">test &#9584;&#9472;&#10095; ok</div>");
        t.deinit(Global);
    }

    // test "Page VT with palette option emits RGB"
    #[test]
    fn page_vt_with_palette_option_emits_rgb() {
        let mut builder: Vec<u8> = Vec::new();
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        {
            let mut s = t.vt_stream();
            // Set a custom palette color and use it
            s.next_slice(b"\x1b]4;1;rgb:ab/cd/ef\x1b\\");
            s.next_slice(b"\x1b[31mred");
        }
        let pages = unsafe { &(*t.screens.active).pages };
        let page = unsafe { last_page(pages) };

        // Without palette option - should emit palette index
        {
            builder.clear();
            let formatter: PageFormatter = PageFormatter::init(page, Options::vt());
            formatter.format(&mut builder).unwrap();
            let output = builder.as_slice();
            assert_eq!(output, b"\x1b[0m\x1b[38;5;1mred\x1b[0m");
        }

        // With palette option - should emit RGB directly
        {
            builder.clear();
            let mut o = Options::vt();
            o.palette = Some(&t.colors.palette.current as *const crate::color::Palette);
            let formatter: PageFormatter = PageFormatter::init(page, o);
            formatter.format(&mut builder).unwrap();
            let output = builder.as_slice();
            assert_eq!(output, b"\x1b[0m\x1b[38;2;171;205;239mred\x1b[0m");
        }
        t.deinit(Global);
    }

    // test "Page html with palette option emits RGB"
    #[test]
    fn page_html_with_palette_option_emits_rgb() {
        let mut builder: Vec<u8> = Vec::new();
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        {
            let mut s = t.vt_stream();
            // Set a custom palette color and use it
            s.next_slice(b"\x1b]4;1;rgb:ab/cd/ef\x1b\\");
            s.next_slice(b"\x1b[31mred");
        }
        let pages = unsafe { &(*t.screens.active).pages };
        let page = unsafe { last_page(pages) };

        // Without palette option - should emit CSS variable
        {
            builder.clear();
            let formatter: PageFormatter = PageFormatter::init(page, Options::html());
            formatter.format(&mut builder).unwrap();
            let output = builder.as_slice();
            assert_eq!(
                output,
                concat!(
                    "<div style=\"font-family: monospace; white-space: pre;\">",
                    "<div style=\"display: inline;color: var(--vt-palette-1);\">red</div>",
                    "</div>"
                )
                .as_bytes()
            );
        }

        // With palette option - should emit RGB directly
        {
            builder.clear();
            let mut o = Options::html();
            o.palette = Some(&t.colors.palette.current as *const crate::color::Palette);
            let formatter: PageFormatter = PageFormatter::init(page, o);
            formatter.format(&mut builder).unwrap();
            let output = builder.as_slice();
            assert_eq!(
                output,
                concat!(
                    "<div style=\"font-family: monospace; white-space: pre;\">",
                    "<div style=\"display: inline;color: rgb(171, 205, 239);\">red</div>",
                    "</div>"
                )
                .as_bytes()
            );
        }
        t.deinit(Global);
    }

    // test "Page VT style reset properly closes styles"
    #[test]
    fn page_vt_style_reset_properly_closes_styles() {
        let mut builder: Vec<u8> = Vec::new();
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        // Set bold, then reset with SGR 0
        {
            let mut s = t.vt_stream();
            s.next_slice(b"\x1b[1mbold\x1b[0mnormal");
        }
        let pages = unsafe { &(*t.screens.active).pages };
        let page = unsafe { last_page(pages) };
        builder.clear();
        let formatter: PageFormatter = PageFormatter::init(page, Options::vt());
        formatter.format(&mut builder).unwrap();
        let output = builder.as_slice();
        // The reset should properly close the bold style
        assert_eq!(output, b"\x1b[0m\x1b[1mbold\x1b[0mnormal");
        t.deinit(Global);
    }

    // test "Page codepoint_map single replacement"
    #[test]
    fn page_codepoint_map_single_replacement() {
        let mut builder: Vec<u8> = Vec::new();
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        {
            let mut s = t.vt_stream();
            s.next_slice(b"hello world");
        }
        let pages = unsafe { &(*t.screens.active).pages };
        let page = unsafe { last_page(pages) };
        // Replace 'o' with 'x'
        let map = vec![CodepointMap {
            range: ['o' as u21, 'o' as u21],
            replacement: Replacement::Codepoint('x' as u21),
        }];
        let mut o = Options::plain();
        o.codepoint_map = Some(map);
        let mut formatter: PageFormatter = PageFormatter::init(page, o);
        let mut point_map: Vec<Coordinate> = Vec::new();
        formatter.point_map = Some(PointMap {
            alloc: Global,
            map: &mut point_map,
            base: 0,
        });
        formatter.format(&mut builder).unwrap();
        let output = builder.as_slice();
        assert_eq!(output, b"hellx wxrld");

        // Verify point map - each output byte should map to original cell position
        assert_eq!(output.len(), point_map.len());
        // "hello world" -> "hellx wxrld"
        // h e l l o   w o r l d
        // 0 1 2 3 4 5 6 7 8 9 10
        assert_eq!(coord(0, 0), point_map[0]); // h
        assert_eq!(coord(1, 0), point_map[1]); // e
        assert_eq!(coord(2, 0), point_map[2]); // l
        assert_eq!(coord(3, 0), point_map[3]); // l
        assert_eq!(coord(4, 0), point_map[4]); // x (was o)
        assert_eq!(coord(5, 0), point_map[5]); // space
        assert_eq!(coord(6, 0), point_map[6]); // w
        assert_eq!(coord(7, 0), point_map[7]); // x (was o)
        assert_eq!(coord(8, 0), point_map[8]); // r
        assert_eq!(coord(9, 0), point_map[9]); // l
        assert_eq!(coord(10, 0), point_map[10]); // d
        t.deinit(Global);
    }

    // test "Page codepoint_map conflicting replacement prefers last"
    #[test]
    fn page_codepoint_map_conflicting_replacement_prefers_last() {
        let mut builder: Vec<u8> = Vec::new();
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        {
            let mut s = t.vt_stream();
            s.next_slice(b"hello");
        }
        let pages = unsafe { &(*t.screens.active).pages };
        let page = unsafe { last_page(pages) };
        // Replace 'o' with 'x', then with 'y' - should prefer last
        let map = vec![
            CodepointMap {
                range: ['o' as u21, 'o' as u21],
                replacement: Replacement::Codepoint('x' as u21),
            },
            CodepointMap {
                range: ['o' as u21, 'o' as u21],
                replacement: Replacement::Codepoint('y' as u21),
            },
        ];
        let mut o = Options::plain();
        o.codepoint_map = Some(map);
        let formatter: PageFormatter = PageFormatter::init(page, o);
        formatter.format(&mut builder).unwrap();
        let output = builder.as_slice();
        assert_eq!(output, b"helly");
        t.deinit(Global);
    }

    // test "Page codepoint_map replace with string"
    #[test]
    fn page_codepoint_map_replace_with_string() {
        let mut builder: Vec<u8> = Vec::new();
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        {
            let mut s = t.vt_stream();
            s.next_slice(b"hello");
        }
        let pages = unsafe { &(*t.screens.active).pages };
        let page = unsafe { last_page(pages) };
        // Replace 'o' with a multi-byte string
        let map = vec![CodepointMap {
            range: ['o' as u21, 'o' as u21],
            replacement: Replacement::String(b"XYZ".to_vec()),
        }];
        let mut o = Options::plain();
        o.codepoint_map = Some(map);
        let mut formatter: PageFormatter = PageFormatter::init(page, o);
        let mut point_map: Vec<Coordinate> = Vec::new();
        formatter.point_map = Some(PointMap {
            alloc: Global,
            map: &mut point_map,
            base: 0,
        });
        formatter.format(&mut builder).unwrap();
        let output = builder.as_slice();
        assert_eq!(output, b"hellXYZ");

        // Verify point map - string replacements should all map to the original cell
        assert_eq!(output.len(), point_map.len());
        // "hello" -> "hellXYZ"
        // h e l l o
        // 0 1 2 3 4
        assert_eq!(coord(0, 0), point_map[0]); // h
        assert_eq!(coord(1, 0), point_map[1]); // e
        assert_eq!(coord(2, 0), point_map[2]); // l
        assert_eq!(coord(3, 0), point_map[3]); // l
                                               // All bytes of the replacement string "XYZ" should point to position 4 (where 'o' was)
        assert_eq!(coord(4, 0), point_map[4]); // X
        assert_eq!(coord(4, 0), point_map[5]); // Y
        assert_eq!(coord(4, 0), point_map[6]); // Z
        t.deinit(Global);
    }

    // test "Page codepoint_map range replacement"
    #[test]
    fn page_codepoint_map_range_replacement() {
        let mut builder: Vec<u8> = Vec::new();
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        {
            let mut s = t.vt_stream();
            s.next_slice(b"abcdefg");
        }
        let pages = unsafe { &(*t.screens.active).pages };
        let page = unsafe { last_page(pages) };
        // Replace 'b' through 'e' with 'X'
        let map = vec![CodepointMap {
            range: ['b' as u21, 'e' as u21],
            replacement: Replacement::Codepoint('X' as u21),
        }];
        let mut o = Options::plain();
        o.codepoint_map = Some(map);
        let formatter: PageFormatter = PageFormatter::init(page, o);
        formatter.format(&mut builder).unwrap();
        let output = builder.as_slice();
        assert_eq!(output, b"aXXXXfg");
        t.deinit(Global);
    }

    // test "Page codepoint_map multiple ranges"
    #[test]
    fn page_codepoint_map_multiple_ranges() {
        let mut builder: Vec<u8> = Vec::new();
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        {
            let mut s = t.vt_stream();
            s.next_slice(b"hello world");
        }
        let pages = unsafe { &(*t.screens.active).pages };
        let page = unsafe { last_page(pages) };
        // Replace 'a'-'m' with 'A' and 'n'-'z' with 'Z'
        let map = vec![
            CodepointMap {
                range: ['a' as u21, 'm' as u21],
                replacement: Replacement::Codepoint('A' as u21),
            },
            CodepointMap {
                range: ['n' as u21, 'z' as u21],
                replacement: Replacement::Codepoint('Z' as u21),
            },
        ];
        let mut o = Options::plain();
        o.codepoint_map = Some(map);
        let formatter: PageFormatter = PageFormatter::init(page, o);
        formatter.format(&mut builder).unwrap();
        let output = builder.as_slice();
        // h e l l o   w o r l d
        // A A A A Z   Z Z Z A A
        assert_eq!(output, b"AAAAZ ZZZAA");
        t.deinit(Global);
    }

    // test "Page codepoint_map unicode replacement"
    #[test]
    fn page_codepoint_map_unicode_replacement() {
        let mut builder: Vec<u8> = Vec::new();
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        {
            let mut s = t.vt_stream();
            s.next_slice("hello ⚡ world".as_bytes());
        }
        let pages = unsafe { &(*t.screens.active).pages };
        let page = unsafe { last_page(pages) };
        // Replace lightning bolt with fire emoji
        let map = vec![CodepointMap {
            range: ['⚡' as u21, '⚡' as u21],
            replacement: Replacement::String("🔥".as_bytes().to_vec()),
        }];
        let mut o = Options::plain();
        o.codepoint_map = Some(map);
        let mut formatter: PageFormatter = PageFormatter::init(page, o);
        let mut point_map: Vec<Coordinate> = Vec::new();
        formatter.point_map = Some(PointMap {
            alloc: Global,
            map: &mut point_map,
            base: 0,
        });
        formatter.format(&mut builder).unwrap();
        let output = builder.as_slice();
        assert_eq!(output, "hello 🔥 world".as_bytes());

        // Verify point map
        assert_eq!(output.len(), point_map.len());
        // "hello ⚡ world"
        // h e l l o   ⚡   w o r l  d
        // 0 1 2 3 4 5 6   8 9 10 11 12
        // Note: ⚡ is a wide character occupying cells 6-7
        for i in 0..6 {
            assert_eq!(coord(i as size::CellCountInt, 0), point_map[i]);
        }
        // 🔥 is 4 UTF-8 bytes, all should map to cell 6 (where ⚡ was)
        let fire_start = 6; // "hello " is 6 bytes
        for i in 0..4 {
            assert_eq!(coord(6, 0), point_map[fire_start + i]);
        }
        // " world" follows
        let world_start = fire_start + 4;
        for i in 0..6 {
            assert_eq!(
                coord((8 + i) as size::CellCountInt, 0),
                point_map[world_start + i]
            );
        }
        t.deinit(Global);
    }

    // test "Page codepoint_map with styled formats"
    #[test]
    fn page_codepoint_map_with_styled_formats() {
        let mut builder: Vec<u8> = Vec::new();
        let mut t = Terminal::init(Global, opts(10, 24)).unwrap();
        {
            let mut s = t.vt_stream();
            s.next_slice(b"\x1b[31mred text\x1b[0m");
        }
        let pages = unsafe { &(*t.screens.active).pages };
        let page = unsafe { last_page(pages) };
        // Replace 'e' with 'X' in styled text
        let map = vec![CodepointMap {
            range: ['e' as u21, 'e' as u21],
            replacement: Replacement::Codepoint('X' as u21),
        }];
        let mut o = Options::vt();
        o.codepoint_map = Some(map);
        let formatter: PageFormatter = PageFormatter::init(page, o);
        formatter.format(&mut builder).unwrap();
        let output = builder.as_slice();
        // Should preserve styles while replacing text
        // "red text" becomes "rXd tXxt"
        // VT format uses \x1b[38;5;1m for palette color 1
        assert_eq!(output, b"\x1b[0m\x1b[38;5;1mrXd tXxt\x1b[0m");
        t.deinit(Global);
    }

    // test "Page codepoint_map empty map"
    #[test]
    fn page_codepoint_map_empty_map() {
        let mut builder: Vec<u8> = Vec::new();
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        {
            let mut s = t.vt_stream();
            s.next_slice(b"hello world");
        }
        let pages = unsafe { &(*t.screens.active).pages };
        let page = unsafe { last_page(pages) };
        // Empty map should not change anything
        let mut o = Options::plain();
        o.codepoint_map = Some(Vec::new());
        let formatter: PageFormatter = PageFormatter::init(page, o);
        formatter.format(&mut builder).unwrap();
        let output = builder.as_slice();
        assert_eq!(output, b"hello world");
        t.deinit(Global);
    }

    // Regression test for S151-F1: `codepoint_map` set on a `ScreenFormatter`
    // (the shape `Surface::copySelectionToClipboards` uses for the
    // `clipboard-codepoint-map` config option) must survive the
    // ScreenFormatter -> PageListFormatter -> PageFormatter delegation chain
    // and actually apply the replacement. Every hop in the reference passes
    // `self.opts` by value (src/terminal/formatter.zig:486,675,897); a
    // Rust `copy_options` helper that drops the map on any *content* hop
    // silently makes the config option inert.
    #[test]
    fn codepoint_map_survives_the_screen_formatter_delegation() {
        let mut builder: Vec<u8> = Vec::new();
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        {
            let mut s = t.vt_stream();
            s.next_slice(b"ab");
        }
        // map 'a' (0x61) -> 'Z' (0x5A), the same shape config uses.
        let map = vec![CodepointMap {
            range: ['a' as u21, 'a' as u21],
            replacement: Replacement::Codepoint('Z' as u21),
        }];
        let mut o = Options::plain();
        o.codepoint_map = Some(map);
        let mut formatter: ScreenFormatter = ScreenFormatter::init(t.screens.active, o);
        formatter.content = Content::Selection(None);
        formatter.format(&mut builder).unwrap();
        let output = String::from_utf8_lossy(&builder);
        assert_eq!(
            output.trim_end(),
            "Zb",
            "codepoint_map was dropped on the way down to PageFormatter"
        );
        t.deinit(Global);
    }

    // test "Page VT background color on trailing blank cells"
    #[test]
    fn page_vt_background_color_on_trailing_blank_cells() {
        // This test reproduces a bug where trailing cells with background color
        // but no text are emitted as plain spaces without SGR sequences.
        // This causes TUIs like htop to lose background colors on rehydration.
        let mut builder: Vec<u8> = Vec::new();
        let mut t = Terminal::init(Global, opts(20, 5)).unwrap();
        {
            let mut s = t.vt_stream();
            // Simulate a TUI row: "CPU:" with text, then trailing cells with red background
            // to end of line (no text after the colored region).
            // \x1b[41m sets red background, then EL fills rest of row with that bg.
            s.next_slice(b"CPU:\x1b[41m\x1b[K");
            // Reset colors and move to next line with different content
            s.next_slice(b"\x1b[0m\r\nline2");
        }
        let pages = unsafe { &(*t.screens.active).pages };
        let page = unsafe { last_page(pages) };
        let mut formatter: PageFormatter = PageFormatter::init(page, Options::vt());
        formatter.opts.trim = false; // Don't trim so we can see the trailing behavior
        formatter.format(&mut builder).unwrap();
        let output = builder.as_slice();

        // The output should preserve the red background SGR for trailing cells on line 1.
        // Bug: the first row outputs "CPU:\r\n" only - losing the background color fill.
        // The red background should appear BEFORE the newline, not after.

        // Find position of CRLF
        let crlf_pos = match index_of(output, b"\r\n") {
            Some(p) => p,
            // No CRLF found, fail the test
            None => panic!("TestUnexpectedResult"),
        };

        // Check that red background (48;5;1) appears BEFORE the newline (on line 1)
        let line1 = &output[0..crlf_pos];
        let has_red_bg_line1 =
            index_of(line1, b"\x1b[41m").is_some() || index_of(line1, b"\x1b[48;5;1m").is_some();

        // This should be true but currently fails due to the bug
        assert!(has_red_bg_line1);
        t.deinit(Global);
    }

    // test "Page HTML with hyperlinks"
    #[test]
    fn page_html_with_hyperlinks() {
        let mut builder: Vec<u8> = Vec::new();
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        // Start a hyperlink, write some text, end it
        {
            let mut s = t.vt_stream();
            s.next_slice(b"\x1b]8;;https://example.com\x1b\\link text\x1b]8;;\x1b\\ normal");
        }
        let pages = unsafe { &(*t.screens.active).pages };
        let page = unsafe { last_page(pages) };
        let formatter: PageFormatter = PageFormatter::init(page, Options::html());
        formatter.format(&mut builder).unwrap();
        let output = builder.as_slice();
        assert_eq!(
            output,
            concat!(
                "<div style=\"font-family: monospace; white-space: pre;\">",
                "<a href=\"https://example.com\">link text</a> normal",
                "</div>"
            )
            .as_bytes()
        );
        t.deinit(Global);
    }

    // test "Page HTML with multiple hyperlinks"
    #[test]
    fn page_html_with_multiple_hyperlinks() {
        let mut builder: Vec<u8> = Vec::new();
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        {
            let mut s = t.vt_stream();
            // Two different hyperlinks
            s.next_slice(b"\x1b]8;;https://first.com\x1b\\first\x1b]8;;\x1b\\ ");
            s.next_slice(b"\x1b]8;;https://second.com\x1b\\second\x1b]8;;\x1b\\");
        }
        let pages = unsafe { &(*t.screens.active).pages };
        let page = unsafe { last_page(pages) };
        let formatter: PageFormatter = PageFormatter::init(page, Options::html());
        formatter.format(&mut builder).unwrap();
        let output = builder.as_slice();
        assert_eq!(
            output,
            concat!(
                "<div style=\"font-family: monospace; white-space: pre;\">",
                "<a href=\"https://first.com\">first</a>",
                " ",
                "<a href=\"https://second.com\">second</a>",
                "</div>"
            )
            .as_bytes()
        );
        t.deinit(Global);
    }

    // test "Page HTML with hyperlink escaping"
    #[test]
    fn page_html_with_hyperlink_escaping() {
        let mut builder: Vec<u8> = Vec::new();
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        // URL with special characters that need escaping
        {
            let mut s = t.vt_stream();
            s.next_slice(b"\x1b]8;;https://example.com?a=1&b=2\x1b\\link\x1b]8;;\x1b\\");
        }
        let pages = unsafe { &(*t.screens.active).pages };
        let page = unsafe { last_page(pages) };
        let formatter: PageFormatter = PageFormatter::init(page, Options::html());
        formatter.format(&mut builder).unwrap();
        let output = builder.as_slice();
        assert_eq!(
            output,
            concat!(
                "<div style=\"font-family: monospace; white-space: pre;\">",
                "<a href=\"https://example.com?a=1&amp;b=2\">link</a>",
                "</div>"
            )
            .as_bytes()
        );
        t.deinit(Global);
    }

    // test "Page HTML with styled hyperlink"
    #[test]
    fn page_html_with_styled_hyperlink() {
        let mut builder: Vec<u8> = Vec::new();
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        // Bold hyperlink
        {
            let mut s = t.vt_stream();
            s.next_slice(b"\x1b]8;;https://example.com\x1b\\\x1b[1mbold link\x1b[0m\x1b]8;;\x1b\\");
        }
        let pages = unsafe { &(*t.screens.active).pages };
        let page = unsafe { last_page(pages) };
        let formatter: PageFormatter = PageFormatter::init(page, Options::html());
        formatter.format(&mut builder).unwrap();
        let output = builder.as_slice();
        assert_eq!(
            output,
            concat!(
                "<div style=\"font-family: monospace; white-space: pre;\">",
                "<div style=\"display: inline;font-weight: bold;\">",
                "<a href=\"https://example.com\">bold link</div></a>",
                "</div>"
            )
            .as_bytes()
        );
        t.deinit(Global);
    }

    // test "Page HTML hyperlink closes style before anchor"
    #[test]
    fn page_html_hyperlink_closes_style_before_anchor() {
        let mut builder: Vec<u8> = Vec::new();
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        // Styled hyperlink followed by plain text
        {
            let mut s = t.vt_stream();
            s.next_slice(b"\x1b]8;;https://example.com\x1b\\\x1b[1mbold\x1b[0m plain");
        }
        let pages = unsafe { &(*t.screens.active).pages };
        let page = unsafe { last_page(pages) };
        let formatter: PageFormatter = PageFormatter::init(page, Options::html());
        formatter.format(&mut builder).unwrap();
        let output = builder.as_slice();
        assert_eq!(
            output,
            concat!(
                "<div style=\"font-family: monospace; white-space: pre;\">",
                "<div style=\"display: inline;font-weight: bold;\">",
                "<a href=\"https://example.com\">bold</div> plain</a>",
                "</div>"
            )
            .as_bytes()
        );
        t.deinit(Global);
    }

    // test "Page HTML hyperlink point map maps closing to previous cell"
    #[test]
    fn page_html_hyperlink_point_map_maps_closing_to_previous_cell() {
        let mut builder: Vec<u8> = Vec::new();
        let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
        {
            let mut s = t.vt_stream();
            s.next_slice(b"\x1b]8;;https://example.com\x1b\\link\x1b]8;;\x1b\\ normal");
        }
        let pages = unsafe { &(*t.screens.active).pages };
        let page = unsafe { last_page(pages) };
        let mut formatter: PageFormatter = PageFormatter::init(page, Options::html());
        let mut point_map: Vec<Coordinate> = Vec::new();
        formatter.point_map = Some(PointMap {
            alloc: Global,
            map: &mut point_map,
            base: 0,
        });
        formatter.format(&mut builder).unwrap();
        let output = builder.as_slice();
        let expected_output = concat!(
            "<div style=\"font-family: monospace; white-space: pre;\">",
            "<a href=\"https://example.com\">link</a> normal",
            "</div>"
        );
        assert_eq!(output, expected_output.as_bytes());
        assert_eq!(expected_output.len(), point_map.len());

        // The </a> closing tag bytes should all map to the last cell of the link
        let closing_idx = index_of(expected_output.as_bytes(), b"</a>").unwrap();
        let expected_coord = point_map[closing_idx - 1];
        for i in closing_idx..closing_idx + "</a>".len() {
            assert_eq!(expected_coord, point_map[i]);
        }
        t.deinit(Global);
    }
}
