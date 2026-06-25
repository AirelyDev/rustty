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
use crate::pagelist;
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
    // pub const default: Colors = .{
    //     .background = .unset,
    //     .foreground = .unset,
    //     .cursor = .unset,
    //     .palette = .default,
    // };
    pub fn default() -> Colors {
        Colors {
            background: DynamicRGB::UNSET,
            foreground: DynamicRGB::UNSET,
            cursor: DynamicRGB::UNSET,
            palette: DynamicPalette::default_palette(),
        }
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

/// PORT NOTE: `scrollUp`/`scrollDown` use a `defer` block to restore the
/// cursor x/y and pending_wrap on every exit. Rust has no `defer`; this RAII
/// guard reproduces it — its `Drop` runs the same restore logic whenever the
/// guard leaves scope (normal exit, early `return`, or error `?`).
struct CursorRestoreGuard {
    screen: *mut Screen,
    old_x: size::CellCountInt,
    old_y: size::CellCountInt,
    old_wrap: bool,
}

impl Drop for CursorRestoreGuard {
    fn drop(&mut self) {
        // self.screens.active.cursorAbsolute(old_x, old_y);
        unsafe { (*self.screen).cursor_absolute(self.old_x, self.old_y) };
        // self.screens.active.cursor.pending_wrap = old_wrap;
        unsafe { (*self.screen).cursor.pending_wrap = self.old_wrap };
    }
}

/// PORT NOTE: `index` defers its cursor semantic-prompt handling to AFTER the
/// scroll because it may need to apply to new rows. Rust has no `defer`; this
/// RAII guard's `Drop` reproduces that deferred block, reading `screen.cursor`
/// at drop time (post-scroll) and running on every exit path of `index`.
struct SemanticIndexGuard {
    screen: *mut Screen,
}

impl Drop for SemanticIndexGuard {
    fn drop(&mut self) {
        let screen = self.screen;
        // if (screen.cursor.semantic_content != .output) {
        if unsafe { (*screen).cursor.semantic_content } != pagepkg::SemanticContent::Output {
            // @branchHint(.unlikely);

            // Always reset any semantic content clear-eol state.
            //
            // The specification is not clear what "end-of-line" means. If we
            // discover that there are more scenarios we should be unsetting
            // this we should document and test it.
            // if (screen.cursor.semantic_content_clear_eol) {
            if unsafe { (*screen).cursor.semantic_content_clear_eol } {
                // screen.cursor.semantic_content = .output;
                unsafe { (*screen).cursor.semantic_content = pagepkg::SemanticContent::Output };
                // screen.cursor.semantic_content_clear_eol = false;
                unsafe { (*screen).cursor.semantic_content_clear_eol = false };
            } else {
                // If we aren't clearing our state at EOL and we're not output,
                // then we mark the new row as a prompt continuation. This is
                // to work around shells that don't send OSC 133 k=s sequences
                // for continuations.
                //
                // This can be a false positive if the shell changes content
                // type later and outputs something. We handle that in the
                // semanticPrompt function.
                // screen.cursor.page_row.semantic_prompt = .prompt_continuation;
                unsafe {
                    (*(*screen).cursor.page_row)
                        .set_semantic_prompt(pagepkg::SemanticPrompt::PromptContinuation)
                };
            }
        } else {
            // This should never be set in the output mode.
            // assert(!screen.cursor.semantic_content_clear_eol);
            assert!(!unsafe { (*screen).cursor.semantic_content_clear_eol });
        }
    }
}

/// PORT NOTE: `insertLines`/`deleteLines` end with a `defer` that returns the
/// cursor to the left margin on the row it started on and clears pending_wrap.
/// This RAII guard reproduces that on every exit path.
struct ShiftLinesEndGuard {
    screen: *mut Screen,
    left: size::CellCountInt,
    start_y: size::CellCountInt,
}

impl Drop for ShiftLinesEndGuard {
    fn drop(&mut self) {
        // self.screens.active.cursorAbsolute(self.scrolling_region.left, start_y);
        unsafe { (*self.screen).cursor_absolute(self.left, self.start_y) };
        // Always unset pending wrap
        // self.screens.active.cursor.pending_wrap = false;
        unsafe { (*self.screen).cursor.pending_wrap = false };
    }
}

/// PORT NOTE: reproduces `defer self.screens.active.pages.untrackPin(cur_p)`.
/// `Drop` untracks the pin on every exit path.
struct TrackedPinGuard {
    screen: *mut Screen,
    pin: *mut crate::pagelist::Pin,
}

impl Drop for TrackedPinGuard {
    fn drop(&mut self) {
        unsafe { (*self.screen).pages.untrack_pin(self.pin) };
    }
}

/// PORT NOTE: helper for the reference slice expression `cells[start..end]`
/// over a raw `*mut [Cell]`. Reproduces the sub-slice as a raw `*mut [Cell]`
/// (no intermediate `&mut [Cell]`, preserving the unprotected-pointer aliasing
/// the reference relies on).
fn ptr_slice_range(cells: *mut [pagepkg::Cell], start: usize, end: usize) -> *mut [pagepkg::Cell] {
    let base: *mut pagepkg::Cell = cells as *mut pagepkg::Cell;
    core::ptr::slice_from_raw_parts_mut(unsafe { base.add(start) }, end - start)
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
                        page,
                        row,
                        core::ptr::slice_from_raw_parts_mut(spacer_cell, 1),
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
                        page,
                        row,
                        core::ptr::slice_from_raw_parts_mut(wide_cell, 1),
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
        // self.screens.active.charset.charsets.set(slot, set);
        self.active_mut().charset.charsets.set(slot, set);
    }

    /// Invoke the charset in slot into the active slot. If single is true,
    /// then this will only be invoked for a single character.
    pub fn invoke_charset(
        &mut self,
        active: charsets::ActiveSlot,
        slot: charsets::Slots,
        single: bool,
    ) {
        if single {
            assert!(active == charsets::ActiveSlot::GL);
            self.active_mut().charset.single_shift = Some(slot);
            return;
        }

        match active {
            charsets::ActiveSlot::GL => self.active_mut().charset.gl = slot,
            charsets::ActiveSlot::GR => self.active_mut().charset.gr = slot,
        }
    }

    /// Carriage return moves the cursor to the first column.
    pub fn carriage_return(&mut self) {
        // Always reset pending wrap state
        // self.screens.active.cursor.pending_wrap = false;
        unsafe { (*self.screens.active).cursor.pending_wrap = false };

        // In origin mode we always move to the left margin
        // self.screens.active.cursorHorizontalAbsolute(if (self.modes.get(.origin))
        //     self.scrolling_region.left
        // else if (self.screens.active.cursor.x >= self.scrolling_region.left)
        //     self.scrolling_region.left
        // else
        //     0);
        // PORT NOTE: the reference's first two branches both evaluate to
        // `scrolling_region.left`; this is intentional verbatim structure (origin
        // mode vs. cursor-at-or-right-of-left-margin are distinct conditions that
        // happen to share a result), so we silence `if_same_then_else`.
        #[allow(clippy::if_same_then_else)]
        let x = if self.modes.get(modespkg::Mode::Origin) {
            self.scrolling_region.left
        } else if unsafe { (*self.screens.active).cursor.x } >= self.scrolling_region.left {
            self.scrolling_region.left
        } else {
            0
        };
        self.active_mut().cursor_horizontal_absolute(x);
    }

    /// Linefeed moves the cursor to the next line.
    pub fn linefeed(&mut self) -> Result<(), OutOfMemory> {
        // try self.index();
        // if (self.modes.get(.linefeed)) self.carriageReturn();
        self.index()?;
        if self.modes.get(modespkg::Mode::Linefeed) {
            self.carriage_return();
        }
        Ok(())
    }

    /// Backspace moves the cursor back a column (but not less than 0).
    pub fn backspace(&mut self) {
        // self.cursorLeft(1);
        self.cursor_left(1);
    }

    /// Move the cursor up amount lines. If amount is greater than the maximum
    /// move distance then it is internally adjusted to the maximum. If amount is
    /// 0, adjust it to 1.
    pub fn cursor_up(&mut self, count_req: usize) {
        // Always resets pending wrap
        // self.screens.active.cursor.pending_wrap = false;
        unsafe { (*self.screens.active).cursor.pending_wrap = false };

        // The maximum amount the cursor can move up depends on scrolling regions
        // const max = if (self.screens.active.cursor.y >= self.scrolling_region.top)
        //     self.screens.active.cursor.y - self.scrolling_region.top
        // else
        //     self.screens.active.cursor.y;
        let cursor_y = unsafe { (*self.screens.active).cursor.y };
        let max: usize = if cursor_y >= self.scrolling_region.top {
            (cursor_y - self.scrolling_region.top) as usize
        } else {
            cursor_y as usize
        };
        // const count = @min(max, @max(count_req, 1));
        let count = core::cmp::min(max, core::cmp::max(count_req, 1));

        // We can safely intCast below because of the min/max clamping we did above.
        // self.screens.active.cursorUp(@intCast(count));
        self.active_mut().cursor_up(count as size::CellCountInt);
    }

    /// Move the cursor down amount lines. If amount is greater than the maximum
    /// move distance then it is internally adjusted to the maximum. This sequence
    /// will not scroll the screen or scroll region. If amount is 0, adjust it to 1.
    pub fn cursor_down(&mut self, count_req: usize) {
        // Always resets pending wrap
        // self.screens.active.cursor.pending_wrap = false;
        unsafe { (*self.screens.active).cursor.pending_wrap = false };

        // The max the cursor can move to depends where the cursor currently is
        // const max = if (self.screens.active.cursor.y <= self.scrolling_region.bottom)
        //     self.scrolling_region.bottom - self.screens.active.cursor.y
        // else
        //     self.rows - self.screens.active.cursor.y - 1;
        let cursor_y = unsafe { (*self.screens.active).cursor.y };
        let max: usize = if cursor_y <= self.scrolling_region.bottom {
            (self.scrolling_region.bottom - cursor_y) as usize
        } else {
            (self.rows - cursor_y - 1) as usize
        };
        // const count = @min(max, @max(count_req, 1));
        let count = core::cmp::min(max, core::cmp::max(count_req, 1));
        // self.screens.active.cursorDown(@intCast(count));
        self.active_mut().cursor_down(count as size::CellCountInt);
    }

    /// Move the cursor right amount columns. If amount is greater than the
    /// maximum move distance then it is internally adjusted to the maximum.
    /// This sequence will not scroll the screen or scroll region. If amount is
    /// 0, adjust it to 1.
    pub fn cursor_right(&mut self, count_req: usize) {
        // Always resets pending wrap
        // self.screens.active.cursor.pending_wrap = false;
        unsafe { (*self.screens.active).cursor.pending_wrap = false };

        // The max the cursor can move to depends where the cursor currently is
        // const max = if (self.screens.active.cursor.x <= self.scrolling_region.right)
        //     self.scrolling_region.right - self.screens.active.cursor.x
        // else
        //     self.cols - self.screens.active.cursor.x - 1;
        let cursor_x = unsafe { (*self.screens.active).cursor.x };
        let max: usize = if cursor_x <= self.scrolling_region.right {
            (self.scrolling_region.right - cursor_x) as usize
        } else {
            (self.cols - cursor_x - 1) as usize
        };
        // const count = @min(max, @max(count_req, 1));
        let count = core::cmp::min(max, core::cmp::max(count_req, 1));
        // self.screens.active.cursorRight(@intCast(count));
        self.active_mut().cursor_right(count as size::CellCountInt);
    }

    /// Move the cursor to the left amount cells. If amount is 0, adjust it to 1.
    pub fn cursor_left(&mut self, count_req: usize) {
        // Wrapping behavior depends on various terminal modes
        // const WrapMode = enum { none, reverse, reverse_extended };
        enum WrapMode {
            None,
            Reverse,
            ReverseExtended,
        }
        // const wrap_mode: WrapMode = wrap_mode: {
        //     if (!self.modes.get(.wraparound)) break :wrap_mode .none;
        //     if (self.modes.get(.reverse_wrap_extended)) break :wrap_mode .reverse_extended;
        //     if (self.modes.get(.reverse_wrap)) break :wrap_mode .reverse;
        //     break :wrap_mode .none;
        // };
        let wrap_mode: WrapMode = 'wrap_mode: {
            if !self.modes.get(modespkg::Mode::Wraparound) {
                break 'wrap_mode WrapMode::None;
            }
            if self.modes.get(modespkg::Mode::ReverseWrapExtended) {
                break 'wrap_mode WrapMode::ReverseExtended;
            }
            if self.modes.get(modespkg::Mode::ReverseWrap) {
                break 'wrap_mode WrapMode::Reverse;
            }
            WrapMode::None
        };

        // var count = @max(count_req, 1);
        let mut count: usize = core::cmp::max(count_req, 1);

        // If we are in no wrap mode, then we move the cursor left and exit
        // since this is the fastest and most typical path.
        if matches!(wrap_mode, WrapMode::None) {
            // self.screens.active.cursorLeft(@min(count, self.screens.active.cursor.x));
            let cursor_x = unsafe { (*self.screens.active).cursor.x };
            self.active_mut()
                .cursor_left(core::cmp::min(count, cursor_x as usize) as size::CellCountInt);
            // self.screens.active.cursor.pending_wrap = false;
            unsafe { (*self.screens.active).cursor.pending_wrap = false };
            return;
        }

        // If we have a pending wrap state and we are in either reverse wrap
        // modes then we decrement the amount we move by one to match xterm.
        // if (self.screens.active.cursor.pending_wrap) {
        //     count -= 1;
        //     self.screens.active.cursor.pending_wrap = false;
        // }
        if unsafe { (*self.screens.active).cursor.pending_wrap } {
            count -= 1;
            unsafe { (*self.screens.active).cursor.pending_wrap = false };
        }

        // The margins we can move to.
        // const top = self.scrolling_region.top;
        // const bottom = self.scrolling_region.bottom;
        // const right_margin = self.scrolling_region.right;
        // const left_margin = if (self.screens.active.cursor.x < self.scrolling_region.left)
        //     0
        // else
        //     self.scrolling_region.left;
        let top = self.scrolling_region.top;
        let bottom = self.scrolling_region.bottom;
        let right_margin = self.scrolling_region.right;
        let left_margin = if unsafe { (*self.screens.active).cursor.x } < self.scrolling_region.left
        {
            0
        } else {
            self.scrolling_region.left
        };

        // Handle some edge cases when our cursor is already on the left margin.
        // if (self.screens.active.cursor.x == left_margin) {
        if unsafe { (*self.screens.active).cursor.x } == left_margin {
            match wrap_mode {
                // In reverse mode, if we're already before the top margin
                // then we just set our cursor to the top-left and we're done.
                // .reverse => if (self.screens.active.cursor.y <= top) {
                //     self.screens.active.cursorAbsolute(left_margin, top);
                //     return;
                // },
                WrapMode::Reverse => {
                    if unsafe { (*self.screens.active).cursor.y } <= top {
                        self.active_mut().cursor_absolute(left_margin, top);
                        return;
                    }
                }

                // Handled in while loop
                WrapMode::ReverseExtended => {}

                // Handled above
                WrapMode::None => unreachable!(),
            }
        }

        // while (true) {
        loop {
            // We can move at most to the left margin.
            // const max = self.screens.active.cursor.x - left_margin;
            let max = unsafe { (*self.screens.active).cursor.x } - left_margin;

            // We want to move at most the number of columns we have left
            // or our remaining count. Do the move.
            // const amount = @min(max, count);
            // count -= amount;
            // self.screens.active.cursorLeft(amount);
            let amount = core::cmp::min(max as usize, count);
            count -= amount;
            self.active_mut().cursor_left(amount as size::CellCountInt);

            // If we have no more to move, then we're done.
            // if (count == 0) break;
            if count == 0 {
                break;
            }

            // If we are at the top, then we are done.
            // if (self.screens.active.cursor.y == top) {
            //     if (wrap_mode != .reverse_extended) break;
            //     self.screens.active.cursorAbsolute(right_margin, bottom);
            //     count -= 1;
            //     continue;
            // }
            if unsafe { (*self.screens.active).cursor.y } == top {
                if !matches!(wrap_mode, WrapMode::ReverseExtended) {
                    break;
                }

                self.active_mut().cursor_absolute(right_margin, bottom);
                count -= 1;
                continue;
            }

            // UNDEFINED TERMINAL BEHAVIOR. This situation is not handled in xterm
            // and currently results in a crash in xterm. Given no other known
            // terminal [to me] implements XTREVWRAP2, I decided to just mimic
            // the behavior of xterm up and not including the crash by wrapping
            // up to the (0, 0) and stopping there. My reasoning is that for an
            // appropriately sized value of "count" this is the behavior that xterm
            // would have. This is unit tested.
            // if (self.screens.active.cursor.y == 0) {
            //     assert(self.screens.active.cursor.x == left_margin);
            //     break;
            // }
            if unsafe { (*self.screens.active).cursor.y } == 0 {
                assert!(unsafe { (*self.screens.active).cursor.x } == left_margin);
                break;
            }

            // If our previous line is not wrapped then we are done.
            // if (wrap_mode != .reverse_extended) {
            //     const prev_row = self.screens.active.cursorRowUp(1);
            //     if (!prev_row.wrap) break;
            // }
            if !matches!(wrap_mode, WrapMode::ReverseExtended) {
                let prev_row = self.active_mut().cursor_row_up(1);
                if !unsafe { (*prev_row).wrap() } {
                    break;
                }
            }

            // self.screens.active.cursorAbsolute(right_margin, self.screens.active.cursor.y - 1);
            // count -= 1;
            let new_y = unsafe { (*self.screens.active).cursor.y } - 1;
            self.active_mut().cursor_absolute(right_margin, new_y);
            count -= 1;
        }
    }

    /// Save cursor position and further state.
    ///
    /// The primary and alternate screen have distinct save state. One saved state
    /// is kept per screen (main / alternative). If for the current screen state
    /// was already saved it is overwritten.
    pub fn save_cursor(&mut self) {
        // self.screens.active.saved_cursor = .{
        //     .x = self.screens.active.cursor.x,
        //     .y = self.screens.active.cursor.y,
        //     .style = self.screens.active.cursor.style,
        //     .protected = self.screens.active.cursor.protected,
        //     .pending_wrap = self.screens.active.cursor.pending_wrap,
        //     .origin = self.modes.get(.origin),
        //     .charset = self.screens.active.charset,
        // };
        let origin = self.modes.get(modespkg::Mode::Origin);
        let active: *mut Screen = self.screens.active;
        let saved = crate::screen::SavedCursor {
            x: unsafe { (*active).cursor.x },
            y: unsafe { (*active).cursor.y },
            style: unsafe { (*active).cursor.style },
            protected: unsafe { (*active).cursor.protected },
            pending_wrap: unsafe { (*active).cursor.pending_wrap },
            origin,
            charset: unsafe { (*active).charset.clone() },
        };
        self.active_mut().saved_cursor = Some(saved);
    }

    /// Restore cursor position and other state.
    ///
    /// The primary and alternate screen have distinct save state.
    /// If no save was done before values are reset to their initial values.
    pub fn restore_cursor(&mut self) {
        // const saved: Screen.SavedCursor = self.screens.active.saved_cursor orelse .{
        //     .x = 0,
        //     .y = 0,
        //     .style = .{},
        //     .protected = false,
        //     .pending_wrap = false,
        //     .origin = false,
        //     .charset = .{},
        // };
        // PORT NOTE: `SavedCursor` is a Zig value struct copied by `orelse`; in
        // Rust it is not `Copy` (it holds a `CharsetState`/`CharsetArray`). We
        // borrow the saved state (or a fresh default), then clone the `charset`
        // only where the reference reads `saved.charset` below.
        let default_saved;
        let saved: &crate::screen::SavedCursor =
            match unsafe { &(*self.screens.active).saved_cursor } {
                Some(s) => s,
                None => {
                    default_saved = crate::screen::SavedCursor {
                        x: 0,
                        y: 0,
                        style: style::Style::default(),
                        protected: false,
                        pending_wrap: false,
                        origin: false,
                        charset: crate::screen::CharsetState::default(),
                    };
                    &default_saved
                }
            };
        // Copy out the scalar saved fields up front so we don't hold the borrow
        // of `self.screens.active.saved_cursor` across the `&mut self` calls.
        let saved_style = saved.style;
        let saved_charset = saved.charset.clone();
        let saved_origin = saved.origin;
        let saved_pending_wrap = saved.pending_wrap;
        let saved_protected = saved.protected;
        let saved_x = saved.x;
        let saved_y = saved.y;

        // Set the style first because it can fail
        // self.screens.active.cursor.style = saved.style;
        // self.screens.active.manualStyleUpdate() catch |err| {
        //     log.warn("restoreCursor error updating style err={}", .{err});
        //     const screen: *Screen = self.screens.active;
        //     screen.cursor.style = .{};
        //     self.screens.active.manualStyleUpdate() catch unreachable;
        // };
        unsafe { (*self.screens.active).cursor.style = saved_style };
        if let Err(_err) = self.active_mut().manual_style_update() {
            // Regardless of the error here, we revert back to an unstyled
            // cursor. It is more important that the restore succeeds in
            // other attributes because terminals have no way to communicate
            // failure back.
            // log.warn("restoreCursor error updating style err={}", .{err});
            let screen: *mut Screen = self.screens.active;
            unsafe { (*screen).cursor.style = style::Style::default() };
            self.active_mut().manual_style_update().unwrap();
        }

        // self.screens.active.charset = saved.charset;
        // self.modes.set(.origin, saved.origin);
        // self.screens.active.cursor.pending_wrap = saved.pending_wrap;
        // self.screens.active.cursor.protected = saved.protected;
        unsafe { (*self.screens.active).charset = saved_charset };
        self.modes.set(modespkg::Mode::Origin, saved_origin);
        unsafe { (*self.screens.active).cursor.pending_wrap = saved_pending_wrap };
        unsafe { (*self.screens.active).cursor.protected = saved_protected };
        // self.screens.active.cursorAbsolute(
        //     @min(saved.x, self.cols - 1),
        //     @min(saved.y, self.rows - 1),
        // );
        let x = core::cmp::min(saved_x, self.cols - 1);
        let y = core::cmp::min(saved_y, self.rows - 1);
        self.active_mut().cursor_absolute(x, y);

        // Ensure our screen is consistent
        // self.screens.active.assertIntegrity();
        self.active_mut().assert_integrity();
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
        // while (self.screens.active.cursor.x < self.scrolling_region.right) {
        while unsafe { (*self.screens.active).cursor.x } < self.scrolling_region.right {
            // Move the cursor right
            // self.screens.active.cursorRight(1);
            self.active_mut().cursor_right(1);

            // If the last cursor position was a tabstop we return. We do
            // "last cursor position" because we want a space to be written
            // at the tabstop unless we're at the end (the while condition).
            // if (self.tabstops.get(self.screens.active.cursor.x)) return;
            if self
                .tabstops
                .get(unsafe { (*self.screens.active).cursor.x } as usize)
            {
                return;
            }
        }
    }

    // Same as horizontalTab but moves to the previous tabstop instead of the next.
    pub fn horizontal_tab_back(&mut self) {
        // With origin mode enabled, our leftmost limit is the left margin.
        // const left_limit = if (self.modes.get(.origin)) self.scrolling_region.left else 0;
        let left_limit = if self.modes.get(modespkg::Mode::Origin) {
            self.scrolling_region.left
        } else {
            0
        };

        // while (true) {
        loop {
            // If we're already at the edge of the screen, then we're done.
            // if (self.screens.active.cursor.x <= left_limit) return;
            if unsafe { (*self.screens.active).cursor.x } <= left_limit {
                return;
            }

            // Move the cursor left
            // self.screens.active.cursorLeft(1);
            // if (self.tabstops.get(self.screens.active.cursor.x)) return;
            self.active_mut().cursor_left(1);
            if self
                .tabstops
                .get(unsafe { (*self.screens.active).cursor.x } as usize)
            {
                return;
            }
        }
    }

    /// Clear tab stops.
    pub fn tab_clear(&mut self, cmd: csi::TabClear) {
        // switch (cmd) {
        //     .current => self.tabstops.unset(self.screens.active.cursor.x),
        //     .all => self.tabstops.reset(0),
        //     else => log.warn("invalid or unknown tab clear setting: {}", .{cmd}),
        // }
        match cmd {
            csi::TabClear::Current => self
                .tabstops
                .unset(unsafe { (*self.screens.active).cursor.x } as usize),
            csi::TabClear::All => self.tabstops.reset(0),
            // log.warn("invalid or unknown tab clear setting: {}", .{cmd}),
            csi::TabClear::Other(_) => {}
        }
    }

    /// Set a tab stop on the current cursor.
    /// TODO: test
    pub fn tab_set(&mut self) {
        // self.tabstops.set(self.screens.active.cursor.x);
        self.tabstops
            .set(unsafe { (*self.screens.active).cursor.x } as usize);
    }

    /// TODO: test
    pub fn tab_reset(&mut self) {
        // self.tabstops.reset(TABSTOP_INTERVAL);
        self.tabstops.reset(TABSTOP_INTERVAL as usize);
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
        // const screen: *Screen = self.screens.active;
        let screen: *mut Screen = self.screens.active;

        // Unset pending wrap state
        // screen.cursor.pending_wrap = false;
        unsafe { (*screen).cursor.pending_wrap = false };

        // We handle our cursor semantic prompt state AFTER doing the
        // scrolling, because we may need to apply to new rows.
        // defer if (screen.cursor.semantic_content != .output) { ... } else { ... };
        // PORT NOTE: Rust has no `defer`; this RAII guard reproduces the
        // semantic-content handling that the reference defers to AFTER the
        // scroll. Its `Drop` reads `screen.cursor` at drop time (post-scroll),
        // matching the reference's deferred evaluation, and runs on every exit
        // path (including the early `return`s and the `?`).
        let _semantic = SemanticIndexGuard { screen };

        // Outside of the scroll region we move the cursor one line down.
        // if (screen.cursor.y < self.scrolling_region.top or
        //     screen.cursor.y > self.scrolling_region.bottom)
        if unsafe { (*screen).cursor.y } < self.scrolling_region.top
            || unsafe { (*screen).cursor.y } > self.scrolling_region.bottom
        {
            // We only move down if we're not already at the bottom of
            // the screen.
            // if (screen.cursor.y < self.rows - 1) {
            if unsafe { (*screen).cursor.y } < self.rows - 1 {
                // screen.cursorDown(1);
                unsafe { (*screen).cursor_down(1) };
            }

            return Ok(());
        }

        // If the cursor is inside the scrolling region and on the bottom-most
        // line, then we scroll up. If our scrolling region is the full screen
        // we create scrollback.
        // if (screen.cursor.y == self.scrolling_region.bottom and
        //     screen.cursor.x >= self.scrolling_region.left and
        //     screen.cursor.x <= self.scrolling_region.right)
        if unsafe { (*screen).cursor.y } == self.scrolling_region.bottom
            && unsafe { (*screen).cursor.x } >= self.scrolling_region.left
            && unsafe { (*screen).cursor.x } <= self.scrolling_region.right
        {
            // if (comptime build_options.kitty_graphics) {
            //     // Scrolling dirties the images because it updates their placements pins.
            //     screen.kitty_images.dirty = true;
            // }
            // PORT NOTE (vt-12 deferral): Kitty graphics `ImageStorage` is
            // deferred, so the `kitty_images.dirty = true` flag set under
            // `if (comptime build_options.kitty_graphics)` is omitted.

            // If our scrolling region is at the top, we create scrollback.
            // if (self.scrolling_region.top == 0 and
            //     self.scrolling_region.left == 0 and
            //     self.scrolling_region.right == self.cols - 1)
            if self.scrolling_region.top == 0
                && self.scrolling_region.left == 0
                && self.scrolling_region.right == self.cols - 1
            {
                // try screen.cursorScrollAbove();
                self.active_mut().cursor_scroll_above()?;
                return Ok(());
            }

            // Slow path for left and right scrolling region margins.
            // if (self.scrolling_region.left != 0 or
            //     self.scrolling_region.right != self.cols - 1 or
            //     !screen.blankCell().isZero())
            if self.scrolling_region.left != 0
                || self.scrolling_region.right != self.cols - 1
                // PERF(mitchellh): If we have an SGR background set then
                // we need to preserve that background in our erased rows.
                // scrollUp does that but eraseRowBounded below does not.
                // However, scrollUp is WAY slower. We should optimize this
                // case to work in the eraseRowBounded codepath and remove
                // this check.
                || !unsafe { (*screen).blank_cell() }.is_zero()
            {
                // try self.scrollUp(1);
                self.scroll_up(1)?;
                return Ok(());
            }

            // Otherwise use a fast path function from PageList to efficiently
            // scroll the contents of the scrolling region.

            // Preserve old cursor just for assertions
            // const old_cursor = screen.cursor;
            let old_cursor_x = unsafe { (*screen).cursor.x };
            let old_cursor_y = unsafe { (*screen).cursor.y };

            // try screen.pages.eraseRowBounded(
            //     .{ .active = .{ .y = self.scrolling_region.top } },
            //     self.scrolling_region.bottom - self.scrolling_region.top,
            // );
            let sr_top = self.scrolling_region.top;
            let sr_bottom = self.scrolling_region.bottom;
            self.active_mut().pages.erase_row_bounded(
                point::Point::Active(point::Coordinate {
                    x: 0,
                    y: sr_top as u32,
                }),
                (sr_bottom - sr_top) as usize,
            )?;

            // eraseRow and eraseRowBounded will end up moving the cursor pin
            // up by 1, so we need to move it back down. A `cursorReload`
            // would be better option but this is more efficient and this is
            // a super hot path so we do this instead.
            // assert(screen.cursor.x == old_cursor.x);
            assert!(unsafe { (*screen).cursor.x } == old_cursor_x);
            // assert(screen.cursor.y == old_cursor.y);
            assert!(unsafe { (*screen).cursor.y } == old_cursor_y);
            // screen.cursor.y -= 1;
            unsafe { (*screen).cursor.y -= 1 };
            // screen.cursorDown(1);
            unsafe { (*screen).cursor_down(1) };

            // The operations above can prune our cursor style so we need to
            // update. This should never fail because the above can only FREE
            // memory.
            // screen.manualStyleUpdate() catch |err| {
            //     std.log.warn("deleteLines manualStyleUpdate err={}", .{err});
            //     screen.cursor.style = .{};
            //     screen.manualStyleUpdate() catch unreachable;
            // };
            if unsafe { (*screen).manual_style_update() }.is_err() {
                // std.log.warn("deleteLines manualStyleUpdate err={}", .{err});
                unsafe { (*screen).cursor.style = style::Style::default() };
                unsafe { (*screen).manual_style_update() }.unwrap();
            }

            return Ok(());
        }

        // Increase cursor by 1, maximum to bottom of scroll region
        // if (screen.cursor.y < self.scrolling_region.bottom) {
        if unsafe { (*screen).cursor.y } < self.scrolling_region.bottom {
            // screen.cursorDown(1);
            unsafe { (*screen).cursor_down(1) };
        }

        Ok(())
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
        // if (self.screens.active.cursor.y != self.scrolling_region.top or
        //     self.screens.active.cursor.x < self.scrolling_region.left or
        //     self.screens.active.cursor.x > self.scrolling_region.right)
        if unsafe { (*self.screens.active).cursor.y } != self.scrolling_region.top
            || unsafe { (*self.screens.active).cursor.x } < self.scrolling_region.left
            || unsafe { (*self.screens.active).cursor.x } > self.scrolling_region.right
        {
            // self.cursorUp(1);
            self.cursor_up(1);
            return;
        }

        // self.scrollDown(1);
        self.scroll_down(1);
    }

    /// Set Cursor Position. Move cursor to the position indicated
    /// by row and column (1-indexed). If column is 0, it is adjusted to 1.
    /// If column is greater than the right-most column it is adjusted to
    /// the right-most column. If row is 0, it is adjusted to 1. If row is
    /// greater than the bottom-most row it is adjusted to the bottom-most
    /// row.
    pub fn set_cursor_pos(&mut self, row_req: usize, col_req: usize) {
        // If cursor origin mode is set the cursor row will be moved relative to
        // the top margin row and adjusted to be above or at bottom-most row in
        // the current scroll region.
        //
        // If origin mode is set and left and right margin mode is set the cursor
        // will be moved relative to the left margin column and adjusted to be on
        // or left of the right margin column.
        // const params: struct {
        //     x_offset: size.CellCountInt = 0,
        //     y_offset: size.CellCountInt = 0,
        //     x_max: size.CellCountInt,
        //     y_max: size.CellCountInt,
        // } = if (self.modes.get(.origin)) .{
        //     .x_offset = self.scrolling_region.left,
        //     .y_offset = self.scrolling_region.top,
        //     .x_max = self.scrolling_region.right + 1, // We need this 1-indexed
        //     .y_max = self.scrolling_region.bottom + 1, // We need this 1-indexed
        // } else .{
        //     .x_max = self.cols,
        //     .y_max = self.rows,
        // };
        struct Params {
            x_offset: size::CellCountInt,
            y_offset: size::CellCountInt,
            x_max: size::CellCountInt,
            y_max: size::CellCountInt,
        }
        let params: Params = if self.modes.get(modespkg::Mode::Origin) {
            Params {
                x_offset: self.scrolling_region.left,
                y_offset: self.scrolling_region.top,
                x_max: self.scrolling_region.right + 1, // We need this 1-indexed
                y_max: self.scrolling_region.bottom + 1, // We need this 1-indexed
            }
        } else {
            Params {
                x_offset: 0,
                y_offset: 0,
                x_max: self.cols,
                y_max: self.rows,
            }
        };

        // Unset pending wrap state
        // self.screens.active.cursor.pending_wrap = false;
        unsafe { (*self.screens.active).cursor.pending_wrap = false };

        // Calculate our new x/y
        // const row = if (row_req == 0) 1 else row_req;
        // const col = if (col_req == 0) 1 else col_req;
        // const x = @min(params.x_max, col + params.x_offset) -| 1;
        // const y = @min(params.y_max, row + params.y_offset) -| 1;
        let row = if row_req == 0 { 1 } else { row_req };
        let col = if col_req == 0 { 1 } else { col_req };
        // PORT NOTE: `col + params.x_offset` is `usize + CellCountInt`; we widen
        // the offset to usize for the add, then `@min` with `x_max` (also widened)
        // keeps the result within `CellCountInt` range before the saturating `-| 1`.
        let x: size::CellCountInt =
            (core::cmp::min(params.x_max as usize, col + params.x_offset as usize)
                as size::CellCountInt)
                .saturating_sub(1);
        let y: size::CellCountInt =
            (core::cmp::min(params.y_max as usize, row + params.y_offset as usize)
                as size::CellCountInt)
                .saturating_sub(1);

        // If the y is unchanged then this is fast pointer math
        // if (y == self.screens.active.cursor.y) {
        //     if (x > self.screens.active.cursor.x) {
        //         self.screens.active.cursorRight(x - self.screens.active.cursor.x);
        //     } else {
        //         self.screens.active.cursorLeft(self.screens.active.cursor.x - x);
        //     }
        //     return;
        // }
        if y == unsafe { (*self.screens.active).cursor.y } {
            let cursor_x = unsafe { (*self.screens.active).cursor.x };
            if x > cursor_x {
                self.active_mut().cursor_right(x - cursor_x);
            } else {
                self.active_mut().cursor_left(cursor_x - x);
            }

            return;
        }

        // If everything changed we do an absolute change which is slightly slower
        // self.screens.active.cursorAbsolute(x, y);
        self.active_mut().cursor_absolute(x, y);
        // log.info("set cursor position: col={} row={}", .{ self.screens.active.cursor.x, self.screens.active.cursor.y });
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
        // const top = @max(1, top_req);
        let top = core::cmp::max(1, top_req);
        // const bottom = @min(self.rows, if (bottom_req == 0) self.rows else bottom_req);
        let bottom = core::cmp::min(
            self.rows as usize,
            if bottom_req == 0 {
                self.rows as usize
            } else {
                bottom_req
            },
        );
        // if (top >= bottom) return;
        if top >= bottom {
            return;
        }

        // self.scrolling_region.top = @intCast(top - 1);
        self.scrolling_region.top = (top - 1) as size::CellCountInt;
        // self.scrolling_region.bottom = @intCast(bottom - 1);
        self.scrolling_region.bottom = (bottom - 1) as size::CellCountInt;
        // self.setCursorPos(1, 1);
        self.set_cursor_pos(1, 1);
    }

    /// DECSLRM
    pub fn set_left_and_right_margin(&mut self, left_req: usize, right_req: usize) {
        // We must have this mode enabled to do anything
        // if (!self.modes.get(.enable_left_and_right_margin)) return;
        if !self.modes.get(modespkg::Mode::EnableLeftAndRightMargin) {
            return;
        }

        // const left = @max(1, left_req);
        let left = core::cmp::max(1, left_req);
        // const right = @min(self.cols, if (right_req == 0) self.cols else right_req);
        let right = core::cmp::min(
            self.cols as usize,
            if right_req == 0 {
                self.cols as usize
            } else {
                right_req
            },
        );
        // if (left >= right) return;
        if left >= right {
            return;
        }

        // self.scrolling_region.left = @intCast(left - 1);
        self.scrolling_region.left = (left - 1) as size::CellCountInt;
        // self.scrolling_region.right = @intCast(right - 1);
        self.scrolling_region.right = (right - 1) as size::CellCountInt;
        // self.setCursorPos(1, 1);
        self.set_cursor_pos(1, 1);
    }

    /// Scroll the text down by one row.
    pub fn scroll_down(&mut self, count: usize) {
        // Preserve our x/y to restore.
        // const old_x = self.screens.active.cursor.x;
        // const old_y = self.screens.active.cursor.y;
        // const old_wrap = self.screens.active.cursor.pending_wrap;
        // defer {
        //     self.screens.active.cursorAbsolute(old_x, old_y);
        //     self.screens.active.cursor.pending_wrap = old_wrap;
        // }
        // PORT NOTE: Rust has no `defer`; this RAII guard restores the cursor
        // x/y and pending_wrap when it drops at end of scope, mirroring the
        // reference `defer`. It runs on every exit path.
        let _restore = CursorRestoreGuard {
            screen: self.screens.active,
            old_x: unsafe { (*self.screens.active).cursor.x },
            old_y: unsafe { (*self.screens.active).cursor.y },
            old_wrap: unsafe { (*self.screens.active).cursor.pending_wrap },
        };

        // Move to the top of the scroll region
        // self.screens.active.cursorAbsolute(self.scrolling_region.left, self.scrolling_region.top);
        let sr_left = self.scrolling_region.left;
        let sr_top = self.scrolling_region.top;
        self.active_mut().cursor_absolute(sr_left, sr_top);
        // self.insertLines(count);
        self.insert_lines(count);
    }

    /// Removes amount lines from the top of the scroll region. The remaining lines
    /// to the bottom margin are shifted up and space from the bottom margin up
    /// is filled with empty lines.
    ///
    /// The new lines are created according to the current SGR state.
    ///
    /// Does not change the (absolute) cursor position.
    pub fn scroll_up(&mut self, count: usize) -> Result<(), OutOfMemory> {
        // Preserve our x/y to restore.
        // const old_x = self.screens.active.cursor.x;
        // const old_y = self.screens.active.cursor.y;
        // const old_wrap = self.screens.active.cursor.pending_wrap;
        // defer {
        //     self.screens.active.cursorAbsolute(old_x, old_y);
        //     self.screens.active.cursor.pending_wrap = old_wrap;
        // }
        // PORT NOTE: see scroll_down — RAII guard mirrors the reference `defer`,
        // restoring the cursor on every return (including the early `?`).
        let _restore = CursorRestoreGuard {
            screen: self.screens.active,
            old_x: unsafe { (*self.screens.active).cursor.x },
            old_y: unsafe { (*self.screens.active).cursor.y },
            old_wrap: unsafe { (*self.screens.active).cursor.pending_wrap },
        };

        // If our scroll region is at the top and we have no left/right
        // margins then we move the scrolled out text into the scrollback.
        if self.scrolling_region.top == 0
            && self.scrolling_region.left == 0
            && self.scrolling_region.right == self.cols - 1
        {
            // Scrolling dirties the images because it updates their placements pins.
            // if (comptime build_options.kitty_graphics) {
            //     self.screens.active.kitty_images.dirty = true;
            // }
            // PORT NOTE (vt-12 deferral): Kitty graphics `ImageStorage` is
            // deferred, so the `kitty_images.dirty = true` flag set under
            // `if (comptime build_options.kitty_graphics)` is omitted (matches
            // the other Kitty-graphics seam deferrals in this file).

            // Clamp count to the scroll region height.
            // const region_height = self.scrolling_region.bottom + 1;
            let region_height = self.scrolling_region.bottom as usize + 1;
            // const adjusted_count = @min(count, region_height);
            let adjusted_count = core::cmp::min(count, region_height);

            // TODO: Create an optimized version that can scroll N times
            // This isn't critical because in most cases, scrollUp is used
            // with count=1, but it's still a big optimization opportunity.

            // Move our cursor to the bottom of the scroll region so we can
            // use the cursorScrollAbove function to create scrollback
            // self.screens.active.cursorAbsolute(0, self.scrolling_region.bottom);
            let sr_bottom = self.scrolling_region.bottom;
            self.active_mut().cursor_absolute(0, sr_bottom);
            // for (0..adjusted_count) |_| try self.screens.active.cursorScrollAbove();
            for _ in 0..adjusted_count {
                self.active_mut().cursor_scroll_above()?;
            }
            return Ok(());
        }

        // Move to the top of the scroll region
        // self.screens.active.cursorAbsolute(self.scrolling_region.left, self.scrolling_region.top);
        let sr_left = self.scrolling_region.left;
        let sr_top = self.scrolling_region.top;
        self.active_mut().cursor_absolute(sr_left, sr_top);
        // self.deleteLines(count);
        self.delete_lines(count);

        Ok(())
    }

    /// Scroll the viewport of the terminal grid.
    pub fn scroll_viewport(&mut self, behavior: ScrollViewport) {
        // self.screens.active.scroll(switch (behavior) {
        //     .top => .{ .top = {} },
        //     .bottom => .{ .active = {} },
        //     .delta => |delta| .{ .delta_row = delta },
        // });
        self.active_mut().scroll(match behavior {
            ScrollViewport::Top => crate::screen::Scroll::Top,
            ScrollViewport::Bottom => crate::screen::Scroll::Active,
            ScrollViewport::Delta(delta) => crate::screen::Scroll::DeltaRow(delta),
        });
    }

    /// To be called before shifting a row (as in insertLines and deleteLines)
    ///
    /// Takes care of boundary conditions such as potentially split wide chars
    /// across scrolling region boundaries and orphaned spacer heads at line
    /// ends.
    // PORT NOTE (SOUNDNESS): the reference takes `page: *Page, row: *Row` and
    // re-reads that memory through `clearGrapheme`/`updateRowGraphemeFlag`
    // (which call `assertIntegrity`) while those pointers are live. Zig
    // pointers are UNPROTECTED, so we mirror them as raw `*mut Page`/`*mut Row`
    // rather than `&mut` to avoid Stacked-Borrows aliasing UB (the
    // clear_cells UB-fix rule). We reconstruct local mutable accesses only for
    // the brief field mutations, never holding a `&mut Page` across an
    // integrity check.
    fn row_will_be_shifted(&mut self, page: *mut Page, row: *mut Row) {
        // const cells = row.cells.ptr(page.memory.ptr);
        let cells: *mut pagepkg::Cell = unsafe { (*row).cells().ptr((*page).memory_ptr()) };

        // If our scrolling region includes the rightmost column then we
        // need to turn any spacer heads in to normal empty cells, since
        // once we move them they no longer correspond with soft-wrapped
        // wide characters.
        //
        // If it contains either of the 2 leftmost columns, then the wide
        // characters in the first column which may be associated with a
        // spacer head will be either moved or cleared, so we also need
        // to turn the spacer heads in to empty cells in that case.
        // if (self.scrolling_region.right == self.cols - 1 or
        //     self.scrolling_region.left < 2)
        if self.scrolling_region.right == self.cols - 1 || self.scrolling_region.left < 2 {
            // const end_cell: *Cell = &cells[page.size.cols - 1];
            let page_cols = unsafe { (*page).size.cols };
            let end_cell: *mut pagepkg::Cell = unsafe { cells.add((page_cols - 1) as usize) };
            // if (end_cell.wide == .spacer_head) {
            if unsafe { (*end_cell).wide() } == pagepkg::Wide::SpacerHead {
                // end_cell.wide = .narrow;
                unsafe { (*end_cell).set_wide(pagepkg::Wide::Narrow) };
            }
        }

        // If the leftmost or rightmost cells of our scrolling region
        // are parts of wide chars, we need to clear the cells' contents
        // since they'd be split by the move.
        // const left_cell: *Cell = &cells[self.scrolling_region.left];
        let left_cell: *mut pagepkg::Cell =
            unsafe { cells.add(self.scrolling_region.left as usize) };
        // const right_cell: *Cell = &cells[self.scrolling_region.right];
        let right_cell: *mut pagepkg::Cell =
            unsafe { cells.add(self.scrolling_region.right as usize) };

        // if (left_cell.wide == .spacer_tail) {
        if unsafe { (*left_cell).wide() } == pagepkg::Wide::SpacerTail {
            // const wide_cell: *Cell = &cells[self.scrolling_region.left - 1];
            let wide_cell: *mut pagepkg::Cell =
                unsafe { cells.add((self.scrolling_region.left - 1) as usize) };
            // if (wide_cell.hasGrapheme()) {
            if unsafe { (*wide_cell).has_grapheme() } {
                // page.clearGrapheme(wide_cell);
                unsafe { (*page).clear_grapheme(wide_cell) };
                // page.updateRowGraphemeFlag(row);
                unsafe { (*page).update_row_grapheme_flag(row) };
            }
            // wide_cell.content.codepoint = 0;
            unsafe { (*wide_cell).set_content_codepoint(0) };
            // wide_cell.wide = .narrow;
            unsafe { (*wide_cell).set_wide(pagepkg::Wide::Narrow) };
            // left_cell.wide = .narrow;
            unsafe { (*left_cell).set_wide(pagepkg::Wide::Narrow) };
        }

        // if (right_cell.wide == .wide) {
        if unsafe { (*right_cell).wide() } == pagepkg::Wide::Wide {
            // const tail_cell: *Cell = &cells[self.scrolling_region.right + 1];
            let tail_cell: *mut pagepkg::Cell =
                unsafe { cells.add((self.scrolling_region.right + 1) as usize) };
            // if (right_cell.hasGrapheme()) {
            if unsafe { (*right_cell).has_grapheme() } {
                // page.clearGrapheme(right_cell);
                unsafe { (*page).clear_grapheme(right_cell) };
                // page.updateRowGraphemeFlag(row);
                unsafe { (*page).update_row_grapheme_flag(row) };
            }
            // right_cell.content.codepoint = 0;
            unsafe { (*right_cell).set_content_codepoint(0) };
            // right_cell.wide = .narrow;
            unsafe { (*right_cell).set_wide(pagepkg::Wide::Narrow) };
            // tail_cell.wide = .narrow;
            unsafe { (*tail_cell).set_wide(pagepkg::Wide::Narrow) };
        }
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
        // Rare, but happens
        // if (count == 0) return;
        if count == 0 {
            return;
        }

        // If the cursor is outside the scroll region we do nothing.
        // if (self.screens.active.cursor.y < self.scrolling_region.top or
        //     self.screens.active.cursor.y > self.scrolling_region.bottom or
        //     self.screens.active.cursor.x < self.scrolling_region.left or
        //     self.screens.active.cursor.x > self.scrolling_region.right) return;
        if unsafe { (*self.screens.active).cursor.y } < self.scrolling_region.top
            || unsafe { (*self.screens.active).cursor.y } > self.scrolling_region.bottom
            || unsafe { (*self.screens.active).cursor.x } < self.scrolling_region.left
            || unsafe { (*self.screens.active).cursor.x } > self.scrolling_region.right
        {
            return;
        }

        // if (comptime build_options.kitty_graphics) {
        //     // Scrolling dirties the images because it updates their placements pins.
        //     self.screens.active.kitty_images.dirty = true;
        // }
        // PORT NOTE (vt-12 deferral): Kitty graphics `kitty_images.dirty` is omitted.

        // At the end we need to return the cursor to the row it started on.
        // const start_y = self.screens.active.cursor.y;
        let start_y = unsafe { (*self.screens.active).cursor.y };
        // defer {
        //     self.screens.active.cursorAbsolute(self.scrolling_region.left, start_y);
        //     // Always unset pending wrap
        //     self.screens.active.cursor.pending_wrap = false;
        // }
        // PORT NOTE: RAII guard reproduces the reference `defer`, restoring the
        // cursor to the left margin / start row and clearing pending_wrap on
        // every exit path.
        let _restore = ShiftLinesEndGuard {
            screen: self.screens.active,
            left: self.scrolling_region.left,
            start_y,
        };

        // We have a slower path if we have left or right scroll margins.
        // const left_right = self.scrolling_region.left > 0 or
        //     self.scrolling_region.right < self.cols - 1;
        let left_right =
            self.scrolling_region.left > 0 || self.scrolling_region.right < self.cols - 1;

        // Remaining rows from our cursor to the bottom of the scroll region.
        // const rem = self.scrolling_region.bottom - self.screens.active.cursor.y + 1;
        let rem = self.scrolling_region.bottom - unsafe { (*self.screens.active).cursor.y } + 1;

        // We can only insert lines up to our remaining lines in the scroll
        // region. So we take whichever is smaller.
        // const adjusted_count = @min(count, rem);
        let adjusted_count = core::cmp::min(count, rem as usize) as size::CellCountInt;

        // Create a new tracked pin which we'll use to navigate the page list
        // so that if we need to adjust capacity it will be properly tracked.
        // var cur_p = self.screens.active.pages.trackPin(
        //     self.screens.active.cursor.page_pin.down(rem - 1).?,
        // ) catch |err| { ... @panic("insertLines trackPin OOM"); };
        // defer self.screens.active.pages.untrackPin(cur_p);
        let down_pin =
            unsafe { (*(*self.screens.active).cursor.page_pin).down((rem - 1) as usize) }.unwrap();
        let cur_p: *mut crate::pagelist::Pin = match self.active_mut().pages.track_pin(down_pin) {
            Ok(p) => p,
            Err(_err) => {
                // comptime assert(@TypeOf(err) == error{OutOfMemory});
                //
                // This error scenario means that our GPA is OOM. This is not a
                // situation we can gracefully handle. We can't just ignore insertLines
                // because it'll result in a corrupted screen. Ideally in the future
                // we flag the state as broken and show an error message to the user.
                // For now, we panic.
                // log.err("insertLines trackPin error err={}", .{err});
                // @panic("insertLines trackPin OOM");
                panic!("insertLines trackPin OOM");
            }
        };
        // PORT NOTE: RAII guard reproduces `defer untrackPin(cur_p)`.
        let _untrack = TrackedPinGuard {
            screen: self.screens.active,
            pin: cur_p,
        };

        // Our current y position relative to the cursor
        // var y: usize = rem;
        let mut y: usize = rem as usize;

        // Traverse from the bottom up
        // while (y > 0) {
        while y > 0 {
            // const cur_rac = cur_p.rowAndCell();
            // const cur_row: *Row = cur_rac.row;
            let cur_row: *mut Row = unsafe { (*cur_p).row_and_cell() }.row;

            // If this is one of the lines we need to shift, do so
            // if (y > adjusted_count) {
            if y > adjusted_count as usize {
                // const off_p = cur_p.up(adjusted_count).?;
                let off_p = unsafe { (*cur_p).up(adjusted_count as usize) }.unwrap();
                // const off_rac = off_p.rowAndCell();
                // const off_row: *Row = off_rac.row;
                let off_row: *mut Row = off_p.row_and_cell().row;

                // self.rowWillBeShifted(&cur_p.node.data, cur_row);
                let cur_page: *mut Page = unsafe { core::ptr::addr_of_mut!((*(*cur_p).node).data) };
                self.row_will_be_shifted(cur_page, cur_row);
                // self.rowWillBeShifted(&off_p.node.data, off_row);
                let off_page: *mut Page = unsafe { core::ptr::addr_of_mut!((*off_p.node).data) };
                self.row_will_be_shifted(off_page, off_row);

                // If our scrolling region is full width, then we unset wrap.
                // if (!left_right) {
                if !left_right {
                    // off_row.wrap = false;
                    unsafe { (*off_row).set_wrap(false) };
                    // cur_row.wrap = false;
                    unsafe { (*cur_row).set_wrap(false) };
                    // off_row.wrap_continuation = false;
                    unsafe { (*off_row).set_wrap_continuation(false) };
                    // cur_row.wrap_continuation = false;
                    unsafe { (*cur_row).set_wrap_continuation(false) };
                }

                // const src_p = off_p;
                // const src_row = off_row;
                // const dst_p = cur_p;
                // const dst_row = cur_row;
                let src_p = off_p;
                let src_row = off_row;
                // dst_p is cur_p (the tracked pin), dst_row is cur_row.

                // If our page doesn't match, then we need to do a copy from
                // one page to another. This is the slow path.
                // if (src_p.node != dst_p.node) {
                if !std::ptr::eq(src_p.node, unsafe { (*cur_p).node }) {
                    // dst_p.node.data.clonePartialRowFrom(
                    //     &src_p.node.data, dst_row, src_row,
                    //     self.scrolling_region.left, self.scrolling_region.right + 1,
                    // ) catch |err| { ... continue; };
                    let dst_page: *mut Page =
                        unsafe { core::ptr::addr_of_mut!((*(*cur_p).node).data) };
                    let src_page: *const Page = unsafe { core::ptr::addr_of!((*src_p.node).data) };
                    let clone_res = unsafe {
                        (*dst_page).clone_partial_row_from(
                            src_page,
                            cur_row,
                            src_row,
                            self.scrolling_region.left as usize,
                            (self.scrolling_region.right + 1) as usize,
                        )
                    };
                    if let Err(err) = clone_res {
                        // Adjust our page capacity to make
                        // room for we didn't have space for
                        // _ = self.screens.active.increaseCapacity(dst_p.node, switch (err) {...})
                        //     catch |e| switch (e) {...};
                        let adjustment = match err {
                            // Rehash the sets
                            pagepkg::CloneFromError::StyleSetNeedsRehash
                            | pagepkg::CloneFromError::HyperlinkSetNeedsRehash => None,

                            // Increase style memory
                            pagepkg::CloneFromError::StyleSetOutOfMemory => {
                                Some(pagelist::grow::IncreaseCapacity::Styles)
                            }

                            // Increase string memory
                            pagepkg::CloneFromError::StringAllocOutOfMemory => {
                                Some(pagelist::grow::IncreaseCapacity::StringBytes)
                            }

                            // Increase hyperlink memory
                            pagepkg::CloneFromError::HyperlinkSetOutOfMemory
                            | pagepkg::CloneFromError::HyperlinkMapOutOfMemory => {
                                Some(pagelist::grow::IncreaseCapacity::HyperlinkBytes)
                            }

                            // Increase grapheme memory
                            pagepkg::CloneFromError::GraphemeMapOutOfMemory
                            | pagepkg::CloneFromError::GraphemeAllocOutOfMemory => {
                                Some(pagelist::grow::IncreaseCapacity::GraphemeBytes)
                            }
                        };
                        let cur_node = unsafe { (*cur_p).node };
                        match self.active_mut().increase_capacity(cur_node, adjustment) {
                            Ok(_) => {}
                            Err(e) => match e {
                                // System OOM. We have no way to recover from this
                                // currently. We should probably change insertLines
                                // to raise an error here.
                                pagelist::grow::IncreaseCapacityError::OutOfMemory => {
                                    panic!("increaseCapacity system allocator OOM")
                                }

                                // The page can't accommodate the managed memory required
                                // for this operation. We previously just corrupted
                                // memory here so a crash is better. The right long
                                // term solution is to allocate a new page here
                                // move this row to the new page, and start over.
                                pagelist::grow::IncreaseCapacityError::OutOfSpace => {
                                    panic!("increaseCapacity OutOfSpace")
                                }
                            },
                        }

                        // Continue the loop to try handling this row again.
                        continue;
                    }
                } else {
                    // if (!left_right) {
                    if !left_right {
                        // Swap the src/dst cells. This ensures that our dst gets the
                        // proper shifted rows and src gets non-garbage cell data that
                        // we can clear.
                        // const dst = dst_row.*;
                        // dst_row.* = src_row.*;
                        // src_row.* = dst;
                        // PORT NOTE: `Row` is `Copy`; the three-step swap of the
                        // pointed-to rows is `core::ptr::swap` (same memory effect,
                        // no overlap since src_p.node == dst_p.node but distinct rows).
                        unsafe { core::ptr::swap(cur_row, src_row) };

                        // Ensure what we did didn't corrupt the page
                        // cur_p.node.data.assertIntegrity();
                        let cur_page: *const Page =
                            unsafe { core::ptr::addr_of!((*(*cur_p).node).data) };
                        unsafe { (*cur_page).assert_integrity() };
                    } else {
                        // Left/right scroll margins we have to
                        // copy cells, which is much slower...
                        // const page = &cur_p.node.data;
                        let page: *mut Page =
                            unsafe { core::ptr::addr_of_mut!((*(*cur_p).node).data) };
                        // page.moveCells(src_row, self.scrolling_region.left,
                        //     dst_row, self.scrolling_region.left,
                        //     (self.scrolling_region.right - self.scrolling_region.left) + 1);
                        unsafe {
                            (*page).move_cells(
                                src_row,
                                self.scrolling_region.left as usize,
                                cur_row,
                                self.scrolling_region.left as usize,
                                ((self.scrolling_region.right - self.scrolling_region.left) + 1)
                                    as usize,
                            )
                        };
                    }
                }
            } else {
                // Clear the cells for this row, it has been shifted.
                // self.rowWillBeShifted(&cur_p.node.data, cur_row);
                let cur_page: *mut Page = unsafe { core::ptr::addr_of_mut!((*(*cur_p).node).data) };
                self.row_will_be_shifted(cur_page, cur_row);
                // const page = &cur_p.node.data;
                let page: *mut Page = unsafe { core::ptr::addr_of_mut!((*(*cur_p).node).data) };
                // const cells = page.getCells(cur_row);
                let cells: *mut [pagepkg::Cell] = unsafe { (*page).get_cells(cur_row) };
                // self.screens.active.clearCells(page, cur_row,
                //     cells[self.scrolling_region.left .. self.scrolling_region.right + 1]);
                let sub = ptr_slice_range(
                    cells,
                    self.scrolling_region.left as usize,
                    (self.scrolling_region.right + 1) as usize,
                );
                self.active_mut().clear_cells(page, cur_row, sub);
            }

            // Mark the row as dirty
            // cur_p.markDirty();
            unsafe { (*cur_p).mark_dirty() };

            // We have successfully processed a line.
            // y -= 1;
            y -= 1;
            // Move our pin up to the next row.
            // if (cur_p.up(1)) |p| cur_p.* = p;
            if let Some(p) = unsafe { (*cur_p).up(1) } {
                unsafe { *cur_p = p };
            }
        }
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
        // Rare, but happens
        // if (count == 0) return;
        if count == 0 {
            return;
        }

        // If the cursor is outside the scroll region we do nothing.
        // if (self.screens.active.cursor.y < self.scrolling_region.top or
        //     self.screens.active.cursor.y > self.scrolling_region.bottom or
        //     self.screens.active.cursor.x < self.scrolling_region.left or
        //     self.screens.active.cursor.x > self.scrolling_region.right) return;
        if unsafe { (*self.screens.active).cursor.y } < self.scrolling_region.top
            || unsafe { (*self.screens.active).cursor.y } > self.scrolling_region.bottom
            || unsafe { (*self.screens.active).cursor.x } < self.scrolling_region.left
            || unsafe { (*self.screens.active).cursor.x } > self.scrolling_region.right
        {
            return;
        }

        // if (comptime build_options.kitty_graphics) {
        //     // Scrolling dirties the images because it updates their placements pins.
        //     self.screens.active.kitty_images.dirty = true;
        // }
        // PORT NOTE (vt-12 deferral): Kitty graphics `kitty_images.dirty` is omitted.

        // At the end we need to return the cursor to the row it started on.
        // const start_y = self.screens.active.cursor.y;
        let start_y = unsafe { (*self.screens.active).cursor.y };
        // defer {
        //     self.screens.active.cursorAbsolute(self.scrolling_region.left, start_y);
        //     // Always unset pending wrap
        //     self.screens.active.cursor.pending_wrap = false;
        // }
        // PORT NOTE: RAII guard reproduces the reference `defer` (see insertLines).
        let _restore = ShiftLinesEndGuard {
            screen: self.screens.active,
            left: self.scrolling_region.left,
            start_y,
        };

        // We have a slower path if we have left or right scroll margins.
        // const left_right = self.scrolling_region.left > 0 or
        //     self.scrolling_region.right < self.cols - 1;
        let left_right =
            self.scrolling_region.left > 0 || self.scrolling_region.right < self.cols - 1;

        // Remaining rows from our cursor to the bottom of the scroll region.
        // const rem = self.scrolling_region.bottom - self.screens.active.cursor.y + 1;
        let rem = self.scrolling_region.bottom - unsafe { (*self.screens.active).cursor.y } + 1;

        // We can only insert lines up to our remaining lines in the scroll
        // region. So we take whichever is smaller.
        // const adjusted_count = @min(count, rem);
        let adjusted_count = core::cmp::min(count, rem as usize) as size::CellCountInt;

        // Create a new tracked pin which we'll use to navigate the page list
        // so that if we need to adjust capacity it will be properly tracked.
        // var cur_p = self.screens.active.pages.trackPin(
        //     self.screens.active.cursor.page_pin.*,
        // ) catch |err| { ... @panic("deleteLines trackPin OOM"); };
        // defer self.screens.active.pages.untrackPin(cur_p);
        let cursor_pin = unsafe { *(*self.screens.active).cursor.page_pin };
        let cur_p: *mut crate::pagelist::Pin = match self.active_mut().pages.track_pin(cursor_pin) {
            Ok(p) => p,
            Err(_err) => {
                // See insertLines
                // comptime assert(@TypeOf(err) == error{OutOfMemory});
                // log.err("deleteLines trackPin error err={}", .{err});
                // @panic("deleteLines trackPin OOM");
                panic!("deleteLines trackPin OOM");
            }
        };
        // PORT NOTE: RAII guard reproduces `defer untrackPin(cur_p)`.
        let _untrack = TrackedPinGuard {
            screen: self.screens.active,
            pin: cur_p,
        };

        // Our current y position relative to the cursor
        // var y: usize = 0;
        let mut y: usize = 0;

        // Traverse from the top down
        // while (y < rem) {
        while y < rem as usize {
            // const cur_rac = cur_p.rowAndCell();
            // const cur_row: *Row = cur_rac.row;
            let cur_row: *mut Row = unsafe { (*cur_p).row_and_cell() }.row;

            // If this is one of the lines we need to shift, do so
            // if (y < rem - adjusted_count) {
            if y < (rem - adjusted_count) as usize {
                // const off_p = cur_p.down(adjusted_count).?;
                let off_p = unsafe { (*cur_p).down(adjusted_count as usize) }.unwrap();
                // const off_rac = off_p.rowAndCell();
                // const off_row: *Row = off_rac.row;
                let off_row: *mut Row = off_p.row_and_cell().row;

                // self.rowWillBeShifted(&cur_p.node.data, cur_row);
                let cur_page: *mut Page = unsafe { core::ptr::addr_of_mut!((*(*cur_p).node).data) };
                self.row_will_be_shifted(cur_page, cur_row);
                // self.rowWillBeShifted(&off_p.node.data, off_row);
                let off_page: *mut Page = unsafe { core::ptr::addr_of_mut!((*off_p.node).data) };
                self.row_will_be_shifted(off_page, off_row);

                // If our scrolling region is full width, then we unset wrap.
                // if (!left_right) {
                if !left_right {
                    // off_row.wrap = false;
                    unsafe { (*off_row).set_wrap(false) };
                    // cur_row.wrap = false;
                    unsafe { (*cur_row).set_wrap(false) };
                    // off_row.wrap_continuation = false;
                    unsafe { (*off_row).set_wrap_continuation(false) };
                    // cur_row.wrap_continuation = false;
                    unsafe { (*cur_row).set_wrap_continuation(false) };
                }

                // const src_p = off_p;
                // const src_row = off_row;
                // const dst_p = cur_p;
                // const dst_row = cur_row;
                let src_p = off_p;
                let src_row = off_row;
                // dst_p is cur_p (the tracked pin), dst_row is cur_row.

                // If our page doesn't match, then we need to do a copy from
                // one page to another. This is the slow path.
                // if (src_p.node != dst_p.node) {
                if !std::ptr::eq(src_p.node, unsafe { (*cur_p).node }) {
                    // dst_p.node.data.clonePartialRowFrom(...) catch |err| { ... continue; };
                    let dst_page: *mut Page =
                        unsafe { core::ptr::addr_of_mut!((*(*cur_p).node).data) };
                    let src_page: *const Page = unsafe { core::ptr::addr_of!((*src_p.node).data) };
                    let clone_res = unsafe {
                        (*dst_page).clone_partial_row_from(
                            src_page,
                            cur_row,
                            src_row,
                            self.scrolling_region.left as usize,
                            (self.scrolling_region.right + 1) as usize,
                        )
                    };
                    if let Err(err) = clone_res {
                        // Adjust our page capacity to make
                        // room for we didn't have space for
                        // _ = self.screens.active.increaseCapacity(dst_p.node, switch (err) {...})
                        //     catch |e| switch (e) {...};
                        let adjustment = match err {
                            // Rehash the sets
                            pagepkg::CloneFromError::StyleSetNeedsRehash
                            | pagepkg::CloneFromError::HyperlinkSetNeedsRehash => None,

                            // Increase style memory
                            pagepkg::CloneFromError::StyleSetOutOfMemory => {
                                Some(pagelist::grow::IncreaseCapacity::Styles)
                            }

                            // Increase string memory
                            pagepkg::CloneFromError::StringAllocOutOfMemory => {
                                Some(pagelist::grow::IncreaseCapacity::StringBytes)
                            }

                            // Increase hyperlink memory
                            pagepkg::CloneFromError::HyperlinkSetOutOfMemory
                            | pagepkg::CloneFromError::HyperlinkMapOutOfMemory => {
                                Some(pagelist::grow::IncreaseCapacity::HyperlinkBytes)
                            }

                            // Increase grapheme memory
                            pagepkg::CloneFromError::GraphemeMapOutOfMemory
                            | pagepkg::CloneFromError::GraphemeAllocOutOfMemory => {
                                Some(pagelist::grow::IncreaseCapacity::GraphemeBytes)
                            }
                        };
                        let cur_node = unsafe { (*cur_p).node };
                        match self.active_mut().increase_capacity(cur_node, adjustment) {
                            Ok(_) => {}
                            Err(e) => match e {
                                // See insertLines
                                pagelist::grow::IncreaseCapacityError::OutOfMemory => {
                                    panic!("increaseCapacity system allocator OOM")
                                }

                                pagelist::grow::IncreaseCapacityError::OutOfSpace => {
                                    panic!("increaseCapacity OutOfSpace")
                                }
                            },
                        }

                        // Continue the loop to try handling this row again.
                        continue;
                    }
                } else {
                    // if (!left_right) {
                    if !left_right {
                        // Swap the src/dst cells. This ensures that our dst gets the
                        // proper shifted rows and src gets non-garbage cell data that
                        // we can clear.
                        // const dst = dst_row.*;
                        // dst_row.* = src_row.*;
                        // src_row.* = dst;
                        // PORT NOTE: see insertLines — `core::ptr::swap` of the rows.
                        unsafe { core::ptr::swap(cur_row, src_row) };

                        // Ensure what we did didn't corrupt the page
                        // cur_p.node.data.assertIntegrity();
                        let cur_page: *const Page =
                            unsafe { core::ptr::addr_of!((*(*cur_p).node).data) };
                        unsafe { (*cur_page).assert_integrity() };
                    } else {
                        // Left/right scroll margins we have to
                        // copy cells, which is much slower...
                        // const page = &cur_p.node.data;
                        let page: *mut Page =
                            unsafe { core::ptr::addr_of_mut!((*(*cur_p).node).data) };
                        // page.moveCells(src_row, self.scrolling_region.left,
                        //     dst_row, self.scrolling_region.left,
                        //     (self.scrolling_region.right - self.scrolling_region.left) + 1);
                        unsafe {
                            (*page).move_cells(
                                src_row,
                                self.scrolling_region.left as usize,
                                cur_row,
                                self.scrolling_region.left as usize,
                                ((self.scrolling_region.right - self.scrolling_region.left) + 1)
                                    as usize,
                            )
                        };
                    }
                }
            } else {
                // Clear the cells for this row, it's from out of bounds.
                // self.rowWillBeShifted(&cur_p.node.data, cur_row);
                let cur_page: *mut Page = unsafe { core::ptr::addr_of_mut!((*(*cur_p).node).data) };
                self.row_will_be_shifted(cur_page, cur_row);
                // const page = &cur_p.node.data;
                let page: *mut Page = unsafe { core::ptr::addr_of_mut!((*(*cur_p).node).data) };
                // const cells = page.getCells(cur_row);
                let cells: *mut [pagepkg::Cell] = unsafe { (*page).get_cells(cur_row) };
                // self.screens.active.clearCells(page, cur_row,
                //     cells[self.scrolling_region.left .. self.scrolling_region.right + 1]);
                let sub = ptr_slice_range(
                    cells,
                    self.scrolling_region.left as usize,
                    (self.scrolling_region.right + 1) as usize,
                );
                self.active_mut().clear_cells(page, cur_row, sub);
            }

            // Mark the row as dirty
            // cur_p.markDirty();
            unsafe { (*cur_p).mark_dirty() };

            // We have successfully processed a line.
            // y += 1;
            y += 1;
            // Move our pin down to the next row.
            // if (cur_p.down(1)) |p| cur_p.* = p;
            if let Some(p) = unsafe { (*cur_p).down(1) } {
                unsafe { *cur_p = p };
            }
        }
    }

    /// Inserts spaces at current cursor position moving existing cell contents
    /// to the right. The contents of the count right-most columns in the scroll
    /// region are lost. The cursor position is not changed.
    ///
    /// This unsets the pending wrap state without wrapping.
    ///
    /// The inserted cells are colored according to the current SGR state.
    pub fn insert_blanks(&mut self, count: usize) {
        // Unset pending wrap state without wrapping. Note: this purposely
        // happens BEFORE the scroll region check below, because that's what
        // xterm does.
        // self.screens.active.cursor.pending_wrap = false;
        unsafe { (*self.screens.active).cursor.pending_wrap = false };

        // If we're given a zero then we do nothing. The rest of this function
        // assumes count > 0 and will crash if zero so return early. Note that
        // this shouldn't be possible with real CSI sequences because the value
        // is clamped to 1 min.
        // if (count == 0) return;
        if count == 0 {
            return;
        }

        // If our cursor is outside the margins then do nothing. We DO reset
        // wrap state still so this must remain below the above logic.
        // if (self.screens.active.cursor.x < self.scrolling_region.left or
        //     self.screens.active.cursor.x > self.scrolling_region.right) return;
        if unsafe { (*self.screens.active).cursor.x } < self.scrolling_region.left
            || unsafe { (*self.screens.active).cursor.x } > self.scrolling_region.right
        {
            return;
        }

        // If our count is larger than the remaining amount, we just erase right.
        // We only do this if we can erase the entire line (no right margin).
        // if (right_limit == self.cols and
        //     count > right_limit - self.screens.active.cursor.x)
        // {
        //     self.eraseLine(.right, false);
        //     return;
        // }

        // left is just the cursor position but as a multi-pointer
        // const left: [*]Cell = @ptrCast(self.screens.active.cursor.page_cell);
        let left: *mut pagepkg::Cell = unsafe { (*self.screens.active).cursor.page_cell };
        // var page = &self.screens.active.cursor.page_pin.node.data;
        let page: *mut Page = unsafe {
            core::ptr::addr_of_mut!((*(*(*self.screens.active).cursor.page_pin).node).data)
        };

        // If our X is a wide spacer tail then we need to erase the
        // previous cell too so we don't split a multi-cell character.
        // if (self.screens.active.cursor.page_cell.wide == .spacer_tail) {
        if unsafe { (*(*self.screens.active).cursor.page_cell).wide() } == pagepkg::Wide::SpacerTail
        {
            // assert(self.screens.active.cursor.x > 0);
            assert!(unsafe { (*self.screens.active).cursor.x } > 0);
            // self.screens.active.clearCells(page, self.screens.active.cursor.page_row, (left - 1)[0..2]);
            let row: *mut Row = unsafe { (*self.screens.active).cursor.page_row };
            let cells = core::ptr::slice_from_raw_parts_mut(unsafe { left.sub(1) }, 2);
            self.active_mut().clear_cells(page, row, cells);
        }

        // Remaining cols from our cursor to the right margin.
        // const rem = self.scrolling_region.right - self.screens.active.cursor.x + 1;
        let rem = self.scrolling_region.right - unsafe { (*self.screens.active).cursor.x } + 1;

        // If the cell at the right margin is wide, its spacer tail is
        // outside the scroll region and would be orphaned by either the
        // shift or the clear. Clean up both halves up front.
        {
            // const right_cell: *Cell = @ptrCast(left + (rem - 1));
            let right_cell: *mut pagepkg::Cell = unsafe { left.add((rem - 1) as usize) };
            // if (right_cell.wide == .wide) self.screens.active.clearCells(
            //     page, self.screens.active.cursor.page_row,
            //     @as([*]Cell, @ptrCast(right_cell))[0..2]);
            if unsafe { (*right_cell).wide() } == pagepkg::Wide::Wide {
                let row: *mut Row = unsafe { (*self.screens.active).cursor.page_row };
                let cells = core::ptr::slice_from_raw_parts_mut(right_cell, 2);
                self.active_mut().clear_cells(page, row, cells);
            }
        }

        // We can only insert blanks up to our remaining cols
        // const adjusted_count = @min(count, rem);
        let adjusted_count = core::cmp::min(count, rem as usize) as size::CellCountInt;

        // This is the amount of space at the right of the scroll region
        // that will NOT be blank, so we need to shift the correct cols right.
        // "scroll_amount" is the number of such cols.
        // const scroll_amount = rem - adjusted_count;
        let scroll_amount = rem - adjusted_count;
        // if (scroll_amount > 0) {
        if scroll_amount > 0 {
            // page.pauseIntegrityChecks(true);
            unsafe { (*page).pause_integrity_checks(true) };
            // defer page.pauseIntegrityChecks(false);

            // var x: [*]Cell = left + (scroll_amount - 1);
            let mut x: *mut pagepkg::Cell = unsafe { left.add((scroll_amount - 1) as usize) };

            // If our last cell we're shifting is wide, then we need to clear
            // it to be empty so we don't split the multi-cell char.
            // const end: *Cell = @ptrCast(x);
            let end: *mut pagepkg::Cell = x;
            // if (end.wide == .wide) {
            if unsafe { (*end).wide() } == pagepkg::Wide::Wide {
                // const end_multi: [*]Cell = @ptrCast(end);
                let end_multi: *mut pagepkg::Cell = end;
                // assert(end_multi[1].wide == .spacer_tail);
                assert!(unsafe { (*end_multi.add(1)).wide() } == pagepkg::Wide::SpacerTail);
                // self.screens.active.clearCells(page, self.screens.active.cursor.page_row, end_multi[0..2]);
                let row: *mut Row = unsafe { (*self.screens.active).cursor.page_row };
                let cells = core::ptr::slice_from_raw_parts_mut(end_multi, 2);
                self.active_mut().clear_cells(page, row, cells);
            }

            // We work backwards so we don't overwrite data.
            // while (@intFromPtr(x) >= @intFromPtr(left)) : (x -= 1) {
            while (x as usize) >= (left as usize) {
                // const src: *Cell = @ptrCast(x);
                let src: *mut pagepkg::Cell = x;
                // const dst: *Cell = @ptrCast(x + adjusted_count);
                let dst: *mut pagepkg::Cell = unsafe { x.add(adjusted_count as usize) };
                // page.swapCells(src, dst);
                unsafe { (*page).swap_cells(src, dst) };
                // PORT NOTE: the reference `x -= 1` decrements a `[*]Cell` (one
                // Cell). On the final iteration `x == left`, so the next
                // decrement would form a pointer one-before-`left`; the loop then
                // exits on the condition. We mirror this without computing an
                // out-of-bounds pointer: decrement only while we won't underflow,
                // otherwise break (the condition `x >= left` would be false).
                if std::ptr::eq(x, left) {
                    break;
                }
                x = unsafe { x.sub(1) };
            }

            unsafe { (*page).pause_integrity_checks(false) };
        }

        // Insert blanks. The blanks preserve the background color.
        // self.screens.active.clearCells(page, self.screens.active.cursor.page_row, left[0..adjusted_count]);
        let row: *mut Row = unsafe { (*self.screens.active).cursor.page_row };
        let cells = core::ptr::slice_from_raw_parts_mut(left, adjusted_count as usize);
        self.active_mut().clear_cells(page, row, cells);

        // Our row is always dirty
        // self.screens.active.cursorMarkDirty();
        self.active_mut().cursor_mark_dirty();
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
        if count_req == 0 {
            return;
        }

        // If our cursor is outside the margins then do nothing. We DO reset
        // wrap state still so this must remain below the above logic.
        if unsafe { (*self.screens.active).cursor.x } < self.scrolling_region.left
            || unsafe { (*self.screens.active).cursor.x } > self.scrolling_region.right
        {
            return;
        }

        // left is just the cursor position but as a multi-pointer
        // const left: [*]Cell = @ptrCast(self.screens.active.cursor.page_cell);
        let left: *mut pagepkg::Cell = unsafe { (*self.screens.active).cursor.page_cell };
        // var page = &self.screens.active.cursor.page_pin.node.data;
        let page: *mut Page = unsafe {
            core::ptr::addr_of_mut!((*(*(*self.screens.active).cursor.page_pin).node).data)
        };

        // Remaining cols from our cursor to the right margin.
        let rem = self.scrolling_region.right - unsafe { (*self.screens.active).cursor.x } + 1;

        // We can only insert blanks up to our remaining cols
        let count = core::cmp::min(count_req, rem as usize) as size::CellCountInt;

        let cursor_x = unsafe { (*self.screens.active).cursor.x };
        let sr_right = self.scrolling_region.right;
        self.active_mut().split_cell_boundary(cursor_x);
        self.active_mut().split_cell_boundary(cursor_x + count);
        self.active_mut().split_cell_boundary(sr_right + 1);

        // This is the amount of space at the right of the scroll region
        // that will NOT be blank, so we need to shift the correct cols right.
        // "scroll_amount" is the number of such cols.
        let scroll_amount = rem - count;
        // var x: [*]Cell = left;
        let mut x: *mut pagepkg::Cell = left;
        if scroll_amount > 0 {
            unsafe { (*page).pause_integrity_checks(true) };
            // defer page.pauseIntegrityChecks(false);

            // const right: [*]Cell = left + (scroll_amount - 1);
            let right: *mut pagepkg::Cell = unsafe { left.add((scroll_amount - 1) as usize) };

            // while (@intFromPtr(x) <= @intFromPtr(right)) : (x += 1) {
            while (x as usize) <= (right as usize) {
                // const src: *Cell = @ptrCast(x + count);
                let src: *mut pagepkg::Cell = unsafe { x.add(count as usize) };
                // const dst: *Cell = @ptrCast(x);
                let dst: *mut pagepkg::Cell = x;
                unsafe { (*page).swap_cells(src, dst) };
                x = unsafe { x.add(1) };
            }

            unsafe { (*page).pause_integrity_checks(false) };
        }

        // Insert blanks. The blanks preserve the background color.
        // self.screens.active.clearCells(page, ..., x[0 .. rem - scroll_amount]);
        let row: *mut Row = unsafe { (*self.screens.active).cursor.page_row };
        let cells = core::ptr::slice_from_raw_parts_mut(x, (rem - scroll_amount) as usize);
        self.active_mut().clear_cells(page, row, cells);

        // Our row's soft-wrap is always reset.
        self.active_mut().cursor_reset_wrap();

        // Our row is always dirty
        self.active_mut().cursor_mark_dirty();
    }

    pub fn erase_chars(&mut self, count_req: usize) {
        let count = {
            let remaining = self.cols - unsafe { (*self.screens.active).cursor.x };
            let mut end = core::cmp::min(
                remaining,
                core::cmp::max(count_req, 1) as size::CellCountInt,
            );

            // If our last cell is a wide char then we need to also clear the
            // cell beyond it since we can't just split a wide char.
            if end != remaining {
                let last = self.active_mut().cursor_cell_right(end - 1);
                if unsafe { (*last).wide() } == pagepkg::Wide::Wide {
                    end += 1;
                }
            }

            end
        };

        // Handle any boundary conditions on the edges of the erased area.
        //
        // TODO(qwerasd): This isn't actually correct if you take in to account
        // protected modes. We need to figure out how to make `clearCells` or at
        // least `clearUnprotectedCells` handle boundary conditions...
        let cursor_x = unsafe { (*self.screens.active).cursor.x };
        self.active_mut().split_cell_boundary(cursor_x);
        self.active_mut().split_cell_boundary(cursor_x + count);

        // Reset our row's soft-wrap.
        self.active_mut().cursor_reset_wrap();

        // Mark our cursor row as dirty
        self.active_mut().cursor_mark_dirty();

        // Clear the cells
        // const cells: [*]Cell = @ptrCast(self.screens.active.cursor.page_cell);
        let cells: *mut pagepkg::Cell = unsafe { (*self.screens.active).cursor.page_cell };

        let page: *mut Page = unsafe {
            core::ptr::addr_of_mut!((*(*(*self.screens.active).cursor.page_pin).node).data)
        };
        let row: *mut Row = unsafe { (*self.screens.active).cursor.page_row };
        let cells_slice = core::ptr::slice_from_raw_parts_mut(cells, count as usize);

        // If we never had a protection mode, then we can assume no cells
        // are protected and go with the fast path. If the last protection
        // mode was not ISO we also always ignore protection attributes.
        if unsafe { (*self.screens.active).protected_mode } != ansi::ProtectedMode::Iso {
            self.active_mut().clear_cells(page, row, cells_slice);
            return;
        }

        self.active_mut()
            .clear_unprotected_cells(page, row, cells_slice);
    }

    /// Erase the line.
    pub fn erase_line(&mut self, mode: csi::EraseLine, protected_req: bool) {
        // Get our start/end positions depending on mode.
        let (start, end): (size::CellCountInt, size::CellCountInt) = match mode {
            csi::EraseLine::Right => {
                let mut x = unsafe { (*self.screens.active).cursor.x };

                // If our X is a wide spacer tail then we need to erase the
                // previous cell too so we don't split a multi-cell character.
                if x > 0
                    && unsafe { (*(*self.screens.active).cursor.page_cell).wide() }
                        == pagepkg::Wide::SpacerTail
                {
                    x -= 1;
                }

                // Reset our row's soft-wrap.
                self.active_mut().cursor_reset_wrap();

                (x, self.cols)
            }

            csi::EraseLine::Left => {
                let mut x = unsafe { (*self.screens.active).cursor.x };

                // If our x is a wide char we need to delete the tail too.
                if unsafe { (*(*self.screens.active).cursor.page_cell).wide() }
                    == pagepkg::Wide::Wide
                {
                    x += 1;
                }

                (0, x + 1)
            }

            // Note that it seems like complete should reset the soft-wrap
            // state of the line but in xterm it does not.
            csi::EraseLine::Complete => (0, self.cols),

            _ => {
                // log.err("unimplemented erase line mode: {}", .{mode});
                return;
            }
        };

        // All modes will clear the pending wrap state and we know we have
        // a valid mode at this point.
        unsafe { (*self.screens.active).cursor.pending_wrap = false };

        // We always mark our row as dirty
        self.active_mut().cursor_mark_dirty();

        // Start of our cells
        // const cells: [*]Cell = cells - self.screens.active.cursor.x;
        let cells: *mut pagepkg::Cell = {
            let cells: *mut pagepkg::Cell = unsafe { (*self.screens.active).cursor.page_cell };
            let x = unsafe { (*self.screens.active).cursor.x };
            unsafe { cells.sub(x as usize) }
        };

        // We respect protected attributes if explicitly requested (probably
        // a DECSEL sequence) or if our last protected mode was ISO even if its
        // not currently set.
        let protected = unsafe { (*self.screens.active).protected_mode }
            == ansi::ProtectedMode::Iso
            || protected_req;

        let page: *mut Page = unsafe {
            core::ptr::addr_of_mut!((*(*(*self.screens.active).cursor.page_pin).node).data)
        };
        let row: *mut Row = unsafe { (*self.screens.active).cursor.page_row };
        // cells[start..end]
        let cells_slice = core::ptr::slice_from_raw_parts_mut(
            unsafe { cells.add(start as usize) },
            (end - start) as usize,
        );

        // If we're not respecting protected attributes, we can use a fast-path
        // to fill the entire line.
        if !protected {
            self.active_mut().clear_cells(page, row, cells_slice);
            return;
        }

        self.active_mut()
            .clear_unprotected_cells(page, row, cells_slice);
    }

    /// Erase the display.
    pub fn erase_display(&mut self, mode: csi::EraseDisplay, protected_req: bool) {
        // We respect protected attributes if explicitly requested (probably
        // a DECSEL sequence) or if our last protected mode was ISO even if its
        // not currently set.
        let protected = unsafe { (*self.screens.active).protected_mode }
            == ansi::ProtectedMode::Iso
            || protected_req;

        match mode {
            csi::EraseDisplay::ScrollComplete => {
                // self.screens.active.scrollClear() catch |err| { ... };
                if self.active_mut().scroll_clear().is_err() {
                    // log.warn("scroll clear failed, doing a normal clear err={}", .{err});
                    self.erase_display(csi::EraseDisplay::Complete, protected_req);
                    return;
                }

                // Unsets pending wrap state
                unsafe { (*self.screens.active).cursor.pending_wrap = false };

                // PORT NOTE: the reference clears Kitty graphics state here inside
                // `if (comptime build_options.kitty_graphics)`. Kitty graphics
                // `ImageStorage` is DEFERRED to vt-12 so we omit the
                // `kitty_images.delete` call (matching `scroll_clear`/`clear_rows`
                // which likewise defer the Kitty graphics seam).
            }

            csi::EraseDisplay::Complete => {
                // If we're on the primary screen and our last non-empty row is
                // a prompt, then we do a scroll_complete instead. This is a
                // heuristic to get the generally desirable behavior that ^L
                // at a prompt scrolls the screen contents prior to clearing.
                // Most shells send `ESC [ H ESC [ 2 J` so we can't just check
                // our current cursor position. See #905
                if self.screens.active_key == crate::screen_set::Key::Primary {
                    'at_prompt: {
                        // Go from the bottom of the active up and see if we're
                        // at a prompt.
                        let active_br =
                            match self.active_mut().pages.get_bottom_right(point::Tag::Active) {
                                Some(p) => p,
                                None => break 'at_prompt,
                            };
                        let tl = self.active_mut().pages.get_top_left(point::Tag::Active);
                        let mut it = active_br
                            .row_iterator(crate::pagelist::iter::Direction::LeftUp, Some(tl));
                        // while (it.next()) |p| { switch (...) { .prompt,
                        // .prompt_continuation => break, .none => break :at_prompt } }
                        // else break :at_prompt;
                        // PORT NOTE: Zig's `while (cond) {...} else {...}` runs the
                        // `else` only on normal loop exhaustion. We mirror it: a
                        // `found` flag set true only on the prompt/continuation
                        // `break`; any other exit (`.none` or exhaustion) leaves it
                        // false and falls through to `break 'at_prompt`.
                        let mut found = false;
                        // PORT NOTE: every match arm `break`s, so this `while let`
                        // only ever runs one iteration (the reference `while` is
                        // identical: the bottom-most active row decides). We keep
                        // the loop form to mirror the reference `while...else`
                        // exhaustion semantics and allow the resulting lint.
                        #[allow(clippy::never_loop)]
                        while let Some(p) = it.next() {
                            let row = p.row_and_cell().row;
                            match unsafe { (*row).semantic_prompt() } {
                                // If we're at a prompt or input area, then we are at a prompt.
                                pagepkg::SemanticPrompt::Prompt
                                | pagepkg::SemanticPrompt::PromptContinuation => {
                                    found = true;
                                    break;
                                }

                                // If we have command output, then we're most certainly not
                                // at a prompt.
                                pagepkg::SemanticPrompt::None => break,
                            }
                        }
                        if !found {
                            break 'at_prompt;
                        }

                        // self.screens.active.scrollClear() catch {};
                        // If we fail, we just fall back to doing a normal clear
                        // so we don't worry about the error.
                        let _ = self.active_mut().scroll_clear();
                    }
                }

                // All active area
                self.active_mut().clear_rows(
                    point::Point::Active(point::Coordinate::default()),
                    None,
                    protected,
                );

                // Unsets pending wrap state
                unsafe { (*self.screens.active).cursor.pending_wrap = false };

                // PORT NOTE: the reference clears Kitty graphics state here inside
                // `if (comptime build_options.kitty_graphics)`. Kitty graphics
                // `ImageStorage` is DEFERRED to vt-12; omitted here (see the
                // `scroll_complete` note above).

                // Cleared screen dirty bit
                self.flags.dirty.clear = true;
            }

            csi::EraseDisplay::Below => {
                // All lines to the right (including the cursor)
                self.erase_line(csi::EraseLine::Right, protected_req);

                // All lines below
                if unsafe { (*self.screens.active).cursor.y } + 1 < self.rows {
                    let y = unsafe { (*self.screens.active).cursor.y } + 1;
                    self.active_mut().clear_rows(
                        point::Point::Active(point::Coordinate { x: 0, y: y as u32 }),
                        None,
                        protected,
                    );
                }

                // Unsets pending wrap state. Should be done by eraseLine.
                assert!(!unsafe { (*self.screens.active).cursor.pending_wrap });
            }

            csi::EraseDisplay::Above => {
                // Erase to the left (including the cursor)
                self.erase_line(csi::EraseLine::Left, protected_req);

                // All lines above
                if unsafe { (*self.screens.active).cursor.y } > 0 {
                    let y = unsafe { (*self.screens.active).cursor.y } - 1;
                    self.active_mut().clear_rows(
                        point::Point::Active(point::Coordinate::default()),
                        Some(point::Point::Active(point::Coordinate {
                            x: 0,
                            y: y as u32,
                        })),
                        protected,
                    );
                }

                // Unsets pending wrap state
                assert!(!unsafe { (*self.screens.active).cursor.pending_wrap });
            }

            csi::EraseDisplay::Scrollback => self.active_mut().erase_history(None),
        }
    }

    /// Resets all margins and fills the whole screen with the character 'E'
    ///
    /// Sets the cursor to the top left corner.
    pub fn decaln(&mut self) -> Result<(), OutOfMemory> {
        // Clear our stylistic attributes. This is the only thing that can
        // fail so we do it first so we can undo it.
        let old_style = unsafe { (*self.screens.active).cursor.style };
        unsafe {
            (*self.screens.active).cursor.style = style::Style {
                bg_color: (*self.screens.active).cursor.style.bg_color,
                fg_color: (*self.screens.active).cursor.style.fg_color,
                ..style::Style::default()
            }
        };
        // errdefer self.screens.active.cursor.style = old_style;
        // PORT NOTE: the reference `errdefer` restores the saved style if
        // `manualStyleUpdate` fails. In Rust we restore explicitly on the error
        // path before returning.
        if self.active_mut().manual_style_update().is_err() {
            unsafe { (*self.screens.active).cursor.style = old_style };
            return Err(OutOfMemory);
        }

        // Reset margins, also sets cursor to top-left
        self.scrolling_region = ScrollingRegion {
            top: 0,
            bottom: self.rows - 1,
            left: 0,
            right: self.cols - 1,
        };

        // Origin mode is disabled
        self.modes.set(modespkg::Mode::Origin, false);

        // Move our cursor to the top-left
        self.set_cursor_pos(1, 1);

        // Use clearRows instead of eraseDisplay because we must NOT respect
        // protected attributes here.
        self.active_mut().clear_rows(
            point::Point::Active(point::Coordinate::default()),
            None,
            false,
        );

        // Fill with Es by moving the cursor but reset it after.
        loop {
            // const page = &self.screens.active.cursor.page_pin.node.data;
            let page: *mut Page = unsafe {
                core::ptr::addr_of_mut!((*(*(*self.screens.active).cursor.page_pin).node).data)
            };
            // const row = self.screens.active.cursor.page_row;
            let row: *mut Row = unsafe { (*self.screens.active).cursor.page_row };
            // const cells_multi: [*]Cell = row.cells.ptr(page.memory);
            // const cells = cells_multi[0..page.size.cols];
            let cells_offset = unsafe { (*row).cells() };
            let cells_multi: *mut pagepkg::Cell = unsafe { cells_offset.ptr((*page).memory_ptr()) };
            let cols = unsafe { (*page).size.cols } as usize;

            // @memset(cells, .{ ... });
            let style_id = unsafe { (*self.screens.active).cursor.style_id };
            for i in 0..cols {
                let cell: *mut pagepkg::Cell = unsafe { cells_multi.add(i) };
                let mut new_cell = pagepkg::Cell::zeroed();
                new_cell.set_content_tag(pagepkg::ContentTag::Codepoint);
                new_cell.set_content_codepoint('E' as u21);
                new_cell.set_style_id(style_id);
                // DECALN does not respect protected state. Verified with xterm.
                new_cell.set_protected(false);
                unsafe { *cell = new_cell };
            }

            // If we have a ref-counted style, increase
            if style_id != style::DEFAULT_ID {
                unsafe {
                    let base = (*page).memory_ptr();
                    (*page).styles.use_multiple(base, style_id, cols as _);
                }
                unsafe { (*row).set_styled(true) };
            }

            // We messed with the page so assert its integrity here.
            unsafe { (*page).assert_integrity() };

            self.active_mut().cursor_mark_dirty();
            if unsafe { (*self.screens.active).cursor.y } == self.rows - 1 {
                break;
            }
            self.active_mut().cursor_down(1);
        }

        // Reset the cursor to the top-left
        self.set_cursor_pos(1, 1);

        Ok(())
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
        // return t.screens.active.pages.getCell(pt).?.isDirty();
        unsafe {
            (*self.screens.active)
                .pages
                .get_cell(pt)
                .unwrap()
                .is_dirty()
        }
    }

    /// Clear all dirty bits. Testing only.
    fn clear_dirty(&mut self) {
        // t.screens.active.pages.clearDirty();
        self.active_mut().pages.clear_dirty();
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

    // Active-point helper mirroring the reference
    // `.{ .active = .{ .x, .y } }` literals used in the dirty-bit assertions.
    fn active_pt(x: size::CellCountInt, y: u32) -> point::Point {
        point::Point::Active(point::Coordinate { x, y })
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

    // test "Terminal: print invoke charset single"
    #[test]
    fn terminal_print_invoke_charset_single() {
        let mut t = Terminal::init(Global, opts(80, 80)).unwrap();

        t.configure_charset(charsets::Slots::G1, charsets::Charset::DecSpecial);

        // Basic grid writing
        t.print('`' as u32).unwrap();
        t.invoke_charset(charsets::ActiveSlot::GL, charsets::Slots::G1, true);
        t.print('`' as u32).unwrap();
        t.print('`' as u32).unwrap();
        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "`◆`".as_bytes());
        }

        t.deinit(Global);
    }

    // PORT NOTE (vt-12b-2 deferral): test "Terminal: linefeed mode automatic
    // carriage return" calls `linefeed()`, which dispatches to `index()` —
    // still `todo!()` until vt-12b-3b. Deferred there with the other
    // `index()`/scroll-dependent tests.

    // test "Terminal: carriage return unsets pending wrap"
    #[test]
    fn terminal_carriage_return_unsets_pending_wrap() {
        let mut t = Terminal::init(Global, opts(5, 80)).unwrap();

        // Basic grid writing
        for c in "hello".chars() {
            t.print(c as u32).unwrap();
        }
        assert!(unsafe { (*t.screens.active).cursor.pending_wrap });
        t.carriage_return();
        assert!(!unsafe { (*t.screens.active).cursor.pending_wrap });

        t.deinit(Global);
    }

    // test "Terminal: carriage return origin mode moves to left margin"
    #[test]
    fn terminal_carriage_return_origin_mode_moves_to_left_margin() {
        let mut t = Terminal::init(Global, opts(5, 80)).unwrap();

        t.modes.set(modespkg::Mode::Origin, true);
        unsafe { (*t.screens.active).cursor.x = 0 };
        t.scrolling_region.left = 2;
        t.carriage_return();
        assert_eq!(unsafe { (*t.screens.active).cursor.x }, 2);

        t.deinit(Global);
    }

    // test "Terminal: carriage return left of left margin moves to zero"
    #[test]
    fn terminal_carriage_return_left_of_left_margin_moves_to_zero() {
        let mut t = Terminal::init(Global, opts(5, 80)).unwrap();

        unsafe { (*t.screens.active).cursor.x = 1 };
        t.scrolling_region.left = 2;
        t.carriage_return();
        assert_eq!(unsafe { (*t.screens.active).cursor.x }, 0);

        t.deinit(Global);
    }

    // test "Terminal: carriage return right of left margin moves to left margin"
    #[test]
    fn terminal_carriage_return_right_of_left_margin_moves_to_left_margin() {
        let mut t = Terminal::init(Global, opts(5, 80)).unwrap();

        unsafe { (*t.screens.active).cursor.x = 3 };
        t.scrolling_region.left = 2;
        t.carriage_return();
        assert_eq!(unsafe { (*t.screens.active).cursor.x }, 2);

        t.deinit(Global);
    }

    // test "Terminal: backspace"
    #[test]
    fn terminal_backspace() {
        let mut t = Terminal::init(Global, opts(80, 80)).unwrap();

        // BS
        for c in "hello".chars() {
            t.print(c as u32).unwrap();
        }
        t.backspace();
        t.print('y' as u32).unwrap();
        assert_eq!(unsafe { (*t.screens.active).cursor.y }, 0);
        assert_eq!(unsafe { (*t.screens.active).cursor.x }, 5);
        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "helly".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: horizontal tabs"
    #[test]
    fn terminal_horizontal_tabs() {
        let mut t = Terminal::init(Global, opts(20, 5)).unwrap();

        // HT
        t.print('1' as u32).unwrap();
        t.horizontal_tab();
        assert_eq!(unsafe { (*t.screens.active).cursor.x }, 8);

        // HT
        t.horizontal_tab();
        assert_eq!(unsafe { (*t.screens.active).cursor.x }, 16);

        // HT at the end
        t.horizontal_tab();
        assert_eq!(unsafe { (*t.screens.active).cursor.x }, 19);
        t.horizontal_tab();
        assert_eq!(unsafe { (*t.screens.active).cursor.x }, 19);

        t.deinit(Global);
    }

    // test "Terminal: horizontal tabs starting on tabstop"
    #[test]
    fn terminal_horizontal_tabs_starting_on_tabstop() {
        let mut t = Terminal::init(Global, opts(20, 5)).unwrap();

        let cursor_y = unsafe { (*t.screens.active).cursor.y } as usize;
        t.set_cursor_pos(cursor_y, 9);
        t.print('X' as u32).unwrap();
        let cursor_y = unsafe { (*t.screens.active).cursor.y } as usize;
        t.set_cursor_pos(cursor_y, 9);
        t.horizontal_tab();
        t.print('A' as u32).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "        X       A".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: horizontal tabs with right margin"
    #[test]
    fn terminal_horizontal_tabs_with_right_margin() {
        let mut t = Terminal::init(Global, opts(20, 5)).unwrap();

        t.scrolling_region.left = 2;
        t.scrolling_region.right = 5;
        let cursor_y = unsafe { (*t.screens.active).cursor.y } as usize;
        t.set_cursor_pos(cursor_y, 1);
        t.print('X' as u32).unwrap();
        t.horizontal_tab();
        t.print('A' as u32).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "X    A".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: horizontal tabs back"
    #[test]
    fn terminal_horizontal_tabs_back() {
        let mut t = Terminal::init(Global, opts(20, 5)).unwrap();

        // Edge of screen
        let cursor_y = unsafe { (*t.screens.active).cursor.y } as usize;
        t.set_cursor_pos(cursor_y, 20);

        // HT
        t.horizontal_tab_back();
        assert_eq!(unsafe { (*t.screens.active).cursor.x }, 16);

        // HT
        t.horizontal_tab_back();
        assert_eq!(unsafe { (*t.screens.active).cursor.x }, 8);

        // HT
        t.horizontal_tab_back();
        assert_eq!(unsafe { (*t.screens.active).cursor.x }, 0);
        t.horizontal_tab_back();
        assert_eq!(unsafe { (*t.screens.active).cursor.x }, 0);

        t.deinit(Global);
    }

    // test "Terminal: horizontal tabs back starting on tabstop"
    #[test]
    fn terminal_horizontal_tabs_back_starting_on_tabstop() {
        let mut t = Terminal::init(Global, opts(20, 5)).unwrap();

        let cursor_y = unsafe { (*t.screens.active).cursor.y } as usize;
        t.set_cursor_pos(cursor_y, 9);
        t.print('X' as u32).unwrap();
        let cursor_y = unsafe { (*t.screens.active).cursor.y } as usize;
        t.set_cursor_pos(cursor_y, 9);
        t.horizontal_tab_back();
        t.print('A' as u32).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "A       X".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: horizontal tabs with left margin in origin mode"
    #[test]
    fn terminal_horizontal_tabs_with_left_margin_in_origin_mode() {
        let mut t = Terminal::init(Global, opts(20, 5)).unwrap();

        t.modes.set(modespkg::Mode::Origin, true);
        t.scrolling_region.left = 2;
        t.scrolling_region.right = 5;
        t.set_cursor_pos(1, 2);
        t.print('X' as u32).unwrap();
        t.horizontal_tab_back();
        t.print('A' as u32).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "  AX".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: cursorPos resets wrap"
    #[test]
    fn terminal_cursor_pos_resets_wrap() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        for c in "ABCDE".chars() {
            t.print(c as u32).unwrap();
        }
        assert!(unsafe { (*t.screens.active).cursor.pending_wrap });
        t.set_cursor_pos(1, 1);
        assert!(!unsafe { (*t.screens.active).cursor.pending_wrap });
        t.print('X' as u32).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "XBCDE".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: cursorPos off the screen"
    #[test]
    fn terminal_cursor_pos_off_the_screen() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.set_cursor_pos(500, 500);
        t.print('X' as u32).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "\n\n\n\n    X".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: cursorPos relative to origin"
    #[test]
    fn terminal_cursor_pos_relative_to_origin() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.scrolling_region.top = 2;
        t.scrolling_region.bottom = 3;
        t.modes.set(modespkg::Mode::Origin, true);
        t.set_cursor_pos(1, 1);
        t.print('X' as u32).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "\n\nX".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: cursorPos relative to origin with left/right"
    #[test]
    fn terminal_cursor_pos_relative_to_origin_with_left_right() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.scrolling_region.top = 2;
        t.scrolling_region.bottom = 3;
        t.scrolling_region.left = 2;
        t.scrolling_region.right = 4;
        t.modes.set(modespkg::Mode::Origin, true);
        t.set_cursor_pos(1, 1);
        t.print('X' as u32).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "\n\n  X".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: cursorPos limits with full scroll region"
    #[test]
    fn terminal_cursor_pos_limits_with_full_scroll_region() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.scrolling_region.top = 2;
        t.scrolling_region.bottom = 3;
        t.scrolling_region.left = 2;
        t.scrolling_region.right = 4;
        t.modes.set(modespkg::Mode::Origin, true);
        t.set_cursor_pos(500, 500);
        t.print('X' as u32).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "\n\n\n    X".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: cursorUp basic"
    #[test]
    fn terminal_cursor_up_basic() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.set_cursor_pos(3, 1);
        t.print('A' as u32).unwrap();
        t.cursor_up(10);
        t.print('X' as u32).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), " X\n\nA".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: cursorUp resets wrap"
    #[test]
    fn terminal_cursor_up_resets_wrap() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        for c in "ABCDE".chars() {
            t.print(c as u32).unwrap();
        }
        assert!(unsafe { (*t.screens.active).cursor.pending_wrap });
        t.cursor_up(1);
        assert!(!unsafe { (*t.screens.active).cursor.pending_wrap });
        t.print('X' as u32).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "ABCDX".as_bytes());
        }

        t.deinit(Global);
    }

    // PORT NOTE (vt-12b-2 deferral): test "Terminal: cursorLeft no wrap" calls
    // `linefeed()` (→ `index()`, still `todo!()`); deferred to vt-12b-3b.

    // test "Terminal: cursorLeft unsets pending wrap state"
    #[test]
    fn terminal_cursor_left_unsets_pending_wrap_state() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        for c in "ABCDE".chars() {
            t.print(c as u32).unwrap();
        }
        assert!(unsafe { (*t.screens.active).cursor.pending_wrap });
        t.cursor_left(1);
        assert!(!unsafe { (*t.screens.active).cursor.pending_wrap });
        t.print('X' as u32).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "ABCXE".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: cursorLeft unsets pending wrap state with longer jump"
    #[test]
    fn terminal_cursor_left_unsets_pending_wrap_state_with_longer_jump() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        for c in "ABCDE".chars() {
            t.print(c as u32).unwrap();
        }
        assert!(unsafe { (*t.screens.active).cursor.pending_wrap });
        t.cursor_left(3);
        assert!(!unsafe { (*t.screens.active).cursor.pending_wrap });
        t.print('X' as u32).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "AXCDE".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: cursorLeft reverse wrap with pending wrap state"
    #[test]
    fn terminal_cursor_left_reverse_wrap_with_pending_wrap_state() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.modes.set(modespkg::Mode::Wraparound, true);
        t.modes.set(modespkg::Mode::ReverseWrap, true);

        for c in "ABCDE".chars() {
            t.print(c as u32).unwrap();
        }
        assert!(unsafe { (*t.screens.active).cursor.pending_wrap });
        t.cursor_left(1);
        assert!(!unsafe { (*t.screens.active).cursor.pending_wrap });
        t.print('X' as u32).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "ABCDX".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: cursorLeft reverse wrap extended with pending wrap state"
    #[test]
    fn terminal_cursor_left_reverse_wrap_extended_with_pending_wrap_state() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.modes.set(modespkg::Mode::Wraparound, true);
        t.modes.set(modespkg::Mode::ReverseWrapExtended, true);

        for c in "ABCDE".chars() {
            t.print(c as u32).unwrap();
        }
        assert!(unsafe { (*t.screens.active).cursor.pending_wrap });
        t.cursor_left(1);
        assert!(!unsafe { (*t.screens.active).cursor.pending_wrap });
        t.print('X' as u32).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "ABCDX".as_bytes());
        }

        t.deinit(Global);
    }

    // PORT NOTE (vt-12b-2 deferral): the following five `cursorLeft` reverse-wrap
    // tests each reach `index()` (via printing past the edge, which calls
    // `print_wrap()` → `index()`, or via `linefeed()`), which is still `todo!()`
    // until vt-12b-3b. They are deferred there with the other `index()`/scroll
    // tests:
    //   test "Terminal: cursorLeft reverse wrap"
    //   test "Terminal: cursorLeft reverse wrap with no soft wrap"
    //   test "Terminal: cursorLeft extended reverse wrap"
    //   test "Terminal: cursorLeft extended reverse wrap bottom wraparound"
    //   test "Terminal: cursorLeft extended reverse wrap is priority if both set"

    // test "Terminal: cursorDown basic"
    #[test]
    fn terminal_cursor_down_basic() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.print('A' as u32).unwrap();
        t.cursor_down(10);
        t.print('X' as u32).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "A\n\n\n\n X".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: cursorDown resets wrap"
    #[test]
    fn terminal_cursor_down_resets_wrap() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        for c in "ABCDE".chars() {
            t.print(c as u32).unwrap();
        }
        assert!(unsafe { (*t.screens.active).cursor.pending_wrap });
        t.cursor_down(1);
        assert!(!unsafe { (*t.screens.active).cursor.pending_wrap });
        t.print('X' as u32).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "ABCDE\n    X".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: cursorRight resets wrap"
    #[test]
    fn terminal_cursor_right_resets_wrap() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        for c in "ABCDE".chars() {
            t.print(c as u32).unwrap();
        }
        assert!(unsafe { (*t.screens.active).cursor.pending_wrap });
        t.cursor_right(1);
        assert!(!unsafe { (*t.screens.active).cursor.pending_wrap });
        t.print('X' as u32).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "ABCDX".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: cursorRight to the edge of screen"
    #[test]
    fn terminal_cursor_right_to_the_edge_of_screen() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.cursor_right(100);
        t.print('X' as u32).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "    X".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: cursorRight left of right margin"
    #[test]
    fn terminal_cursor_right_left_of_right_margin() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.scrolling_region.right = 2;
        t.cursor_right(100);
        t.print('X' as u32).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "  X".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: cursorRight right of right margin"
    #[test]
    fn terminal_cursor_right_right_of_right_margin() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.scrolling_region.right = 2;
        t.set_cursor_pos(1, 4);
        t.cursor_right(100);
        t.print('X' as u32).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "    X".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: saveCursor position"
    #[test]
    fn terminal_save_cursor_position() {
        let mut t = Terminal::init(Global, opts(10, 5)).unwrap();

        t.set_cursor_pos(1, 5);
        t.print('A' as u32).unwrap();
        t.save_cursor();
        t.set_cursor_pos(1, 1);
        t.print('B' as u32).unwrap();
        t.restore_cursor();
        t.print('X' as u32).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "B   AX".as_bytes());
        }

        t.deinit(Global);
    }

    // PORT NOTE (vt-12b-2 deferral): test "Terminal: saveCursor pending wrap
    // state" restores a pending-wrap cursor then prints, which soft-wraps via
    // `print_wrap()` → `index()` (still `todo!()`); deferred to vt-12b-3b.

    // === vt-12b-3a: erase methods ===========================================
    //
    // PORT NOTE (vt-12b-3a deferral): erase tests that drive still-`todo!()`
    // methods are deferred to vt-12b-3b. Specifically deferred are any test
    // that:
    //   - soft-wraps (multi-row via `print_wrap()` → `index()`), or uses
    //     `linefeed()`/`\n` in `print_string` (also `index()`),
    //   - calls `set_attribute` (SGR; the "preserves background sgr",
    //     "handles refcounted styles", "preserves color", "preserves cursor"
    //     groups),
    //   - calls `set_protected_mode` (the protected-attribute groups),
    //   - calls `set_top_and_bottom_margin` / `set_left_and_right_margin` /
    //     `scroll_down` (the decaln + scroll-region + right-margin groups).
    // The `decaln`, `erase_display` (all modes but `Scrollback`/single-row),
    // and protected `erase_line`/`erase_chars` test groups land in vt-12b-3b
    // once those methods are filled.

    // test "Terminal: eraseChars simple operation"
    #[test]
    fn terminal_erase_chars_simple_operation() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        for c in "ABC".chars() {
            t.print(c as u32).unwrap();
        }
        t.set_cursor_pos(1, 1);
        t.clear_dirty();
        t.erase_chars(2);
        t.print('X' as u32).unwrap();

        assert!(t.is_dirty(active_pt(0, 0)));
        assert!(!t.is_dirty(active_pt(0, 1)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "X C".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: eraseChars minimum one"
    #[test]
    fn terminal_erase_chars_minimum_one() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        for c in "ABC".chars() {
            t.print(c as u32).unwrap();
        }
        t.set_cursor_pos(1, 1);
        t.clear_dirty();
        t.erase_chars(0);
        t.print('X' as u32).unwrap();
        assert!(t.is_dirty(active_pt(0, 0)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "XBC".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: eraseChars beyond screen edge"
    #[test]
    fn terminal_erase_chars_beyond_screen_edge() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        for c in "  ABC".chars() {
            t.print(c as u32).unwrap();
        }
        t.set_cursor_pos(1, 4);
        t.erase_chars(10);

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "  A".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: eraseChars wide character"
    #[test]
    fn terminal_erase_chars_wide_character() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.print('橋' as u32).unwrap();
        for c in "BC".chars() {
            t.print(c as u32).unwrap();
        }
        t.set_cursor_pos(1, 1);
        t.erase_chars(1);
        t.print('X' as u32).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "X BC".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: eraseChars resets pending wrap"
    #[test]
    fn terminal_erase_chars_resets_pending_wrap() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        for c in "ABCDE".chars() {
            t.print(c as u32).unwrap();
        }
        assert!(unsafe { (*t.screens.active).cursor.pending_wrap });
        t.erase_chars(1);
        assert!(!unsafe { (*t.screens.active).cursor.pending_wrap });
        t.print('X' as u32).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "ABCDX".as_bytes());
        }

        t.deinit(Global);
    }

    // PORT NOTE (vt-12b-3a deferral): test "Terminal: eraseChars resets wrap"
    // prints "ABCDE123" on a 5-col screen, which soft-wraps via `print_wrap()`
    // → `index()` (still `todo!()`); deferred to vt-12b-3b.

    // PORT NOTE (vt-12b-3a deferral): tests "Terminal: eraseChars preserves
    // background sgr", "eraseChars handles refcounted styles", and the
    // "eraseChars protected attributes *" group drive `set_attribute` /
    // `set_protected_mode` (still `todo!()`); deferred to vt-12b-3b.

    // test "Terminal: eraseChars wide char boundary conditions"
    #[test]
    fn terminal_erase_chars_wide_char_boundary_conditions() {
        let mut t = Terminal::init(Global, opts(8, 1)).unwrap();

        t.print_string("😀a😀b😀".as_bytes()).unwrap();
        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "😀a😀b😀".as_bytes());
        }

        t.set_cursor_pos(1, 2);
        t.erase_chars(3);
        unsafe {
            (*core::ptr::addr_of_mut!((*(*(*t.screens.active).cursor.page_pin).node).data))
                .assert_integrity()
        };

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "     b😀".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: eraseChars wide char splits proper cell boundaries"
    #[test]
    fn terminal_erase_chars_wide_char_splits_proper_cell_boundaries() {
        let mut t = Terminal::init(Global, opts(30, 1)).unwrap();

        // This is a test for a bug: https://github.com/ghostty-org/ghostty/issues/2817
        // To explain the setup:
        // (1) We need our wide characters starting on an even (1-based) column.
        // (2) We need our cursor to be in the middle somewhere.
        // (3) We need our count to be less than our cursor X and on a split cell.
        // The bug was that we split the wrong cell boundaries.

        t.print_string("x食べて下さい".as_bytes()).unwrap();
        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "x食べて下さい".as_bytes());
        }

        t.set_cursor_pos(1, 6); // At: て
        t.erase_chars(4); // Delete: て下
        unsafe {
            (*core::ptr::addr_of_mut!((*(*(*t.screens.active).cursor.page_pin).node).data))
                .assert_integrity()
        };

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "x食べ    さい".as_bytes());
        }

        t.deinit(Global);
    }

    // PORT NOTE (vt-12b-3a deferral): test "Terminal: eraseChars wide char wrap
    // boundary conditions" soft-wraps via `print_string` → `print_wrap()` →
    // `index()` (still `todo!()`); deferred to vt-12b-3b.

    // test "Terminal: deleteChars"
    #[test]
    fn terminal_delete_chars() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        for c in "ABCDE".chars() {
            t.print(c as u32).unwrap();
        }
        t.set_cursor_pos(1, 2);

        t.clear_dirty();
        t.delete_chars(2);
        assert!(t.is_dirty(active_pt(0, 0)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "ADE".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: deleteChars zero count"
    #[test]
    fn terminal_delete_chars_zero_count() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        for c in "ABCDE".chars() {
            t.print(c as u32).unwrap();
        }
        t.set_cursor_pos(1, 2);

        t.clear_dirty();
        t.delete_chars(0);
        assert!(!t.is_dirty(active_pt(0, 0)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "ABCDE".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: deleteChars more than half"
    #[test]
    fn terminal_delete_chars_more_than_half() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        for c in "ABCDE".chars() {
            t.print(c as u32).unwrap();
        }
        t.set_cursor_pos(1, 2);

        t.clear_dirty();
        t.delete_chars(3);
        assert!(t.is_dirty(active_pt(0, 0)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "AE".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: deleteChars more than line width"
    #[test]
    fn terminal_delete_chars_more_than_line_width() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        for c in "ABCDE".chars() {
            t.print(c as u32).unwrap();
        }
        t.set_cursor_pos(1, 2);

        t.clear_dirty();
        t.delete_chars(10);
        assert!(t.is_dirty(active_pt(0, 0)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "A".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: deleteChars should shift left"
    #[test]
    fn terminal_delete_chars_should_shift_left() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        for c in "ABCDE".chars() {
            t.print(c as u32).unwrap();
        }
        t.set_cursor_pos(1, 2);

        t.clear_dirty();
        t.delete_chars(1);
        assert!(t.is_dirty(active_pt(0, 0)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "ACDE".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: deleteChars resets pending wrap"
    #[test]
    fn terminal_delete_chars_resets_pending_wrap() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        for c in "ABCDE".chars() {
            t.print(c as u32).unwrap();
        }
        assert!(unsafe { (*t.screens.active).cursor.pending_wrap });
        t.delete_chars(1);
        assert!(!unsafe { (*t.screens.active).cursor.pending_wrap });
        t.print('X' as u32).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "ABCDX".as_bytes());
        }

        t.deinit(Global);
    }

    // PORT NOTE (vt-12b-3a deferral): test "Terminal: deleteChars resets wrap"
    // prints "ABCDE123" on a 5-col screen, soft-wrapping via `print_wrap()` →
    // `index()` (still `todo!()`); deferred to vt-12b-3b.

    // test "Terminal: deleteChars simple operation"
    #[test]
    fn terminal_delete_chars_simple_operation() {
        let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

        t.print_string("ABC123".as_bytes()).unwrap();
        t.set_cursor_pos(1, 3);

        t.clear_dirty();
        t.delete_chars(2);
        assert!(t.is_dirty(active_pt(0, 0)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "AB23".as_bytes());
        }

        t.deinit(Global);
    }

    // PORT NOTE (vt-12b-3a deferral): test "Terminal: deleteChars preserves
    // background sgr" drives `set_attribute` (still `todo!()`); deferred to
    // vt-12b-3b.

    // test "Terminal: deleteChars outside scroll region"
    #[test]
    fn terminal_delete_chars_outside_scroll_region() {
        let mut t = Terminal::init(Global, opts(6, 10)).unwrap();

        t.print_string("ABC123".as_bytes()).unwrap();
        t.scrolling_region.left = 2;
        t.scrolling_region.right = 4;
        assert!(unsafe { (*t.screens.active).cursor.pending_wrap });
        t.clear_dirty();
        t.delete_chars(2);
        assert!(!t.is_dirty(active_pt(0, 0)));
        assert!(unsafe { (*t.screens.active).cursor.pending_wrap });

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "ABC123".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: deleteChars inside scroll region"
    #[test]
    fn terminal_delete_chars_inside_scroll_region() {
        let mut t = Terminal::init(Global, opts(6, 10)).unwrap();

        t.print_string("ABC123".as_bytes()).unwrap();
        t.scrolling_region.left = 2;
        t.scrolling_region.right = 4;
        t.set_cursor_pos(1, 4);

        t.clear_dirty();
        t.delete_chars(1);
        assert!(t.is_dirty(active_pt(0, 0)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "ABC2 3".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: deleteChars split wide character from spacer tail"
    #[test]
    fn terminal_delete_chars_split_wide_character_from_spacer_tail() {
        let mut t = Terminal::init(Global, opts(6, 10)).unwrap();

        t.print_string("A橋123".as_bytes()).unwrap();
        t.set_cursor_pos(1, 3);
        t.delete_chars(1);

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "A 123".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: deleteChars split wide character from wide"
    #[test]
    fn terminal_delete_chars_split_wide_character_from_wide() {
        let mut t = Terminal::init(Global, opts(6, 10)).unwrap();

        t.print_string("橋123".as_bytes()).unwrap();
        t.set_cursor_pos(1, 1);
        t.delete_chars(1);

        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 0)).unwrap();
            let cell = list_cell.cell;
            assert_eq!(unsafe { (*cell).content_codepoint() }, 0);
            assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Narrow);
        }
        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(1, 0)).unwrap();
            let cell = list_cell.cell;
            assert_eq!(unsafe { (*cell).content_codepoint() }, '1' as u21);
            assert_eq!(unsafe { (*cell).wide() }, pagepkg::Wide::Narrow);
        }

        t.deinit(Global);
    }

    // test "Terminal: deleteChars split wide character from end"
    #[test]
    fn terminal_delete_chars_split_wide_character_from_end() {
        let mut t = Terminal::init(Global, opts(6, 10)).unwrap();

        t.print_string("A橋123".as_bytes()).unwrap();
        t.set_cursor_pos(1, 1);
        t.delete_chars(1);

        {
            let list_cell = t.active_mut().pages.get_cell(screen_pt(0, 0)).unwrap();
            let cell = list_cell.cell;
            assert_eq!(unsafe { (*cell).content_codepoint() }, 0x6A4B);
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

    // PORT NOTE (vt-12b-3a deferral): test "Terminal: deleteChars with a spacer
    // head at the end" prints "0123橋123" on a 5-col screen, soft-wrapping the
    // wide char (spacer head) via `print_wrap()` → `index()` (still `todo!()`);
    // deferred to vt-12b-3b.

    // test "Terminal: deleteChars split wide character tail"
    #[test]
    fn terminal_delete_chars_split_wide_character_tail() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.set_cursor_pos(1, (t.cols - 1) as usize);
        t.print(0x6A4B).unwrap(); // 橋
        t.carriage_return();
        t.delete_chars((t.cols - 1) as usize);
        t.print('0' as u32).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "0".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: deleteChars wide char boundary conditions"
    #[test]
    fn terminal_delete_chars_wide_char_boundary_conditions() {
        let mut t = Terminal::init(Global, opts(8, 1)).unwrap();

        // EXPLANATION(qwerasd):
        //
        // There are 3 or 4 boundaries to be concerned with in deleteChars,
        // depending on how you count them. Consider the following terminal:
        //
        //   +--------+
        // 0 |.ABCDEF.|
        //   : ^      : (^ = cursor)
        //   +--------+
        //
        // if we DCH 3 we get
        //
        //   +--------+
        // 0 |.DEF....|
        //   +--------+
        //
        // The boundaries exist at the following points then:
        //
        //   +--------+
        // 0 |.ABCDEF.|
        //   :11 22 33:
        //   +--------+
        //
        // I'm counting 2 for double since it's both the end of the deleted
        // content and the start of the content that is shifted in to place.
        //
        // Now consider wide characters (represented as `WW`) at these boundaries:
        //
        //   +--------+
        // 0 |WWaWWbWW|
        //   : ^      : (^ = cursor)
        //   : ^^^    : (^ = deleted by DCH 3)
        //   +--------+
        //
        // -> DCH 3
        // -> The first 2 wide characters are split & destroyed (verified in xterm)
        //
        //   +--------+
        // 0 |..bWW...|
        //   +--------+

        t.print_string("😀a😀b😀".as_bytes()).unwrap();
        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "😀a😀b😀".as_bytes());
        }

        t.set_cursor_pos(1, 2);
        t.delete_chars(3);
        unsafe {
            (*core::ptr::addr_of_mut!((*(*(*t.screens.active).cursor.page_pin).node).data))
                .assert_integrity()
        };

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "  b😀".as_bytes());
        }

        t.deinit(Global);
    }

    // PORT NOTE (vt-12b-3a deferral): test "Terminal: deleteChars wide char wrap
    // boundary conditions" soft-wraps via `print_string` → `print_wrap()` →
    // `index()`, and "deleteChars wide char across right margin" drives
    // `set_left_and_right_margin` (both still `todo!()`); deferred to vt-12b-3b.

    // test "Terminal: eraseLine simple erase right"
    #[test]
    fn terminal_erase_line_simple_erase_right() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        for c in "ABCDE".chars() {
            t.print(c as u32).unwrap();
        }
        t.set_cursor_pos(1, 3);
        t.clear_dirty();
        t.erase_line(csi::EraseLine::Right, false);
        assert!(t.is_dirty(active_pt(0, 0)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "AB".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: eraseLine resets pending wrap"
    #[test]
    fn terminal_erase_line_resets_pending_wrap() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        for c in "ABCDE".chars() {
            t.print(c as u32).unwrap();
        }
        assert!(unsafe { (*t.screens.active).cursor.pending_wrap });
        t.erase_line(csi::EraseLine::Right, false);
        assert!(!unsafe { (*t.screens.active).cursor.pending_wrap });
        t.print('B' as u32).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "ABCDB".as_bytes());
        }

        t.deinit(Global);
    }

    // PORT NOTE (vt-12b-3a deferral): test "Terminal: eraseLine resets wrap"
    // prints "ABCDE123" on a 5-col screen, soft-wrapping via `print_wrap()` →
    // `index()` (still `todo!()`); deferred to vt-12b-3b.

    // PORT NOTE (vt-12b-3a deferral): tests "Terminal: eraseLine right preserves
    // background sgr" and the "eraseLine right protected *" group drive
    // `set_attribute` / `set_protected_mode` (still `todo!()`); deferred to
    // vt-12b-3b.

    // test "Terminal: eraseLine right wide character"
    #[test]
    fn terminal_erase_line_right_wide_character() {
        let mut t = Terminal::init(Global, opts(10, 5)).unwrap();

        for c in "AB".chars() {
            t.print(c as u32).unwrap();
        }
        t.print('橋' as u32).unwrap();
        for c in "DE".chars() {
            t.print(c as u32).unwrap();
        }
        t.set_cursor_pos(1, 4);
        t.clear_dirty();
        t.erase_line(csi::EraseLine::Right, false);
        assert!(t.is_dirty(active_pt(0, 0)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "AB".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: eraseLine simple erase left"
    #[test]
    fn terminal_erase_line_simple_erase_left() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        for c in "ABCDE".chars() {
            t.print(c as u32).unwrap();
        }
        t.set_cursor_pos(1, 3);
        t.clear_dirty();
        t.erase_line(csi::EraseLine::Left, false);
        assert!(t.is_dirty(active_pt(0, 0)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "   DE".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: eraseLine left resets wrap"
    #[test]
    fn terminal_erase_line_left_resets_wrap() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        for c in "ABCDE".chars() {
            t.print(c as u32).unwrap();
        }
        assert!(unsafe { (*t.screens.active).cursor.pending_wrap });
        t.clear_dirty();
        t.erase_line(csi::EraseLine::Left, false);
        assert!(t.is_dirty(active_pt(0, 0)));
        assert!(!unsafe { (*t.screens.active).cursor.pending_wrap });
        t.print('B' as u32).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "    B".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: eraseLine left wide character"
    #[test]
    fn terminal_erase_line_left_wide_character() {
        let mut t = Terminal::init(Global, opts(10, 5)).unwrap();

        for c in "AB".chars() {
            t.print(c as u32).unwrap();
        }
        t.print('橋' as u32).unwrap();
        for c in "DE".chars() {
            t.print(c as u32).unwrap();
        }
        t.set_cursor_pos(1, 3);
        t.clear_dirty();
        t.erase_line(csi::EraseLine::Left, false);
        assert!(t.is_dirty(active_pt(0, 0)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "    DE".as_bytes());
        }

        t.deinit(Global);
    }

    // PORT NOTE (vt-12b-3a deferral): the "Terminal: eraseLine left protected
    // *", "eraseLine complete *", "eraseLine left preserves background sgr"
    // groups drive `set_attribute` / `set_protected_mode` (still `todo!()`);
    // deferred to vt-12b-3b.

    // PORT NOTE (vt-12b-3a deferral): the "Terminal: eraseDisplay *" tests that
    // drive `set_attribute` / `set_protected_mode` (SGR/protected variants)
    // remain deferred — those methods are still `todo!()`. The non-SGR
    // eraseDisplay tests (`simple erase below/above`, `below/above split
    // multi-cell`, `scroll complete`) are now portable (`index()`/`linefeed()`
    // exist) but are tracked with the broader eraseDisplay group and ported in
    // a follow-up to keep this batch focused on scroll/lines/margins.

    // ===================================================================
    // vt-12b-3b: scroll / lines / margins tests (+ deferred linefeed tests)
    // ===================================================================

    // test "Terminal: linefeed and carriage return"
    #[test]
    fn terminal_linefeed_and_carriage_return() {
        let mut t = Terminal::init(Global, opts(80, 80)).unwrap();

        // Print and CR.
        for c in "hello".chars() {
            t.print(c as u32).unwrap();
        }
        t.clear_dirty();
        t.carriage_return();

        // CR should not mark row dirty because it doesn't change rendering.
        assert!(!t.is_dirty(screen_pt(0, 0)));

        t.linefeed().unwrap();

        // LF marks row dirty due to cursor movement
        assert!(t.is_dirty(screen_pt(0, 0)));
        assert!(t.is_dirty(screen_pt(0, 1)));

        for c in "world".chars() {
            t.print(c as u32).unwrap();
        }
        assert_eq!(unsafe { (*t.screens.active).cursor.y }, 1);
        assert_eq!(unsafe { (*t.screens.active).cursor.x }, 5);
        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "hello\nworld".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: linefeed unsets pending wrap"
    #[test]
    fn terminal_linefeed_unsets_pending_wrap() {
        let mut t = Terminal::init(Global, opts(5, 80)).unwrap();

        // Basic grid writing
        for c in "hello".chars() {
            t.print(c as u32).unwrap();
        }
        assert!(unsafe { (*t.screens.active).cursor.pending_wrap });
        t.clear_dirty();
        t.linefeed().unwrap();
        assert!(t.is_dirty(screen_pt(0, 0)));
        assert!(t.is_dirty(screen_pt(0, 1)));
        assert!(!unsafe { (*t.screens.active).cursor.pending_wrap });

        t.deinit(Global);
    }

    // test "Terminal: linefeed mode automatic carriage return"
    #[test]
    fn terminal_linefeed_mode_automatic_carriage_return() {
        let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

        // Basic grid writing
        t.modes.set(modespkg::Mode::Linefeed, true);
        t.print_string("123456".as_bytes()).unwrap();
        t.linefeed().unwrap();
        t.print('X' as u32).unwrap();
        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "123456\nX".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: setTopAndBottomMargin simple"
    #[test]
    fn terminal_set_top_and_bottom_margin_simple() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.print_string("ABC".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("DEF".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("GHI".as_bytes()).unwrap();
        t.set_top_and_bottom_margin(0, 0);

        t.clear_dirty();
        t.scroll_down(1);

        // Mark the rows we moved as dirty.
        assert!(t.is_dirty(active_pt(0, 0)));
        assert!(t.is_dirty(active_pt(0, 1)));
        assert!(t.is_dirty(active_pt(0, 2)));
        assert!(t.is_dirty(active_pt(0, 3)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "\nABC\nDEF\nGHI".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: setTopAndBottomMargin top only"
    #[test]
    fn terminal_set_top_and_bottom_margin_top_only() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.print_string("ABC".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("DEF".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("GHI".as_bytes()).unwrap();
        t.set_top_and_bottom_margin(2, 0);

        t.clear_dirty();
        t.scroll_down(1);

        // This is dirty because the cursor moves from this row
        assert!(t.is_dirty(active_pt(0, 0)));
        assert!(t.is_dirty(active_pt(0, 1)));
        assert!(t.is_dirty(active_pt(0, 2)));
        assert!(t.is_dirty(active_pt(0, 3)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "ABC\n\nDEF\nGHI".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: setTopAndBottomMargin top and bottom"
    #[test]
    fn terminal_set_top_and_bottom_margin_top_and_bottom() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.print_string("ABC".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("DEF".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("GHI".as_bytes()).unwrap();
        t.set_top_and_bottom_margin(1, 2);

        t.clear_dirty();
        t.scroll_down(1);

        assert!(t.is_dirty(active_pt(0, 0)));
        assert!(t.is_dirty(active_pt(0, 1)));
        assert!(!t.is_dirty(active_pt(0, 2)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "\nABC\nGHI".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: setTopAndBottomMargin top equal to bottom"
    #[test]
    fn terminal_set_top_and_bottom_margin_top_equal_to_bottom() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.print_string("ABC".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("DEF".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("GHI".as_bytes()).unwrap();
        t.set_top_and_bottom_margin(2, 2);

        t.clear_dirty();
        t.scroll_down(1);

        assert!(t.is_dirty(active_pt(0, 0)));
        assert!(t.is_dirty(active_pt(0, 1)));
        assert!(t.is_dirty(active_pt(0, 2)));
        assert!(t.is_dirty(active_pt(0, 3)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "\nABC\nDEF\nGHI".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: setLeftAndRightMargin simple"
    #[test]
    fn terminal_set_left_and_right_margin_simple() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.print_string("ABC".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("DEF".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("GHI".as_bytes()).unwrap();
        t.modes.set(modespkg::Mode::EnableLeftAndRightMargin, true);
        t.set_left_and_right_margin(0, 0);

        t.clear_dirty();
        t.erase_chars(1);

        assert!(t.is_dirty(active_pt(0, 0)));
        assert!(!t.is_dirty(active_pt(0, 1)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), " BC\nDEF\nGHI".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: setLeftAndRightMargin left only"
    #[test]
    fn terminal_set_left_and_right_margin_left_only() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.print_string("ABC".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("DEF".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("GHI".as_bytes()).unwrap();
        t.modes.set(modespkg::Mode::EnableLeftAndRightMargin, true);
        t.set_left_and_right_margin(2, 0);
        assert_eq!(t.scrolling_region.left as usize, 1);
        assert_eq!(t.scrolling_region.right as usize, (t.cols - 1) as usize);
        t.set_cursor_pos(1, 2);

        t.clear_dirty();
        t.insert_lines(1);

        assert!(t.is_dirty(active_pt(0, 0)));
        assert!(t.is_dirty(active_pt(0, 1)));
        assert!(t.is_dirty(active_pt(0, 2)));
        assert!(t.is_dirty(active_pt(0, 3)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "A\nDBC\nGEF\n HI".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: setLeftAndRightMargin left and right"
    #[test]
    fn terminal_set_left_and_right_margin_left_and_right() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.print_string("ABC".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("DEF".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("GHI".as_bytes()).unwrap();
        t.modes.set(modespkg::Mode::EnableLeftAndRightMargin, true);
        t.set_left_and_right_margin(1, 2);
        t.set_cursor_pos(1, 2);

        t.clear_dirty();
        t.insert_lines(1);

        assert!(t.is_dirty(active_pt(0, 0)));
        assert!(t.is_dirty(active_pt(0, 1)));
        assert!(t.is_dirty(active_pt(0, 2)));
        assert!(t.is_dirty(active_pt(0, 3)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "  C\nABF\nDEI\nGH".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: setLeftAndRightMargin left equal right"
    #[test]
    fn terminal_set_left_and_right_margin_left_equal_right() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.print_string("ABC".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("DEF".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("GHI".as_bytes()).unwrap();
        t.modes.set(modespkg::Mode::EnableLeftAndRightMargin, true);
        t.set_left_and_right_margin(2, 2);
        t.set_cursor_pos(1, 2);

        t.clear_dirty();
        t.insert_lines(1);

        assert!(t.is_dirty(active_pt(0, 0)));
        assert!(t.is_dirty(active_pt(0, 1)));
        assert!(t.is_dirty(active_pt(0, 2)));
        assert!(t.is_dirty(active_pt(0, 3)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "\nABC\nDEF\nGHI".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: setLeftAndRightMargin mode 69 unset"
    #[test]
    fn terminal_set_left_and_right_margin_mode_69_unset() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.print_string("ABC".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("DEF".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("GHI".as_bytes()).unwrap();
        t.modes.set(modespkg::Mode::EnableLeftAndRightMargin, false);
        t.set_left_and_right_margin(1, 2);
        t.set_cursor_pos(1, 2);

        t.clear_dirty();
        t.insert_lines(1);

        assert!(t.is_dirty(active_pt(0, 0)));
        assert!(t.is_dirty(active_pt(0, 1)));
        assert!(t.is_dirty(active_pt(0, 2)));
        assert!(t.is_dirty(active_pt(0, 3)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "\nABC\nDEF\nGHI".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: insertLines simple"
    #[test]
    fn terminal_insert_lines_simple() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.print_string("ABC".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("DEF".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("GHI".as_bytes()).unwrap();
        t.set_cursor_pos(2, 2);

        t.clear_dirty();
        t.insert_lines(1);

        assert!(!t.is_dirty(active_pt(0, 0)));
        assert!(t.is_dirty(active_pt(0, 1)));
        assert!(t.is_dirty(active_pt(0, 2)));
        assert!(t.is_dirty(active_pt(0, 3)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "ABC\n\nDEF\nGHI".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: insertLines outside of scroll region"
    #[test]
    fn terminal_insert_lines_outside_of_scroll_region() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.print_string("ABC".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("DEF".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("GHI".as_bytes()).unwrap();
        t.set_top_and_bottom_margin(3, 4);
        t.set_cursor_pos(2, 2);

        t.clear_dirty();
        t.insert_lines(1);

        assert!(!t.is_dirty(active_pt(0, 0)));
        assert!(!t.is_dirty(active_pt(0, 1)));
        assert!(!t.is_dirty(active_pt(0, 2)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "ABC\nDEF\nGHI".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: insertLines top/bottom scroll region"
    #[test]
    fn terminal_insert_lines_top_bottom_scroll_region() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.print_string("ABC".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("DEF".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("GHI".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("123".as_bytes()).unwrap();
        t.set_top_and_bottom_margin(1, 3);
        t.set_cursor_pos(2, 2);

        t.clear_dirty();
        t.insert_lines(1);

        assert!(!t.is_dirty(active_pt(0, 0)));
        assert!(t.is_dirty(active_pt(0, 1)));
        assert!(t.is_dirty(active_pt(0, 2)));
        assert!(!t.is_dirty(active_pt(0, 3)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "ABC\n\nDEF\n123".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: insertLines zero"
    #[test]
    fn terminal_insert_lines_zero() {
        let mut t = Terminal::init(Global, opts(2, 5)).unwrap();

        // This should do nothing
        t.set_cursor_pos(1, 1);
        t.insert_lines(0);

        t.deinit(Global);
    }

    // test "Terminal: insertLines with scroll region"
    #[test]
    fn terminal_insert_lines_with_scroll_region() {
        let mut t = Terminal::init(Global, opts(2, 6)).unwrap();

        // Initial value
        t.print('A' as u32).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print('B' as u32).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print('C' as u32).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print('D' as u32).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print('E' as u32).unwrap();

        t.set_top_and_bottom_margin(1, 2);
        t.set_cursor_pos(1, 1);

        t.clear_dirty();
        t.insert_lines(1);

        assert!(t.is_dirty(active_pt(0, 0)));
        assert!(t.is_dirty(active_pt(0, 1)));
        assert!(!t.is_dirty(active_pt(0, 2)));

        t.print('X' as u32).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "X\nA\nC\nD\nE".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: insertLines more than remaining"
    #[test]
    fn terminal_insert_lines_more_than_remaining() {
        let mut t = Terminal::init(Global, opts(2, 5)).unwrap();

        // Initial value
        t.print('A' as u32).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print('B' as u32).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print('C' as u32).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print('D' as u32).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print('E' as u32).unwrap();

        // Move to row 2
        t.set_cursor_pos(2, 1);

        // Insert a bunch of  lines
        t.clear_dirty();
        t.insert_lines(20);

        assert!(!t.is_dirty(active_pt(0, 0)));
        assert!(t.is_dirty(active_pt(0, 1)));
        assert!(t.is_dirty(active_pt(0, 2)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "A".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: insertLines resets pending wrap"
    #[test]
    fn terminal_insert_lines_resets_pending_wrap() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        for c in "ABCDE".chars() {
            t.print(c as u32).unwrap();
        }
        assert!(unsafe { (*t.screens.active).cursor.pending_wrap });
        t.insert_lines(1);
        assert!(!unsafe { (*t.screens.active).cursor.pending_wrap });
        t.print('B' as u32).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "B\nABCDE".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: insertLines resets wrap"
    #[test]
    fn terminal_insert_lines_resets_wrap() {
        let mut t = Terminal::init(Global, opts(3, 3)).unwrap();

        t.print('1' as u32).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        for c in "ABCDEF".chars() {
            t.print(c as u32).unwrap();
        }
        t.set_cursor_pos(1, 1);
        t.insert_lines(1);
        t.print('X' as u32).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "X\n1\nABC".as_bytes());
        }

        {
            let list_cell = unsafe { (*t.screens.active).pages.get_cell(active_pt(0, 2)) }.unwrap();
            let row = list_cell.row;
            assert!(!unsafe { (*row).wrap() });
        }

        t.deinit(Global);
    }

    // test "Terminal: insertLines left/right scroll region"
    #[test]
    fn terminal_insert_lines_left_right_scroll_region() {
        let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

        t.print_string("ABC123".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("DEF456".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("GHI789".as_bytes()).unwrap();
        t.scrolling_region.left = 1;
        t.scrolling_region.right = 3;
        t.set_cursor_pos(2, 2);

        t.clear_dirty();
        t.insert_lines(1);

        assert!(!t.is_dirty(active_pt(0, 0)));
        assert!(t.is_dirty(active_pt(0, 1)));
        assert!(t.is_dirty(active_pt(0, 2)));
        assert!(t.is_dirty(active_pt(0, 3)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "ABC123\nD   56\nGEF489\n HI7".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: scrollUp simple"
    #[test]
    fn terminal_scroll_up_simple() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.print_string("ABC".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("DEF".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("GHI".as_bytes()).unwrap();
        t.set_cursor_pos(2, 2);

        let cursor_x = unsafe { (*t.screens.active).cursor.x };
        let cursor_y = unsafe { (*t.screens.active).cursor.y };
        let viewport_before = t.active_mut().pages.get_top_left(point::Tag::Viewport);
        t.scroll_up(1).unwrap();
        assert_eq!(cursor_x, unsafe { (*t.screens.active).cursor.x });
        assert_eq!(cursor_y, unsafe { (*t.screens.active).cursor.y });

        // Viewport should have moved. Our entire page should've scrolled!
        // The viewport moving will cause our render state to make the full
        // frame as dirty.
        let viewport_after = t.active_mut().pages.get_top_left(point::Tag::Viewport);
        assert!(!viewport_before.eql(viewport_after));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "DEF\nGHI".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: scrollDown simple"
    #[test]
    fn terminal_scroll_down_simple() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.print_string("ABC".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("DEF".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("GHI".as_bytes()).unwrap();
        t.set_cursor_pos(2, 2);

        let cursor_x = unsafe { (*t.screens.active).cursor.x };
        let cursor_y = unsafe { (*t.screens.active).cursor.y };
        t.clear_dirty();
        t.scroll_down(1);
        assert_eq!(cursor_x, unsafe { (*t.screens.active).cursor.x });
        assert_eq!(cursor_y, unsafe { (*t.screens.active).cursor.y });

        for y in 0..5u32 {
            assert!(t.is_dirty(active_pt(0, y)));
        }

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "\nABC\nDEF\nGHI".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: reverseIndex"
    #[test]
    fn terminal_reverse_index() {
        let mut t = Terminal::init(Global, opts(2, 5)).unwrap();

        // Initial value
        t.print('A' as u32).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print('B' as u32).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print('C' as u32).unwrap();
        t.reverse_index();
        t.print('D' as u32).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.carriage_return();
        t.linefeed().unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "A\nBD\nC".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: index"
    #[test]
    fn terminal_index() {
        let mut t = Terminal::init(Global, opts(2, 5)).unwrap();

        t.index().unwrap();
        t.print('A' as u32).unwrap();

        assert!(t.is_dirty(active_pt(0, 0)));
        assert!(t.is_dirty(active_pt(0, 1)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "\nA".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: index from the bottom"
    #[test]
    fn terminal_index_from_the_bottom() {
        let mut t = Terminal::init(Global, opts(2, 5)).unwrap();

        t.set_cursor_pos(5, 1);
        t.print('A' as u32).unwrap();
        t.cursor_left(1); // undo moving right from 'A'

        t.clear_dirty();
        t.index().unwrap();
        t.print('B' as u32).unwrap();

        assert!(t.is_dirty(active_pt(0, 3)));
        assert!(t.is_dirty(active_pt(0, 4)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "\n\n\nA\nB".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: index outside of scrolling region"
    #[test]
    fn terminal_index_outside_of_scrolling_region() {
        let mut t = Terminal::init(Global, opts(2, 5)).unwrap();

        assert_eq!(unsafe { (*t.screens.active).cursor.y }, 0);
        t.set_top_and_bottom_margin(2, 5);
        t.index().unwrap();
        assert_eq!(unsafe { (*t.screens.active).cursor.y }, 1);

        t.deinit(Global);
    }

    // test "Terminal: index from the bottom outside of scroll region"
    #[test]
    fn terminal_index_from_the_bottom_outside_of_scroll_region() {
        let mut t = Terminal::init(Global, opts(2, 5)).unwrap();

        t.set_top_and_bottom_margin(1, 2);
        t.set_cursor_pos(5, 1);
        t.print('A' as u32).unwrap();
        t.clear_dirty();
        t.index().unwrap();
        t.print('B' as u32).unwrap();
        assert!(t.is_dirty(active_pt(0, 4)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "\n\n\n\nAB".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: index no scroll region, top of screen"
    #[test]
    fn terminal_index_no_scroll_region_top_of_screen() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.print('A' as u32).unwrap();
        t.clear_dirty();
        t.index().unwrap();
        t.print('X' as u32).unwrap();

        assert!(t.is_dirty(active_pt(0, 0)));
        assert!(t.is_dirty(active_pt(0, 1)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "A\n X".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: deleteLines simple"
    #[test]
    fn terminal_delete_lines_simple() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();

        t.print_string("ABC".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("DEF".as_bytes()).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print_string("GHI".as_bytes()).unwrap();
        t.set_cursor_pos(2, 2);

        t.clear_dirty();
        t.delete_lines(1);

        assert!(!t.is_dirty(active_pt(0, 0)));
        assert!(t.is_dirty(active_pt(0, 1)));
        assert!(t.is_dirty(active_pt(0, 2)));
        assert!(t.is_dirty(active_pt(0, 3)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "ABC\nGHI".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: deleteLines with scroll region"
    #[test]
    fn terminal_delete_lines_with_scroll_region() {
        let mut t = Terminal::init(Global, opts(80, 80)).unwrap();

        // Initial value
        t.print('A' as u32).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print('B' as u32).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print('C' as u32).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print('D' as u32).unwrap();

        t.set_top_and_bottom_margin(1, 3);
        t.set_cursor_pos(1, 1);

        t.clear_dirty();
        t.delete_lines(1);

        assert!(t.is_dirty(active_pt(0, 0)));
        assert!(t.is_dirty(active_pt(0, 1)));
        assert!(t.is_dirty(active_pt(0, 2)));
        assert!(!t.is_dirty(active_pt(0, 3)));

        t.print('E' as u32).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "E\nC\n\nD".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: deleteLines with scroll region, large count"
    #[test]
    fn terminal_delete_lines_with_scroll_region_large_count() {
        let mut t = Terminal::init(Global, opts(80, 80)).unwrap();

        // Initial value
        t.print('A' as u32).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print('B' as u32).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print('C' as u32).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();
        t.print('D' as u32).unwrap();

        t.set_top_and_bottom_margin(1, 3);
        t.set_cursor_pos(1, 1);

        t.clear_dirty();
        t.delete_lines(5);

        assert!(t.is_dirty(active_pt(0, 0)));
        assert!(t.is_dirty(active_pt(0, 1)));
        assert!(t.is_dirty(active_pt(0, 2)));
        assert!(!t.is_dirty(active_pt(0, 3)));

        t.print('E' as u32).unwrap();
        t.carriage_return();
        t.linefeed().unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "E\n\n\nD".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: insertBlanks zero"
    #[test]
    fn terminal_insert_blanks_zero() {
        let mut t = Terminal::init(Global, opts(5, 2)).unwrap();

        t.print('A' as u32).unwrap();
        t.print('B' as u32).unwrap();
        t.print('C' as u32).unwrap();
        t.set_cursor_pos(1, 1);

        t.insert_blanks(0);

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "ABC".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: insertBlanks"
    #[test]
    fn terminal_insert_blanks() {
        // NOTE: this is not verified with conformance tests, so these
        // tests might actually be verifying wrong behavior.
        let mut t = Terminal::init(Global, opts(5, 2)).unwrap();

        t.print('A' as u32).unwrap();
        t.print('B' as u32).unwrap();
        t.print('C' as u32).unwrap();
        t.set_cursor_pos(1, 1);

        t.clear_dirty();
        t.insert_blanks(2);
        assert!(t.is_dirty(active_pt(0, 0)));
        assert!(!t.is_dirty(active_pt(0, 1)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "  ABC".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: insertBlanks pushes off end"
    #[test]
    fn terminal_insert_blanks_pushes_off_end() {
        // NOTE: this is not verified with conformance tests, so these
        // tests might actually be verifying wrong behavior.
        let mut t = Terminal::init(Global, opts(3, 2)).unwrap();

        t.print('A' as u32).unwrap();
        t.print('B' as u32).unwrap();
        t.print('C' as u32).unwrap();
        t.set_cursor_pos(1, 1);

        t.clear_dirty();
        t.insert_blanks(2);
        assert!(t.is_dirty(active_pt(0, 0)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "  A".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: insertBlanks more than size"
    #[test]
    fn terminal_insert_blanks_more_than_size() {
        // NOTE: this is not verified with conformance tests, so these
        // tests might actually be verifying wrong behavior.
        let mut t = Terminal::init(Global, opts(3, 2)).unwrap();

        t.print('A' as u32).unwrap();
        t.print('B' as u32).unwrap();
        t.print('C' as u32).unwrap();
        t.set_cursor_pos(1, 1);

        t.clear_dirty();
        t.insert_blanks(5);
        assert!(t.is_dirty(active_pt(0, 0)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: insertBlanks no scroll region, fits"
    #[test]
    fn terminal_insert_blanks_no_scroll_region_fits() {
        let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

        for c in "ABC".chars() {
            t.print(c as u32).unwrap();
        }
        t.set_cursor_pos(1, 1);

        t.clear_dirty();
        t.insert_blanks(2);
        assert!(t.is_dirty(active_pt(0, 0)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "  ABC".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: insertBlanks shift off screen"
    #[test]
    fn terminal_insert_blanks_shift_off_screen() {
        let mut t = Terminal::init(Global, opts(5, 10)).unwrap();

        for c in "  ABC".chars() {
            t.print(c as u32).unwrap();
        }
        t.set_cursor_pos(1, 3);
        t.clear_dirty();
        t.insert_blanks(2);
        assert!(t.is_dirty(active_pt(0, 0)));
        t.print('X' as u32).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "  X A".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: insertBlanks split multi-cell character"
    #[test]
    fn terminal_insert_blanks_split_multi_cell_character() {
        let mut t = Terminal::init(Global, opts(5, 10)).unwrap();

        for c in "123".chars() {
            t.print(c as u32).unwrap();
        }
        t.print('橋' as u32).unwrap();
        t.set_cursor_pos(1, 1);
        t.clear_dirty();
        t.insert_blanks(1);
        assert!(t.is_dirty(active_pt(0, 0)));

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), " 123".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: insertBlanks inside left/right scroll region"
    #[test]
    fn terminal_insert_blanks_inside_left_right_scroll_region() {
        let mut t = Terminal::init(Global, opts(10, 10)).unwrap();

        t.scrolling_region.left = 2;
        t.scrolling_region.right = 4;
        t.set_cursor_pos(1, 3);
        for c in "ABC".chars() {
            t.print(c as u32).unwrap();
        }
        t.set_cursor_pos(1, 3);

        t.clear_dirty();
        t.insert_blanks(2);
        assert!(t.is_dirty(active_pt(0, 0)));
        t.print('X' as u32).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "  X A".as_bytes());
        }

        t.deinit(Global);
    }

    // test "Terminal: insertBlanks outside left/right scroll region"
    #[test]
    fn terminal_insert_blanks_outside_left_right_scroll_region() {
        let mut t = Terminal::init(Global, opts(6, 10)).unwrap();

        t.set_cursor_pos(1, 4);
        for c in "ABC".chars() {
            t.print(c as u32).unwrap();
        }
        t.scrolling_region.left = 2;
        t.scrolling_region.right = 4;
        assert!(unsafe { (*t.screens.active).cursor.pending_wrap });
        t.clear_dirty();
        t.insert_blanks(2);
        assert!(!t.is_dirty(active_pt(0, 0)));
        assert!(!unsafe { (*t.screens.active).cursor.pending_wrap });
        t.print('X' as u32).unwrap();

        {
            let str = t.plain_string(Global).unwrap();
            assert_eq!(str.as_slice(), "   ABX".as_bytes());
        }

        t.deinit(Global);
    }

    // PORT NOTE: NOT a port of a reference `test {}` block. This is the
    // page-pool tracking-allocator gate for this batch's page-touching methods
    // (scroll_up/scroll_down/insert_lines/delete_lines, which delegate row
    // movement and scrollback creation to `pagelist`). Under `cfg(test)` the
    // page pool's backing allocator records into the global `page_tracking`
    // tracker; this test drives multi-page scroll + insert churn and asserts
    // every page allocated during the scope is returned after `deinit` (no
    // page leak across the tracked-pin / page-split paths).
    #[test]
    fn terminal_pagepool_no_leak_scroll_insert_multi_page() {
        let scope = crate::pagelist::page_tracking::PageLeakScope::new();
        {
            let mut t = Terminal::init(Global, opts(20, 5)).unwrap();

            // Fill enough rows (with scrollback) to force several pages of
            // churn through the scroll path.
            for i in 0..400u32 {
                let line = format!("row{i:04}");
                t.print_string(line.as_bytes()).unwrap();
                t.carriage_return();
                t.linefeed().unwrap();
            }

            // scroll_up at the full-screen top creates scrollback (cursorScrollAbove)
            for _ in 0..50 {
                t.scroll_up(1).unwrap();
            }

            // scroll_down delegates to insert_lines.
            for _ in 0..20 {
                t.scroll_down(1);
            }

            // Drive insert/delete lines with a scroll region too.
            t.set_top_and_bottom_margin(1, 4);
            t.set_cursor_pos(1, 1);
            for _ in 0..30 {
                t.insert_lines(2);
                t.delete_lines(1);
            }

            // We should have produced real page churn.
            assert!(
                scope.delta_allocs() > 0,
                "expected scroll/insert churn to allocate at least one new page"
            );

            t.deinit(Global);
        }
        // After deinit every page allocated during this scope must be returned.
        scope.assert_returned_to_baseline();
    }
}
