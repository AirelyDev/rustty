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

// PORT NOTE: the reference allocator maps to `crate::alloc::Global`
// (the crate's allocator seam).
use crate::alloc::Allocator;

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
    // PORT NOTE: `std.ArrayList(Coordinate) = .empty`, grown through the pin
    // map's `alloc` (parity tail S7-8, N330). Zig's unmanaged list takes the
    // allocator per call; this allocator-bound list is bound to it on first
    // growth (`alloc::bind`) and debug-asserts every later call passes the
    // same one. The page formatter's `PointMap` writes into it with Zig's
    // `growCapacity` (`zig_list`).
    pub points: crate::alloc::AVec<Coordinate>,

    /// The page node for ranges of output bytes, ordered by offset.
    // PORT NOTE: `std.ArrayList(NodeRun) = .empty`, bound as `points` is.
    pub nodes: crate::alloc::AVec<NodeRun>,
}

/// `ArrayList(NodeRun)`'s `init_capacity` (`@sizeOf(NodeRun)` is 16 on the
/// 64-bit targets the port builds: a `usize` and a pointer).
const NODE_RUN_INIT_CAPACITY: usize =
    crate::zig_list::init_capacity(2 * core::mem::size_of::<usize>());

impl PinMapMap {
    // pub const empty: Map = .{};
    pub const EMPTY: PinMapMap = PinMapMap {
        points: crate::alloc::AVec::new_in(crate::alloc::VtAllocator::GLOBAL),
        nodes: crate::alloc::AVec::new_in(crate::alloc::VtAllocator::GLOBAL),
    };

    pub fn deinit(&mut self, alloc: Allocator) {
        // self.points.deinit(alloc);
        // self.nodes.deinit(alloc);
        // PORT NOTE: each list is bound to the allocator it grew through, so
        // dropping it frees through that allocator; `alloc` must be it.
        if self.points.capacity() != 0 {
            alloc.debug_assert_same(*self.points.allocator());
        }
        if self.nodes.capacity() != 0 {
            alloc.debug_assert_same(*self.nodes.allocator());
        }
        self.points = crate::alloc::AVec::new_in(crate::alloc::VtAllocator::GLOBAL);
        self.nodes = crate::alloc::AVec::new_in(crate::alloc::VtAllocator::GLOBAL);
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
    pub fn set_node(
        &mut self,
        alloc: Allocator,
        node: PinMapNode,
    ) -> Result<(), crate::alloc::OutOfMemory> {
        if let Some(last) = self.nodes.last() {
            if core::ptr::eq(last.node, node) {
                return Ok(());
            }
        }

        // try self.nodes.append(alloc, .{ .offset = self.points.items.len, .node = node });
        crate::alloc::bind(&mut self.nodes, alloc);
        let offset = self.points.len();
        crate::zig_list::append(
            &mut self.nodes,
            NodeRun { offset, node },
            NODE_RUN_INIT_CAPACITY,
        )
    }

    /// Append `n` bytes that map to `pin`.
    pub fn append(
        &mut self,
        alloc: Allocator,
        pin: Pin,
        n: usize,
    ) -> Result<(), crate::alloc::OutOfMemory> {
        if n == 0 {
            return Ok(());
        }
        self.set_node(alloc, pin.node)?;
        // try self.points.appendNTimes(alloc, .{ .x = pin.x, .y = pin.y }, n);
        crate::alloc::bind(&mut self.points, alloc);
        crate::zig_list::append_n_times(
            &mut self.points,
            Coordinate {
                x: pin.x,
                y: pin.y as u32,
            },
            n,
            COORDINATE_INIT_CAPACITY,
        )
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
                    m.map
                        .append(
                            m.alloc,
                            unsafe { &*terminal.screens.active }
                                .pages
                                .get_top_left(point::Tag::Screen),
                            discarding.count,
                        )
                        .map_err(point_map_write_failed)?;
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
                m.map
                    .append(
                        m.alloc,
                        unsafe { &*terminal.screens.active }
                            .pages
                            .get_top_left(point::Tag::Screen),
                        discarding.count,
                    )
                    .map_err(point_map_write_failed)?;
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
                m.map
                    .append(
                        m.alloc,
                        unsafe { &*terminal.screens.active }
                            .pages
                            .get_top_left(point::Tag::Screen),
                        discarding.count,
                    )
                    .map_err(point_map_write_failed)?;
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
                m.map
                    .append(m.alloc, pin, discarding.count)
                    .map_err(point_map_write_failed)?;
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
            ScreenFormatter::init(terminal.screens.active, clone_options(&self.opts));
        screen_formatter.content = Content::None;
        screen_formatter.extra = self.extra.screen;

        // In origin mode (DECOM), which the modes above restore, CUP counts
        // from the top-left of the scrolling region restored above, so the
        // cursor position is written relative to it.
        if self.opts.emit == Format::Vt
            && self.extra.modes
            && self.extra.scrolling_region
            && terminal.modes.get(crate::modes::Mode::Origin)
        {
            screen_formatter.cursor_origin = CursorOrigin {
                x: terminal.scrolling_region.left,
                y: terminal.scrolling_region.top,
            };
            // PORT NOTE (UD381): Rustty-only. The full region, so the screen
            // formatter can restore a cursor that lies outside it.
            screen_formatter.cursor_margins = Some(CursorMargins {
                top: terminal.scrolling_region.top,
                bottom: terminal.scrolling_region.bottom,
                left: terminal.scrolling_region.left,
                right: terminal.scrolling_region.right,
            });
        }

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

    /// The point the emitted cursor position (CUP) counts from. This is the
    /// top-left of the screen unless the output is replayed in origin mode
    /// (DECOM), where CUP counts from the top-left of the scrolling region.
    pub cursor_origin: CursorOrigin,

    /// PORT NOTE (UD381): Rustty-only. The scrolling region the output is
    /// replayed with in origin mode (DECOM), set together with
    /// `cursor_origin`. In origin mode CUP cannot address a cell outside the
    /// margins, yet the cursor can be there (DECRC of a cursor saved in
    /// origin mode, after the margins changed). Upstream writes a CUP that
    /// the replaying terminal clamps into the region; with this set, such a
    /// cursor is restored exactly instead (see the cursor extra below).
    pub cursor_margins: Option<CursorMargins>,
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

// PORT NOTE: nested `ScreenFormatter.CursorOrigin` struct, hoisted to a
// top-level name like `Content` and `ScreenExtra`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CursorOrigin {
    pub x: size::CellCountInt,
    pub y: size::CellCountInt,
}

// PORT NOTE (UD381): Rustty-only, the type of `ScreenFormatter.cursor_margins`.
// Inclusive, 0-indexed, like `Terminal.scrolling_region`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CursorMargins {
    pub top: size::CellCountInt,
    pub bottom: size::CellCountInt,
    pub left: size::CellCountInt,
    pub right: size::CellCountInt,
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
            cursor_origin: CursorOrigin::default(),
            cursor_margins: None,
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
            // PORT NOTE (UD381): Rustty-only. In origin mode with the cursor
            // outside the margins, no CUP can address the cursor, so the
            // state is rebuilt the way a program reaches it: full-screen
            // margins (origin mode is already on, so CUP is then absolute),
            // the position below, DECSC, the real margins, and DECRC, which
            // restores the position, the pending wrap and origin mode. This
            // overwrites the replaying terminal's saved cursor; the formatter
            // never reproduces the saved cursor, and DECRC restores the
            // style, charset and protection DECSC just saved. Every other
            // case keeps upstream's output byte for byte.
            let outside_margins: Option<CursorMargins> = self.cursor_margins.filter(|m| {
                let cursor = &screen.cursor;
                cursor.y < m.top || cursor.y > m.bottom || cursor.x < m.left || cursor.x > m.right
            });
            let full_top_bottom =
                |m: &CursorMargins| m.top == 0 && m.bottom == screen.pages.rows - 1;
            let full_left_right =
                |m: &CursorMargins| m.left == 0 && m.right == screen.pages.cols - 1;
            if let Some(m) = &outside_margins {
                // DECSTBM and DECSLRM (with DECLRMM on, which non-full left
                // and right margins imply) with no parameters reset to the
                // full screen.
                if !full_top_bottom(m) {
                    writer.write_all(b"\x1b[r")?;
                }
                if !full_left_right(m) {
                    writer.write_all(b"\x1b[s")?;
                }
            }

            'cursor: {
                let cursor = &screen.cursor;
                let origin = if outside_margins.is_some() {
                    CursorOrigin::default()
                } else {
                    self.cursor_origin
                };

                // If we don't have pending wrap, then we can just use CUP.
                if !cursor.pending_wrap || cursor.x != screen.pages.cols - 1 {
                    write!(
                        writer,
                        "\x1b[{};{}H",
                        u32::from(cursor.y.saturating_sub(origin.y)) + 1,
                        u32::from(cursor.x.saturating_sub(origin.x)) + 1,
                    )?;
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
                write!(
                    writer,
                    "\x1b[{};{}H",
                    u32::from(cursor.y.saturating_sub(origin.y)) + 1,
                    u32::from(start_x.saturating_sub(origin.x)) + 1,
                )?;

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

            // PORT NOTE (UD381): save the absolute position, restore the
            // real margins (which home the cursor), then restore the cursor.
            if let Some(m) = &outside_margins {
                writer.write_all(b"\x1b7")?;
                if !full_top_bottom(m) {
                    write!(writer, "\x1b[{};{}r", m.top + 1, m.bottom + 1)?;
                }
                if !full_left_right(m) {
                    write!(writer, "\x1b[{};{}s", m.left + 1, m.right + 1)?;
                }
                writer.write_all(b"\x1b8")?;
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
                opts: clone_options(&self.opts),
                content: Content::None,
                extra: self.extra,
                pin_map: None,
                cursor_origin: self.cursor_origin,
                cursor_margins: self.cursor_margins,
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
            m.map
                .append(m.alloc, pin, discarding.count)
                .map_err(point_map_write_failed)?;
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
// (src/terminal/formatter.zig:486, 675, 726, 897). `copy_options` below is a
// narrow optimization reserved for the SPECIFIC passes proven never to reach
// a codepoint lookup; `clone_options` (below it) is the default-safe choice
// and MUST be used everywhere else — including passes with `content ==
// Content::None`, see next paragraph.
//
// `content == Content::None` is NOT by itself a sufficient condition for
// `copy_options`. A `ScreenFormatter` with `content: Content::None` still
// reaches `PageFormatter` if its `extra.cursor` is set and the cursor has a
// pending wrap: `ScreenFormatter::format`'s `extra.cursor` block builds a
// single-cell `PageFormatter` to reformat the cursor's cell, and the oracle
// passes `self.opts` (map included) into it (formatter.zig:724-726) exactly
// like every other delegation hop. The actually-sufficient condition is:
// every formatter this pass constructs, transitively, is provably free of
// any `PageListFormatter`/`PageFormatter` construction — in practice,
// `content == Content::None` AND no `ScreenExtra::cursor` is ever set on any
// `ScreenFormatter` reachable from this pass.
//
// `copy_options` is safe ONLY for the `TerminalFormatter::format` `pin_map`
// byte-counting "discarding writer" replays at the palette/modes/tabstops/
// scrolling-region+keyboard+pwd sites: each builds a fresh formatter with
// `extra: TerminalExtra::NONE` plus exactly one non-`screen` flag set, so the
// `ScreenFormatter` it in turn constructs (in its own `TerminalFormatter::
// format` trailing pass) always gets `extra.screen == ScreenExtra::NONE` and
// returns before ever reaching `extra.cursor`.
//
// Every other site — including `ScreenFormatter::format`'s OWN trailing
// "extra screen state" pass and its `pin_map` byte-counting replay, both of
// which inherit the CALLER's `extra` (cursor included), and obviously every
// hop that reaches `PageListFormatter`/`PageFormatter` directly — MUST use
// `clone_options` instead, or the `codepoint_map` a caller set (e.g. the
// `clipboard-codepoint-map` config option via
// `Surface::copySelectionToClipboards`) silently stops applying, or (worse)
// applies inconsistently between a formatter's real output and its own
// `pin_map` byte-count replay of that same output.
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
                // m.map.setNode(m.alloc, chunk.node) catch return error.WriteFailed;
                m.map
                    .set_node(m.alloc, chunk.node)
                    .map_err(point_map_write_failed)?;
                // The page formatter grows `points` through `m.alloc`.
                crate::alloc::bind(&mut m.map.points, m.alloc);
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
//
// PORT NOTE (parity tail S7-6): `map` is an allocator-bound list, the
// reference's `*std.ArrayList(Coordinate)` grown through `alloc`. It grows
// through the allocator it is bound to: the search sliding window binds its
// `cell_map` to the window's allocator (the reference's `.alloc =
// self.alloc`). Growth follows `ArrayList.appendNTimes` (`growCapacity`,
// `remap` first) and a failure is the reference's `catch return
// error.WriteFailed` (an `io::ErrorKind::OutOfMemory` error).
pub struct PointMap<'a> {
    pub alloc: Allocator,
    pub map: &'a mut crate::alloc::AVec<Coordinate>,

    /// The index in `map` at which this formatter's output begins.
    /// Entries before this index belong to a caller (e.g. previous
    /// pages of a PageListFormatter) and aren't inspected. This
    /// exists so that callers can share one list across pages.
    // PORT NOTE: the reference field default (`base: usize = 0`) is supplied
    // explicitly at each construction site (Rust struct literals have no
    // field defaults).
    pub base: usize,
}

/// `ArrayList(Coordinate)`'s `init_capacity` (`@sizeOf(Coordinate)` is 8).
const COORDINATE_INIT_CAPACITY: usize = crate::zig_list::init_capacity(8);

/// `error.WriteFailed` for a failed point-map growth.
fn point_map_write_failed(_: crate::alloc::OutOfMemory) -> io::Error {
    io::Error::from(io::ErrorKind::OutOfMemory)
}

impl PointMap<'_> {
    /// `map.map.appendNTimes(map.alloc, coord, n) catch return error.WriteFailed`.
    #[inline]
    fn append_n_times(&mut self, coord: Coordinate, n: usize) -> io::Result<()> {
        // The list is bound to the allocator the reference passes per call.
        self.alloc.debug_assert_same(*self.map.allocator());
        crate::zig_list::append_n_times(self.map, coord, n, COORDINATE_INIT_CAPACITY)
            .map_err(point_map_write_failed)
    }

    /// `map.map.append(map.alloc, coord) catch return error.WriteFailed`.
    #[inline]
    fn append(&mut self, coord: Coordinate) -> io::Result<()> {
        self.append_n_times(coord, 1)
    }

    /// `map.map.ensureUnusedCapacity(map.alloc, n) catch return error.WriteFailed`.
    #[inline]
    fn ensure_unused_capacity(&mut self, n: usize) -> io::Result<()> {
        self.alloc.debug_assert_same(*self.map.allocator());
        crate::zig_list::ensure_unused_capacity(self.map, n, COORDINATE_INIT_CAPACITY)
            .map_err(point_map_write_failed)
    }
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
                    map.append_n_times(Coordinate { x: 0, y: 0 }, header.len())?;
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
                    map.append_n_times(Coordinate { x: 0, y: 0 }, header.len())?;
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
                    map.append_n_times(
                        Coordinate {
                            x: start.x,
                            y: start.y,
                        },
                        sequence.len(),
                    )?;

                    // All others have x = 0 since they reference their prior
                    // blank line.
                    for y_offset_usize in 1..blank_rows {
                        let y_offset: u32 = y_offset_usize as u32;
                        map.append_n_times(
                            Coordinate {
                                x: 0,
                                y: start.y + y_offset,
                            },
                            sequence.len(),
                        )?;
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
                        self.append_blank_points(blank_cells, x, y)?;
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
                                map.append_n_times(Coordinate { x, y: y as u32 }, count)?;
                            }
                        }
                    }

                    // Cells with only background color (no text). Emit a space
                    // with the appropriate background color SGR sequence.
                    pagepkg::ContentTag::BgColorPalette | pagepkg::ContentTag::BgColorRgb => {
                        writer.write_all(b" ")?;
                        if let Some(map) = self.point_map.as_mut() {
                            map.append(Coordinate { x, y: y as u32 })?;
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
                // PORT NOTE: `ensureUnusedCapacity` + `appendNTimesAssumeCapacity`;
                // the second cannot grow after the first.
                map.ensure_unused_capacity(closing.len())?;
                let last = map.map[map.map.len() - 1];
                map.append_n_times(last, closing.len())?;
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
                    self.append_blank_points(pending, x, run_y)?;
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
                map.append_n_times(Coordinate { x, y: run_y as u32 }, cell_bytes)?;
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
    fn append_blank_points(
        &mut self,
        count: usize,
        x: size::CellCountInt,
        y: size::CellCountInt,
    ) -> io::Result<()> {
        let cols = unsafe { &*self.page }.size.cols;
        let map = self.point_map.as_mut().unwrap();
        // map.map.ensureUnusedCapacity(
        //     map.alloc,
        //     count,
        // ) catch return error.WriteFailed;
        map.ensure_unused_capacity(count)?;

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

            // map.map.appendAssumeCapacity(.{ .x = blank_x, .y = blank_y });
            map.map.push(Coordinate {
                x: blank_x,
                y: blank_y as u32,
            });
            remaining -= 1;
        }
        Ok(())
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
            m.append_n_times(last, s.len())?;
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
            m.append_n_times(last, s.len())?;
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
mod tests;
