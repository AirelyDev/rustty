//! Port of `libvaxis` `Vaxis.zig` (1411 LOC, 0 `test{}`) — the main `Vaxis` object plus
//! the render diff-and-emit engine (VAXIS_C4A_DESIGN.md). This module REPLACES the
//! Chunk-3b INHABITED `Vaxis` placeholder with the REAL `Vaxis<'a>`, preserving the
//! field/method surface `loop_.rs` names (`caps`/`state`/`screen`/`query_futex`/
//! `queries_done`/`opts` + `translate_mouse`/`device_status_report`/`wake_query_futex`),
//! so `handle_event_generic` + `winsize_callback` still compile (intrusive-ptr-and-
//! placeholder-fill discipline).
//!
//! PORT-NOTE (`Vaxis<'a>` lifetime): `Vaxis` OWNS `Screen<'a>` (whose cells borrow their
//! grapheme bytes `&'a [u8]`), so `Vaxis` inherits `'a`. `Default`/`init` produce a
//! `Vaxis<'a>` with an empty (0-length, no-borrow) buffer, so `'a` is free at
//! construction.
//!
//! PORT-NOTE (`*IoWriter` → `&mut W: std::io::Write`): Zig's `std.io.Writer` is a generic
//! writer; the faithful Rust image is a generic type param `W: std::io::Write`. This makes
//! the render engine pure over a generic sink (Miri-able, R6) AND directly testable against
//! a `Vec<u8>`. `TtyWriter` (native) infers `W = TtyWriter` at the loop_ call sites.
//!
//! PORT-NOTE (`tty.print(template, args)` → inline byte emission): Zig applies the
//! `ctlseqs.*` `{d}`/`{s}`/`{c}`/`{x:0>2}` templates at runtime via `std.fmt`. Rust cannot
//! feed a `&[u8]` const to `write!`, so the render engine emits inline: `write_all` the
//! const sequences; `write!` the numeric slots; `write_all` the raw byte-slice (`{s}`)
//! slots (graphemes/URIs are non-UTF-8 bytes — must NOT go through `{}`). Every emit
//! carries a comment naming the `ctlseqs` template it mirrors.
//!
//! PORT-NOTE (`log`/`catch` dropped): `log = std.log.scoped(.vaxis)` and every `log.*`
//! call is dropped (crate precedent — see `internal_screen.rs`/`tty.rs` PORT-NOTEs).

use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use crate::cell::{Color, CursorShape, Hyperlink, Kind, Scale, Style};
use crate::ctlseqs;
use crate::gwidth::{self, Method};
use crate::image::{Image, Placement, Source, TransmitFormat, TransmitMedium};
use crate::internal_screen::InternalScreen;
use crate::key::KittyFlags;
use crate::mouse::{Mouse, Shape};
use crate::screen::Screen;
use crate::window::Window;
use crate::Winsize;
// `Cell`/`Character` are used by the render engine via `screen.buf` and by the tests.
#[cfg(test)]
use crate::cell::{Cell, Character};

pub struct Capabilities {
    pub kitty_keyboard: bool,
    pub kitty_graphics: bool,
    pub rgb: bool,
    pub unicode: Method,
    pub sgr_pixels: bool,
    pub color_scheme_updates: bool,
    pub explicit_width: bool,
    pub scaled_text: bool,
    pub multi_cursor: bool,
}

impl Default for Capabilities {
    fn default() -> Self {
        Capabilities {
            kitty_keyboard: false,
            kitty_graphics: false,
            rgb: false,
            unicode: Method::Wcwidth,
            sgr_pixels: false,
            color_scheme_updates: false,
            explicit_width: false,
            scaled_text: false,
            multi_cursor: false,
        }
    }
}

// Zig Options default: `kitty_keyboard_flags = .{}` (KittyFlags default), and
// `system_clipboard_allocator = null` → `system_clipboard = false`. Both are the field
// `Default`s, so `derive(Default)` is faithful to the `.{}` literal.
#[derive(Default)]
pub struct Options {
    pub kitty_keyboard_flags: KittyFlags,
    /// When supplied, this allocator will be used for system clipboard
    /// requests. If not supplied, it won't be possible to request the system
    /// clipboard
    ///
    /// PORT-NOTE (§1/§10): Zig `system_clipboard_allocator: ?std.mem.Allocator` — the
    /// allocator's *presence* is what gated `requestSystemClipboard` (`== null →
    /// NoClipboardAllocator`). Ported as `system_clipboard: bool` (default `false`); a
    /// `bool` faithfully preserves that guard while paste/clipboard own their own `Vec`.
    pub system_clipboard: bool,
}

/// Zig's `sgr: enum { standard, legacy }`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sgr {
    Standard,
    Legacy,
}

/// The nested cursor position (Zig anonymous `cursor: struct { row, col }`).
#[derive(Clone, Copy, Debug, Default)]
pub struct CursorPos {
    pub row: u16,
    pub col: u16,
}

/// Zig's anonymous `state: struct {...}`.
#[derive(Clone, Copy, Debug, Default)]
pub struct State {
    /// if we are in the alt screen
    pub alt_screen: bool,
    /// if we have entered kitty keyboard
    pub kitty_keyboard: bool,
    pub bracketed_paste: bool,
    pub mouse: bool,
    pub pixel_mouse: bool,
    pub color_scheme_updates: bool,
    pub in_band_resize: bool,
    pub changed_default_fg: bool,
    pub changed_default_bg: bool,
    pub changed_cursor_color: bool,
    pub cursor: CursorPos,
}

pub struct Vaxis<'a> {
    /// the screen we write to
    pub screen: Screen<'a>,
    /// The last screen we drew. We keep this so we can efficiently update on
    /// the next render
    pub screen_last: InternalScreen,

    pub caps: Capabilities,

    pub opts: Options,

    /// if we should redraw the entire screen on the next render
    pub refresh: bool,

    /// blocks the main thread until a DA1 query has been received, or the
    /// futex times out
    ///
    /// PORT-NOTE (atomics): Zig uses `.unordered` loads/stores; the Rust port uses
    /// `AtomicU32` (`query_futex`) + `AtomicBool` (`queries_done`), both accessed
    /// `Relaxed` (weakest, closest to `.unordered`).
    pub query_futex: AtomicU32,

    /// If Queries were sent, we set this to false. We reset to true when all queries are complete. This
    /// is used because we do explicit cursor position reports in the queries, which interfere with F3
    /// key encoding. This can be used as a flag to determine how we should evaluate this sequence
    pub queries_done: AtomicBool,

    // images
    pub next_img_id: u32,

    pub sgr: Sgr,

    /// Enable workarounds for escape sequence handling issues/bugs in terminals
    /// So far this just enables a UL escape sequence workaround for conpty
    pub enable_workarounds: bool,

    pub state: State,
}

impl Default for Vaxis<'_> {
    fn default() -> Self {
        Vaxis {
            screen: Screen::default(),
            screen_last: InternalScreen::init(0, 0),
            caps: Capabilities::default(),
            opts: Options::default(),
            refresh: false,
            query_futex: AtomicU32::new(0),
            // NOTE default TRUE (Zig `atomic.Value(bool).init(true)`): the placeholder's
            // `Default` set it `false`; the real `Vaxis` must set `true` to match
            // `queryTerminalSend`'s `store(false)` → `queryTerminal`'s `store(true)` cycle.
            queries_done: AtomicBool::new(true),
            next_img_id: 1,
            sgr: Sgr::Standard,
            enable_workarounds: true,
            state: State::default(),
        }
    }
}

/// Error surface for the image transmit paths (Zig `error.NoGraphicsCapability` /
/// `error.PathTooLong` plus the writer error union).
#[derive(Debug)]
pub enum ImageError {
    NoGraphicsCapability,
    PathTooLong,
    Io(io::Error),
}

impl From<io::Error> for ImageError {
    fn from(e: io::Error) -> Self {
        ImageError::Io(e)
    }
}

impl<'a> Vaxis<'a> {
    /// Initialize Vaxis with runtime options
    ///
    /// PORT-NOTE: `alloc` dropped; `screen = Screen::default()` (0×0), `screen_last =
    /// InternalScreen::init(0, 0)`.
    pub fn init(opts: Options) -> Vaxis<'a> {
        Vaxis {
            opts,
            screen: Screen::default(),
            screen_last: InternalScreen::init(0, 0),
            ..Vaxis::default()
        }
    }

    /// Resets the terminal to it's original state. If an allocator is
    /// passed, this will free resources associated with Vaxis. This is left as an
    /// optional so applications can choose to not free resources when the
    /// application will be exiting anyways
    ///
    /// PORT-NOTE: `alloc?` dropped → `Vec` `Drop` frees the buffers (Rust always frees
    /// on drop); the `resetState` errors are swallowed (Zig `catch {}`).
    pub fn deinit<W: Write>(&mut self, tty: &mut W) {
        let _ = self.reset_state(tty);

        self.screen.deinit();
        self.screen_last.deinit();
    }

    /// resets enabled features, sends cursor to home and clears below cursor
    pub fn reset_state<W: Write>(&mut self, tty: &mut W) -> io::Result<()> {
        // always show the cursor on state reset
        let _ = tty.write_all(ctlseqs::show_cursor);
        let _ = tty.write_all(ctlseqs::sgr_reset);
        if self.screen.cursor_shape != CursorShape::Default {
            // In many terminals, `.default` will set to the configured cursor shape. Others, it will
            // change to a blinking block.
            // ctlseqs::cursor_shape = "\x1b[{d} q" ; @intFromEnum(CursorShape.default) = 0
            let _ = write!(tty, "\x1b[{} q", cursor_shape_int(CursorShape::Default));
        }
        if self.state.kitty_keyboard {
            tty.write_all(ctlseqs::csi_u_pop)?;
            self.state.kitty_keyboard = false;
        }
        if self.state.mouse {
            self.set_mouse_mode(tty, false)?;
        }
        if self.state.bracketed_paste {
            self.set_bracketed_paste(tty, false)?;
        }
        if self.state.alt_screen {
            tty.write_all(ctlseqs::home)?;
            tty.write_all(ctlseqs::erase_below_cursor)?;
            self.exit_alt_screen(tty)?;
        } else {
            tty.write_all(b"\r")?;
            let mut i: u16 = 0;
            while i < self.state.cursor.row {
                tty.write_all(ctlseqs::ri)?;
                i += 1;
            }
            tty.write_all(ctlseqs::erase_below_cursor)?;
        }
        if self.state.color_scheme_updates {
            tty.write_all(ctlseqs::color_scheme_reset)?;
            self.state.color_scheme_updates = false;
        }
        if self.state.in_band_resize {
            tty.write_all(ctlseqs::in_band_resize_reset)?;
            self.state.in_band_resize = false;
        }
        if self.state.changed_default_fg {
            tty.write_all(ctlseqs::osc10_reset)?;
            self.state.changed_default_fg = false;
        }
        if self.state.changed_default_bg {
            tty.write_all(ctlseqs::osc11_reset)?;
            self.state.changed_default_bg = false;
        }
        if self.state.changed_cursor_color {
            tty.write_all(ctlseqs::osc12_reset)?;
            self.state.changed_cursor_color = false;
        }

        tty.flush()?;
        Ok(())
    }

    /// resize allocates a slice of cells equal to the number of cells
    /// required to display the screen (ie width x height). Any previous screen is
    /// freed when resizing. The cursor will be sent to it's home position and a
    /// hardware clear-below-cursor will be sent
    ///
    /// PORT-NOTE: `log.debug` dropped; `alloc` dropped.
    pub fn resize<W: Write>(&mut self, tty: &mut W, winsize: Winsize) -> io::Result<()> {
        self.screen.deinit();
        self.screen = Screen::init(winsize);
        self.screen.width_method = self.caps.unicode;
        // try self.screen.int(alloc, winsize.cols, winsize.rows);
        // we only init our current screen. This has the effect of redrawing
        // every cell
        self.screen_last.deinit();
        self.screen_last = InternalScreen::init(winsize.cols, winsize.rows);
        if self.state.alt_screen {
            tty.write_all(ctlseqs::home)?;
        } else {
            for _ in 0..self.state.cursor.row {
                tty.write_all(ctlseqs::ri)?;
            }
            tty.write_all(b"\r")?;
        }
        self.state.cursor.row = 0;
        self.state.cursor.col = 0;
        // Zig `sgr_reset ++ erase_below_cursor` → two writes
        tty.write_all(ctlseqs::sgr_reset)?;
        tty.write_all(ctlseqs::erase_below_cursor)?;
        tty.flush()?;
        Ok(())
    }

    /// returns a Window comprising of the entire terminal screen
    pub fn window(&mut self) -> Window<'a> {
        Window {
            x_off: 0,
            y_off: 0,
            parent_x_off: 0,
            parent_y_off: 0,
            width: self.screen.width,
            height: self.screen.height,
            screen: &mut self.screen as *mut Screen<'a>,
        }
    }

    /// enter the alternate screen. The alternate screen will automatically
    /// be exited if calling deinit while in the alt screen.
    pub fn enter_alt_screen<W: Write>(&mut self, tty: &mut W) -> io::Result<()> {
        tty.write_all(ctlseqs::smcup)?;
        tty.flush()?;
        self.state.alt_screen = true;
        Ok(())
    }

    /// exit the alternate screen. Does not flush the writer.
    pub fn exit_alt_screen<W: Write>(&mut self, tty: &mut W) -> io::Result<()> {
        tty.write_all(ctlseqs::rmcup)?;
        tty.flush()?;
        self.state.alt_screen = false;
        Ok(())
    }

    /// write queries to the terminal to determine capabilities. Individual
    /// capabilities will be delivered to the client and possibly intercepted by
    /// Vaxis to enable features.
    ///
    /// This call will block until Vaxis.query_futex is woken up, or the timeout.
    /// Event loops can wake up this futex when cap_da1 is received
    pub fn query_terminal<W: Write>(&mut self, tty: &mut W, timeout_ns: u64) -> io::Result<()> {
        self.query_terminal_send(tty)?;
        // 1 second timeout
        //
        // PORT-NOTE (§8.1): Rust std has no stable `std.Thread.Futex` API. `Futex.timedWait`
        // blocks until the word changes from `expect` (0) or the timeout elapses; the DA1
        // handler calls `Futex.wake`. The faithful image is a bounded timed wait keyed on the
        // atomic (`query_futex`): `wake_query_futex` stores a non-zero value so the loop
        // observes the change. This is a divergence in *mechanism* (poll-with-deadline vs
        // kernel futex) but identical in observable behavior (block until DA1 or timeout).
        // The `Futex.timedWait` error is swallowed (`catch {}` — timeout is normal).
        let deadline = std::time::Instant::now() + std::time::Duration::from_nanos(timeout_ns);
        while self.query_futex.load(Ordering::Relaxed) == 0 && std::time::Instant::now() < deadline
        {
            std::thread::yield_now();
        }
        self.queries_done.store(true, Ordering::Relaxed);
        self.enable_detected_features(tty)?;
        Ok(())
    }

    /// write queries to the terminal to determine capabilities. This function
    /// is only for use with a custom main loop. Call Vaxis.queryTerminal() if
    /// you are using Loop.run()
    pub fn query_terminal_send<W: Write>(&mut self, tty: &mut W) -> io::Result<()> {
        self.queries_done.store(false, Ordering::Relaxed);
        // PORT-NOTE (§8.1 repeated-call fidelity): Zig's `Futex.wake` never mutates the
        // word, so `timedWait(&query_futex, 0, …)` always waits fresh on value==0. Our
        // `wake_query_futex` *stores 1*, so the word must be reset to 0 before each wait or a
        // second `queryTerminal` returns instantly. Reset here to re-arm the wait.
        self.query_futex.store(0, Ordering::Relaxed);

        // TODO: re-enable this
        // const colorterm = std.posix.getenv("COLORTERM") orelse "";
        // if (std.mem.eql(u8, colorterm, "truecolor") or
        //     std.mem.eql(u8, colorterm, "24bit"))
        // {
        //     if (@hasField(Event, "cap_rgb")) {
        //         self.postEvent(.cap_rgb);
        //     }
        // }

        // TODO: XTGETTCAP queries ("RGB", "Smulx")
        // TODO: decide if we actually want to query for focus and sync. It
        // doesn't hurt to blindly use them
        // _ = try tty.write(ctlseqs.decrqm_focus);
        // _ = try tty.write(ctlseqs.decrqm_sync);
        //
        // Zig writes a single `++`-concatenated const; ported as sequential writes in the
        // exact order.
        tty.write_all(ctlseqs::decrqm_sgr_pixels)?;
        tty.write_all(ctlseqs::decrqm_unicode)?;
        tty.write_all(ctlseqs::decrqm_color_scheme)?;
        tty.write_all(ctlseqs::in_band_resize_set)?;

        // Explicit width query. We send the cursor home, then do an explicit width command, then
        // query the position. If the parsed value is an F3 with shift, we support explicit width.
        // The returned response will be something like \x1b[1;2R...which when parsed as a Key is a
        // shift + F3 (the row is ignored). We only care if the column has moved from 1->2, which is
        // why we see a Shift modifier
        tty.write_all(ctlseqs::home)?;
        tty.write_all(ctlseqs::explicit_width_query)?;
        tty.write_all(ctlseqs::cursor_position_request)?;
        // Explicit width query. We send the cursor home, then do an scaled text command, then
        // query the position. If the parsed value is an F3 with al, we support scaled text.
        // The returned response will be something like \x1b[1;3R...which when parsed as a Key is a
        // alt + F3 (the row is ignored). We only care if the column has moved from 1->3, which is
        // why we see a Shift modifier
        tty.write_all(ctlseqs::home)?;
        tty.write_all(ctlseqs::scaled_text_query)?;
        tty.write_all(ctlseqs::multi_cursor_query)?;
        tty.write_all(ctlseqs::cursor_position_request)?;
        tty.write_all(ctlseqs::xtversion)?;
        tty.write_all(ctlseqs::csi_u_query)?;
        tty.write_all(ctlseqs::kitty_graphics_query)?;
        tty.write_all(ctlseqs::primary_device_attrs)?;

        tty.flush()?;
        Ok(())
    }

    /// Enable features detected by responses to queryTerminal. This function
    /// is only for use with a custom main loop. Call Vaxis.queryTerminal() if
    /// you are using Loop.run()
    pub fn enable_detected_features<W: Write>(&mut self, tty: &mut W) -> io::Result<()> {
        // PORT-NOTE: `builtin.os.tag` switch → `#[cfg(windows)]`/`#[cfg(not(windows))]`;
        // `std.posix.getenv` → `std::env::var_os`.
        #[cfg(windows)]
        {
            // No feature detection on windows. We just hard enable some knowns for ConPTY
            self.sgr = Sgr::Legacy;
        }
        #[cfg(not(windows))]
        {
            // Apply any environment variables
            if std::env::var_os("TERMUX_VERSION").is_some() {
                self.sgr = Sgr::Legacy;
            }
            if std::env::var_os("VHS_RECORD").is_some() {
                self.caps.unicode = Method::Wcwidth;
                self.caps.kitty_keyboard = false;
                self.sgr = Sgr::Legacy;
            }
            if let Some(prg) = std::env::var_os("TERM_PROGRAM") {
                if prg == "vscode" {
                    self.sgr = Sgr::Legacy;
                }
            }
            if std::env::var_os("VAXIS_FORCE_LEGACY_SGR").is_some() {
                self.sgr = Sgr::Legacy;
            }
            if std::env::var_os("VAXIS_FORCE_WCWIDTH").is_some() {
                self.caps.unicode = Method::Wcwidth;
            }
            if std::env::var_os("VAXIS_FORCE_UNICODE").is_some() {
                self.caps.unicode = Method::Unicode;
            }

            // enable detected features
            if self.caps.kitty_keyboard {
                let flags = self.opts.kitty_keyboard_flags;
                self.enable_kitty_keyboard(tty, flags)?;
            }
            // Only enable mode 2027 if we don't have explicit width
            if self.caps.unicode == Method::Unicode && !self.caps.explicit_width {
                tty.write_all(ctlseqs::unicode_set)?;
            }
        }

        tty.flush()?;
        Ok(())
    }

    // the next render call will refresh the entire screen
    pub fn queue_refresh(&mut self) {
        self.refresh = true;
    }

    /// draws the screen to the terminal
    pub fn render<W: Write>(&mut self, tty: &mut W) -> io::Result<()> {
        // defer self.refresh = false; — runs on ALL (Ok/Err) exits (§6.6). The asserts in
        // `render_framed` panic on failure (Zig `assert` aborts too → no defer), so the
        // reset correctly does NOT run then.
        let result = self.render_framed(tty);
        self.refresh = false;
        result
    }

    fn render_framed<W: Write>(&mut self, tty: &mut W) -> io::Result<()> {
        assert!(self.screen.buf.len() == self.screen.width as usize * self.screen.height as usize); // correct size
        assert!(self.screen.buf.len() == self.screen_last.buf.len()); // same size

        // Set up sync before we write anything
        // TODO: optimize sync so we only sync _when we have changes_. This
        // requires a smarter buffered writer, we'll probably have to write
        // our own
        tty.write_all(ctlseqs::sync_set)?;
        // errdefer tty.writeAll(ctlseqs.sync_reset) catch {} — fires ONLY on an error return
        // between here and the normal §779 `sync_reset` (which lives inside `render_diff`).
        let res = self.render_diff(tty);
        if res.is_err() {
            let _ = tty.write_all(ctlseqs::sync_reset);
        }
        res
    }

    fn render_diff<W: Write>(&mut self, tty: &mut W) -> io::Result<()> {
        // Send the cursor to 0,0
        // TODO: this needs to move after we optimize writes. We only do
        // this if we have an update to make. We also need to hide cursor
        // and then reshow it if needed
        tty.write_all(ctlseqs::hide_cursor)?;
        if self.state.alt_screen {
            tty.write_all(ctlseqs::home)?;
        } else {
            tty.write_all(b"\r")?;
            for _ in 0..self.state.cursor.row {
                tty.write_all(ctlseqs::ri)?;
            }
        }
        tty.write_all(ctlseqs::sgr_reset)?;

        // initialize some variables
        let mut reposition: bool = false;
        let mut row: u16 = 0;
        let mut col: u16 = 0;
        let mut cursor: Style = Style::default();
        let mut link: Hyperlink = Hyperlink::default();
        // the nested `cursor_pos` anon struct → two `u16` locals
        let mut cursor_pos_row: u16 = 0;
        let mut cursor_pos_col: u16 = 0;

        // Clear all images
        if self.caps.kitty_graphics {
            tty.write_all(ctlseqs::kitty_graphics_clear)?;
        }

        // Reset skip flag on all last_screen cells
        for last_cell in &mut self.screen_last.buf {
            last_cell.skip = false;
        }

        let mut i: usize = 0;
        while i < self.screen.buf.len() {
            let cell = self.screen.buf[i];
            let w: u16 = if cell.char.width != 0 {
                cell.char.width as u16
            } else {
                let method: Method = self.caps.unicode;
                let width: u16 = gwidth::gwidth(cell.char.grapheme, method);
                std::cmp::max(1, width)
            };
            if col >= self.screen.width {
                row += 1;
                col = 0;
                // Rely on terminal wrapping to reposition into next row instead of forcing it
                if !cell.wrapped {
                    reposition = true;
                }
            }
            // The Zig loop body has two `defer`s (§6.4): the ADVANCE (declared first → runs
            // LAST on every exit incl. `continue`) and the cursor/link update (declared after
            // the skip-`continue` → runs on the scale-continue + normal-end but NOT the
            // skip-continue). `'body` maps the `continue`s to `break 'body`; the ADVANCE runs
            // once after the block; the cursor/link update sits inside the block past the skip
            // check.
            'body: {
                // If cell is the same as our last frame, we don't need to do
                // anything.
                //
                // PORT-NOTE (§6.5): Zig `const last = self.screen_last.buf[i]` is a COPY;
                // `InternalCell` is NOT `Copy` (owns three `Vec`s). Capture the three needed
                // values into `bool` locals before any `&mut self.screen_last` write.
                let (last_eql, last_skipped, last_skip) = {
                    let last = &self.screen_last.buf[i];
                    (last.eql(&cell), last.skipped, last.skip)
                };
                if (!self.refresh && last_eql && !last_skipped && cell.image.is_none()) || last_skip
                {
                    reposition = true;
                    // Close any osc8 sequence we might be in before
                    // repositioning
                    if !link.uri.is_empty() {
                        tty.write_all(ctlseqs::osc8_clear)?;
                    }
                    break 'body; // ADVANCE only; cursor/link update does NOT run
                }
                self.screen_last.buf[i].skipped = false;
                // Set this cell in the last frame
                self.screen_last.write_cell(col, row, &cell);

                // If we support scaled text, we set the flags now
                if self.caps.scaled_text && cell.scale.scale > 1 {
                    // The cell is scaled. Set appropriate skips. We only need to do this if the scale factor is
                    // > 1
                    assert!(cell.char.width > 0);
                    let cols = cell.scale.scale as usize * cell.char.width as usize;
                    let rows = cell.scale.scale as usize;
                    for skipped_row in 0..rows {
                        for skipped_col in 0..cols {
                            if skipped_row == 0 && skipped_col == 0 {
                                continue;
                            }
                            let skipped_i = (skipped_row + row as usize)
                                * self.screen_last.width as usize
                                + (skipped_col + col as usize);
                            self.screen_last.buf[skipped_i].skip = true;
                        }
                    }
                }

                // reposition the cursor, if needed
                if reposition {
                    reposition = false;
                    link = Hyperlink::default();
                    if self.state.alt_screen {
                        // ctlseqs::cup = "\x1b[{d};{d}H"
                        write!(tty, "\x1b[{};{}H", row + 1, col + 1)?;
                    } else if cursor_pos_row == row {
                        let n = col - cursor_pos_col;
                        if n > 0 {
                            // ctlseqs::cuf = "\x1b[{d}C"
                            write!(tty, "\x1b[{}C", n)?;
                        }
                    } else {
                        let n = row - cursor_pos_row;
                        for _ in 0..n {
                            tty.write_all(b"\n")?;
                        }
                        tty.write_all(b"\r")?;
                        if col > 0 {
                            // ctlseqs::cuf = "\x1b[{d}C"
                            write!(tty, "\x1b[{}C", col)?;
                        }
                    }
                }

                if let Some(img) = cell.image {
                    emit_image(tty, img)?;
                }

                // something is different, so let's loop through everything and
                // find out what
                emit_style_diff(tty, &cursor, cell.style, self.sgr, self.enable_workarounds)?;

                // url
                if link.uri != cell.link.uri {
                    let mut ps: &[u8] = cell.link.params;
                    if cell.link.uri.is_empty() {
                        // Empty out the params no matter what if we don't have
                        // a url
                        ps = b"";
                    }
                    // ctlseqs::osc8 = "\x1b]8;{s};{s}\x1b\\" (params, uri) — `{s}` → write_all
                    tty.write_all(b"\x1b]8;")?;
                    tty.write_all(ps)?;
                    tty.write_all(b";")?;
                    tty.write_all(cell.link.uri)?;
                    tty.write_all(b"\x1b\\")?;
                }

                // scale
                if self.caps.scaled_text && !cell.scale.eql(Scale::default()) {
                    let scale = cell.scale;
                    // We have a scaled cell.
                    match cell.scale.denominator {
                        // Denominator cannot be 0
                        0 => unreachable!(),
                        1 => {
                            // no fractional scaling, just a straight scale factor
                            // ctlseqs::scaled_text = "\x1b]66;s={d}:w={d};{s}\x1b\\"
                            write!(tty, "\x1b]66;s={}:w={};", scale.scale, w)?;
                            tty.write_all(cell.char.grapheme)?;
                            tty.write_all(b"\x1b\\")?;
                        }
                        _ => {
                            // fractional scaling
                            // no fractional scaling, just a straight scale factor
                            // ctlseqs::scaled_text_with_fractions =
                            //   "\x1b]66;s={d}:w={d}:n={d}:d={d}:v={d};{s}\x1b\\"
                            write!(
                                tty,
                                "\x1b]66;s={}:w={}:n={}:d={}:v={};",
                                scale.scale,
                                w,
                                scale.numerator,
                                scale.denominator,
                                scale.vertical_alignment as u8, // @intFromEnum
                            )?;
                            tty.write_all(cell.char.grapheme)?;
                            tty.write_all(b"\x1b\\")?;
                        }
                    }
                    cursor_pos_col = col + (w * scale.scale as u16);
                    cursor_pos_row = row;
                    // both defers run before this `continue`: cursor/link update THEN ADVANCE
                    cursor = cell.style;
                    link = cell.link;
                    break 'body;
                }

                // If we have explicit width and our width is greater than 1, let's use it
                if self.caps.explicit_width && w > 1 {
                    // ctlseqs::explicit_width = "\x1b]66;w={d};{s}\x1b\\"
                    write!(tty, "\x1b]66;w={};", w)?;
                    tty.write_all(cell.char.grapheme)?;
                    tty.write_all(b"\x1b\\")?;
                } else {
                    tty.write_all(cell.char.grapheme)?;
                }
                cursor_pos_col = col + w;
                cursor_pos_row = row;

                // cursor/link update (§451 defer) — runs on the scale-continue + normal-end
                cursor = cell.style;
                link = cell.link;
            }
            // ADVANCE (§415 defer): advance by the width of this char mod 1
            debug_assert!(w > 0);
            let mut j = i + 1;
            while j < i + w as usize {
                if j >= self.screen_last.buf.len() {
                    break;
                }
                self.screen_last.buf[j].skipped = true;
                j += 1;
            }
            col += w;
            i += w as usize;
        }
        if self.screen.cursor_vis {
            if self.state.alt_screen {
                // ctlseqs::cup
                write!(
                    tty,
                    "\x1b[{};{}H",
                    self.screen.cursor_row + 1,
                    self.screen.cursor_col + 1
                )?;
            } else {
                // TODO: position cursor relative to current location
                tty.write_all(b"\r")?;
                if self.screen.cursor_row >= cursor_pos_row {
                    for _ in 0..(self.screen.cursor_row - cursor_pos_row) {
                        tty.write_all(b"\n")?;
                    }
                } else {
                    for _ in 0..(cursor_pos_row - self.screen.cursor_row) {
                        tty.write_all(ctlseqs::ri)?;
                    }
                }
                if self.screen.cursor_col > 0 {
                    // ctlseqs::cuf
                    write!(tty, "\x1b[{}C", self.screen.cursor_col)?;
                }
            }
            self.state.cursor.row = self.screen.cursor_row;
            self.state.cursor.col = self.screen.cursor_col;
            tty.write_all(ctlseqs::show_cursor)?;
        } else {
            self.state.cursor.row = cursor_pos_row;
            self.state.cursor.col = cursor_pos_col;
        }
        if self.screen.mouse_shape != self.screen_last.mouse_shape {
            // ctlseqs::osc22_mouse_shape = "\x1b]22;{s}\x1b\\" ; @tagName(mouse_shape)
            tty.write_all(b"\x1b]22;")?;
            tty.write_all(shape_tag_name(self.screen.mouse_shape))?;
            tty.write_all(b"\x1b\\")?;
            self.screen_last.mouse_shape = self.screen.mouse_shape;
        }
        if self.screen.cursor_shape != self.screen_last.cursor_shape {
            // ctlseqs::cursor_shape = "\x1b[{d} q" ; @intFromEnum(cursor_shape)
            write!(tty, "\x1b[{} q", cursor_shape_int(self.screen.cursor_shape))?;
            self.screen_last.cursor_shape = self.screen.cursor_shape;
        }

        tty.write_all(ctlseqs::sync_reset)?;
        tty.flush()?;
        Ok(())
    }

    fn enable_kitty_keyboard<W: Write>(
        &mut self,
        tty: &mut W,
        flags: KittyFlags,
    ) -> io::Result<()> {
        // const flag_int: u5 = @bitCast(flags);
        let flag_int: u8 = kitty_flags_bits(flags);
        // ctlseqs::csi_u_push = "\x1b[>{d}u"
        write!(tty, "\x1b[>{}u", flag_int)?;
        tty.flush()?;
        self.state.kitty_keyboard = true;
        Ok(())
    }

    /// send a system notification
    pub fn notify<W: Write>(
        &self,
        tty: &mut W,
        title: Option<&[u8]>,
        body: &[u8],
    ) -> io::Result<()> {
        match title {
            Some(t) => {
                // ctlseqs::osc777_notify = "\x1b]777;notify;{s};{s}\x1b\\"
                tty.write_all(b"\x1b]777;notify;")?;
                tty.write_all(t)?;
                tty.write_all(b";")?;
                tty.write_all(body)?;
                tty.write_all(b"\x1b\\")?;
            }
            None => {
                // ctlseqs::osc9_notify = "\x1b]9;{s}\x1b\\"
                tty.write_all(b"\x1b]9;")?;
                tty.write_all(body)?;
                tty.write_all(b"\x1b\\")?;
            }
        }

        tty.flush()?;
        Ok(())
    }

    /// sets the window title
    pub fn set_title<W: Write>(&self, tty: &mut W, title: &[u8]) -> io::Result<()> {
        // ctlseqs::osc2_set_title = "\x1b]2;{s}\x1b\\"
        tty.write_all(b"\x1b]2;")?;
        tty.write_all(title)?;
        tty.write_all(b"\x1b\\")?;
        tty.flush()?;
        Ok(())
    }

    // turn bracketed paste on or off. An event will be sent at the
    // beginning and end of a detected paste. All keystrokes between these
    // events were pasted
    pub fn set_bracketed_paste<W: Write>(&mut self, tty: &mut W, enable: bool) -> io::Result<()> {
        let seq = if enable {
            ctlseqs::bp_set
        } else {
            ctlseqs::bp_reset
        };
        tty.write_all(seq)?;
        tty.flush()?;
        self.state.bracketed_paste = enable;
        Ok(())
    }

    /// set the mouse shape
    pub fn set_mouse_shape(&mut self, shape: Shape) {
        self.screen.mouse_shape = shape;
    }

    /// Change the mouse reporting mode
    ///
    /// PORT-NOTE: `log.debug` dropped.
    pub fn set_mouse_mode<W: Write>(&mut self, tty: &mut W, enable: bool) -> io::Result<()> {
        if enable {
            self.state.mouse = true;
            if self.caps.sgr_pixels {
                self.state.pixel_mouse = true;
                tty.write_all(ctlseqs::mouse_set_pixels)?;
            } else {
                tty.write_all(ctlseqs::mouse_set)?;
            }
        } else {
            tty.write_all(ctlseqs::mouse_reset)?;
        }

        tty.flush()?;
        Ok(())
    }

    /// Translate pixel mouse coordinates to cell + offset
    ///
    /// PORT-NOTE: Zig takes `self` by value; `&self` is faithful (it only reads).
    /// Fills the Chunk-3b placeholder leaf.
    pub fn translate_mouse(&self, mouse: Mouse) -> Mouse {
        if self.screen.width == 0 || self.screen.height == 0 {
            return mouse;
        }
        let mut result = mouse;
        if self.state.pixel_mouse {
            assert!(mouse.xoffset == 0);
            assert!(mouse.yoffset == 0);
            let xpos = mouse.col;
            let ypos = mouse.row;
            let xextra = self.screen.width_pix % self.screen.width;
            let yextra = self.screen.height_pix % self.screen.height;
            let xcell = (self.screen.width_pix - xextra) / self.screen.width;
            let ycell = (self.screen.height_pix - yextra) / self.screen.height;
            if xcell == 0 || ycell == 0 {
                return mouse;
            }
            result.col = xpos / xcell;
            result.row = ypos / ycell;
            result.xoffset = xpos % xcell;
            result.yoffset = ypos % ycell;
        }
        result
    }

    /// Transmit an image using the local filesystem. Allocates only for base64 encoding
    ///
    /// PORTABLE (no zigimg): base64-encodes the filesystem `payload` and emits the kitty
    /// transmit sequence. `alloc` → a local `Vec`.
    pub fn transmit_local_image_path<W: Write>(
        &mut self,
        tty: &mut W,
        payload: &[u8],
        width: u16,
        height: u16,
        medium: TransmitMedium,
        format: TransmitFormat,
    ) -> Result<Image, ImageError> {
        if !self.caps.kitty_graphics {
            return Err(ImageError::NoGraphicsCapability);
        }

        // defer self.next_img_id += 1; — runs on the PathTooLong error return and the normal
        // return (both past this point), NOT the NoGraphicsCapability return above.
        let id = self.next_img_id;
        let res = transmit_local_image_path_inner(tty, id, payload, width, height, medium, format);
        self.next_img_id += 1;
        res
    }

    /// Transmit an image which has been pre-base64 encoded
    ///
    /// PORTABLE (no zigimg): emits pre-b64 bytes, chunking at 4096 (`m=1`/`m=0`).
    pub fn transmit_pre_encoded_image<W: Write>(
        &mut self,
        tty: &mut W,
        bytes: &[u8],
        width: u16,
        height: u16,
        format: TransmitFormat,
    ) -> Result<Image, ImageError> {
        if !self.caps.kitty_graphics {
            return Err(ImageError::NoGraphicsCapability);
        }

        // defer self.next_img_id += 1;
        let id = self.next_img_id;
        let res = transmit_pre_encoded_image_inner(tty, id, bytes, width, height, format);
        self.next_img_id += 1;
        res
    }

    // R5 SEAM (zigimg — the DEFERRED dep): `transmitImage` decodes/converts a
    // `zigimg.Image` (writeToMemory(.png) / convert(.rgb24/.rgba32) + rawBytes) into an
    // arena buffer, base64-encodes it, then calls `transmitPreEncodedImage`. `zigimg` is
    // absent, so the `img: *zigimg.Image` param is dropped and the body is an
    // `unimplemented!()` seam (placeholder-leaf-doctrine). NOT reached by list_themes/boo.
    // Verbatim Zig body:
    //
    // pub fn transmitImage(self, alloc, tty, img: *zigimg.Image, format) !Image {
    //     if (!self.caps.kitty_graphics) return error.NoGraphicsCapability;
    //     var arena = std.heap.ArenaAllocator.init(alloc);
    //     defer arena.deinit();
    //     const buf = switch (format) {
    //         .png => png: {
    //             const png_buf = try arena.allocator().alloc(u8, img.imageByteSize());
    //             const png = try img.writeToMemory(png_buf, .{ .png = .{} });
    //             break :png png;
    //         },
    //         .rgb => rgb: {
    //             try img.convert(.rgb24);
    //             break :rgb img.rawBytes();
    //         },
    //         .rgba => rgba: {
    //             try img.convert(.rgba32);
    //             break :rgba img.rawBytes();
    //         },
    //     };
    //     const b64_buf = try arena.allocator().alloc(u8, base64Encoder.calcSize(buf.len));
    //     const encoded = base64Encoder.encode(b64_buf, buf);
    //     return self.transmitPreEncodedImage(tty, encoded, @intCast(img.width), @intCast(img.height), format);
    // }
    pub fn transmit_image<W: Write>(
        &mut self,
        _tty: &mut W,
        _format: TransmitFormat,
    ) -> Result<Image, ImageError> {
        unimplemented!("transmit_image — zigimg R5 seam (DEFERRALS.md)")
    }

    // R5 SEAM (zigimg — the DEFERRED dep): `loadImage` decodes an image via
    // `zigimg.Image.fromFilePath`/`fromMemory` then calls `transmitImage`. Verbatim Zig body:
    //
    // pub fn loadImage(self, alloc, tty, src: Image.Source) !Image {
    //     if (!self.caps.kitty_graphics) return error.NoGraphicsCapability;
    //     var read_buffer: [1024 * 1024]u8 = undefined; // 1MB buffer
    //     var img = switch (src) {
    //         .path => |path| try zigimg.Image.fromFilePath(alloc, path, &read_buffer),
    //         .mem => |bytes| try zigimg.Image.fromMemory(alloc, bytes),
    //     };
    //     defer img.deinit();
    //     return self.transmitImage(alloc, tty, &img, .png);
    // }
    pub fn load_image<W: Write>(
        &mut self,
        _tty: &mut W,
        _src: Source,
    ) -> Result<Image, ImageError> {
        unimplemented!("load_image — zigimg R5 seam (DEFERRALS.md)")
    }

    /// deletes an image from the terminal's memory
    ///
    /// PORT-NOTE: errors swallowed (Zig `catch`); `log.err` dropped.
    pub fn free_image<W: Write>(&self, tty: &mut W, id: u32) {
        // tty.print("\x1b_Ga=d,d=I,i={d};\x1b\\", .{id}) catch |err| { log.err(...); return; };
        if write!(tty, "\x1b_Ga=d,d=I,i={};\x1b\\", id).is_err() {
            return;
        }
        // tty.flush() catch {};
        let _ = tty.flush();
    }

    pub fn copy_to_system_clipboard<W: Write>(&self, tty: &mut W, text: &[u8]) -> io::Result<()> {
        // encode_allocator dropped; base64-encode `text` into a local Vec.
        let b64 = base64_standard_encode(text);
        // ctlseqs::osc52_clipboard_copy = "\x1b]52;c;{s}\x1b\\"
        tty.write_all(b"\x1b]52;c;")?;
        tty.write_all(&b64)?;
        tty.write_all(b"\x1b\\")?;

        tty.flush()?;
        Ok(())
    }

    pub fn request_system_clipboard<W: Write>(&self, tty: &mut W) -> io::Result<()> {
        // PORT-NOTE (§10): Zig `if (self.opts.system_clipboard_allocator == null) return
        // error.NoClipboardAllocator;` — the allocator's presence is ported as the
        // `system_clipboard: bool` guard.
        if !self.opts.system_clipboard {
            return Err(io::Error::other("NoClipboardAllocator"));
        }
        tty.write_all(ctlseqs::osc52_clipboard_request)?;
        tty.flush()?;
        Ok(())
    }

    /// Set the default terminal foreground color
    pub fn set_terminal_foreground_color<W: Write>(
        &mut self,
        tty: &mut W,
        rgb: [u8; 3],
    ) -> io::Result<()> {
        // ctlseqs::osc10_set = "\x1b]10;rgb:{x:0>2}{x:0>2}/{x:0>2}{x:0>2}/{x:0>2}{x:0>2}\x1b\\"
        write!(
            tty,
            "\x1b]10;rgb:{:02x}{:02x}/{:02x}{:02x}/{:02x}{:02x}\x1b\\",
            rgb[0], rgb[0], rgb[1], rgb[1], rgb[2], rgb[2]
        )?;
        tty.flush()?;
        self.state.changed_default_fg = true;
        Ok(())
    }

    /// Set the default terminal background color
    pub fn set_terminal_background_color<W: Write>(
        &mut self,
        tty: &mut W,
        rgb: [u8; 3],
    ) -> io::Result<()> {
        // ctlseqs::osc11_set
        write!(
            tty,
            "\x1b]11;rgb:{:02x}{:02x}/{:02x}{:02x}/{:02x}{:02x}\x1b\\",
            rgb[0], rgb[0], rgb[1], rgb[1], rgb[2], rgb[2]
        )?;
        tty.flush()?;
        self.state.changed_default_bg = true;
        Ok(())
    }

    /// Set the terminal cursor color
    pub fn set_terminal_cursor_color<W: Write>(
        &mut self,
        tty: &mut W,
        rgb: [u8; 3],
    ) -> io::Result<()> {
        // ctlseqs::osc12_set
        write!(
            tty,
            "\x1b]12;rgb:{:02x}{:02x}/{:02x}{:02x}/{:02x}{:02x}\x1b\\",
            rgb[0], rgb[0], rgb[1], rgb[1], rgb[2], rgb[2]
        )?;
        tty.flush()?;
        self.state.changed_cursor_color = true;
        Ok(())
    }

    /// Request a color report from the terminal. Note: not all terminals support
    /// reporting colors. It is always safe to try, but you may not receive a
    /// response.
    pub fn query_color<W: Write>(&self, tty: &mut W, kind: Kind) -> io::Result<()> {
        match kind {
            Kind::Fg => tty.write_all(ctlseqs::osc10_query)?,
            Kind::Bg => tty.write_all(ctlseqs::osc11_query)?,
            Kind::Cursor => tty.write_all(ctlseqs::osc12_query)?,
            // ctlseqs::osc4_query = "\x1b]4;{d};?\x1b\\"
            Kind::Index(idx) => write!(tty, "\x1b]4;{};?\x1b\\", idx)?,
        }
        tty.flush()?;
        Ok(())
    }

    /// Subscribe to color theme updates. A `color_scheme: Color.Scheme` tag must
    /// exist on your Event type to receive the response. This is a queried
    /// capability. Support can be detected by checking the value of
    /// vaxis.caps.color_scheme_updates. The initial scheme will be reported when
    /// subscribing.
    pub fn subscribe_to_color_scheme_updates<W: Write>(&mut self, tty: &mut W) -> io::Result<()> {
        tty.write_all(ctlseqs::color_scheme_request)?;
        tty.write_all(ctlseqs::color_scheme_set)?;
        tty.flush()?;
        self.state.color_scheme_updates = true;
        Ok(())
    }

    /// PORT-NOTE: fills the Chunk-3b placeholder leaf; `loop_.rs`'s `stop` call site
    /// (`(*vx).device_status_report((*tty).writer())`) infers `W = TtyWriter`.
    pub fn device_status_report<W: Write>(&self, tty: &mut W) -> io::Result<()> {
        tty.write_all(ctlseqs::device_status_report)?;
        tty.flush()?;
        Ok(())
    }

    /// The inlined `std.Thread.Futex.wake(&vx.query_futex, 10)` — the DA1-response wake.
    /// Store a non-zero value into `query_futex` to release the §8.1 wait. Documented as
    /// the faithful image of `Futex.wake` (Rust std has no futex API). Fills the
    /// Chunk-3b placeholder leaf.
    pub fn wake_query_futex(&self) {
        self.query_futex.store(1, Ordering::Relaxed);
    }

    /// prettyPrint is used to print the contents of the Screen to the tty. The state is not stored, and
    /// the cursor will be put on the next line after the last line is printed. This is useful to
    /// sequentially print data in a styled format to eg. stdout. This function returns an error if you
    /// are not in the alt screen. The cursor is always hidden, and mouse shapes are not available
    pub fn pretty_print<W: Write>(&mut self, tty: &mut W) -> io::Result<()> {
        if self.state.alt_screen {
            return Err(io::Error::other("NotInPrimaryScreen"));
        }

        tty.write_all(ctlseqs::hide_cursor)?;
        tty.write_all(ctlseqs::sync_set)?;
        // Zig declares two trailing `defer`s here (LIFO at scope exit, AFTER the body's
        // final `\r\n` + flush): `defer sync_reset` (§1126, declared FIRST) then `defer
        // sgr_reset` (§1128, declared LAST). So the byte stream at exit is:
        //   … \r\n → flush → sgr_reset (LIFO-first) → sync_reset (LIFO-last).
        // The LEADING `sgr_reset` (§1127) is a normal write inside the body, before the loop.
        // Both defers fire on ALL exits (Ok/Err); emit them after the body in LIFO order.
        let res = self.pretty_print_body(tty);
        let _ = tty.write_all(ctlseqs::sgr_reset);
        let _ = tty.write_all(ctlseqs::sync_reset);
        res
    }

    fn pretty_print_body<W: Write>(&mut self, tty: &mut W) -> io::Result<()> {
        tty.write_all(ctlseqs::sgr_reset)?; // leading sgr_reset (§1127)

        let mut reposition: bool = false;
        let mut row: u16 = 0;
        let mut col: u16 = 0;
        let mut cursor: Style = Style::default();
        let mut link: Hyperlink = Hyperlink::default();
        let mut cursor_pos_row: u16 = 0;
        let mut cursor_pos_col: u16 = 0;

        // PORT-NOTE (§7): Zig `prettyPrint` uses `var i: u16` (§1140) whereas `render` uses
        // `usize`. Reproduce `u16` for strict fidelity (matching the `w: u16` advance); `j`
        // is computed in `usize` for indexing (benign — only differs for >65535-cell screens,
        // where Zig's `i + w` would overflow-panic).
        let mut i: u16 = 0;
        while (i as usize) < self.screen.buf.len() {
            let cell = self.screen.buf[i as usize];
            let w: u16 = if cell.char.width != 0 {
                cell.char.width as u16
            } else {
                let method: Method = self.caps.unicode;
                let width: u16 = gwidth::gwidth(cell.char.grapheme, method);
                std::cmp::max(1, width)
            };
            if col >= self.screen.width {
                row += 1;
                col = 0;
                // Rely on terminal wrapping to reposition into next row instead of forcing it
                if !cell.wrapped {
                    reposition = true;
                }
            }
            'body: {
                if cell.default {
                    reposition = true;
                    break 'body; // ADVANCE only (cursor/link update declared after → skipped)
                }

                // reposition the cursor, if needed
                if reposition {
                    reposition = false;
                    link = Hyperlink::default();
                    if cursor_pos_row == row {
                        let n = col - cursor_pos_col;
                        if n > 0 {
                            // ctlseqs::cuf
                            write!(tty, "\x1b[{}C", n)?;
                        }
                    } else {
                        let n = row - cursor_pos_row;
                        for _ in 0..n {
                            tty.write_all(b"\n")?;
                        }
                        tty.write_all(b"\r")?;
                        if col > 0 {
                            // ctlseqs::cuf
                            write!(tty, "\x1b[{}C", col)?;
                        }
                    }
                }

                if let Some(img) = cell.image {
                    emit_image(tty, img)?;
                }

                // something is different, so let's loop through everything and
                // find out what
                emit_style_diff(tty, &cursor, cell.style, self.sgr, self.enable_workarounds)?;

                // url
                if link.uri != cell.link.uri {
                    let mut ps: &[u8] = cell.link.params;
                    if cell.link.uri.is_empty() {
                        // Empty out the params no matter what if we don't have
                        // a url
                        ps = b"";
                    }
                    // ctlseqs::osc8 = "\x1b]8;{s};{s}\x1b\\"
                    tty.write_all(b"\x1b]8;")?;
                    tty.write_all(ps)?;
                    tty.write_all(b";")?;
                    tty.write_all(cell.link.uri)?;
                    tty.write_all(b"\x1b\\")?;
                }
                tty.write_all(cell.char.grapheme)?;
                cursor_pos_col = col + w;
                cursor_pos_row = row;

                // cursor/link update (§1172 defer) — non-default exit only
                cursor = cell.style;
                link = cell.link;
            }
            // ADVANCE (§1150 defer): advance by the width of this char mod 1
            debug_assert!(w > 0);
            let mut j = (i as usize) + 1;
            while j < (i as usize) + w as usize {
                if j >= self.screen_last.buf.len() {
                    break;
                }
                self.screen_last.buf[j].skipped = true;
                j += 1;
            }
            col += w;
            i += w;
        }
        tty.write_all(b"\r\n")?;
        tty.flush()?;
        Ok(())
    }

    /// Set the terminal's current working directory
    pub fn set_terminal_working_directory<W: Write>(
        &self,
        tty: &mut W,
        path: &[u8],
    ) -> io::Result<()> {
        if path.is_empty() || path[0] != b'/' {
            return Err(io::Error::other("InvalidAbsolutePath"));
        }
        // hostname via HOSTNAME env (windows → null) or "localhost"
        #[cfg(windows)]
        let hostname: Option<std::ffi::OsString> = None;
        #[cfg(not(windows))]
        let hostname: Option<std::ffi::OsString> = std::env::var_os("HOSTNAME");

        // PORT-NOTE (§10.4): Zig builds a `std.Uri{ scheme:"file", host:.{.raw=hostname},
        // path:.{.raw=path} }` and formats it into `ctlseqs::osc7` (`"\x1b]7;{f}\x1b\\"`).
        // The `.raw` components are emitted VERBATIM (no percent-encoding), so the URI is
        // `file://<hostname><path>` written byte-for-byte. Used by neither list_themes nor boo.
        tty.write_all(b"\x1b]7;")?;
        tty.write_all(b"file://")?;
        match hostname {
            Some(ref h) => tty.write_all(h.as_encoded_bytes())?,
            None => tty.write_all(b"localhost")?,
        }
        tty.write_all(path)?;
        tty.write_all(b"\x1b\\")?;
        tty.flush()?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Module-local helpers (the ctlseqs format-template application seam, §5).
// ---------------------------------------------------------------------------

/// The kitty-graphics placement emission (§6.2 image block) — identical in `render` and
/// `pretty_print`. `Placement` is a plain value (no zigimg).
fn emit_image<W: Write>(tty: &mut W, img: Placement) -> io::Result<()> {
    // ctlseqs::kitty_graphics_preamble = "\x1b_Ga=p,i={d}"
    write!(tty, "\x1b_Ga=p,i={}", img.img_id)?;
    if let Some(offset) = img.options.pixel_offset {
        write!(tty, ",X={},Y={}", offset.x, offset.y)?;
    }
    if let Some(clip) = img.options.clip_region {
        if let Some(x) = clip.x {
            write!(tty, ",x={}", x)?;
        }
        if let Some(y) = clip.y {
            write!(tty, ",y={}", y)?;
        }
        if let Some(width) = clip.width {
            write!(tty, ",w={}", width)?;
        }
        if let Some(height) = clip.height {
            write!(tty, ",h={}", height)?;
        }
    }
    if let Some(size) = img.options.size {
        if let Some(rows) = size.rows {
            write!(tty, ",r={}", rows)?;
        }
        if let Some(cols) = size.cols {
            write!(tty, ",c={}", cols)?;
        }
    }
    if let Some(z) = img.options.z_index {
        write!(tty, ",z={}", z)?;
    }
    // ctlseqs::kitty_graphics_closing = ",C=1\x1b\\"
    tty.write_all(ctlseqs::kitty_graphics_closing)?;
    Ok(())
}

/// The SGR minimization block (§6.2 / §7) — shared by `render` and `pretty_print` so the
/// two emit BYTE-IDENTICAL SGR sequences for identical input (fidelity win; one source for
/// the SGR bytes). Emits only on change vs the tracked `cursor` Style.
fn emit_style_diff<W: Write>(
    tty: &mut W,
    cursor: &Style,
    style: Style,
    sgr: Sgr,
    enable_workarounds: bool,
) -> io::Result<()> {
    // foreground
    if !Color::eql(cursor.fg, style.fg) {
        match style.fg {
            Color::Default => tty.write_all(ctlseqs::fg_reset)?, // "\x1b[39m"
            Color::Index(idx) => match idx {
                0..=7 => write!(tty, "\x1b[3{}m", idx)?, // fg_base "\x1b[3{d}m"
                8..=15 => write!(tty, "\x1b[9{}m", idx - 8)?, // fg_bright "\x1b[9{d}m"
                _ => match sgr {
                    Sgr::Standard => write!(tty, "\x1b[38:5:{}m", idx)?, // fg_indexed
                    Sgr::Legacy => write!(tty, "\x1b[38;5;{}m", idx)?,   // fg_indexed_legacy
                },
            },
            Color::Rgb(rgb) => match sgr {
                // fg_rgb "\x1b[38:2:{d}:{d}:{d}m"
                Sgr::Standard => write!(tty, "\x1b[38:2:{}:{}:{}m", rgb[0], rgb[1], rgb[2])?,
                // fg_rgb_legacy "\x1b[38;2;{d};{d};{d}m"
                Sgr::Legacy => write!(tty, "\x1b[38;2;{};{};{}m", rgb[0], rgb[1], rgb[2])?,
            },
        }
    }
    // background
    if !Color::eql(cursor.bg, style.bg) {
        match style.bg {
            Color::Default => tty.write_all(ctlseqs::bg_reset)?, // "\x1b[49m"
            Color::Index(idx) => match idx {
                0..=7 => write!(tty, "\x1b[4{}m", idx)?, // bg_base "\x1b[4{d}m"
                8..=15 => write!(tty, "\x1b[10{}m", idx - 8)?, // bg_bright "\x1b[10{d}m"
                _ => match sgr {
                    Sgr::Standard => write!(tty, "\x1b[48:5:{}m", idx)?, // bg_indexed
                    Sgr::Legacy => write!(tty, "\x1b[48;5;{}m", idx)?,   // bg_indexed_legacy
                },
            },
            Color::Rgb(rgb) => match sgr {
                Sgr::Standard => write!(tty, "\x1b[48:2:{}:{}:{}m", rgb[0], rgb[1], rgb[2])?,
                Sgr::Legacy => write!(tty, "\x1b[48;2;{};{};{}m", rgb[0], rgb[1], rgb[2])?,
            },
        }
    }
    // underline color
    if !Color::eql(cursor.ul, style.ul) {
        match style.ul {
            Color::Default => tty.write_all(ctlseqs::ul_reset)?, // "\x1b[59m"
            Color::Index(idx) => match sgr {
                Sgr::Standard => write!(tty, "\x1b[58:5:{}m", idx)?, // ul_indexed
                Sgr::Legacy => write!(tty, "\x1b[58;5;{}m", idx)?,   // ul_indexed_legacy
            },
            Color::Rgb(rgb) => {
                if enable_workarounds {
                    // ul_rgb_conpty "\x1b[58:2::{d}:{d}:{d}m"
                    write!(tty, "\x1b[58:2::{}:{}:{}m", rgb[0], rgb[1], rgb[2])?;
                } else {
                    match sgr {
                        Sgr::Standard => {
                            write!(tty, "\x1b[58:2:{}:{}:{}m", rgb[0], rgb[1], rgb[2])?;
                            // ul_rgb
                        }
                        Sgr::Legacy => {
                            write!(tty, "\x1b[58;2;{};{};{}m", rgb[0], rgb[1], rgb[2])?;
                            // ul_rgb_legacy
                        }
                    }
                }
            }
        }
    }
    // underline style
    if cursor.ul_style != style.ul_style {
        let seq = match style.ul_style {
            crate::cell::Underline::Off => ctlseqs::ul_off,
            crate::cell::Underline::Single => ctlseqs::ul_single,
            crate::cell::Underline::Double => ctlseqs::ul_double,
            crate::cell::Underline::Curly => ctlseqs::ul_curly,
            crate::cell::Underline::Dotted => ctlseqs::ul_dotted,
            crate::cell::Underline::Dashed => ctlseqs::ul_dashed,
        };
        tty.write_all(seq)?;
    }
    // bold
    if cursor.bold != style.bold {
        let seq = if style.bold {
            ctlseqs::bold_set
        } else {
            ctlseqs::bold_dim_reset
        };
        tty.write_all(seq)?;
        if style.dim {
            tty.write_all(ctlseqs::dim_set)?;
        }
    }
    // dim
    if cursor.dim != style.dim {
        let seq = if style.dim {
            ctlseqs::dim_set
        } else {
            ctlseqs::bold_dim_reset
        };
        tty.write_all(seq)?;
        if style.bold {
            tty.write_all(ctlseqs::bold_set)?;
        }
    }
    // dim
    if cursor.italic != style.italic {
        let seq = if style.italic {
            ctlseqs::italic_set
        } else {
            ctlseqs::italic_reset
        };
        tty.write_all(seq)?;
    }
    // dim
    if cursor.blink != style.blink {
        let seq = if style.blink {
            ctlseqs::blink_set
        } else {
            ctlseqs::blink_reset
        };
        tty.write_all(seq)?;
    }
    // reverse
    if cursor.reverse != style.reverse {
        let seq = if style.reverse {
            ctlseqs::reverse_set
        } else {
            ctlseqs::reverse_reset
        };
        tty.write_all(seq)?;
    }
    // invisible
    if cursor.invisible != style.invisible {
        let seq = if style.invisible {
            ctlseqs::invisible_set
        } else {
            ctlseqs::invisible_reset
        };
        tty.write_all(seq)?;
    }
    // strikethrough
    if cursor.strikethrough != style.strikethrough {
        let seq = if style.strikethrough {
            ctlseqs::strikethrough_set
        } else {
            ctlseqs::strikethrough_reset
        };
        tty.write_all(seq)?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn transmit_local_image_path_inner<W: Write>(
    tty: &mut W,
    id: u32,
    payload: &[u8],
    width: u16,
    height: u16,
    medium: TransmitMedium,
    format: TransmitFormat,
) -> Result<Image, ImageError> {
    // base64Encoder.calcSize(n) = ((n + 2) / 3) * 4  (== n.div_ceil(3) * 4)
    let size = payload.len().div_ceil(3) * 4;
    if size >= 4096 {
        return Err(ImageError::PathTooLong);
    }

    let encoded = base64_standard_encode(payload);

    let medium_char: u8 = match medium {
        TransmitMedium::File => b'f',
        TransmitMedium::TempFile => b't',
        TransmitMedium::SharedMem => b's',
    };

    match format {
        TransmitFormat::Rgb => {
            // "\x1b_Gf=24,s={d},v={d},i={d},t={c};{s}\x1b\\"
            write!(
                tty,
                "\x1b_Gf=24,s={},v={},i={},t={};",
                width, height, id, medium_char as char
            )?;
            tty.write_all(&encoded)?;
            tty.write_all(b"\x1b\\")?;
        }
        TransmitFormat::Rgba => {
            // "\x1b_Gf=32,s={d},v={d},i={d},t={c};{s}\x1b\\"
            write!(
                tty,
                "\x1b_Gf=32,s={},v={},i={},t={};",
                width, height, id, medium_char as char
            )?;
            tty.write_all(&encoded)?;
            tty.write_all(b"\x1b\\")?;
        }
        TransmitFormat::Png => {
            // "\x1b_Gf=100,i={d},t={c};{s}\x1b\\"
            write!(tty, "\x1b_Gf=100,i={},t={};", id, medium_char as char)?;
            tty.write_all(&encoded)?;
            tty.write_all(b"\x1b\\")?;
        }
    }

    tty.flush()?;
    Ok(Image { id, width, height })
}

fn transmit_pre_encoded_image_inner<W: Write>(
    tty: &mut W,
    id: u32,
    bytes: &[u8],
    width: u16,
    height: u16,
    format: TransmitFormat,
) -> Result<Image, ImageError> {
    let fmt: u8 = match format {
        TransmitFormat::Rgb => 24,
        TransmitFormat::Rgba => 32,
        TransmitFormat::Png => 100,
    };

    if bytes.len() < 4096 {
        // "\x1b_Gf={d},s={d},v={d},i={d};{s}\x1b\\"
        write!(tty, "\x1b_Gf={},s={},v={},i={};", fmt, width, height, id)?;
        tty.write_all(bytes)?;
        tty.write_all(b"\x1b\\")?;
    } else {
        let n0: usize = 4096;

        // "\x1b_Gf={d},s={d},v={d},i={d},m=1;{s}\x1b\\"
        write!(
            tty,
            "\x1b_Gf={},s={},v={},i={},m=1;",
            fmt, width, height, id
        )?;
        tty.write_all(&bytes[0..n0])?;
        tty.write_all(b"\x1b\\")?;
        let mut n = n0;
        while n < bytes.len() {
            let end: usize = std::cmp::min(n + 4096, bytes.len());
            let m: u8 = if end == bytes.len() { 0 } else { 1 };
            // "\x1b_Gm={d};{s}\x1b\\"
            write!(tty, "\x1b_Gm={};", m)?;
            tty.write_all(&bytes[n..end])?;
            tty.write_all(b"\x1b\\")?;
            n += 4096;
        }
    }

    tty.flush()?;
    Ok(Image { id, width, height })
}

/// `@tagName(Mouse.Shape)` for `ctlseqs::osc22_mouse_shape` (§5.2). Returns Zig's EXACT
/// tag bytes — note the kebab `ew-resize`/`ns-resize` (Zig's `@"ew-resize"`/`@"ns-resize"`).
fn shape_tag_name(shape: Shape) -> &'static [u8] {
    match shape {
        Shape::Default => b"default",
        Shape::Text => b"text",
        Shape::Pointer => b"pointer",
        Shape::Help => b"help",
        Shape::Progress => b"progress",
        Shape::Wait => b"wait",
        Shape::EwResize => b"ew-resize",
        Shape::NsResize => b"ns-resize",
        Shape::Cell => b"cell",
    }
}

/// `@intFromEnum(Cell.CursorShape)` (§5.3) — declaration order `Default=0 … Beam=6`.
fn cursor_shape_int(shape: CursorShape) -> u8 {
    match shape {
        CursorShape::Default => 0,
        CursorShape::BlockBlink => 1,
        CursorShape::Block => 2,
        CursorShape::UnderlineBlink => 3,
        CursorShape::Underline => 4,
        CursorShape::BeamBlink => 5,
        CursorShape::Beam => 6,
    }
}

/// `@bitCast(KittyFlags) -> u5` (§8.3) — LSB-first pack: `disambiguate0 report_events1
/// report_alternate_keys2 report_all_as_ctl_seqs3 report_text4`.
fn kitty_flags_bits(flags: KittyFlags) -> u8 {
    (flags.disambiguate as u8)
        | ((flags.report_events as u8) << 1)
        | ((flags.report_alternate_keys as u8) << 2)
        | ((flags.report_all_as_ctl_seqs as u8) << 3)
        | ((flags.report_text as u8) << 4)
}

/// A faithful port of `std.base64.standard.Encoder` (§5.4): standard alphabet
/// `A–Za–z0–9+/`, `=` padding, `calcSize(n) = ((n + 2) / 3) * 4`. No external dep.
fn base64_standard_encode(source: &[u8]) -> Vec<u8> {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    const PAD: u8 = b'=';
    // calcSize(n) = ((n + 2) / 3) * 4  (== n.div_ceil(3) * 4)
    let out_len = source.len().div_ceil(3) * 4;
    let mut dest = Vec::with_capacity(out_len);
    let mut chunks = source.chunks_exact(3);
    for chunk in chunks.by_ref() {
        let n = ((chunk[0] as u32) << 16) | ((chunk[1] as u32) << 8) | (chunk[2] as u32);
        dest.push(ALPHABET[((n >> 18) & 0x3f) as usize]);
        dest.push(ALPHABET[((n >> 12) & 0x3f) as usize]);
        dest.push(ALPHABET[((n >> 6) & 0x3f) as usize]);
        dest.push(ALPHABET[(n & 0x3f) as usize]);
    }
    let rem = chunks.remainder();
    match rem.len() {
        1 => {
            let n = (rem[0] as u32) << 16;
            dest.push(ALPHABET[((n >> 18) & 0x3f) as usize]);
            dest.push(ALPHABET[((n >> 12) & 0x3f) as usize]);
            dest.push(PAD);
            dest.push(PAD);
        }
        2 => {
            let n = ((rem[0] as u32) << 16) | ((rem[1] as u32) << 8);
            dest.push(ALPHABET[((n >> 18) & 0x3f) as usize]);
            dest.push(ALPHABET[((n >> 12) & 0x3f) as usize]);
            dest.push(ALPHABET[((n >> 6) & 0x3f) as usize]);
            dest.push(PAD);
        }
        _ => {}
    }
    dest
}

// ---------------------------------------------------------------------------
// The ADDED render-to-`Vec<u8>` smoke tests (§12). Vaxis.zig has 0 `test{}`; these are the
// render engine's executable fidelity pins (the workflow-mandated substitute). Miri-clean
// (pure over `Vec<u8>`, no FFI).
// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;

    fn contains(haystack: &[u8], needle: &[u8]) -> bool {
        haystack.windows(needle.len()).any(|w| w == needle)
    }

    #[test]
    fn base64_matches_std() {
        // spot-check the encoder against known vectors (RFC 4648).
        assert_eq!(base64_standard_encode(b""), b"");
        assert_eq!(base64_standard_encode(b"f"), b"Zg==");
        assert_eq!(base64_standard_encode(b"fo"), b"Zm8=");
        assert_eq!(base64_standard_encode(b"foo"), b"Zm9v");
        assert_eq!(base64_standard_encode(b"foob"), b"Zm9vYg==");
        assert_eq!(base64_standard_encode(b"fooba"), b"Zm9vYmE=");
        assert_eq!(base64_standard_encode(b"foobar"), b"Zm9vYmFy");
    }

    #[test]
    fn render_emits_minimal_escapes() {
        // 2x1 primary-screen Vaxis: build screen + screen_last same size (render asserts equal len)
        let mut vx = Vaxis::init(Options::default()); // 0x0
        vx.screen = Screen::init(Winsize {
            rows: 1,
            cols: 2,
            x_pixel: 0,
            y_pixel: 0,
        });
        vx.screen_last = InternalScreen::init(2, 1);
        // write a known cell 'A' (default=false) at (0,0); (1,0) stays default space
        vx.screen.write_cell(
            0,
            0,
            Cell {
                char: Character {
                    grapheme: b"A",
                    width: 1,
                },
                default: false,
                ..Default::default()
            },
        );
        let mut out: Vec<u8> = Vec::new();
        vx.render(&mut out).unwrap();

        // Prologue (primary screen, cursor.row=0): sync_set, hide_cursor, "\r", sgr_reset
        assert!(out.starts_with(b"\x1b[?2026h\x1b[?25l\r\x1b[m"));
        // the changed cell emits 'A' (grapheme, write_all — raw byte)
        assert!(contains(&out, b"A"));
        // epilogue: sync_reset + (flush no-op on Vec)
        assert!(out.ends_with(b"\x1b[?2026l"));

        // 2nd render with NO change emits NO 'A' (diff suppresses it) — proves the diff works
        let mut out2 = Vec::new();
        vx.render(&mut out2).unwrap();
        assert!(!contains(&out2, b"A"));
    }

    #[test]
    fn render_emits_sgr_on_color_change() {
        let mut vx = Vaxis::init(Options::default());
        vx.screen = Screen::init(Winsize {
            rows: 1,
            cols: 2,
            x_pixel: 0,
            y_pixel: 0,
        });
        vx.screen_last = InternalScreen::init(2, 1);
        let style = Style {
            fg: Color::Index(1),
            ..Default::default()
        };
        vx.screen.write_cell(
            0,
            0,
            Cell {
                char: Character {
                    grapheme: b"X",
                    width: 1,
                },
                style,
                default: false,
                ..Default::default()
            },
        );
        let mut out: Vec<u8> = Vec::new();
        vx.render(&mut out).unwrap();
        // fg_base for index 1 (0..=7) → "\x1b[31m"
        assert!(contains(&out, b"\x1b[31m"));
    }
}
