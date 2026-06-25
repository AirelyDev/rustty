//! The primary terminal emulation structure. This represents a single
//! "terminal" containing a grid of characters and exposes various operations
//! on that grid. This also maintains the scrollback buffer.
//!
//! rustty-vt terminal
//!
//! PORTS: ghostty src/terminal/Terminal.zig
//!
//! PORT STATUS: skeleton (struct-first; method bodies are `todo!()`, ported in
//! vt-12b)

// PORT NOTE (vt-12a skeleton): every method body is `todo!()` and many fields
// are not yet read, so we allow the lints that the skeleton trivially trips.
// These allows are removed as vt-12b fills the bodies.
#![allow(clippy::todo)]
#![allow(dead_code)]
#![allow(unused_variables)]
#![allow(clippy::new_without_default)]
// PORT NOTE: the reference exposes `Colors.default` as a named constant; the
// Rust port mirrors it as a `Colors::default()` inherent constructor. This is
// intentional parity, not the std `Default` trait.
#![allow(clippy::should_implement_trait)]

// const std = @import("std");
// const build_options = @import("terminal_options");
// const lib = @import("lib.zig");
// const assert = @import("../quirks.zig").inlineAssert;
// const testing = std.testing;
// const Allocator = std.mem.Allocator;
// const unicode = @import("../unicode/main.zig");
// const uucode = @import("uucode");
//
// const ansi = @import("ansi.zig");
// const modespkg = @import("modes.zig");
// const charsets = @import("charsets.zig");
// const csi = @import("csi.zig");
// const hyperlink = @import("hyperlink.zig");
// const glyph = @import("apc/glyph.zig");
// const kitty = @import("kitty.zig");
// const osc = @import("osc.zig");
// const point = @import("point.zig");
// const sgr = @import("sgr.zig");
// const Tabstops = @import("Tabstops.zig");
// const color = @import("color.zig");
// const mouse = @import("mouse.zig");
// const Stream = @import("stream_terminal.zig").Stream;
//
// const size = @import("size.zig");
// const pagepkg = @import("page.zig");
// const style = @import("style.zig");
// const Screen = @import("Screen.zig");
// const ScreenSet = @import("ScreenSet.zig");
// const Page = pagepkg.Page;
// const Cell = pagepkg.Cell;
// const Row = pagepkg.Row;
//
// PORT NOTE: the reference `Allocator.Error` maps to `pagelist::OutOfMemory`
// throughout this crate (matching Screen/ScreenSet); fallible (`!`) returns
// become `Result<_, OutOfMemory>`.
use crate::ansi;
use crate::charsets;
use crate::csi;
use crate::kitty;
use crate::modes as modespkg;
use crate::mouse;
use crate::osc;
use crate::page as pagepkg;
use crate::page::u21;
use crate::page::{Page, Row};
use crate::pagelist::OutOfMemory;
use crate::point;
use crate::screen::Screen;
use crate::screen_set::ScreenSet;
use crate::sgr;
use crate::size;
use crate::stream_terminal::{Handler, Stream};
use crate::style;
use crate::tabstops::Tabstops;
use color::{DynamicPalette, DynamicRGB};

// PORT NOTE: `terminal/color.zig` is `crate::color`.
use crate::color;

// const unicode = @import("../unicode/main.zig");
// PORT NOTE: the width/grapheme-break seam (`unicode.table.get`,
// `unicode.graphemeBreak`) maps to the `rustty_unicode` crate, matching Screen.
use rustty_unicode as unicode;

use allocator_api2::alloc::Global as Allocator;

// PORT NOTE: `vtStream` threads `self.gpa()` (a `std.mem.Allocator`) into the
// OSC parser via `Stream.initAlloc`, which in rustty wants a
// `ZigAllocatorRef<'static>`. The reference `gpa()` returns the screen's
// general-purpose allocator (the global allocator). We back the OSC parser with
// a process-static `GlobalZigAllocator` (a stateless ZST) so the returned
// `ZigAllocatorRef` is `'static` and coerces to the stream's lifetime.
static GLOBAL_ZIG_ALLOCATOR: rustty_lib::allocator::GlobalZigAllocator =
    rustty_lib::allocator::GlobalZigAllocator;

// const log = std.log.scoped(.terminal);

/// Default tabstop interval
const TABSTOP_INTERVAL: u16 = 8;

/// Per-session Glyph Protocol registrations.
///
/// PORT NOTE (vt-12a deferral): the Glyph Protocol is font-blocked and remains
/// deferred past vt-12 (see DEFERRALS.md). `apc/glyph.zig`'s `Glossary` is a
/// ZST/no-op stub here; the `glyphProtocol` request/response types
/// (`glyph.Request`/`glyph.Response`) are likewise ZST stubs until reactivated.
#[derive(Debug, Default)]
pub struct GlyphGlossary;

impl GlyphGlossary {
    /// `.empty` — an empty glossary.
    pub const EMPTY: GlyphGlossary = GlyphGlossary;

    // pub fn deinit(self: *Glossary, alloc: Allocator) void
    pub fn deinit(&mut self, alloc: Allocator) {
        // PORT NOTE (vt-12a deferral): the Glyph Protocol is font-blocked and
        // `apc/glyph.zig`'s `Glossary` is a ZST/no-op stub here (see the struct
        // doc). It holds no allocated state, so `deinit` is a no-op. `Terminal::
        // deinit` calls this, so it must be a (no-op) body rather than `todo!()`.
        let _ = alloc;
    }
}

/// `glyph.Request` — Glyph Protocol request (font-blocked ZST stub; see
/// `GlyphGlossary`).
#[derive(Debug)]
pub struct GlyphRequest;

/// `glyph.Response` — Glyph Protocol response (font-blocked ZST stub; see
/// `GlyphGlossary`).
#[derive(Debug)]
pub struct GlyphResponse;

/// The most recently set mouse shift capture state.
///
/// Set via the XTSHIFTESCAPE sequence. If true (XTSHIFTESCAPE = 1)
/// then we want to capture the shift key for the mouse protocol
/// if the configuration allows it.
// enum(u2) { null, false, true }
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MouseShiftCapture {
    #[default]
    Null,
    False,
    True,
}

/// These are just a packed set of flags we may set on the terminal.
// flags: packed struct { ... } = .{}
#[derive(Debug, Clone, Copy)]
pub struct Flags {
    // This supports a Kitty extension where programs using semantic
    // prompts (OSC133) can annotate their new prompts with `redraw=0` to
    // disable clearing the prompt on resize.
    pub shell_redraws_prompt: osc::semantic_prompt::Redraw,

    // This is set via ESC[4;2m. Any other modify key mode just sets
    // this to false and we act in mode 1 by default.
    pub modify_other_keys_2: bool,

    /// The mouse event mode and format. These are set to the last
    /// set mode in modes. You can't get the right event/format to use
    /// based on modes alone because modes don't show you what order
    /// this was called so we have to track it separately.
    pub mouse_event: mouse::Event,
    pub mouse_format: mouse::Format,

    /// Set via the XTSHIFTESCAPE sequence. If true (XTSHIFTESCAPE = 1)
    /// then we want to capture the shift key for the mouse protocol
    /// if the configuration allows it.
    pub mouse_shift_capture: MouseShiftCapture,

    /// True if the window is focused.
    pub focused: bool,

    /// True if the terminal is in a password entry mode. This is set
    /// to true based on termios state. This is set
    /// to true based on termios state.
    pub password_input: bool,

    /// True if the terminal should perform selection scrolling.
    pub selection_scroll: bool,

    /// Dirty flag used only by the search thread. The renderer is expected
    /// to set this to true if the viewport was dirty as it was rendering.
    /// This is used by the search thread to more efficiently re-search the
    /// viewport and active area.
    ///
    /// Since the renderer is going to inspect the viewport/active area ANYWAYS,
    /// this lets our search thread do less work and hold the lock less time,
    /// resulting in more throughput for everything.
    pub search_viewport_dirty: bool,

    /// Dirty flags for the renderer.
    pub dirty: Dirty,
}

impl Default for Flags {
    fn default() -> Flags {
        Flags {
            shell_redraws_prompt: osc::semantic_prompt::Redraw::True,
            modify_other_keys_2: false,
            mouse_event: mouse::Event::None,
            mouse_format: mouse::Format::X10,
            mouse_shift_capture: MouseShiftCapture::Null,
            focused: true,
            password_input: false,
            selection_scroll: false,
            search_viewport_dirty: false,
            dirty: Dirty::default(),
        }
    }
}

/// The various color configurations a terminal maintains and that can
/// be set dynamically via OSC, with defaults usually coming from a
/// configuration.
pub struct Colors {
    pub background: DynamicRGB,
    pub foreground: DynamicRGB,
    pub cursor: DynamicRGB,
    pub palette: DynamicPalette,
}

impl Colors {
    // pub const default: Colors = .{ ... };
    pub fn default() -> Colors {
        todo!()
    }
}

/// This is a set of dirty flags the renderer can use to determine
/// what parts of the screen need to be redrawn. It is up to the renderer
/// to clear these flags.
///
/// This only contains dirty flags for terminal state, not for the screen
/// state. The screen state has its own dirty flags.
// pub const Dirty = packed struct { ... }
#[derive(Debug, Clone, Copy, Default)]
pub struct Dirty {
    /// Set when the color palette is modified in any way.
    pub palette: bool,

    /// Set when the reverse colors mode is modified.
    pub reverse_colors: bool,

    /// Screen clear of some kind. This can be due to a screen change,
    /// erase display, etc.
    pub clear: bool,

    /// Set when the pre-edit is modified.
    pub preedit: bool,

    /// Set when Glyph Protocol registrations may have changed. Registered
    /// glyphs can affect already-visible PUA cells, so this requires a full
    /// render-state rebuild.
    pub glyph_glossary: bool,
}

/// Scrolling region is the area of the screen designated where scrolling
/// occurs. When scrolling the screen, only this viewport is scrolled.
pub struct ScrollingRegion {
    // Top and bottom of the scroll region (0-indexed)
    // Precondition: top < bottom
    pub top: size::CellCountInt,
    pub bottom: size::CellCountInt,

    // Left/right scroll regions.
    // Precondition: right > left
    // Precondition: right <= cols - 1
    pub left: size::CellCountInt,
    pub right: size::CellCountInt,
}

pub struct Options {
    pub cols: size::CellCountInt,
    pub rows: size::CellCountInt,
    pub max_scrollback: usize,
    pub colors: Colors,

    /// The default mode state. When the terminal gets a reset, it
    /// will revert back to this state.
    pub default_modes: modespkg::ModePacked,

    /// The total storage limit for Kitty images in bytes. Has no effect
    /// if kitty images are disabled at build-time.
    // kitty_image_storage_limit: usize = switch (build_options.artifact) { ... }
    pub kitty_image_storage_limit: usize,

    /// The limits for what medium types are allowed for Kitty image loading.
    /// Has no effect if kitty images are disabled otherwise. For example,
    // if no `sys.decode_png` hook is specified, png formats are disabled
    // no matter what.
    // kitty_image_loading_limits: if (build_options.kitty_graphics)
    //     kitty.graphics.LoadingImage.Limits
    // else
    //     void
    pub kitty_image_loading_limits: kitty::graphics_image::Limits,
}

/// The semantic prompt type. This is used when tracking a line type and
/// requires integration with the shell. By default, we mark a line as "none"
/// meaning we don't know what type it is.
///
/// See: https://gitlab.freedesktop.org/Per_Bothner/specifications/blob/master/proposals/semantic-prompts.md
pub enum SemanticPrompt {
    Prompt,
    PromptContinuation,
    Input,
    Command,
}

/// Options for scrolling the viewport of the terminal grid.
///
/// PORT NOTE: the reference `ScrollViewport = union(Tag)` carries a C-ABI
/// `TaggedUnion` (`C`/`CValue`/`cval`). That C-ABI surface is hoisted into
/// `rustty-vt-c` (deferred); the pure-VT type here is a plain Rust enum.
pub enum ScrollViewport {
    /// Scroll to the top of the scrollback
    Top,

    /// Scroll to the bottom, i.e. the top of the active area
    Bottom,

    /// Scroll by some delta amount, up is negative.
    Delta(isize),
}

/// The modes for DECCOLM.
// pub const DeccolmMode = enum(u1) { @"80_cols" = 0, @"132_cols" = 1 };
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeccolmMode {
    Cols80 = 0,
    Cols132 = 1,
}

/// Modal screen changes. These map to the literal terminal
/// modes to enable or disable alternate screen modes. They each
/// have subtle behaviors so we define them as an enum here.
pub enum SwitchScreenMode {
    /// Legacy alternate screen mode. This goes to the alternate
    /// screen or primary screen and only copies the cursor. The
    /// screen is not erased.
    // @"47"
    M47,

    /// Alternate screen mode where the alternate screen is cleared
    /// on exit. The primary screen is never cleared. The cursor is
    /// copied.
    // @"1047"
    M1047,

    /// Save primary screen cursor, switch to alternate screen,
    /// and clear the alternate screen on entry. On exit,
    /// do not clear the screen, and restore the cursor on the
    /// primary screen.
    // @"1049"
    M1049,
}

/// The primary terminal emulation structure. This represents a single
/// "terminal" containing a grid of characters and exposes various operations
/// on that grid. This also maintains the scrollback buffer.
// const Terminal = @This();
pub struct Terminal {
    /// The set of screens behind this terminal (e.g. primary vs alternate).
    pub screens: ScreenSet,

    /// Whether we're currently writing to the status line (DECSASD and DECSSDT).
    /// We don't support a status line currently so we just black hole this
    /// data so that it doesn't mess up our main display.
    pub status_display: ansi::StatusDisplay,

    /// Where the tabstops are.
    pub tabstops: Tabstops,

    /// The size of the terminal.
    pub rows: size::CellCountInt,
    pub cols: size::CellCountInt,

    /// The size of the screen in pixels. This is used for pty events and images
    pub width_px: u32,
    pub height_px: u32,

    /// The current scrolling region.
    pub scrolling_region: ScrollingRegion,

    /// The last reported pwd, if any.
    // pwd: std.ArrayList(u8)
    // PORT NOTE: Zig `std.ArrayList(u8)` byte payload -> `Vec<u8>` with a
    // trailing-0 sentinel (NEVER lossy String). Getter returns `Option<&[u8]>`
    // (NUL-stripped).
    pub pwd: Vec<u8>,

    /// The title of the terminal as set by escape sequences (e.g. OSC 0/2).
    pub title: Vec<u8>,

    /// The color state for this terminal.
    pub colors: Colors,

    /// The previous printed character. This is used for the repeat previous
    /// char CSI (ESC [ <n> b).
    // previous_char: ?u21 = null
    pub previous_char: Option<u32>,

    /// The modes that this terminal currently has active.
    pub modes: modespkg::ModeState,

    /// The most recently set mouse shape for the terminal.
    pub mouse_shape: mouse::Shape,

    /// Per-session Glyph Protocol registrations.
    pub glyph_glossary: GlyphGlossary,

    /// These are just a packed set of flags we may set on the terminal.
    pub flags: Flags,
}

impl Terminal {
    /// Initialize a new terminal.
    pub fn init(alloc: Allocator, opts: Options) -> Result<Terminal, OutOfMemory> {
        let cols = opts.cols;
        let rows = opts.rows;

        // var screen_set: ScreenSet = try .init(alloc, .{ ... });
        // errdefer screen_set.deinit(alloc);
        let mut screen_set: ScreenSet = ScreenSet::init(
            alloc,
            crate::screen::Options {
                cols,
                rows,
                max_scrollback: opts.max_scrollback,
                kitty_image_storage_limit: opts.kitty_image_storage_limit,
                kitty_image_loading_limits: opts.kitty_image_loading_limits,
            },
        )?;

        // PORT NOTE: the reference `errdefer screen_set.deinit(alloc)` covers the
        // tabstops-init failure path. In Rust the `?` on `Tabstops::init` returns
        // before `screen_set` is moved into the returned struct, so we deinit it
        // explicitly on that error to mirror the errdefer (no leak).
        let tabstops = match Tabstops::init(alloc, cols as usize, TABSTOP_INTERVAL as usize) {
            Ok(t) => t,
            Err(_e) => {
                // PORT NOTE: `Tabstops`'s allocation-failure type is its own
                // `OutOfMemory`; both map to the reference `Allocator.Error`. We
                // normalize to `pagelist::OutOfMemory` (this crate's canonical
                // OOM) to match the `init` signature.
                screen_set.deinit(alloc);
                return Err(OutOfMemory);
            }
        };

        Ok(Terminal {
            cols,
            rows,
            width_px: 0,
            height_px: 0,
            screens: screen_set,
            status_display: ansi::StatusDisplay::Main,
            tabstops,
            scrolling_region: ScrollingRegion {
                top: 0,
                bottom: rows - 1,
                left: 0,
                right: cols - 1,
            },
            pwd: Vec::new(),
            title: Vec::new(),
            colors: opts.colors,
            previous_char: None,
            modes: modespkg::ModeState {
                values: opts.default_modes,
                default: opts.default_modes,
                ..modespkg::ModeState::default()
            },
            mouse_shape: mouse::Shape::Text,
            glyph_glossary: GlyphGlossary::EMPTY,
            flags: Flags::default(),
        })
    }

    pub fn deinit(&mut self, alloc: Allocator) {
        // PORT NOTE: `Tabstops::deinit` consumes `self` by value (the owning
        // allocator frees on drop); we take it out via `core::mem::take` (leaving
        // an empty default) to call deinit, matching the reference
        // `self.tabstops.deinit(alloc)`.
        core::mem::take(&mut self.tabstops).deinit(alloc);
        self.screens.deinit(alloc);
        // self.pwd.deinit(alloc); self.title.deinit(alloc);
        // PORT NOTE: `pwd`/`title` are `Vec<u8>`; their backing frees on drop. We
        // clear+shrink to mirror the reference freeing the ArrayList storage.
        self.pwd = Vec::new();
        self.title = Vec::new();
        self.glyph_glossary.deinit(alloc);
        // self.* = undefined;
    }

    /// Return a terminal.Stream that can process VT streams and update this
    /// terminal state. The streams will only process read-only data that
    /// modifies terminal state.
    ///
    /// Sequences that query or otherwise require output will be ignored.
    /// If you want to handle side effects, use `vtHandler` and set the
    /// effects field yourself, then initialize a stream.
    ///
    /// This must be deinitialized by the caller.
    ///
    /// Important: this creates a new stream each time with fresh parser state.
    /// If you need to persist parser state across multiple writes (e.g.
    /// for handling escape sequences split across write boundaries), you
    /// must store and reuse the returned stream.
    pub fn vt_stream(&mut self) -> Stream<'_> {
        // return .initAlloc(self.gpa(), self.vtHandler());
        // PORT NOTE: the reference threads `self.gpa()` (a `std.mem.Allocator`)
        // into the OSC parser. rustty's `Stream::init_alloc` wants a
        // `ZigAllocatorRef`; `gpa()` is the global allocator, so we back the OSC
        // parser with the process-static `GLOBAL_ZIG_ALLOCATOR` (see its note).
        let _gpa = self.gpa();
        let handler = self.vt_handler();
        Stream::init_alloc(
            rustty_lib::allocator::ZigAllocatorRef::new(&GLOBAL_ZIG_ALLOCATOR),
            handler,
        )
    }

    /// This is the handler-side only for vtStream.
    pub fn vt_handler(&mut self) -> Handler<'_> {
        // return .init(self);
        let self_ptr: *mut Terminal = self;
        Handler::init(self_ptr)
    }

    /// The general allocator we should use for this terminal.
    pub fn gpa(&mut self) -> Allocator {
        // return self.screens.active.alloc;
        unsafe { (*self.screens.active).alloc }
    }

    /// Print UTF-8 encoded string to the terminal.
    pub fn print_string(&mut self, str: &[u8]) -> Result<(), OutOfMemory> {
        // const view = try std.unicode.Utf8View.init(str);
        // var it = view.iterator();
        // while (it.nextCodepoint()) |cp| { ... }
        // PORT NOTE: `std.unicode.Utf8View.init` validates UTF-8 then iterates
        // codepoints; an invalid sequence is an error. We decode via
        // `str::from_utf8` (the same validation) and iterate `chars()`.
        let view = core::str::from_utf8(str).map_err(|_| OutOfMemory)?;
        for cp in view.chars() {
            match cp {
                '\n' => {
                    self.carriage_return();
                    self.linefeed()?;
                }

                _ => self.print(cp as u32)?,
            }
        }
        Ok(())
    }

    /// Print the previous printed character a repeated amount of times.
    pub fn print_repeat(&mut self, count_req: usize) -> Result<(), OutOfMemory> {
        if let Some(c) = self.previous_char {
            let count = core::cmp::max(count_req, 1);
            for _ in 0..count {
                self.print(c)?;
            }
        }
        Ok(())
    }

    pub fn print(&mut self, c: u32) -> Result<(), OutOfMemory> {
        // log.debug("print={x} y={} x={}", .{ c, self.screens.active.cursor.y, self.screens.active.cursor.x });

        // If we're not on the main display, do nothing for now
        if self.status_display != ansi::StatusDisplay::Main {
            return Ok(());
        }

        // After doing any printing, wrapping, scrolling, etc. we want to ensure
        // that our screen remains in a consistent state.
        // defer self.screens.active.assertIntegrity();
        // PORT NOTE: Rust has no `defer`; the reference's `defer
        // assertIntegrity()` covers every path after the status check, so we run
        // `print_inner` and assert on the way out (both Ok and Err).
        let result = self.print_inner(c);
        self.active_mut().assert_integrity();
        result
    }

    fn print_inner(&mut self, c: u32) -> Result<(), OutOfMemory> {
        let active: *mut Screen = self.screens.active;

        // Our right margin depends where our cursor is now.
        let right_limit = if unsafe { (*active).cursor.x } > self.scrolling_region.right {
            self.cols
        } else {
            self.scrolling_region.right + 1
        };

        // Perform grapheme clustering if grapheme support is enabled (mode 2027).
        // This is MUCH slower than the normal path so the conditional below is
        // purposely ordered in least-likely to most-likely so we can drop out
        // as quickly as possible.
        if c > 255
            && self.modes.get(modespkg::Mode::GraphemeCluster)
            && unsafe { (*active).cursor.x } > 0
        {
            'grapheme: {
                // We need the previous cell to determine if we're at a grapheme
                // break or not. If we are NOT, then we are still combining the
                // same grapheme, and will be appending to prev.cell. Otherwise, we are
                // in a new cell.
                // const Prev = struct { cell: *Cell, left: size.CellCountInt };
                let left: size::CellCountInt = {
                    // If we have wraparound, then we use the prev col unless
                    // there's a pending wrap, in which case we use the current.
                    if self.modes.get(modespkg::Mode::Wraparound) {
                        u8::from(!unsafe { (*active).cursor.pending_wrap }) as size::CellCountInt
                    } else if unsafe { (*active).cursor.x } != right_limit - 1 {
                        // If we do not have wraparound, the logic is trickier. If
                        // we're not on the last column, then we just use the previous
                        // column. Otherwise, we need to check if there is text to
                        // figure out if we're attaching to the prev or current.
                        1
                    } else {
                        u8::from(unsafe { (*(*active).cursor.page_cell).codepoint() } == 0)
                            as size::CellCountInt
                    }
                };

                // If the previous cell is a wide spacer tail, then we actually
                // want to use the cell before that because that has the actual
                // content.
                let immediate = self.active_mut().cursor_cell_left(left);
                let (mut prev_cell, prev_left): (*mut pagepkg::Cell, size::CellCountInt) =
                    match unsafe { (*immediate).wide() } {
                        pagepkg::Wide::SpacerTail => {
                            (self.active_mut().cursor_cell_left(left + 1), left + 1)
                        }
                        _ => (immediate, left),
                    };

                // If our cell has no content, then this is a new cell and
                // necessarily a grapheme break.
                if unsafe { (*prev_cell).codepoint() } == 0 {
                    break 'grapheme;
                }

                let active: *mut Screen = self.screens.active;

                let mut previous_codepoint: u21 = unsafe { (*prev_cell).content_codepoint() };
                let grapheme_break = {
                    let mut state: unicode::grapheme::BreakState =
                        unicode::grapheme::BreakState::Default;
                    if unsafe { (*prev_cell).has_grapheme() } {
                        let page: *mut Page = unsafe {
                            core::ptr::addr_of_mut!((*(*(*active).cursor.page_pin).node).data)
                        };
                        let cps = unsafe { (*page).lookup_grapheme(prev_cell) }.unwrap();
                        for &cp2 in cps {
                            // log.debug("cp1={x} cp2={x}", .{ previous_codepoint, cp2 });
                            assert!(!unicode::graphemeBreak(previous_codepoint, cp2, &mut state));
                            previous_codepoint = cp2;
                        }
                    }

                    // log.debug("cp1={x} cp2={x} end", .{ previous_codepoint, c });
                    unicode::graphemeBreak(previous_codepoint, c, &mut state)
                };

                // If we can NOT break, this means that "c" is part of a grapheme
                // with the previous char.
                if !grapheme_break {
                    // enum { no_change, wide, narrow }
                    enum DesiredWide {
                        NoChange,
                        Wide,
                        Narrow,
                    }
                    let mut desired_wide: DesiredWide = DesiredWide::NoChange;

                    // If this is an emoji variation selector then we need to modify
                    // the cell width accordingly. VS16 makes the character wide and
                    // VS15 makes it narrow.
                    if c == 0xFE0F || c == 0xFE0E {
                        let prev_props = unicode::table().get(previous_codepoint);
                        // Check if it is a valid variation sequence in
                        // emoji-variation-sequences.txt, and if not, ignore the char.
                        if !prev_props.emoji_vs_base {
                            return Ok(());
                        }

                        match c {
                            0xFE0F => desired_wide = DesiredWide::Wide,
                            0xFE0E => desired_wide = DesiredWide::Narrow,
                            _ => unreachable!(),
                        }
                    } else if !unicode::table().get(c).width_zero_in_grapheme {
                        // If we have a code point that contributes to the width of a
                        // grapheme, it necessarily means that we're at least at width
                        // 2, since the first code point must be at least width 1 to
                        // start. (Note that Prepend code points could effectively mean
                        // the first code point should be width 0, but we don't handle
                        // that yet.)
                        desired_wide = DesiredWide::Wide;
                    }

                    match desired_wide {
                        DesiredWide::Wide => 'wide: {
                            if unsafe { (*prev_cell).wide() } == pagepkg::Wide::Wide {
                                break 'wide;
                            }

                            // Move our cursor back to the previous. We'll move
                            // the cursor within this block to the proper location.
                            self.active_mut().cursor_left(prev_left);

                            let active: *mut Screen = self.screens.active;

                            // If we don't have space for the wide char, we need to
                            // insert spacers and wrap. We need special handling if the
                            // previous cell has grapheme data.
                            if unsafe { (*active).cursor.x } == right_limit - 1 {
                                if !self.modes.get(modespkg::Mode::Wraparound) {
                                    return Ok(());
                                }

                                // This path can write a spacer_head before printWrap
                                // which can trigger integrity violations so mark
                                // the wrap first to keep the intermediary state valid
                                // if we're wrapping.
                                let row_wrap = right_limit == self.cols;
                                if row_wrap {
                                    unsafe { (*(*active).cursor.page_row).set_wrap(true) };
                                }

                                let prev_cp = unsafe { (*prev_cell).content_codepoint() };
                                if unsafe { (*prev_cell).has_grapheme() } {
                                    // This is like printCell but without clearing the
                                    // grapheme data from the cell, so we can move it
                                    // later.
                                    unsafe {
                                        (*prev_cell).set_wide(if row_wrap {
                                            pagepkg::Wide::SpacerHead
                                        } else {
                                            pagepkg::Wide::Narrow
                                        })
                                    };
                                    unsafe { (*prev_cell).set_content_codepoint(0) };

                                    self.print_wrap()?;
                                    self.print_cell(prev_cp, pagepkg::Wide::Wide);

                                    let active: *mut Screen = self.screens.active;
                                    let new_pin = unsafe { *(*active).cursor.page_pin };
                                    let new_rac = new_pin.row_and_cell();

                                    'transfer_graphemes: {
                                        let mut old_pin = match new_pin.up(1) {
                                            Some(p) => p,
                                            None => break 'transfer_graphemes,
                                        };
                                        old_pin.x = right_limit - 1;
                                        let old_rac = old_pin.row_and_cell();

                                        if std::ptr::eq(new_pin.node, old_pin.node) {
                                            let page: *mut Page = unsafe {
                                                core::ptr::addr_of_mut!((*new_pin.node).data)
                                            };
                                            unsafe {
                                                (*page).move_grapheme(prev_cell, new_rac.cell)
                                            };
                                            unsafe {
                                                (*prev_cell)
                                                    .set_content_tag(pagepkg::ContentTag::Codepoint)
                                            };
                                            unsafe {
                                                (*new_rac.cell).set_content_tag(
                                                    pagepkg::ContentTag::CodepointGrapheme,
                                                )
                                            };
                                            unsafe { (*new_rac.row).set_grapheme(true) };
                                        } else {
                                            let old_page: *mut Page = unsafe {
                                                core::ptr::addr_of_mut!((*old_pin.node).data)
                                            };
                                            let cps = unsafe {
                                                (*old_page).lookup_grapheme(old_rac.cell)
                                            }
                                            .unwrap();
                                            // PORT NOTE: `appendGrapheme` mutates the
                                            // page; collect the codepoints first so we
                                            // don't alias the borrow across the call.
                                            let cps: Vec<u21> = cps.to_vec();
                                            for cp in cps {
                                                self.active_mut()
                                                    .append_grapheme(new_rac.cell, cp)
                                                    .map_err(|_| OutOfMemory)?;
                                            }
                                            let old_page: *mut Page = unsafe {
                                                core::ptr::addr_of_mut!((*old_pin.node).data)
                                            };
                                            unsafe { (*old_page).clear_grapheme(old_rac.cell) };
                                        }

                                        let old_page: *mut Page = unsafe {
                                            core::ptr::addr_of_mut!((*old_pin.node).data)
                                        };
                                        unsafe {
                                            (*old_page).update_row_grapheme_flag(old_rac.row)
                                        };
                                    }

                                    // Point prev.cell to our new previous cell that
                                    // we'll be appending graphemes to
                                    prev_cell = new_rac.cell;
                                } else {
                                    self.print_cell(
                                        0,
                                        if row_wrap {
                                            pagepkg::Wide::SpacerHead
                                        } else {
                                            pagepkg::Wide::Narrow
                                        },
                                    );
                                    self.print_wrap()?;
                                    self.print_cell(prev_cp, pagepkg::Wide::Wide);

                                    // Point prev.cell to our new previous cell that
                                    // we'll be appending graphemes to
                                    prev_cell = unsafe { (*self.screens.active).cursor.page_cell };
                                }
                            } else {
                                unsafe { (*prev_cell).set_wide(pagepkg::Wide::Wide) };
                            }

                            // Write our spacer, since prev.cell is now wide
                            self.active_mut().cursor_right(1);
                            self.print_cell(0, pagepkg::Wide::SpacerTail);

                            // Move the cursor again so we're beyond our spacer
                            let active: *mut Screen = self.screens.active;
                            if unsafe { (*active).cursor.x } == right_limit - 1 {
                                unsafe { (*active).cursor.pending_wrap = true };
                            } else {
                                self.active_mut().cursor_right(1);
                            }
                        }

                        DesiredWide::Narrow => 'narrow: {
                            // Prev cell is no longer wide
                            if unsafe { (*prev_cell).wide() } != pagepkg::Wide::Wide {
                                break 'narrow;
                            }
                            unsafe { (*prev_cell).set_wide(pagepkg::Wide::Narrow) };

                            // Remove the wide spacer tail
                            let cell = self.active_mut().cursor_cell_left(prev_left - 1);
                            unsafe { (*cell).set_wide(pagepkg::Wide::Narrow) };

                            // Back track the cursor so that we don't end up with
                            // an extra space after the character. Since xterm is
                            // not VS aware, it cannot be used as a reference for
                            // this behavior; but it does follow the principle of
                            // least surprise, and also matches the behavior that
                            // can be observed in Kitty, which is one of the only
                            // other VS aware terminals.
                            let active: *mut Screen = self.screens.active;
                            if unsafe { (*active).cursor.x } == right_limit - 1 {
                                // If we're already at the right edge, we stay
                                // here and set the pending wrap to false since
                                // when we pend a wrap, we only move our cursor once
                                // even for wide chars (tests verify).
                                unsafe { (*active).cursor.pending_wrap = false };
                            } else {
                                // Otherwise, move back.
                                self.active_mut().cursor_left(1);
                            }

                            #[allow(clippy::needless_return)]
                            {
                                break 'narrow;
                            }
                        }

                        DesiredWide::NoChange => {}
                    }

                    // log.debug("c={X} grapheme attach to left={} primary_cp={X}", ...);
                    self.active_mut().cursor_mark_dirty();
                    self.active_mut()
                        .append_grapheme(prev_cell, c)
                        .map_err(|_| OutOfMemory)?;
                    return Ok(());
                }
            }
        }

        let active: *mut Screen = self.screens.active;

        // Determine the width of this character so we can handle
        // non-single-width characters properly. We have a fast-path for
        // byte-sized characters since they're so common. We can ignore
        // control characters because they're always filtered prior.
        let width: usize = if c <= 0xFF {
            1
        } else {
            unicode::table().get(c).width as usize
        };

        // Note: it is possible to have a width of "3" and a width of "-1" from
        // uucode.x's wcwidth. We should look into those cases and handle them
        // appropriately.
        assert!(width <= 2);
        // log.debug("c={x} width={}", .{ c, width });

        // Attach zero-width characters to our cell as grapheme data.
        if width == 0 {
            // If we have grapheme clustering enabled, we don't blindly attach
            // any zero width character to our cells and we instead just ignore
            // it.
            if self.modes.get(modespkg::Mode::GraphemeCluster) {
                return Ok(());
            }

            // If we have wraparound enabled and a pending wrap, the character
            // we're attaching to is still under the cursor. Otherwise, it's the
            // cell to the left.
            let left: size::CellCountInt = if self.modes.get(modespkg::Mode::Wraparound)
                && unsafe { (*active).cursor.pending_wrap }
            {
                0
            } else {
                1
            };

            // If we're at cell zero and not pending a wrap, then this is malformed
            // data and we don't print anything or even store this. Zero-width
            // characters are ALWAYS attached to some other non-zero-width
            // character at the time of writing.
            if unsafe { (*active).cursor.x } == 0 && left == 1 {
                // log.warn("zero-width character with no prior character, ignoring", .{});
                return Ok(());
            }

            // Find our previous cell
            let prev: *mut pagepkg::Cell = {
                let immediate = self.active_mut().cursor_cell_left(left);
                if unsafe { (*immediate).wide() } != pagepkg::Wide::SpacerTail {
                    immediate
                } else {
                    self.active_mut().cursor_cell_left(left + 1)
                }
            };

            // If our previous cell has no text, just ignore the zero-width character
            if !unsafe { (*prev).has_text() } {
                // log.warn("zero-width character with no prior character, ignoring", .{});
                return Ok(());
            }

            // If this is a emoji variation selector, prev must be an emoji
            if c == 0xFE0F || c == 0xFE0E {
                let prev_props = unicode::table().get(unsafe { (*prev).content_codepoint() });
                let emoji = prev_props.grapheme_break
                    == unicode::props::GraphemeBreakNoControl::ExtendedPictographic;
                if !emoji {
                    return Ok(());
                }
            }

            self.active_mut()
                .append_grapheme(prev, c)
                .map_err(|_| OutOfMemory)?;
            return Ok(());
        }

        // We have a printable character, save it
        self.previous_char = Some(c);

        // If we're soft-wrapping, then handle that first.
        if unsafe { (*active).cursor.pending_wrap } && self.modes.get(modespkg::Mode::Wraparound) {
            self.print_wrap()?;
        }

        let active: *mut Screen = self.screens.active;

        // If we have insert mode enabled then we need to handle that. We
        // only do insert mode if we're not at the end of the line.
        if self.modes.get(modespkg::Mode::Insert)
            && unsafe { (*active).cursor.x } + (width as size::CellCountInt) < self.cols
        {
            self.insert_blanks(width);
        }

        let active: *mut Screen = self.screens.active;

        match width {
            // Single cell is very easy: just write in the cell
            1 => {
                self.active_mut().cursor_mark_dirty();
                self.print_cell(c, pagepkg::Wide::Narrow);
            }

            // Wide character requires a spacer. We print this by
            // using two cells: the first is flagged "wide" and has the
            // wide char. The second is guaranteed to be a spacer if
            // we're not at the end of the line.
            2 => {
                if (right_limit - self.scrolling_region.left) > 1 {
                    // If we don't have space for the wide char, we need
                    // to insert spacers and wrap. Then we just print the wide
                    // char as normal.
                    if unsafe { (*active).cursor.x } == right_limit - 1 {
                        // If we don't have wraparound enabled then we don't print
                        // this character at all and don't move the cursor. This is
                        // how xterm behaves.
                        if !self.modes.get(modespkg::Mode::Wraparound) {
                            return Ok(());
                        }

                        // We only create a spacer head if we're at the real edge
                        // of the screen. Otherwise, we clear the space with a narrow.
                        // This allows soft wrapping to work correctly.
                        if right_limit == self.cols {
                            // Special-case: we need to set wrap to true even
                            // though we call printWrap below because if there is
                            // a page resize during printCell then it'll fail
                            // integrity checks.
                            unsafe { (*(*active).cursor.page_row).set_wrap(true) };
                            self.print_cell(0, pagepkg::Wide::SpacerHead);
                        } else {
                            self.print_cell(0, pagepkg::Wide::Narrow);
                        }
                        self.print_wrap()?;
                    }

                    self.active_mut().cursor_mark_dirty();
                    self.print_cell(c, pagepkg::Wide::Wide);
                    self.active_mut().cursor_right(1);
                    self.print_cell(0, pagepkg::Wide::SpacerTail);
                } else {
                    // This is pretty broken, terminals should never be only 1-wide.
                    // We should prevent this downstream.
                    self.active_mut().cursor_mark_dirty();
                    self.print_cell(0, pagepkg::Wide::Narrow);
                }
            }

            _ => unreachable!(),
        }

        let active: *mut Screen = self.screens.active;

        // If we're at the column limit, then we need to wrap the next time.
        // In this case, we don't move the cursor.
        if unsafe { (*active).cursor.x } == right_limit - 1 {
            unsafe { (*active).cursor.pending_wrap = true };
            return Ok(());
        }

        // Move the cursor
        self.active_mut().cursor_right(1);
        Ok(())
    }

    fn print_cell(&mut self, unmapped_c: u32, wide: pagepkg::Wide) {
        // defer self.screens.active.assertIntegrity();

        // TODO: spacers should use a bgcolor only cell

        let active: *mut Screen = self.screens.active;

        let c: u32 = {
            // TODO: non-utf8 handling, gr

            // If we're single shifting, then we use the key exactly once.
            let key = if let Some(key_once) = unsafe { (*active).charset.single_shift } {
                unsafe { (*active).charset.single_shift = None };
                key_once
            } else {
                unsafe { (*active).charset.gl }
            };

            let set = unsafe { (*active).charset.charsets.get(key) };

            // UTF-8 or ASCII is used as-is
            if set == charsets::Charset::Utf8 || set == charsets::Charset::Ascii {
                unmapped_c
            } else if unmapped_c > u8::MAX as u32 {
                // If we're outside of ASCII range this is an invalid value in
                // this table so we just return space.
                ' ' as u32
            } else {
                // Get our lookup table and map it
                let table = charsets::table(set);
                table[unmapped_c as usize] as u32
            }
        };

        let cell: *mut pagepkg::Cell = unsafe { (*active).cursor.page_cell };

        // If the wide property of this cell is the same, then we don't
        // need to do the special handling here because the structure will
        // be the same. If it is NOT the same, then we may need to clear some
        // cells.
        if unsafe { (*cell).wide() } != wide {
            match unsafe { (*cell).wide() } {
                // Previous cell was narrow. Do nothing.
                pagepkg::Wide::Narrow => {}

                // Previous cell was wide. We need to clear the tail and head.
                pagepkg::Wide::Wide => 'wide: {
                    if unsafe { (*active).cursor.x } >= self.cols - 1 {
                        break 'wide;
                    }

                    let spacer_cell = self.active_mut().cursor_cell_right(1);
                    let active: *mut Screen = self.screens.active;
                    let page: *mut Page = unsafe {
                        core::ptr::addr_of_mut!((*(*(*active).cursor.page_pin).node).data)
                    };
                    let row = unsafe { (*active).cursor.page_row };
                    self.active_mut().clear_cells(
                        unsafe { &mut *page },
                        unsafe { &mut *row },
                        unsafe { core::slice::from_raw_parts_mut(spacer_cell, 1) },
                    );

                    // If we're near the left edge, a wide char may have
                    // wrapped from the previous row, leaving a spacer_head
                    // at the end of that row. Clear it so the previous row
                    // doesn't keep a stale spacer_head.
                    let active: *mut Screen = self.screens.active;
                    if unsafe { (*active).cursor.y } > 0 && unsafe { (*active).cursor.x } <= 1 {
                        let head_cell = self.active_mut().cursor_cell_end_of_prev();
                        if unsafe { (*head_cell).wide() } == pagepkg::Wide::SpacerHead {
                            unsafe { (*head_cell).set_wide(pagepkg::Wide::Narrow) };
                        }
                    }
                }

                pagepkg::Wide::SpacerTail => {
                    assert!(unsafe { (*active).cursor.x } > 0);

                    // So integrity checks pass. We fix this up later so we don't
                    // need to do this without safety checks.
                    if cfg!(debug_assertions) {
                        unsafe { (*cell).set_wide(pagepkg::Wide::Narrow) };
                    }

                    let wide_cell = self.active_mut().cursor_cell_left(1);
                    let active: *mut Screen = self.screens.active;
                    let page: *mut Page = unsafe {
                        core::ptr::addr_of_mut!((*(*(*active).cursor.page_pin).node).data)
                    };
                    let row = unsafe { (*active).cursor.page_row };
                    self.active_mut().clear_cells(
                        unsafe { &mut *page },
                        unsafe { &mut *row },
                        unsafe { core::slice::from_raw_parts_mut(wide_cell, 1) },
                    );
                    // If we're near the left edge, a wide char may have
                    // wrapped from the previous row, leaving a spacer_head
                    // at the end of that row. Clear it so the previous row
                    // doesn't keep a stale spacer_head.
                    let active: *mut Screen = self.screens.active;
                    if unsafe { (*active).cursor.y } > 0 && unsafe { (*active).cursor.x } <= 1 {
                        let head_cell = self.active_mut().cursor_cell_end_of_prev();
                        if unsafe { (*head_cell).wide() } == pagepkg::Wide::SpacerHead {
                            unsafe { (*head_cell).set_wide(pagepkg::Wide::Narrow) };
                        }
                    }
                }

                // TODO: this case was not handled in the old terminal implementation
                // but it feels like we should do something. investigate other
                // terminals (xterm mainly) and see what's up.
                pagepkg::Wide::SpacerHead => {}
            }
        }

        let active: *mut Screen = self.screens.active;

        // If the prior value had graphemes, clear those
        if unsafe { (*cell).has_grapheme() } {
            let page: *mut Page =
                unsafe { core::ptr::addr_of_mut!((*(*(*active).cursor.page_pin).node).data) };
            unsafe { (*page).clear_grapheme(cell) };
            unsafe { (*page).update_row_grapheme_flag((*active).cursor.page_row) };
        }

        // We don't need to update the style refs unless the
        // cell's new style will be different after writing.
        let style_changed = unsafe { (*cell).style_id() } != unsafe { (*active).cursor.style_id };
        if style_changed {
            let page: *mut Page =
                unsafe { core::ptr::addr_of_mut!((*(*(*active).cursor.page_pin).node).data) };

            // Release the old style.
            if unsafe { (*cell).style_id() } != style::DEFAULT_ID {
                assert!(unsafe { (*(*active).cursor.page_row).styled() });
                unsafe {
                    let base = (*page).memory_ptr();
                    (*page).styles.release(base, (*cell).style_id());
                }
            }
        }

        // Keep track if we had a hyperlink so we can unset it.
        let had_hyperlink = unsafe { (*cell).hyperlink() };

        // Write
        let mut new_cell = pagepkg::Cell::zeroed();
        new_cell.set_content_tag(pagepkg::ContentTag::Codepoint);
        new_cell.set_content_codepoint(c as u21);
        new_cell.set_style_id(unsafe { (*active).cursor.style_id });
        new_cell.set_wide(wide);
        new_cell.set_protected(unsafe { (*active).cursor.protected });
        new_cell.set_semantic_content(unsafe { (*active).cursor.semantic_content });
        unsafe { *cell = new_cell };

        if style_changed {
            let page: *mut Page =
                unsafe { core::ptr::addr_of_mut!((*(*(*active).cursor.page_pin).node).data) };

            // Use the new style.
            if unsafe { (*cell).style_id() } != style::DEFAULT_ID {
                unsafe {
                    let base = (*page).memory_ptr();
                    (*page).styles.use_id(base, (*cell).style_id());
                }
                unsafe { (*(*active).cursor.page_row).set_styled(true) };
            }
        }

        // If this is a Kitty unicode placeholder then we need to mark the
        // row so that the renderer can lookup rows with these much faster.
        if crate::options::KITTY_GRAPHICS && c == kitty::graphics_unicode::PLACEHOLDER {
            unsafe { (*(*active).cursor.page_row).set_kitty_virtual_placeholder(true) };
        }

        // We check for an active hyperlink first because setHyperlink
        // handles clearing the old hyperlink and an optimization if we're
        // overwriting the same hyperlink.
        if unsafe { (*active).cursor.hyperlink_id } > 0 {
            if let Err(_err) = self.active_mut().cursor_set_hyperlink() {
                // log.warn("error reallocating for more hyperlink space, ignoring hyperlink err={}", .{err});
                assert!(!unsafe { (*cell).hyperlink() });
            }
        } else if had_hyperlink {
            // If the previous cell had a hyperlink then we need to clear it.
            let page: *mut Page =
                unsafe { core::ptr::addr_of_mut!((*(*(*active).cursor.page_pin).node).data) };
            unsafe { (*page).clear_hyperlink(cell) };
            unsafe { (*page).update_row_hyperlink_flag((*active).cursor.page_row) };
        }

        self.active_mut().assert_integrity();
    }

    fn print_wrap(&mut self) -> Result<(), OutOfMemory> {
        // We only mark that we soft-wrapped if we're at the edge of our
        // full screen. We don't mark the row as wrapped if we're in the
        // middle due to a right margin.
        // const cursor: *Screen.Cursor = &self.screens.active.cursor;
        // const mark_wrap = cursor.x == self.cols - 1;
        // if (mark_wrap) cursor.page_row.wrap = true;
        let cols = self.cols;
        let active: *mut Screen = self.screens.active;
        let mark_wrap = unsafe { (*active).cursor.x } == cols - 1;
        if mark_wrap {
            unsafe { (*(*active).cursor.page_row).set_wrap(true) };
        }

        // Get the old semantic prompt so we can extend it to the next
        // line. We need to do this before we index() because we may
        // modify memory.
        let old_semantic = unsafe { (*active).cursor.semantic_content };
        let old_semantic_clear = unsafe { (*active).cursor.semantic_content_clear_eol };

        // Move to the next line
        self.index()?;
        let active: *mut Screen = self.screens.active;
        let sr_left = self.scrolling_region.left;
        self.active_mut().cursor_horizontal_absolute(sr_left);

        // Our pointer should never move
        // assert(cursor == &self.screens.active.cursor);

        // We always reset our semantic prompt state
        unsafe { (*active).cursor.semantic_content = old_semantic };
        unsafe { (*active).cursor.semantic_content_clear_eol = old_semantic_clear };
        match old_semantic {
            pagepkg::SemanticContent::Output | pagepkg::SemanticContent::Input => {}
            pagepkg::SemanticContent::Prompt => unsafe {
                (*(*active).cursor.page_row)
                    .set_semantic_prompt(pagepkg::SemanticPrompt::PromptContinuation)
            },
        }

        if mark_wrap {
            let row = unsafe { (*active).cursor.page_row };
            // Always mark the row as a continuation
            unsafe { (*row).set_wrap_continuation(true) };
        }

        // Assure that our screen is consistent
        self.active_mut().assert_integrity();
        Ok(())
    }

    /// Set the charset into the given slot.
    pub fn configure_charset(&mut self, slot: charsets::Slots, set: charsets::Charset) {
        todo!()
    }

    /// Invoke the charset in slot into the active slot. If single is true,
    /// then this will only be invoked for a single character.
    pub fn invoke_charset(
        &mut self,
        active: charsets::ActiveSlot,
        slot: charsets::Slots,
        single: bool,
    ) {
        todo!()
    }

    /// Carriage return moves the cursor to the first column.
    pub fn carriage_return(&mut self) {
        todo!()
    }

    /// Linefeed moves the cursor to the next line.
    pub fn linefeed(&mut self) -> Result<(), OutOfMemory> {
        todo!()
    }

    /// Backspace moves the cursor back a column (but not less than 0).
    pub fn backspace(&mut self) {
        todo!()
    }

    /// Move the cursor up amount lines. If amount is greater than the maximum
    /// move distance then it is internally adjusted to the maximum. If amount is
    /// 0, adjust it to 1.
    pub fn cursor_up(&mut self, count_req: usize) {
        todo!()
    }

    /// Move the cursor down amount lines. If amount is greater than the maximum
    /// move distance then it is internally adjusted to the maximum. This sequence
    /// will not scroll the screen or scroll region. If amount is 0, adjust it to 1.
    pub fn cursor_down(&mut self, count_req: usize) {
        todo!()
    }

    /// Move the cursor right amount columns. If amount is greater than the
    /// maximum move distance then it is internally adjusted to the maximum.
    /// This sequence will not scroll the screen or scroll region. If amount is
    /// 0, adjust it to 1.
    pub fn cursor_right(&mut self, count_req: usize) {
        todo!()
    }

    /// Move the cursor to the left amount cells. If amount is 0, adjust it to 1.
    pub fn cursor_left(&mut self, count_req: usize) {
        todo!()
    }

    /// Save cursor position and further state.
    ///
    /// The primary and alternate screen have distinct save state. One saved state
    /// is kept per screen (main / alternative). If for the current screen state
    /// was already saved it is overwritten.
    pub fn save_cursor(&mut self) {
        todo!()
    }

    /// Restore cursor position and other state.
    ///
    /// The primary and alternate screen have distinct save state.
    /// If no save was done before values are reset to their initial values.
    pub fn restore_cursor(&mut self) {
        todo!()
    }

    /// Set the character protection mode for the terminal.
    pub fn set_protected_mode(&mut self, mode: ansi::ProtectedMode) {
        todo!()
    }

    /// Perform a semantic prompt command.
    ///
    /// If there is an error, we do our best to get the terminal into
    /// some coherent state, since callers typically can't handle errors
    /// (since they're sending sequences via the pty).
    pub fn semantic_prompt(&mut self, cmd: osc::SemanticPrompt) -> Result<(), OutOfMemory> {
        todo!()
    }

    // OSC 133;L
    fn semantic_prompt_fresh_line(&mut self) -> Result<(), OutOfMemory> {
        todo!()
    }

    /// Returns true if the cursor is currently at a prompt. Another way to look
    /// at this is it returns false if the shell is currently outputting something.
    /// This requires shell integration (semantic prompt integration).
    ///
    /// If the shell integration doesn't exist, this will always return false.
    pub fn cursor_is_at_prompt(&mut self) -> bool {
        todo!()
    }

    /// Horizontal tab moves the cursor to the next tabstop, clearing
    /// the screen to the left the tabstop.
    pub fn horizontal_tab(&mut self) {
        todo!()
    }

    // Same as horizontalTab but moves to the previous tabstop instead of the next.
    pub fn horizontal_tab_back(&mut self) {
        todo!()
    }

    /// Clear tab stops.
    pub fn tab_clear(&mut self, cmd: csi::TabClear) {
        todo!()
    }

    /// Set a tab stop on the current cursor.
    /// TODO: test
    pub fn tab_set(&mut self) {
        todo!()
    }

    /// TODO: test
    pub fn tab_reset(&mut self) {
        todo!()
    }

    /// Move the cursor to the next line in the scrolling region, possibly scrolling.
    ///
    /// If the cursor is outside of the scrolling region: move the cursor one line
    /// down if it is not on the bottom-most line of the screen.
    ///
    /// If the cursor is inside the scrolling region:
    ///   If the cursor is on the bottom-most line of the scrolling region:
    ///     invoke scroll up with amount=1
    ///   If the cursor is not on the bottom-most line of the scrolling region:
    ///     move the cursor one line down
    ///
    /// This unsets the pending wrap state without wrapping.
    pub fn index(&mut self) -> Result<(), OutOfMemory> {
        todo!()
    }

    /// Move the cursor to the previous line in the scrolling region, possibly
    /// scrolling.
    ///
    /// If the cursor is outside of the scrolling region, move the cursor one
    /// line up if it is not on the top-most line of the screen.
    ///
    /// If the cursor is inside the scrolling region:
    ///
    ///   * If the cursor is on the top-most line of the scrolling region:
    ///     invoke scroll down with amount=1
    ///   * If the cursor is not on the top-most line of the scrolling region:
    ///     move the cursor one line up
    pub fn reverse_index(&mut self) {
        todo!()
    }

    /// Set Cursor Position. Move cursor to the position indicated
    /// by row and column (1-indexed). If column is 0, it is adjusted to 1.
    /// If column is greater than the right-most column it is adjusted to
    /// the right-most column. If row is 0, it is adjusted to 1. If row is
    /// greater than the bottom-most row it is adjusted to the bottom-most
    /// row.
    pub fn set_cursor_pos(&mut self, row_req: usize, col_req: usize) {
        todo!()
    }

    /// Set Top and Bottom Margins If bottom is not specified, 0 or bigger than
    /// the number of the bottom-most row, it is adjusted to the number of the
    /// bottom most row.
    ///
    /// If top < bottom set the top and bottom row of the scroll region according
    /// to top and bottom and move the cursor to the top-left cell of the display
    /// (when in cursor origin mode is set to the top-left cell of the scroll region).
    ///
    /// Otherwise: Set the top and bottom row of the scroll region to the top-most
    /// and bottom-most line of the screen.
    ///
    /// Top and bottom are 1-indexed.
    pub fn set_top_and_bottom_margin(&mut self, top_req: usize, bottom_req: usize) {
        todo!()
    }

    /// DECSLRM
    pub fn set_left_and_right_margin(&mut self, left_req: usize, right_req: usize) {
        todo!()
    }

    /// Scroll the text down by one row.
    pub fn scroll_down(&mut self, count: usize) {
        todo!()
    }

    /// Removes amount lines from the top of the scroll region. The remaining lines
    /// to the bottom margin are shifted up and space from the bottom margin up
    /// is filled with empty lines.
    ///
    /// The new lines are created according to the current SGR state.
    ///
    /// Does not change the (absolute) cursor position.
    pub fn scroll_up(&mut self, count: usize) -> Result<(), OutOfMemory> {
        todo!()
    }

    /// Scroll the viewport of the terminal grid.
    pub fn scroll_viewport(&mut self, behavior: ScrollViewport) {
        todo!()
    }

    /// To be called before shifting a row (as in insertLines and deleteLines)
    ///
    /// Takes care of boundary conditions such as potentially split wide chars
    /// across scrolling region boundaries and orphaned spacer heads at line
    /// ends.
    fn row_will_be_shifted(&mut self, page: &mut Page, row: &mut Row) {
        todo!()
    }

    /// Insert amount lines at the current cursor row. The contents of the line
    /// at the current cursor row and below (to the bottom-most line in the
    /// scrolling region) are shifted down by amount lines. The contents of the
    /// amount bottom-most lines in the scroll region are lost.
    ///
    /// This unsets the pending wrap state without wrapping. If the current cursor
    /// position is outside of the current scroll region it does nothing.
    ///
    /// If amount is greater than the remaining number of lines in the scrolling
    /// region it is adjusted down (still allowing for scrolling out every remaining
    /// line in the scrolling region)
    ///
    /// In left and right margin mode the margins are respected; lines are only
    /// scrolled in the scroll region.
    ///
    /// All cleared space is colored according to the current SGR state.
    ///
    /// Moves the cursor to the left margin.
    pub fn insert_lines(&mut self, count: usize) {
        todo!()
    }

    /// Removes amount lines from the current cursor row down. The remaining lines
    /// to the bottom margin are shifted up and space from the bottom margin up is
    /// filled with empty lines.
    ///
    /// If the current cursor position is outside of the current scroll region it
    /// does nothing. If amount is greater than the remaining number of lines in the
    /// scrolling region it is adjusted down.
    ///
    /// In left and right margin mode the margins are respected; lines are only
    /// scrolled in the scroll region.
    ///
    /// If the cell movement splits a multi cell character that character cleared,
    /// by replacing it by spaces, keeping its current attributes. All other
    /// cleared space is colored according to the current SGR state.
    ///
    /// Moves the cursor to the left margin.
    pub fn delete_lines(&mut self, count: usize) {
        todo!()
    }

    /// Inserts spaces at current cursor position moving existing cell contents
    /// to the right. The contents of the count right-most columns in the scroll
    /// region are lost. The cursor position is not changed.
    ///
    /// This unsets the pending wrap state without wrapping.
    ///
    /// The inserted cells are colored according to the current SGR state.
    pub fn insert_blanks(&mut self, count: usize) {
        todo!()
    }

    /// Removes amount characters from the current cursor position to the right.
    /// The remaining characters are shifted to the left and space from the right
    /// margin is filled with spaces.
    ///
    /// If amount is greater than the remaining number of characters in the
    /// scrolling region, it is adjusted down.
    ///
    /// Does not change the cursor position.
    pub fn delete_chars(&mut self, count_req: usize) {
        todo!()
    }

    pub fn erase_chars(&mut self, count_req: usize) {
        todo!()
    }

    /// Erase the line.
    pub fn erase_line(&mut self, mode: csi::EraseLine, protected_req: bool) {
        todo!()
    }

    /// Erase the display.
    pub fn erase_display(&mut self, mode: csi::EraseDisplay, protected_req: bool) {
        todo!()
    }

    /// Resets all margins and fills the whole screen with the character 'E'
    ///
    /// Sets the cursor to the top left corner.
    pub fn decaln(&mut self) -> Result<(), OutOfMemory> {
        todo!()
    }

    /// Execute a kitty graphics command. The buf is used to populate with
    /// the response that should be sent as an APC sequence. The response will
    /// be a full, valid APC sequence.
    ///
    /// If an error occurs, the caller should response to the pty that a
    /// an error occurred otherwise the behavior of the graphics protocol is
    /// undefined.
    pub fn kitty_graphics(
        &mut self,
        alloc: Allocator,
        cmd: &mut kitty::graphics::Command,
    ) -> Option<kitty::graphics::Response> {
        todo!()
    }

    /// Execute a Glyph Protocol APC command against this terminal's per-session
    /// glossary. The returned response, if any, should be sent back to the pty as
    /// a complete APC sequence via `Response.formatWire`.
    pub fn glyph_protocol(
        &mut self,
        alloc: Allocator,
        req: &GlyphRequest,
    ) -> Option<GlyphResponse> {
        todo!()
    }

    /// Set the storage size limit for Kitty graphics across all screens.
    pub fn set_kitty_graphics_size_limit(
        &mut self,
        alloc: Allocator,
        limit: usize,
    ) -> Result<(), OutOfMemory> {
        todo!()
    }

    /// Set the allowed medium types for Kitty graphics image loading
    /// across all screens.
    pub fn set_kitty_graphics_loading_limits(&mut self, limits: kitty::graphics_image::Limits) {
        todo!()
    }

    /// Set a style attribute.
    pub fn set_attribute(&mut self, attr: sgr::Attribute) -> Result<(), OutOfMemory> {
        todo!()
    }

    /// Print the active attributes as a string. This is used to respond to DECRQSS
    /// requests.
    ///
    /// Boolean attributes are printed first, followed by foreground color, then
    /// background color. Each attribute is separated by a semicolon.
    pub fn print_attributes<'a>(&mut self, buf: &'a mut [u8]) -> Result<&'a [u8], OutOfMemory> {
        todo!()
    }

    /// DECCOLM changes the terminal width between 80 and 132 columns. This
    /// function call will do NOTHING unless `setDeccolmSupported` has been
    /// called with "true".
    ///
    /// This breaks the expectation around modern terminals that they resize
    /// with the window. This will fix the grid at either 80 or 132 columns.
    /// The rows will continue to be variable.
    pub fn deccolm(&mut self, alloc: Allocator, mode: DeccolmMode) -> Result<(), OutOfMemory> {
        todo!()
    }

    /// Resize the underlying terminal.
    pub fn resize(
        &mut self,
        alloc: Allocator,
        cols: size::CellCountInt,
        rows: size::CellCountInt,
    ) -> Result<(), OutOfMemory> {
        todo!()
    }

    /// Set the pwd for the terminal.
    pub fn set_pwd(&mut self, pwd: &[u8]) -> Result<(), OutOfMemory> {
        todo!()
    }

    /// Returns the pwd for the terminal, if any. The memory is owned by the
    /// Terminal and is not copied. It is safe until a reset or setPwd.
    // pub fn getPwd(self: *const Terminal) ?[:0]const u8
    // PORT NOTE: Zig `[:0]const u8` -> `Option<&[u8]>` (NUL sentinel stripped).
    pub fn get_pwd(&self) -> Option<&[u8]> {
        todo!()
    }

    /// Set the title for the terminal, as set by escape sequences (e.g. OSC 0/2).
    pub fn set_title(&mut self, t: &[u8]) -> Result<(), OutOfMemory> {
        todo!()
    }

    /// Returns the title for the terminal, if any. The memory is owned by the
    /// Terminal and is not copied. It is safe until a reset or setTitle.
    pub fn get_title(&self) -> Option<&[u8]> {
        todo!()
    }

    /// Switch to the given screen type (alternate or primary).
    ///
    /// This does NOT handle behaviors such as clearing the screen,
    /// copying the cursor, etc. This should be handled by downstream
    /// callers.
    ///
    /// After calling this function, the `self.screen` field will point
    /// to the current screen, and the returned value will be the previous
    /// screen. If the return value is null, then the screen was not
    /// switched because it was already the active screen.
    ///
    /// Note: This is written in a generic way so that we can support
    /// more than two screens in the future if needed. There isn't
    /// currently a spec for this, but it is something I think might
    /// be useful in the future.
    pub fn switch_screen(
        &mut self,
        key: crate::screen_set::Key,
    ) -> Result<Option<*mut Screen>, OutOfMemory> {
        todo!()
    }

    /// Switch screen via a mode switch (e.g. mode 47, 1047, 1049).
    /// This is a much more opinionated operation than `switchScreen`
    /// since it also handles the behaviors of the specific mode,
    /// such as clearing the screen, saving/restoring the cursor,
    /// etc.
    ///
    /// This should be used for legacy compatibility with VT protocols,
    /// but more modern usage should use `switchScreen` instead and handle
    /// details like clearing the screen, cursor saving, etc. manually.
    pub fn switch_screen_mode(
        &mut self,
        mode: SwitchScreenMode,
        enabled: bool,
    ) -> Result<(), OutOfMemory> {
        todo!()
    }

    /// Return the current string value of the terminal. Newlines are
    /// encoded as "\n". This omits any formatting such as fg/bg.
    ///
    /// The caller must free the string.
    pub fn plain_string(&mut self, alloc: Allocator) -> Result<Vec<u8>, OutOfMemory> {
        // return try self.screens.active.dumpStringAlloc(alloc, .{ .viewport = .{} });
        self.active_mut()
            .dump_string_alloc(alloc, point::Point::Viewport(point::Coordinate::default()))
    }

    /// Same as plainString, but respects row wrap state when building the string.
    pub fn plain_string_unwrapped(&mut self, alloc: Allocator) -> Result<Vec<u8>, OutOfMemory> {
        // return try self.screens.active.dumpStringAllocUnwrapped(alloc, .{ .viewport = .{} });
        self.active_mut().dump_string_alloc_unwrapped(
            alloc,
            point::Point::Viewport(point::Coordinate::default()),
        )
    }

    /// Full reset.
    ///
    /// This will attempt to free the existing screen memory but if that fails
    /// this will reuse the existing memory. In the latter case, memory may
    /// be wasted (since its unused) but it isn't leaked.
    pub fn full_reset(&mut self) {
        todo!()
    }

    /// Returns true if the point is dirty, used for testing.
    fn is_dirty(&self, pt: point::Point) -> bool {
        todo!()
    }

    /// Clear all dirty bits. Testing only.
    fn clear_dirty(&mut self) {
        todo!()
    }

    // --- PORT NOTE: Rust-only glue (no logic; not from Terminal.zig) -------
    // The reference accesses the active screen inline as `self.screens.active`
    // (a `*Screen` raw pointer). The Rust port centralizes that raw-pointer
    // deref behind a helper, mirroring `screen_set` exposing `active: *mut
    // Screen`. This is the `*mut Screen` seam the method bodies use in vt-12b.
    /// The active screen for this terminal (`self.screens.active`).
    ///
    /// # Safety
    /// `self.screens.active` aliases an entry owned by `self.screens.all`; this
    /// returns a `&mut Screen` borrowed from `&mut self`, so the usual aliasing
    /// rules apply (don't hold it across a `screens` mutation that reallocates).
    #[inline]
    pub fn active_mut(&mut self) -> &mut Screen {
        // unsafe { &mut *self.screens.active }
        unsafe { &mut *self.screens.active }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use allocator_api2::alloc::Global;

    // PORT NOTE: the reference tests call `init(alloc, .{ .cols = .., .rows = .. })`
    // and rely on `Options` field defaults (`max_scrollback = 10_000`,
    // `colors = .default`, `default_modes = .{}`, kitty limits). Rust `Options`
    // has no field defaults, so this helper supplies them verbatim, mirroring the
    // reference `Options` declaration. `colors = .default` expands to
    // `Colors{ background/foreground/cursor = .unset, palette = .default }`
    // (the still-`todo!()` `Colors::default()` inlined here for tests only).
    fn opts(cols: size::CellCountInt, rows: size::CellCountInt) -> Options {
        Options {
            cols,
            rows,
            max_scrollback: 10_000,
            colors: Colors {
                background: DynamicRGB::UNSET,
                foreground: DynamicRGB::UNSET,
                cursor: DynamicRGB::UNSET,
                palette: DynamicPalette::default_palette(),
            },
            default_modes: modespkg::ModePacked::default(),
            kitty_image_storage_limit: 320 * 1000 * 1000,
            kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
        }
    }

    // Screen-point getCell helper mirroring the reference
    // `t.screens.active.pages.getCell(.{ .screen = .{ .x, .y } })`.
    fn screen_pt(x: size::CellCountInt, y: u32) -> point::Point {
        point::Point::Screen(point::Coordinate { x, y })
    }

    // test "Terminal: zero-width character attaches to pending wrap cell"
    // https://github.com/ghostty-org/ghostty/pull/12581
    #[test]
    fn terminal_zero_width_character_attaches_to_pending_wrap_cell() {
        let mut t = Terminal::init(Global, opts(2, 2)).unwrap();

        // Disable grapheme clustering to exercise the fallback path.
        t.modes.set(modespkg::Mode::GraphemeCluster, false);

        t.print('x' as u32).unwrap();
        t.print('å' as u32).unwrap();
        t.print(0x0332).unwrap(); // Combining low line.

        let str = t.plain_string(Global).unwrap();
        assert_eq!(str.as_slice(), "xå̲".as_bytes());

        t.deinit(Global);
    }

    // test "Terminal: VS16 doesn't make character with 2027 disabled"
    #[test]
    fn terminal_vs16_doesnt_make_character_with_2027_disabled() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        // Disable grapheme clustering
        t.modes.set(modespkg::Mode::GraphemeCluster, false);

        t.print(0x2764).unwrap(); // Heart
        t.print(0xFE0F).unwrap(); // VS16 to make wide

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "❤️".as_bytes());
        }

        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 0)).unwrap();
            let cell = list_cell.cell;
            assert_eq!(unsafe { (*cell).content_codepoint() }, 0x2764);
            assert!(unsafe { (*cell).has_grapheme() });
            assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Narrow);
            let cps = unsafe { (*list_cell.node).data.lookup_grapheme(cell) }.unwrap();
            assert_eq!(cps.len(), 1);
        }

        t.deinit(Global);
    }

    // test "Terminal: print invalid VS16 non-grapheme"
    #[test]
    fn terminal_print_invalid_vs16_non_grapheme() {
        let mut t = Terminal::init(Global, opts(80, 80)).unwrap();

        // https://github.com/mitchellh/ghostty/issues/1482
        t.print('x' as u32).unwrap();
        t.print(0xFE0F).unwrap();

        // We should have 1 narrow cell.
        assert_eq!(unsafe { (*t.screens.active).cursor.y }, 0);
        assert_eq!(unsafe { (*t.screens.active).cursor.x }, 1);

        // Assert various properties about our screen to verify
        // we have all expected cells.
        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 0)).unwrap();
            let cell = list_cell.cell;
            assert_eq!(unsafe { (*cell).content_codepoint() }, 'x' as u21);
            assert!(!unsafe { (*cell).has_grapheme() });
            assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Narrow);
        }
        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(1, 0)).unwrap();
            let cell = list_cell.cell;
            assert_eq!(unsafe { (*cell).content_codepoint() }, 0);
        }

        t.deinit(Global);
    }

    // test "Terminal: variation selectors apply to preceding codepoint"
    // https://github.com/ghostty-org/ghostty/pull/12596
    #[test]
    fn terminal_variation_selectors_apply_to_preceding_codepoint() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        // Enable grapheme clustering
        t.modes.set(modespkg::Mode::GraphemeCluster, true);

        // Pirate flag: black flag + ZWJ + skull and crossbones + VS16.
        t.print(0x1F3F4).unwrap();
        t.print(0x200D).unwrap();
        t.print(0x2620).unwrap();
        t.print(0xFE0F).unwrap();

        let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 0)).unwrap();
        let cell = list_cell.cell;
        assert_eq!(unsafe { (*cell).content_codepoint() }, 0x1F3F4);
        assert!(unsafe { (*cell).has_grapheme() });
        assert_eq!(
            unsafe { (*list_cell.node).data.lookup_grapheme(cell) }.unwrap(),
            &[0x200D, 0x2620, 0xFE0F]
        );

        t.deinit(Global);
    }

    // test "Terminal: print invalid VS15 following emoji is wide"
    #[test]
    fn terminal_print_invalid_vs15_following_emoji_is_wide() {
        let mut t = Terminal::init(Global, opts(80, 80)).unwrap();

        // Enable grapheme clustering
        t.modes.set(modespkg::Mode::GraphemeCluster, true);

        t.print('\u{1F9E0}' as u32).unwrap(); // 🧠
        t.print(0xFE0E).unwrap(); // not valid with U+1F9E0 as base

        // We should have 2 cells taken up. It is one character but "wide".
        assert_eq!(unsafe { (*t.screens.active).cursor.y }, 0);
        assert_eq!(unsafe { (*t.screens.active).cursor.x }, 2);

        // Assert various properties about our screen to verify
        // we have all expected cells.
        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 0)).unwrap();
            let cell = list_cell.cell;
            assert_eq!(unsafe { (*cell).content_codepoint() }, '\u{1F9E0}' as u21);
            assert!(!unsafe { (*cell).has_grapheme() });
            assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Wide);
        }
        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(1, 0)).unwrap();
            let cell = list_cell.cell;
            assert_eq!(unsafe { (*cell).content_codepoint() }, 0);
            assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::SpacerTail);
        }

        t.deinit(Global);
    }

    // test "Terminal: print invalid VS15 in emoji ZWJ sequence"
    #[test]
    fn terminal_print_invalid_vs15_in_emoji_zwj_sequence() {
        let mut t = Terminal::init(Global, opts(80, 80)).unwrap();

        // Enable grapheme clustering
        t.modes.set(modespkg::Mode::GraphemeCluster, true);

        t.print('\u{1F469}' as u32).unwrap(); // 👩
        t.print(0xFE0E).unwrap(); // not valid with U+1F469 as base
        t.print('\u{200D}' as u32).unwrap(); // ZWJ
        t.print('\u{1F466}' as u32).unwrap(); // 👦

        // We should have 2 cells taken up. It is one character but "wide".
        assert_eq!(unsafe { (*t.screens.active).cursor.y }, 0);
        assert_eq!(unsafe { (*t.screens.active).cursor.x }, 2);

        // Assert various properties about our screen to verify
        // we have all expected cells.
        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 0)).unwrap();
            let cell = list_cell.cell;
            assert_eq!(unsafe { (*cell).content_codepoint() }, '\u{1F469}' as u21);
            assert!(unsafe { (*cell).has_grapheme() });
            assert_eq!(
                unsafe { (*list_cell.node).data.lookup_grapheme(cell) }.unwrap(),
                &['\u{200D}' as u21, '\u{1F466}' as u21]
            );
            assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Wide);
        }
        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(1, 0)).unwrap();
            let cell = list_cell.cell;
            assert_eq!(unsafe { (*cell).content_codepoint() }, 0);
            assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::SpacerTail);
        }

        t.deinit(Global);
    }

    // test "Terminal: print invalid VS16 grapheme"
    #[test]
    fn terminal_print_invalid_vs16_grapheme() {
        let mut t = Terminal::init(Global, opts(80, 80)).unwrap();

        // Enable grapheme clustering
        t.modes.set(modespkg::Mode::GraphemeCluster, true);

        // https://github.com/mitchellh/ghostty/issues/1482
        t.print('x' as u32).unwrap();
        t.print(0xFE0F).unwrap(); // invalid VS16

        // We should have 1 cells taken up, and narrow.
        assert_eq!(unsafe { (*t.screens.active).cursor.y }, 0);
        assert_eq!(unsafe { (*t.screens.active).cursor.x }, 1);

        // Assert various properties about our screen to verify
        // we have all expected cells.
        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 0)).unwrap();
            let cell = list_cell.cell;
            assert_eq!(unsafe { (*cell).content_codepoint() }, 'x' as u21);
            assert!(!unsafe { (*cell).has_grapheme() });
            assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Narrow);
        }
        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(1, 0)).unwrap();
            let cell = list_cell.cell;
            assert_eq!(unsafe { (*cell).content_codepoint() }, 0);
            assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Narrow);
        }

        t.deinit(Global);
    }

    // test "Terminal: print invalid VS16 with second char"
    #[test]
    fn terminal_print_invalid_vs16_with_second_char() {
        let mut t = Terminal::init(Global, opts(80, 80)).unwrap();

        // Enable grapheme clustering
        t.modes.set(modespkg::Mode::GraphemeCluster, true);

        // https://github.com/mitchellh/ghostty/issues/1482
        t.print('x' as u32).unwrap();
        t.print(0xFE0F).unwrap();
        t.print('y' as u32).unwrap();

        // We should have 2 cells taken up, from two separate narrow characters.
        assert_eq!(unsafe { (*t.screens.active).cursor.y }, 0);
        assert_eq!(unsafe { (*t.screens.active).cursor.x }, 2);

        // Assert various properties about our screen to verify
        // we have all expected cells.
        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 0)).unwrap();
            let cell = list_cell.cell;
            assert_eq!(unsafe { (*cell).content_codepoint() }, 'x' as u21);
            assert!(!unsafe { (*cell).has_grapheme() });
            assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Narrow);
        }

        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(1, 0)).unwrap();
            let cell = list_cell.cell;
            assert_eq!(unsafe { (*cell).content_codepoint() }, 'y' as u21);
            assert!(!unsafe { (*cell).has_grapheme() });
            assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Narrow);
        }

        t.deinit(Global);
    }

    // test "Terminal: print grapheme ò (o with nonspacing mark) should be narrow"
    #[test]
    fn terminal_print_grapheme_o_with_nonspacing_mark_should_be_narrow() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        // Enable grapheme clustering
        t.modes.set(modespkg::Mode::GraphemeCluster, true);

        t.print('o' as u32).unwrap();
        t.print(0x0300).unwrap(); // combining grave accent

        // We should have 1 cell taken up.
        assert_eq!(unsafe { (*t.screens.active).cursor.y }, 0);
        assert_eq!(unsafe { (*t.screens.active).cursor.x }, 1);

        // Assert various properties about our screen to verify
        // we have all expected cells.
        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 0)).unwrap();
            let cell = list_cell.cell;
            assert_eq!(unsafe { (*cell).content_codepoint() }, 'o' as u21);
            assert!(unsafe { (*cell).has_grapheme() });
            assert_eq!(
                unsafe { (*list_cell.node).data.lookup_grapheme(cell) }.unwrap(),
                &[0x0300]
            );
            assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Narrow);
        }

        t.deinit(Global);
    }

    // test "Terminal: print Devanagari grapheme should be wide"
    #[test]
    fn terminal_print_devanagari_grapheme_should_be_wide() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        // Enable grapheme clustering
        t.modes.set(modespkg::Mode::GraphemeCluster, true);

        // क्‍ष
        t.print(0x0915).unwrap();
        t.print(0x094D).unwrap();
        t.print(0x200D).unwrap();
        t.print(0x0937).unwrap();

        // We should have 2 cells taken up. It is one character but "wide".
        assert_eq!(unsafe { (*t.screens.active).cursor.y }, 0);
        assert_eq!(unsafe { (*t.screens.active).cursor.x }, 2);

        // Assert various properties about our screen to verify
        // we have all expected cells.
        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 0)).unwrap();
            let cell = list_cell.cell;
            assert_eq!(unsafe { (*cell).content_codepoint() }, 0x0915);
            assert!(unsafe { (*cell).has_grapheme() });
            assert_eq!(
                unsafe { (*list_cell.node).data.lookup_grapheme(cell) }.unwrap(),
                &[0x094D, 0x200D, 0x0937]
            );
            assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Wide);
        }
        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(1, 0)).unwrap();
            let cell = list_cell.cell;
            assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::SpacerTail);
        }

        t.deinit(Global);
    }

    // test "Terminal: print invalid VS16 with second char (combining)"
    #[test]
    fn terminal_print_invalid_vs16_with_second_char_combining() {
        let mut t = Terminal::init(Global, opts(80, 80)).unwrap();

        // Enable grapheme clustering
        t.modes.set(modespkg::Mode::GraphemeCluster, true);

        // https://github.com/mitchellh/ghostty/issues/1482
        t.print('n' as u32).unwrap();
        t.print(0xFE0F).unwrap(); // invalid VS16
        t.print(0x0303).unwrap(); // combining tilde

        // We should have 1 cells taken up, and narrow.
        assert_eq!(unsafe { (*t.screens.active).cursor.y }, 0);
        assert_eq!(unsafe { (*t.screens.active).cursor.x }, 1);

        // Assert various properties about our screen to verify
        // we have all expected cells.
        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 0)).unwrap();
            let cell = list_cell.cell;
            assert_eq!(unsafe { (*cell).content_codepoint() }, 'n' as u21);
            assert!(unsafe { (*cell).has_grapheme() });
            assert_eq!(
                unsafe { (*list_cell.node).data.lookup_grapheme(cell) }.unwrap(),
                &['\u{0303}' as u21]
            );
            assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Narrow);
        }
        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(1, 0)).unwrap();
            let cell = list_cell.cell;
            assert_eq!(unsafe { (*cell).content_codepoint() }, 0);
            assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Narrow);
        }

        t.deinit(Global);
    }

    // test "Terminal: print breaks valid grapheme cluster with Prepend + ASCII for speed"
    #[test]
    fn terminal_print_breaks_valid_grapheme_cluster_with_prepend_ascii_for_speed() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();
        t.modes.set(modespkg::Mode::GraphemeCluster, true);

        // Make sure we're not at cursor.x == 0 for the next char.
        t.print('_' as u32).unwrap();

        // U+0600 ARABIC NUMBER SIGN (Prepend)
        t.print(0x0600).unwrap();
        t.print('1' as u32).unwrap();

        // We should have 3 cells taken up, each narrow. Note that this is
        // **incorrect** grapheme break behavior, since a Prepend code point should
        // not break with the one following it per UAX #29 GB9b. However, as an
        // optimization we assume a grapheme break when c <= 255, and note that
        // this deviation only affects these very uncommon scenarios (e.g. the
        // Arabic number sign should precede Arabic-script digits).
        assert_eq!(unsafe { (*t.screens.active).cursor.y }, 0);
        assert_eq!(unsafe { (*t.screens.active).cursor.x }, 3);
        // This is what we'd expect if we did break correctly:
        //try testing.expectEqual(@as(usize, 2), t.screens.active.cursor.x);

        // Assert various properties about our screen to verify
        // we have all expected cells.
        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(1, 0)).unwrap();
            let cell = list_cell.cell;
            assert_eq!(unsafe { (*cell).content_codepoint() }, 0x0600);
            assert!(!unsafe { (*cell).has_grapheme() });
            // This is what we'd expect if we did break correctly:
            //try testing.expect(cell.hasGrapheme());
            //try testing.expectEqualSlices(u21, &.{'1'}, list_cell.node.data.lookupGrapheme(cell).?);
            assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Narrow);
        }
        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(2, 0)).unwrap();
            let cell = list_cell.cell;
            assert_eq!(unsafe { (*cell).content_codepoint() }, '1' as u21);
            // This is what we'd expect if we did break correctly:
            //try testing.expectEqual(@as(u21, 0), cell.content.codepoint);
            assert!(!unsafe { (*cell).has_grapheme() });
            assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Narrow);
        }

        t.deinit(Global);
    }
}
