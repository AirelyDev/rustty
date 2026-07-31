//! Primary terminal IO ("termio") state. This maintains the terminal state,
//! pty, subprocess, etc. This is flexible enough to be used in environments
//! that don't have a pty and simply provides the input/output using raw
//! bytes.
//!
//! PORTS: ghostty src/termio/Termio.zig (0 `test{}` blocks).
//!
//! PORT STATUS: struct shapes + self-contained logic ported (integration-tier
//! stage 2): the `Termio` struct, `MutexState`, `ThreadEnterState` (in full),
//! `DerivedConfig` (in full), `ThreadData`, and the pure delegation methods.
//! The mutex-only IO-thread methods (`changeConfig`/`resize`/`sizeReport*`/
//! `resetSynchronizedOutput`/`clearScreen`/`scrollViewport`/`jumpToPrompt`/
//! `focusGained`/`colorSchemeReport*`) are ported LIVE (core C-T) against the
//! guard-less C0 `rustty_zigstd::thread::Mutex`; the xev `renderer_wakeup.
//! notify()` is the real `Async::notify` leaf (rustty-xev). `processOutput`/
//! `processOutputLocked` landed in xev B3 (RAW `*mut Termio<M>`, [ADV-FIX F2]);
//! `init` (in-place out-param, [ADV-FIX F1]) and `threadEnter` (RAW `*mut`,
//! [ADV-FIX F2]) landed in xev B4 — the writer thread now drives the live xev
//! loop end-to-end. See .port/phase-2/XEV_MILESTONE_B_BLUEPRINT.md.

// PARITY: the reference's identifiers are camelCase (`queueMessage`,
// `threadEnter`, ...); preserved verbatim per R1/R2.
#![allow(non_snake_case)]

// pub const Termio = @This();
//
// const std = @import("std");
// const assert = @import("../quirks.zig").inlineAssert;
// const Allocator = std.mem.Allocator;
// const ArenaAllocator = std.heap.ArenaAllocator;
// const EnvMap = std.process.EnvMap;
// const posix = std.posix;
// const termio = @import("../termio.zig");
// const StreamHandler = @import("stream_handler.zig").StreamHandler;
// const terminalpkg = @import("../terminal/main.zig");
// const xev = @import("../global.zig").xev;
// const renderer = @import("../renderer.zig");
// const apprt = @import("../apprt.zig");
// const internal_os = @import("../os/main.zig");
// const windows = internal_os.windows;
// const configpkg = @import("../config.zig");
// const ProcessInfo = @import("../pty.zig").ProcessInfo;
//
// PORT NOTE: `assert`/`EnvMap`/`posix`/`windows` are imported by the reference
// but only used by paths deferred with the bodies below (or not at all in this
// file); no `use` is bound for them. `StreamHandler` landed with stage 4 (see
// the `StreamHandlerStream` alias below). `ProcessInfo` maps to the rustty-os
// `pty.rs` split (see `getForegroundPid`/`getTtyName`).
use allocator_api2::alloc::Global as Allocator;
use rustty_apprt_api::surface::AppMailbox;
use rustty_config::conditional;
use rustty_config::io::RepeatableReadableIO;
use rustty_config::value_types::{ClipboardAccess, Color, OSCColorReportFormat, TerminalColor};
use rustty_config::Config;
use rustty_datastruct::blocking_queue::Timeout;
use rustty_renderer::message::Message as RendererMessage;
use rustty_renderer::size::Size;
use rustty_renderer::state::State;
use rustty_renderer::thread;
use rustty_vt::color;
use rustty_vt::cursor::Style as CursorStyle;
use rustty_vt::device_status;
use rustty_vt::focus;
use rustty_vt::kitty::graphics_image::Limits;
use rustty_vt::modes::Mode;
use rustty_vt::size_report;
use rustty_vt::terminal::Terminal;

use crate::backend;
use crate::mailbox::Mailbox;
use crate::message::Message;
use crate::options::Options;
use crate::stream_handler::StreamHandler;
use crate::thread::Thread;
use crate::xev_seam as xev;
use crate::Backend;
use crate::Error;

/// PORT NOTE: the process-static Zig allocator that backs the terminal
/// `Stream`'s OSC parser (`terminal_stream = .initAlloc(alloc, handler)`). The
/// reference threads the same `std.mem.Allocator` it passes to `Termio.init`;
/// Rustty's `Stream::init_alloc` wants a `ZigAllocatorRef`, so we back the OSC
/// parser with the process-static global zig allocator — the identical choice
/// `Terminal::vt_stream` and the vt-c `TerminalWrapper` make (supports dynamic
/// OSC growth, e.g. large OSC 52 clipboard payloads). `GlobalZigAllocator` is a
/// stateless ZST, so sharing the static is sound.
static GLOBAL_ZIG_ALLOCATOR_FOR_OSC: rustty_lib::allocator::GlobalZigAllocator =
    rustty_lib::allocator::GlobalZigAllocator;

// const log = std.log.scoped(.io_exec);
//
// PORT NOTE: routed through named log helpers below that preserve the reference's
// messages verbatim (rustty-os cf_release_thread precedent).

/// PORT NOTE: the reference `terminal_stream` field type `StreamHandler.Stream`
/// (= `terminal.Stream(StreamHandler)`, src/termio/stream_handler.zig — landed
/// stage 4). The vt Stream's OSC-parser allocator lifetime is `'static` here
/// (the field owns its handler for the Termio lifetime; only the deferred
/// `Termio::init` constructs it — `.initAlloc(alloc, handler)`).
pub type StreamHandlerStream<M> = crate::stream_handler::Stream<'static, M>;

/// Mutex state argument for queueMessage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MutexState {
    Locked,
    Unlocked,
}

/// Primary terminal IO ("termio") state.
//
// PORT NOTE: the reference is a top-level file-struct (`Termio.zig`); the Rust
// port names it `Termio`. `M` is the `apprt.surface.Mailbox` seam
// (`rustty_apprt_api::surface::AppMailbox`, implemented by rustty-core later).
pub struct Termio<M: AppMailbox> {
    /// Allocator
    //
    // PORT NOTE: `alloc: Allocator` mirrors rustty-vt's Terminal convention
    // (`allocator_api2::alloc::Global`), which the owned `terminal`'s
    // deinit/resize APIs take.
    pub alloc: Allocator,

    /// This is the implementation responsible for io.
    pub backend: Backend,

    /// The derived configuration for this termio implementation.
    pub config: DerivedConfig,

    /// The terminal emulator internal state. This is the abstract "terminal"
    /// that manages input, grid updating, etc. and is renderer-agnostic. It
    /// just stores internal state about a grid.
    pub terminal: Terminal,

    /// The shared render state
    //
    // PORT NOTE: reference `renderer_state: *renderer.State` -> raw `*mut`
    // (renderer state.rs precedent — shared mutable across threads).
    pub renderer_state: *mut State,

    /// A handle to wake up the renderer. This hints to the renderer that
    /// a repaint should happen.
    //
    // PORT NOTE: `xev.Async` — the D6 deferred seam (crate::xev_seam).
    pub renderer_wakeup: xev::Async,

    /// The mailbox for notifying the renderer of things.
    pub renderer_mailbox: *mut thread::Mailbox,

    /// The mailbox for communicating with the surface.
    pub surface_mailbox: M,

    /// The cached size info
    pub size: Size,

    /// The mailbox implementation to use.
    pub mailbox: Mailbox,

    /// The stream parser. This parses the stream of escape codes and so on
    /// from the child process and calls callbacks in the stream handler.
    pub terminal_stream: StreamHandlerStream<M>,

    /// Last time the cursor was reset. This is used to prevent message
    /// flooding with cursor resets.
    // (reference default: `= null`)
    pub last_cursor_reset: Option<std::time::Instant>,

    /// State we have for thread enter. This may be null if we don't need
    /// to keep track of any state or if its already been freed.
    // (reference default: `= null`)
    //
    // PORT NOTE: reference `?*ThreadEnterState` (heap pointer) -> owned
    // `Option<Box<...>>`.
    pub thread_enter_state: Option<Box<ThreadEnterState>>,
}

/// The state we need to keep around only until we enter the IO
/// thread. Then we can throw it all away.
//
// PORT NOTE: reference file-private `const ThreadEnterState = struct`; `pub`
// here because it appears in the `Termio` field shape. The `arena` field
// (which owns `input`'s parsed copies and the struct itself) collapses into
// `Box`/`Vec` ownership (crash-tier arena-collapse precedent).
pub struct ThreadEnterState {
    /// Initial input to send to the subprocess after starting. This
    /// memory is freed once the subprocess start is attempted, even
    /// if it fails, because Exec only starts once.
    pub input: RepeatableReadableIO,
}

impl ThreadEnterState {
    // PORT NOTE: reference `pub fn create(alloc: Allocator, config: *const
    // configpkg.Config) !?*ThreadEnterState` — the `alloc` param and the `!`
    // (arena OOM) collapse with the arena.
    pub fn create(config: &Config) -> Option<Box<ThreadEnterState>> {
        // If we have no input then we have no thread enter state
        if config.input.list.is_empty() {
            return None;
        }

        // Create our arena allocator
        // (PORT NOTE: collapses into the owned Box/Vec allocations below.)

        // Allocate our ThreadEnterState
        // (PORT NOTE: the `arena_alloc.create(ThreadEnterState)` is the Box below.)

        // Copy the input from the config
        let input = config.input.clone_parsed();

        // Return the initialized state
        Some(Box::new(ThreadEnterState { input }))
    }

    // PORT NOTE: reference `pub fn destroy(self: *ThreadEnterState) void {
    // self.arena.deinit(); }` — consuming drop (BlockingQueue::destroy
    // precedent).
    pub fn destroy(self: Box<ThreadEnterState>) {
        drop(self);
    }

    /// Prepare the inputs for use. Allocations happen on the arena.
    //
    // PORT NOTE: reference returns `(Allocator.Error || error{InputNotFound})!
    // []const Input`; the OOM half collapses with the arena, leaving
    // `InputNotFound`.
    pub fn prepareInput(&mut self) -> Result<Vec<Input>, PrepareInputError> {
        let mut input = Vec::with_capacity(self.input.list.len());
        for item in &self.input.list {
            input.push(match item {
                // .raw => |v| .{ .string = try alloc.dupe(u8, v) },
                rustty_config::io::ReadableIO::Raw(v) => Input::String(v.clone()),
                // .path => |path| file: { ... },
                rustty_config::io::ReadableIO::Path(path) => {
                    let f = match std::fs::File::open(os_str_from_bytes(path)) {
                        Ok(f) => f,
                        Err(err) => {
                            log_warn_input_open_failed(path, &err);
                            return Err(PrepareInputError::InputNotFound);
                        }
                    };

                    Input::File(f)
                }
            });
        }

        Ok(input)
    }
}

// PORT NOTE: reference file-private `const Input = union(enum)`; `pub` here
// because it is `prepareInput`'s element type.
pub enum Input {
    // string: []const u8,
    //
    // PORT NOTE: the reference `string` is the arena-`dupe`d byte slice ->
    // owned `Vec<u8>` (R1: byte slices stay raw bytes, never a lossy String).
    String(Vec<u8>),
    // file: std.fs.File,
    File(std::fs::File),
}

/// The error set of [`ThreadEnterState::prepareInput`] (see its PORT NOTE).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrepareInputError {
    InputNotFound,
}

/// The configuration for this IO that is derived from the main
/// configuration. This must be exported so that we don't need to
/// pass around Config pointers which makes memory management a pain.
pub struct DerivedConfig {
    // PORT NOTE: reference first field `arena: ArenaAllocator` (which owns
    // `enquiry_response`) collapses into the owned `Vec` below (crash-tier
    // arena-collapse precedent).
    pub palette: color::Palette,
    pub image_storage_limit: usize,
    pub cursor_style: CursorStyle,
    pub cursor_blink: Option<bool>,
    pub cursor_color: Option<TerminalColor>,
    pub foreground: Color,
    pub background: Color,
    pub osc_color_report_format: OSCColorReportFormat,
    pub clipboard_write: ClipboardAccess,
    // PORT NOTE: `enquiry_response: []const u8` -> owned `Vec<u8>` (R1 raw
    // bytes; arena-`dupe`d in the reference).
    pub enquiry_response: Vec<u8>,
    pub conditional_state: conditional::State,
}

impl DerivedConfig {
    // PORT NOTE: reference `pub fn init(alloc_gpa: Allocator, config: *const
    // configpkg.Config) !DerivedConfig` — the `alloc_gpa` param and the `!`
    // (arena/dupe OOM) collapse with the arena.
    pub fn init(config: &Config) -> DerivedConfig {
        // var arena = ArenaAllocator.init(alloc_gpa);
        // errdefer arena.deinit();
        // const alloc = arena.allocator();
        // (PORT NOTE: the arena collapses to Rust ownership — the `dupe` below is a
        // `Vec` clone with the global allocator.)

        let palette: color::Palette = 'palette: {
            if config.palette_generate {
                'generate: {
                    // if (config.palette.mask.findFirstSet() == null)
                    if config.palette.mask.iter_set().next().is_none() {
                        // If the user didn't set any values manually, then
                        // we're using the default palette and we don't need
                        // to apply the generation code to it.
                        break 'generate;
                    }

                    break 'palette color::generate_256_color(
                        config.palette.value,
                        config.palette.mask,
                        config.background.to_terminal_rgb(),
                        config.foreground.to_terminal_rgb(),
                        config.palette_harmonious,
                    );
                }
            }

            break 'palette config.palette.value;
        };

        DerivedConfig {
            palette,
            image_storage_limit: config.image_storage_limit as usize,
            cursor_style: config.cursor_style,
            cursor_blink: config.cursor_style_blink,
            cursor_color: config.cursor_color,
            foreground: config.foreground,
            background: config.background,
            osc_color_report_format: config.osc_color_report_format,
            clipboard_write: config.clipboard_write,
            enquiry_response: config.enquiry_response.clone(),
            conditional_state: config._conditional_state,
            // This has to be last so that we copy AFTER the arena allocations
            // above happen (Zig assigns in order).
            //
            // (PORT NOTE: the `arena` field collapsed — see the struct PORT NOTE.)
        }
    }

    // pub fn deinit(self: *DerivedConfig) void {
    //     self.arena.deinit();
    // }
    //
    // PORT NOTE: the arena's memory is the owned `Vec`, freed on `Drop`; kept
    // for API parity (renderer `Preedit::deinit` precedent).
    pub fn deinit(&mut self) {
        let _ = self;
    }
}

impl<M: AppMailbox> Termio<M> {
    /// Initialize the termio state.
    ///
    /// This will also start the child process if the termio is configured
    /// to run a child process.
    //
    // DEFERRED (stage-4 stream_handler + D6, 0 tests — R1-safe): the body
    // constructs the `StreamHandler` (stage 4) and threads the xev handles
    // through; preserved verbatim (the reference `init(self: *Termio, ...)
    // !void` out-pointer becomes the Rust constructor return — vt
    // `Terminal::init` precedent):
    //
    // pub fn init(self: *Termio, alloc: Allocator, opts: termio.Options) !void {
    //     // The default terminal modes based on our config.
    //     const default_modes: terminalpkg.ModePacked = modes: {
    //         var modes: terminalpkg.ModePacked = .{};
    //
    //         // Setup our initial grapheme cluster support if enabled. We use a
    //         // switch to ensure we get a compiler error if more cases are added.
    //         switch (opts.full_config.@"grapheme-width-method") {
    //             .unicode => modes.grapheme_cluster = true,
    //             .legacy => {},
    //         }
    //
    //         // Set default cursor blink settings
    //         modes.cursor_blinking = opts.config.cursor_blink orelse true;
    //
    //         break :modes modes;
    //     };
    //
    //     // Create our terminal
    //     var term = try terminalpkg.Terminal.init(alloc, opts: {
    //         const grid_size = opts.size.grid();
    //         break :opts .{
    //             .cols = grid_size.columns,
    //             .rows = grid_size.rows,
    //             .max_scrollback_bytes = opts.full_config.@"scrollback-limit-bytes".optional(),
    //             .max_scrollback_lines = opts.full_config.@"scrollback-limit-lines".optional(),
    //             .default_modes = default_modes,
    //             .colors = .{
    //                 .background = .init(opts.config.background.toTerminalRGB()),
    //                 .foreground = .init(opts.config.foreground.toTerminalRGB()),
    //                 .cursor = cursor: {
    //                     const color = opts.config.cursor_color orelse break :cursor .unset;
    //                     const rgb = color.toTerminalRGB() orelse break :cursor .unset;
    //                     break :cursor .init(rgb);
    //                 },
    //                 .palette = .init(opts.config.palette),
    //             },
    //             .kitty_image_storage_limit = opts.config.image_storage_limit,
    //             .kitty_image_loading_limits = .all,
    //         };
    //     });
    //     errdefer term.deinit(alloc);
    //
    //     // Set our default cursor style
    //     term.screens.active.cursor.cursor_style = opts.config.cursor_style;
    //
    //     // Setup our terminal size in pixels for certain requests.
    //     term.width_px = term.cols * opts.size.cell.width;
    //     term.height_px = term.rows * opts.size.cell.height;
    //
    //     // Setup our backend.
    //     var backend = opts.backend;
    //     backend.initTerminal(&term);
    //
    //     // Create our stream handler. This points to memory in self so it
    //     // isn't safe to use until self.* is set.
    //     const handler: StreamHandler = .{
    //         .alloc = alloc,
    //         .termio_mailbox = &self.mailbox,
    //         .surface_mailbox = opts.surface_mailbox,
    //         .renderer_state = opts.renderer_state,
    //         .renderer_wakeup = opts.renderer_wakeup,
    //         .renderer_mailbox = opts.renderer_mailbox,
    //         .size = &self.size,
    //         .terminal = &self.terminal,
    //         .osc_color_report_format = opts.config.osc_color_report_format,
    //         .clipboard_write = opts.config.clipboard_write,
    //         .enquiry_response = opts.config.enquiry_response,
    //         .default_cursor_style = opts.config.cursor_style,
    //         .default_cursor_blink = opts.config.cursor_blink,
    //     };
    //
    //     const thread_enter_state = try ThreadEnterState.create(
    //         alloc,
    //         opts.full_config,
    //     );
    //
    //     self.* = .{
    //         .alloc = alloc,
    //         .terminal = term,
    //         .config = opts.config,
    //         .renderer_state = opts.renderer_state,
    //         .renderer_wakeup = opts.renderer_wakeup,
    //         .renderer_mailbox = opts.renderer_mailbox,
    //         .surface_mailbox = opts.surface_mailbox,
    //         .size = opts.size,
    //         .backend = backend,
    //         .mailbox = opts.mailbox,
    //         .terminal_stream = .initAlloc(alloc, handler),
    //         .thread_enter_state = thread_enter_state,
    //     };
    // }
    //
    // PORTED (xev B4, [ADV-FIX F1], 0 reference tests — R1-safe). The reference
    // `init(self: *Termio, ...) !void` is an IN-PLACE out-param initializer — the
    // faithful Rust form is `unsafe fn init(self_: *mut Termio<M>, ...)`, NOT a
    // by-value return. This matters: `StreamHandler` holds raw self-pointers back
    // into the owning Termio (`terminal`/`size`/`termio_mailbox` =
    // `&self.terminal`/`&self.size`/`&self.mailbox`, stream_handler.rs:95/97/101).
    // A by-value return + a caller `ptr::write` would copy those pointer *values*
    // but leave them aimed at the OLD return slot → dangling. Here we `ptr::write`
    // every field into `*self_` at its FINAL address, THEN build the handler with
    // pointers into `*self_`, so the self-references are valid for the Termio's
    // life (matches the reference's "points to memory in self so it isn't safe to
    // use until self.* is set" + the surface's own `renderer_state.terminal =
    // &raw mut (*self_).io.terminal` precedent). `M: Copy` is Rust-only glue: the
    // reference copies `opts.surface_mailbox` by value into BOTH `self.*` and the
    // handler (`apprt.surface.Mailbox` is a trivially-copyable value type —
    // `SurfaceMailbox<A>` is `Copy`).
    ///
    /// # Safety
    /// `self_` must point at uninitialized (or otherwise overwritable)
    /// `Termio<M>`-sized, aligned storage; on `Ok` every field is initialized and
    /// the handler self-pointers are valid for the Termio's lifetime. On `Err`
    /// nothing is written (the sole fallible step, `Terminal::init`, runs before
    /// any write).
    pub unsafe fn init(
        self_: *mut Termio<M>,
        alloc: Allocator,
        opts: Options<'_, M>,
    ) -> Result<(), Error>
    where
        M: Copy + 'static,
    {
        // The default terminal modes based on our config.
        // const default_modes: terminalpkg.ModePacked = modes: {
        //     var modes: terminalpkg.ModePacked = .{};
        //     switch (opts.full_config.@"grapheme-width-method") {
        //         .unicode => modes.grapheme_cluster = true,
        //         .legacy => {},
        //     }
        //     modes.cursor_blinking = opts.config.cursor_blink orelse true;
        //     break :modes modes;
        // };
        //
        // (PORT NOTE: the packed-bit-set field writes `modes.grapheme_cluster = true`
        // / `modes.cursor_blinking = ...` use the public `ModeState::set` +
        // `.values` path since `ModePacked`'s per-field bit helpers are private —
        // the `terminal_full_reset_default_modes` test precedent.)
        let default_modes: rustty_vt::modes::ModePacked = {
            let mut modes = rustty_vt::modes::ModeState::default();

            // Setup our initial grapheme cluster support if enabled. We use a
            // switch to ensure we get a compiler error if more cases are added.
            match opts.full_config.grapheme_width_method {
                rustty_config::value_types::GraphemeWidthMethod::Unicode => {
                    modes.set(rustty_vt::modes::Mode::GraphemeCluster, true);
                }
                rustty_config::value_types::GraphemeWidthMethod::Legacy => {}
            }

            // Set default cursor blink settings
            modes.set(
                rustty_vt::modes::Mode::CursorBlinking,
                opts.config.cursor_blink.unwrap_or(true),
            );

            modes.values
        };

        // Create our terminal
        // var term = try terminalpkg.Terminal.init(alloc, opts: { ... });
        let grid_size = opts.size.grid();
        let mut term = Terminal::init(
            alloc,
            rustty_vt::terminal::Options {
                cols: grid_size.columns,
                rows: grid_size.rows,
                max_scrollback_bytes: opts.full_config.scrollback_limit_bytes.optional(),
                max_scrollback_lines: opts.full_config.scrollback_limit_lines.optional(),
                default_modes,
                colors: rustty_vt::terminal::Colors {
                    // .background = .init(opts.config.background.toTerminalRGB()),
                    background: color::DynamicRGB::init(opts.config.background.to_terminal_rgb()),
                    // .foreground = .init(opts.config.foreground.toTerminalRGB()),
                    foreground: color::DynamicRGB::init(opts.config.foreground.to_terminal_rgb()),
                    // .cursor = cursor: {
                    //     const color = opts.config.cursor_color orelse break :cursor .unset;
                    //     const rgb = color.toTerminalRGB() orelse break :cursor .unset;
                    //     break :cursor .init(rgb);
                    // },
                    cursor: 'cursor: {
                        let Some(cursor_color) = opts.config.cursor_color else {
                            break 'cursor color::DynamicRGB::UNSET;
                        };
                        let Some(rgb) = cursor_color.to_terminal_rgb() else {
                            break 'cursor color::DynamicRGB::UNSET;
                        };
                        color::DynamicRGB::init(rgb)
                    },
                    // .palette = .init(opts.config.palette),
                    palette: color::DynamicPalette::init(opts.config.palette),
                },
                kitty_image_storage_limit: opts.config.image_storage_limit,
                // .kitty_image_loading_limits = .all,
                kitty_image_loading_limits: Limits::ALL,
            },
        )
        // (PORT NOTE: `Terminal::init`'s `OutOfMemory` maps to the opaque termio
        // `crate::Error` — the reference `try` bubbles the alloc failure.)
        .map_err(|_| Error)?;
        // errdefer term.deinit(alloc);
        // (PORT NOTE: `Terminal::init` is the sole fallible step and runs BEFORE any
        // write into `*self_`; on error we return before writing, so no
        // partial-init cleanup is needed — the errdefer collapses.)

        // Set our default cursor style
        // term.screens.active.cursor.cursor_style = opts.config.cursor_style;
        //
        // SAFETY: `term.screens.active` is the live active-screen pointer just
        // returned by `Terminal::init`.
        unsafe { (*term.screens.active).cursor.cursor_style = opts.config.cursor_style };

        // Setup our terminal size in pixels for certain requests.
        // term.width_px = term.cols * opts.size.cell.width;
        // term.height_px = term.rows * opts.size.cell.height;
        term.width_px = term.cols as u32 * opts.size.cell.width;
        term.height_px = term.rows as u32 * opts.size.cell.height;

        // Setup our backend.
        // var backend = opts.backend;
        // backend.initTerminal(&term);
        let mut backend = opts.backend;
        backend.initTerminal(&mut term);

        // (PORT NOTE: read every `opts.config.*` scalar the handler needs BEFORE the
        // `opts.config` value is moved into `self.config` below — Zig copies the
        // fields by value; Rust must not read a moved-out value. `enquiry_response`
        // is the one owned field, cloned as the handler's owned copy — see the
        // StreamHandler field PORT NOTE.)
        let handler_osc = opts.config.osc_color_report_format;
        let handler_clipboard_write = opts.config.clipboard_write;
        let handler_enquiry_response = opts.config.enquiry_response.clone();
        let handler_cursor_style = opts.config.cursor_style;
        let handler_cursor_blink = opts.config.cursor_blink;

        // (PORT NOTE: `opts.surface_mailbox`/`opts.renderer_wakeup`/`opts.renderer_state`
        // /`opts.renderer_mailbox`/`alloc` are all `Copy` — the reference uses each
        // in BOTH `self.*` and the handler; the `M: Copy` bound makes the double
        // use a copy, not a move.)
        let surface_mailbox = opts.surface_mailbox;
        let renderer_state = opts.renderer_state;
        let renderer_wakeup = opts.renderer_wakeup;
        let renderer_mailbox = opts.renderer_mailbox;

        // const thread_enter_state = try ThreadEnterState.create(alloc, opts.full_config);
        // (PORT NOTE: `ThreadEnterState::create` is infallible — the arena OOM half of
        // the reference `!` collapsed; returns `Option<Box<ThreadEnterState>>`.)
        let thread_enter_state = ThreadEnterState::create(opts.full_config);

        // self.* = .{ ... };
        //
        // (PORT NOTE: the reference `self.* = .{...}` overwrites the out-pointer's
        // storage in one shot; Rust writes each field via `ptr::write` into the
        // uninitialized `*self_` — no old value is dropped, and every field is
        // written exactly once. `terminal`/`size`/`mailbox` are written FIRST so
        // the handler below can take their FINAL addresses.)
        //
        // SAFETY: `self_` is caller-provided overwritable `Termio<M>` storage; each
        // `write` targets a distinct field and none is read before it is written.
        unsafe {
            (&raw mut (*self_).alloc).write(alloc);
            (&raw mut (*self_).backend).write(backend);
            (&raw mut (*self_).config).write(opts.config);
            (&raw mut (*self_).terminal).write(term);
            (&raw mut (*self_).renderer_state).write(renderer_state);
            (&raw mut (*self_).renderer_wakeup).write(renderer_wakeup);
            (&raw mut (*self_).renderer_mailbox).write(renderer_mailbox);
            (&raw mut (*self_).surface_mailbox).write(surface_mailbox);
            (&raw mut (*self_).size).write(opts.size);
            (&raw mut (*self_).mailbox).write(opts.mailbox);
            (&raw mut (*self_).last_cursor_reset).write(None);
            (&raw mut (*self_).thread_enter_state).write(thread_enter_state);
        }

        // Create our stream handler. This points to memory in self so it
        // isn't safe to use until self.* is set.
        // const handler: StreamHandler = .{ ... };
        //
        // SAFETY: the three self-pointers (`termio_mailbox`/`size`/`terminal`) are
        // taken AFTER the corresponding fields are `ptr::write`n above, so they
        // point at the live, final Termio storage (the whole point of [ADV-FIX F1]).
        let handler: StreamHandler<M> = StreamHandler {
            // .alloc = alloc,
            alloc,
            // .termio_mailbox = &self.mailbox,
            termio_mailbox: unsafe { &raw mut (*self_).mailbox },
            // .surface_mailbox = opts.surface_mailbox,
            surface_mailbox,
            // .renderer_state = opts.renderer_state,
            renderer_state,
            // .renderer_wakeup = opts.renderer_wakeup,
            renderer_wakeup,
            // .renderer_mailbox = opts.renderer_mailbox,
            renderer_mailbox,
            // .size = &self.size,
            size: unsafe { &raw mut (*self_).size },
            // .terminal = &self.terminal,
            terminal: unsafe { &raw mut (*self_).terminal },
            // .osc_color_report_format = opts.config.osc_color_report_format,
            osc_color_report_format: handler_osc,
            // .clipboard_write = opts.config.clipboard_write,
            clipboard_write: handler_clipboard_write,
            // .enquiry_response = opts.config.enquiry_response,
            enquiry_response: handler_enquiry_response,
            // .default_cursor_style = opts.config.cursor_style,
            default_cursor_style: handler_cursor_style,
            // .default_cursor_blink = opts.config.cursor_blink,
            default_cursor_blink: handler_cursor_blink,
            // (reference field defaults for the handler fields not in the literal:
            // `default_cursor = true`, `apc = .{}`, `dcs = .{}`,
            // `tmux_viewer = null`, `termio_messaged = false`, `seen_title = false`.)
            default_cursor: true,
            apc: rustty_vt::apc::Handler::default(),
            dcs: rustty_vt::dcs::Handler::default(),
            tmux_viewer: None,
            termio_messaged: false,
            seen_title: false,
        };

        // .terminal_stream = .initAlloc(alloc, handler),
        //
        // SAFETY: `terminal_stream` is the last unwritten field of `*self_`; the
        // handler it takes references the already-written self fields above.
        let terminal_stream: StreamHandlerStream<M> = crate::stream_handler::Stream::init_alloc(
            rustty_lib::allocator::ZigAllocatorRef::new(&GLOBAL_ZIG_ALLOCATOR_FOR_OSC),
            handler,
        );
        unsafe { (&raw mut (*self_).terminal_stream).write(terminal_stream) };

        Ok(())
    }

    // pub fn deinit(self: *Termio) void — consuming `self` (the reference
    // invalidates the struct; cf_release `Thread::deinit` precedent).
    pub fn deinit(mut self) {
        self.backend.deinit();
        self.terminal.deinit(self.alloc);
        self.config.deinit();
        // self.mailbox.deinit(self.alloc);
        self.mailbox.deinit();

        // Clear any StreamHandler state
        // self.terminal_stream.deinit();
        self.terminal_stream.deinit();

        // Clear any initial state if we have it
        if let Some(v) = self.thread_enter_state.take() {
            v.destroy();
        }
    }

    // DEFERRED (D6 live-thread wiring, 0 tests — R1-safe): the body constructs
    // the on-thread `ThreadData` around the xev loop (`.loop = &thread.loop`,
    // `.backend = undefined` — an out-parameter protocol the live thread
    // owns) and queues the prepared inputs through the backend. Preserved
    // verbatim:
    //
    // pub fn threadEnter(
    //     self: *Termio,
    //     thread: *termio.Thread,
    //     data: *ThreadData,
    // ) !void {
    //     // Always free our thread enter state when we're done.
    //     defer if (self.thread_enter_state) |v| {
    //         v.destroy();
    //         self.thread_enter_state = null;
    //     };
    //
    //     // If we have thread enter state then we're going to validate
    //     // and set that all up now so that we can error before we actually
    //     // start the command and pty.
    //     const inputs: ?[]const ThreadEnterState.Input = if (self.thread_enter_state) |v|
    //         try v.prepareInput()
    //     else
    //         null;
    //
    //     data.* = .{
    //         .alloc = self.alloc,
    //         .loop = &thread.loop,
    //         .renderer_state = self.renderer_state,
    //         .surface_mailbox = self.surface_mailbox,
    //         .mailbox = &self.mailbox,
    //         .backend = undefined, // Backend must replace this on threadEnter
    //     };
    //
    //     // Setup our backend
    //     try self.backend.threadEnter(self.alloc, self, data);
    //     errdefer self.backend.threadExit(data);
    //
    //     // If we have inputs, then queue them all up.
    //     for (inputs orelse &.{}) |input| switch (input) {
    //         .string => |v| self.queueWrite(data, v, false) catch |err| {
    //             log.warn("failed to queue input string err={}", .{err});
    //             return error.InputFailed;
    //         },
    //         .file => |f| self.queueWrite(
    //             data,
    //             f.readToEndAlloc(
    //                 self.alloc,
    //                 10 * 1024 * 1024, // 10 MiB max
    //             ) catch |err| {
    //                 log.warn("failed to read input file err={}", .{err});
    //                 return error.InputFailed;
    //             },
    //             false,
    //         ) catch |err| {
    //             log.warn("failed to queue input file err={}", .{err});
    //             return error.InputFailed;
    //         },
    //     };
    // }
    //
    // PORTED (xev B4, [ADV-FIX F2], 0 reference tests — R1-safe). The reference
    // threads a RAW `self: *Termio` + `data: *ThreadData` (out-param). The port
    // keeps BOTH raw (NOT `&mut self`) because `self.backend.threadEnter(...)`
    // spawns the read thread, which then aliases `*self_` via `processOutput`
    // concurrently with the writer thread; a `&mut self` would be a protected
    // `Unique` retag for the whole call, aliasing the reader
    // (`rust-mut-args-strongly-protected`). We form only transient, field-scoped
    // borrows. `data` is an OUT-PARAM pointing at uninitialized `ThreadData`
    // storage (the reference `data.* = .{...}` writes it whole; `backend` is the
    // reference's `undefined` — here a droppable placeholder that
    // `Exec.threadEnter` overwrites, see `backend::ThreadData::placeholder`).
    // `M: Copy` is Rust-only glue (the reference copies `self.surface_mailbox` by
    // value into `data`).
    ///
    /// # Safety
    /// `self_` must point at a live, initialized `Termio<M>`; `thread` at the live
    /// owning `Thread`; `data` at uninitialized `ThreadData<M>` storage that is
    /// stable for the loop's life (the frame-pinned `CallbackData::data`). On `Ok`,
    /// `*data` is fully initialized.
    pub unsafe fn threadEnter(
        self_: *mut Termio<M>,
        thread: *mut Thread,
        data: *mut ThreadData<M>,
    ) -> Result<(), Error>
    where
        M: Copy + 'static,
    {
        // The reference's `defer if (self.thread_enter_state) |v| { v.destroy();
        // self.thread_enter_state = null; }` frees the thread-enter state at EVERY
        // exit. We run the body, then free once at the single exit below — the
        // faithful `defer` semantics.
        let body = || -> Result<(), Error> {
            // If we have thread enter state then we're going to validate
            // and set that all up now so that we can error before we actually
            // start the command and pty.
            // const inputs: ?[]const ThreadEnterState.Input = if (self.thread_enter_state) |v|
            //     try v.prepareInput() else null;
            let inputs: Option<Vec<Input>> = match unsafe { (*self_).thread_enter_state.as_mut() } {
                // (PORT NOTE: `try v.prepareInput()` — the `InputNotFound` error maps to
                // the opaque `crate::Error`; the reference bubbles it to
                // `threadMain`'s error switch, unreachable while `Error` is opaque.)
                Some(v) => Some(v.prepareInput().map_err(|_| Error)?),
                None => None,
            };

            // data.* = .{ .alloc, .loop = &thread.loop, .renderer_state,
            //             .surface_mailbox, .mailbox = &self.mailbox,
            //             .backend = undefined };
            //
            // SAFETY: `data` is caller-provided uninitialized `ThreadData<M>`
            // storage; `ptr::write` initializes it whole (no old value dropped).
            // `backend` is the droppable `undefined` placeholder that
            // `Exec.threadEnter` overwrites below.
            unsafe {
                data.write(ThreadData {
                    alloc: (*self_).alloc,
                    loop_: &raw mut (*thread).loop_,
                    renderer_state: (*self_).renderer_state,
                    surface_mailbox: (*self_).surface_mailbox,
                    backend: backend::ThreadData::placeholder(),
                    mailbox: &raw mut (*self_).mailbox,
                });
            }

            // Setup our backend
            // try self.backend.threadEnter(self.alloc, self, data);
            //
            // SAFETY: forms the field-scoped `&mut (*self_).backend` receiver and
            // hands the raw whole-Termio `self_` as `io` + the raw `data` — matching
            // the reference `self.backend.threadEnter(self.alloc, self, data)`; the
            // spawned read thread aliases `*self_`'s OTHER fields only, disjoint from
            // the `.backend` receiver.
            unsafe {
                (*self_)
                    .backend
                    .threadEnter((*self_).alloc, self_, &mut *data)
            }?;
            // errdefer self.backend.threadExit(data);
            // (PORT NOTE: fired explicitly on each input-queue error below — the sole
            // steps after `backend.threadEnter` succeeds.)

            // If we have inputs, then queue them all up.
            // for (inputs orelse &.{}) |input| switch (input) { ... }
            for input in inputs.into_iter().flatten() {
                let queued: Result<(), Error> = match input {
                    // .string => |v| self.queueWrite(data, v, false) catch |err| {
                    //     log.warn("failed to queue input string err={}", .{err});
                    //     return error.InputFailed;
                    // },
                    Input::String(v) => unsafe { (*self_).queueWrite(&mut *data, &v, false) },
                    // .file => |f| self.queueWrite(
                    //     data,
                    //     f.readToEndAlloc(self.alloc, 10 * 1024 * 1024) catch |err| {
                    //         log.warn("failed to read input file err={}", .{err});
                    //         return error.InputFailed;
                    //     },
                    //     false,
                    // ) catch |err| {
                    //     log.warn("failed to queue input file err={}", .{err});
                    //     return error.InputFailed;
                    // },
                    Input::File(f) => match read_input_file(f, 10 * 1024 * 1024) {
                        Ok(bytes) => unsafe { (*self_).queueWrite(&mut *data, &bytes, false) },
                        Err(_err) => {
                            // log.warn("failed to read input file err={}", .{err});
                            Err(Error)
                        }
                    },
                };
                if queued.is_err() {
                    // (log.warn for the string/file queue failures — see the
                    // reference arms above; both return `error.InputFailed`.)
                    // errdefer self.backend.threadExit(data);
                    //
                    // SAFETY: `data` is fully initialized (backend.threadEnter
                    // succeeded); the read thread has NOT yet been joined, so
                    // `threadExit` (which writes the quit pipe + joins) is the
                    // correct teardown.
                    unsafe { (*self_).backend.threadExit(&mut *data) };
                    return Err(Error);
                }
            }

            Ok(())
        };

        let result = body();

        // defer if (self.thread_enter_state) |v| { v.destroy(); self.thread_enter_state = null; };
        if let Some(v) = unsafe { (*self_).thread_enter_state.take() } {
            v.destroy();
        }

        result
    }

    pub fn threadExit(&mut self, data: &mut ThreadData<M>) {
        self.backend.threadExit(data);
    }

    /// Send a message to the mailbox. Depending on the mailbox type in use
    /// this may process now or it may just enqueue and process later.
    ///
    /// This will also notify the mailbox thread to process the message. If
    /// you're sending a lot of messages, it may be more efficient to use
    /// the mailbox directly and then call notify separately.
    pub fn queueMessage(&mut self, msg: Message<'static>, mutex: MutexState) {
        // PORT NOTE: `self.renderer_state.mutex` is the raw shared-state read
        // (renderer state.rs precedent); the `*mut Mutex` is handed to the
        // mailbox untouched.
        let mutex_ptr = match mutex {
            // .locked => self.renderer_state.mutex,
            MutexState::Locked => Some(unsafe { (*self.renderer_state).mutex }),
            // .unlocked => null,
            MutexState::Unlocked => None,
        };
        self.mailbox.send(msg, mutex_ptr);
        self.mailbox.notify();
    }

    /// Queue a write directly to the pty.
    ///
    /// If you're using termio.Thread, this must ONLY be called from the
    /// mailbox thread. If you're not on the thread, use queueMessage with
    /// mailbox messages instead.
    ///
    /// If you're not using termio.Thread, this is not threadsafe.
    //
    // PORT NOTE: reference `pub inline fn` -> `#[inline]`.
    #[inline]
    pub fn queueWrite(
        &mut self,
        td: &mut ThreadData<M>,
        data: &[u8],
        linefeed: bool,
    ) -> Result<(), Error> {
        self.backend.queueWrite(self.alloc, td, data, linefeed)
    }

    /// Update the configuration.
    // pub fn changeConfig(self: *Termio, td: *ThreadData, config: *DerivedConfig) !void {
    //
    // PORT NOTE (core C-T un-defer): the reference `config: *DerivedConfig` is
    // taken BY VALUE here — the reference does `self.config = config.*` (a value
    // copy that moves the derived config's owned storage into `self`) and its
    // caller (the deferred D6 drainMailbox) then frees the now-moved-from box;
    // Rust models that move by consuming the value. Subsequent reads that the
    // reference makes via the `config` pointer read the identical value through
    // `self.config`. The renderer lock is the guard-less C0
    // `rustty_zigstd::thread::Mutex`; its `defer unlock` is the explicit unlock
    // before every return below.
    pub fn changeConfig(
        &mut self,
        td: &mut ThreadData<M>,
        config: DerivedConfig,
    ) -> Result<(), Error> {
        // The remainder of this function is modifying terminal state or
        // the read thread data, all of which requires holding the renderer
        // state lock.
        // self.renderer_state.mutex.lock();
        // defer self.renderer_state.mutex.unlock();
        //
        // SAFETY: `renderer_state` points at the live shared render State for
        // the Termio's lifetime; `.mutex` is the guard-less shared lock.
        unsafe { (*(*self.renderer_state).mutex).lock() };

        // Deinit our old config. We do this in the lock because the
        // stream handler may be referencing the old config (i.e. enquiry resp)
        // self.config.deinit();
        self.config.deinit();
        // self.config = config.*;
        self.config = config;

        // Update our stream handler. The stream handler uses the same
        // renderer mutex so this is safe to do despite being executed
        // from another thread.
        // self.terminal_stream.handler.changeConfig(&self.config);
        self.terminal_stream.handler.changeConfig(&mut self.config);
        // td.backend.changeConfig(&self.config);
        td.backend.changeConfig(&mut self.config);

        // Update the configuration that we know about.
        //
        // Specific things we don't update:
        //   - command, working-directory: we never restart the underlying
        //   process so we don't care or need to know about these.

        // Update the default palette.
        // self.terminal.colors.palette.changeDefault(config.palette);
        self.terminal
            .colors
            .palette
            .change_default(self.config.palette);
        // self.terminal.flags.dirty.palette = true;
        self.terminal.flags.dirty.palette = true;

        // Update all our other colors
        // self.terminal.colors.background.default = config.background.toTerminalRGB();
        self.terminal.colors.background.default = Some(self.config.background.to_terminal_rgb());
        // self.terminal.colors.foreground.default = config.foreground.toTerminalRGB();
        self.terminal.colors.foreground.default = Some(self.config.foreground.to_terminal_rgb());
        // self.terminal.colors.cursor.default = cursor: {
        //     const color = config.cursor_color orelse break :cursor null;
        //     break :cursor color.toTerminalRGB() orelse break :cursor null;
        // };
        self.terminal.colors.cursor.default = self
            .config
            .cursor_color
            .and_then(|color| color.to_terminal_rgb());

        // Set the image limits
        // try self.terminal.setKittyGraphicsSizeLimit(self.alloc, config.image_storage_limit);
        //
        // (PORT NOTE: the reference `try` returns early on error, and the guard-less
        // `defer unlock` still fires; `setKittyGraphicsLoadingLimits` below is
        // only reached on success — mirrored by the `is_ok()` fall-through.)
        let size_limit = self
            .terminal
            .set_kitty_graphics_size_limit(self.alloc, self.config.image_storage_limit);
        if size_limit.is_ok() {
            // self.terminal.setKittyGraphicsLoadingLimits(.all);
            self.terminal.set_kitty_graphics_loading_limits(Limits::ALL);
        }

        // (the deferred renderer-state unlock)
        unsafe { (*(*self.renderer_state).mutex).unlock() };
        size_limit.map_err(|_| Error)
    }

    /// Resize the terminal.
    // pub fn resize(
    //     self: *Termio,
    //     td: *ThreadData,
    //     size: renderer.Size,
    // ) !void {
    //
    // PORT NOTE (core C-T un-defer): the renderer lock is the guard-less C0
    // `rustty_zigstd::thread::Mutex`; the reference's `{ lock; defer unlock; ...
    // try ...; }` critical block is a labeled block returning the fallible
    // section's `Result`, unlocked once after (the `defer`), then propagated
    // (`locked?`) — so a resize/size-report error unlocks and returns WITHOUT
    // mailing the renderer, exactly as the reference `try` inside the block. The
    // final `renderer_wakeup.notify()` is the xev placeholder leaf (D6): live
    // now, runtime-panics only once the render thread runs (CORE_BLUEPRINT §5).
    pub fn resize(&mut self, td: &mut ThreadData<M>, size: Size) -> Result<(), Error> {
        // self.size = size;
        self.size = size;
        // const grid_size = size.grid();
        let grid_size = size.grid();

        // Update the size of our pty.
        // try self.backend.resize(grid_size, size.terminal());
        self.backend.resize(grid_size, size.terminal())?;

        // Enter the critical area that we want to keep small
        // {
        //     self.renderer_state.mutex.lock();
        //     defer self.renderer_state.mutex.unlock();
        //
        // SAFETY: `renderer_state` points at the live shared render State for
        // the Termio's lifetime; `.mutex` is the guard-less shared lock.
        unsafe { (*(*self.renderer_state).mutex).lock() };
        let locked: Result<(), Error> = 'locked: {
            // Update the size of our terminal state
            // try self.terminal.resize(self.alloc, grid_size.columns, grid_size.rows);
            if self
                .terminal
                .resize(self.alloc, grid_size.columns, grid_size.rows)
                .is_err()
            {
                break 'locked Err(Error);
            }

            // Update our pixel sizes
            // self.terminal.width_px = grid_size.columns * self.size.cell.width;
            self.terminal.width_px = u32::from(grid_size.columns) * self.size.cell.width;
            // self.terminal.height_px = grid_size.rows * self.size.cell.height;
            self.terminal.height_px = u32::from(grid_size.rows) * self.size.cell.height;

            // Disable synchronized output mode so that we show changes
            // immediately for a resize. This is allowed by the spec.
            // self.terminal.modes.set(.synchronized_output, false);
            self.terminal.modes.set(Mode::SynchronizedOutput, false);

            // If we have size reporting enabled we need to send a report.
            // if (self.terminal.modes.get(.in_band_size_reports)) {
            //     try self.sizeReportLocked(td, .mode_2048);
            // }
            if self.terminal.modes.get(Mode::InBandSizeReports) {
                if let Err(e) = self.sizeReportLocked(td, size_report::Style::Mode2048) {
                    break 'locked Err(e);
                }
            }

            Ok(())
        };
        // }
        // (the deferred renderer-state unlock)
        unsafe { (*(*self.renderer_state).mutex).unlock() };
        locked?;

        // Mail the renderer so that it can update the GPU and re-render
        // _ = self.renderer_mailbox.push(.{ .resize = size }, .{ .forever = {} });
        //
        // SAFETY: `renderer_mailbox` points at the live renderer thread mailbox.
        let _ = unsafe { &*self.renderer_mailbox }
            .push(RendererMessage::Resize(size), Timeout::Forever);
        // self.renderer_wakeup.notify() catch {};
        let _ = self.renderer_wakeup.notify();
        Ok(())
    }

    /// Make a size report.
    // pub fn sizeReport(self: *Termio, td: *ThreadData, style: termio.Message.SizeReport) !void {
    pub fn sizeReport(
        &mut self,
        td: &mut ThreadData<M>,
        style: crate::message::SizeReport,
    ) -> Result<(), Error> {
        // self.renderer_state.mutex.lock();
        // defer self.renderer_state.mutex.unlock();
        //
        // SAFETY: guard-less shared renderer-state lock (see `changeConfig`).
        unsafe { (*(*self.renderer_state).mutex).lock() };
        // try self.sizeReportLocked(td, style);
        let result = self.sizeReportLocked(td, style);
        // (the deferred renderer-state unlock — fires on the `try` error too.)
        unsafe { (*(*self.renderer_state).mutex).unlock() };
        result
    }

    // fn sizeReportLocked(self: *Termio, td: *ThreadData, style: termio.Message.SizeReport) !void {
    fn sizeReportLocked(
        &mut self,
        td: &mut ThreadData<M>,
        style: crate::message::SizeReport,
    ) -> Result<(), Error> {
        // const grid_size = self.size.grid();
        let grid_size = self.size.grid();
        // const report_size: terminalpkg.size_report.Size = .{
        //     .rows = grid_size.rows,
        //     .columns = grid_size.columns,
        //     .cell_width = self.size.cell.width,
        //     .cell_height = self.size.cell.height,
        // };
        let report_size = size_report::Size {
            rows: grid_size.rows,
            columns: grid_size.columns,
            cell_width: self.size.cell.width,
            cell_height: self.size.cell.height,
        };

        // 1024 bytes should be enough for size report since report
        // in columns and pixels.
        // var buf: [1024]u8 = undefined;
        // var writer: std.Io.Writer = .fixed(&buf);
        let mut buf = [0u8; 1024];
        let mut writer = FixedWriter::new(&mut buf);
        // try terminalpkg.size_report.encode(&writer, style, report_size);
        size_report::encode(&mut writer, style, report_size).map_err(|_| Error)?;

        // try self.queueWrite(td, writer.buffered(), false);
        self.queueWrite(td, writer.buffered(), false)
    }

    /// Reset the synchronized output mode. This is usually called by timer
    /// expiration from the termio thread.
    // pub fn resetSynchronizedOutput(self: *Termio) void {
    //
    // PORT NOTE (core C-T un-defer): guard-less C0 renderer lock; the final
    // `renderer_wakeup.notify()` is the xev placeholder leaf (D6, live-now).
    pub fn resetSynchronizedOutput(&mut self) {
        // self.renderer_state.mutex.lock();
        // defer self.renderer_state.mutex.unlock();
        //
        // SAFETY: guard-less shared renderer-state lock (see `changeConfig`).
        unsafe { (*(*self.renderer_state).mutex).lock() };
        // self.terminal.modes.set(.synchronized_output, false);
        self.terminal.modes.set(Mode::SynchronizedOutput, false);
        // self.renderer_wakeup.notify() catch {};
        let _ = self.renderer_wakeup.notify();
        // (the deferred renderer-state unlock)
        unsafe { (*(*self.renderer_state).mutex).unlock() };
    }

    /// Clear the screen.
    // pub fn clearScreen(self: *Termio, td: *ThreadData, history: bool) !void {
    //
    // PORT NOTE (core C-T un-defer): the reference `{ lock; defer unlock; ...
    // return; ... }` critical block is a labeled block computing whether to
    // send the form-feed (its two `return`s become `break 'locked false` — the
    // reference returns without the form-feed; the fall-through is `true`),
    // unlocked once after (the `defer`). The Kitty `screens.active.kitty_images.
    // delete(active.alloc, &self.terminal, ...)` aliases the storage (inside the
    // terminal) with `&self.terminal`; as in vt's graphics_exec we MOVE the
    // storage out, run `delete` against the borrowed terminal, then move it back
    // (memory `rust-mut-args-strongly-protected`). Guard-less C0 renderer lock.
    pub fn clearScreen(&mut self, td: &mut ThreadData<M>, history: bool) -> Result<(), Error> {
        // {
        //     self.renderer_state.mutex.lock();
        //     defer self.renderer_state.mutex.unlock();
        //
        // SAFETY: guard-less shared renderer-state lock (see `changeConfig`).
        unsafe { (*(*self.renderer_state).mutex).lock() };
        let send_form_feed: bool = 'locked: {
            // If we're on the alternate screen, we do not clear. Since this is an
            // emulator-level screen clear, this messes up the running programs
            // knowledge of where the cursor is and causes rendering issues. So,
            // for alt screen, we do nothing.
            // if (self.terminal.screens.active_key == .alternate) return;
            if self.terminal.screens.active_key == rustty_vt::screen_set::Key::Alternate {
                break 'locked false;
            }

            // Clear our selection
            // self.terminal.screens.active.clearSelection();
            //
            // SAFETY: `screens.active` is the live active `*mut Screen` owned by
            // the terminal's screen set; the deref is short-lived.
            unsafe { &mut *self.terminal.screens.active }.clear_selection();

            // Clear our scrollback
            // if (history) self.terminal.eraseDisplay(.scrollback, false);
            if history {
                self.terminal
                    .erase_display(rustty_vt::csi::EraseDisplay::Scrollback, false);
            }

            // If we're not at a prompt, we just delete above the cursor.
            // if (!self.terminal.cursorIsAtPrompt()) {
            if !self.terminal.cursor_is_at_prompt() {
                // if (self.terminal.screens.active.cursor.y > 0) {
                //     self.terminal.screens.active.eraseActive(
                //         self.terminal.screens.active.cursor.y - 1,
                //     );
                // }
                let cursor_y = unsafe { (*self.terminal.screens.active).cursor.y };
                if cursor_y > 0 {
                    unsafe { &mut *self.terminal.screens.active }.erase_active(cursor_y - 1);
                }

                // Clear all Kitty graphics state for this screen. This copies
                // Kitty's behavior when Cmd+K deletes all Kitty graphics. I
                // didn't spend time researching whether it only deletes Kitty
                // graphics that are placed above the cursor or if it deletes
                // all of them. We delete all of them for now but if this behavior
                // isn't fully correct we should fix this later.
                // self.terminal.screens.active.kitty_images.delete(
                //     self.terminal.screens.active.alloc,
                //     &self.terminal,
                //     .{ .all = true },
                // );
                //
                // SAFETY: `active` is the stable active `*mut Screen`; we move
                // its `kitty_images` out (leaving a default), run `delete`
                // against `&mut self.terminal` (no alias while the storage is
                // moved out), then move the updated storage back.
                let active = self.terminal.screens.active;
                let alloc = unsafe { (*active).alloc };
                let mut storage = core::mem::take(unsafe { &mut (*active).kitty_images });
                storage.delete(
                    alloc,
                    &mut self.terminal,
                    rustty_vt::kitty::graphics_command::Delete::All(true),
                );
                unsafe { (*active).kitty_images = storage };

                // return;
                break 'locked false;
            }

            // At a prompt, we want to first fully clear the screen, and then after
            // send a FF (0x0C) to the shell so that it can repaint the screen.
            // Mark the current row as a not a prompt so we can properly
            // clear the full screen in the next eraseDisplay call.
            // TODO: fix this
            // self.terminal.markSemanticPrompt(.command);
            // assert(!self.terminal.cursorIsAtPrompt());
            // self.terminal.eraseDisplay(.complete, false);
            self.terminal
                .erase_display(rustty_vt::csi::EraseDisplay::Complete, false);

            // (fall-through: at a prompt — send the form-feed below)
            true
        };
        // }
        // (the deferred renderer-state unlock)
        unsafe { (*(*self.renderer_state).mutex).unlock() };

        // If we reached here it means we're at a prompt, so we send a form-feed.
        // try self.queueWrite(td, &[_]u8{0x0C}, false);
        if send_form_feed {
            self.queueWrite(td, &[0x0C], false)?;
        }
        Ok(())
    }

    /// Scroll the viewport
    // pub fn scrollViewport(
    //     self: *Termio,
    //     scroll: terminalpkg.Terminal.ScrollViewport,
    // ) void {
    //
    // PORT NOTE (core C-T un-defer): guard-less C0 renderer lock.
    pub fn scrollViewport(&mut self, scroll: rustty_vt::terminal::ScrollViewport) {
        // self.renderer_state.mutex.lock();
        // defer self.renderer_state.mutex.unlock();
        //
        // SAFETY: guard-less shared renderer-state lock (see `changeConfig`).
        unsafe { (*(*self.renderer_state).mutex).lock() };
        // self.terminal.scrollViewport(scroll);
        self.terminal.scroll_viewport(scroll);
        // (the deferred renderer-state unlock)
        unsafe { (*(*self.renderer_state).mutex).unlock() };
    }

    /// Jump the viewport to the prompt.
    // pub fn jumpToPrompt(self: *Termio, delta: isize) !void {
    //
    // PORT NOTE (core C-T un-defer): guard-less C0 renderer lock; the final
    // `renderer_wakeup.notify()` is the xev placeholder leaf (D6, live-now).
    pub fn jumpToPrompt(&mut self, delta: isize) -> Result<(), Error> {
        // {
        //     self.renderer_state.mutex.lock();
        //     defer self.renderer_state.mutex.unlock();
        //
        // SAFETY: guard-less shared renderer-state lock (see `changeConfig`).
        unsafe { (*(*self.renderer_state).mutex).lock() };
        //     self.terminal.screens.active.scroll(.{ .delta_prompt = delta });
        //
        // SAFETY: `screens.active` is the live active `*mut Screen`.
        unsafe { &mut *self.terminal.screens.active }
            .scroll(rustty_vt::screen::Scroll::DeltaPrompt(delta));
        // }
        // (the deferred renderer-state unlock)
        unsafe { (*(*self.renderer_state).mutex).unlock() };

        // try self.renderer_wakeup.notify();
        self.renderer_wakeup.notify().map_err(|_| Error)
    }

    /// Called when focus is gained or lost (when focus events are enabled)
    // pub fn focusGained(self: *Termio, td: *ThreadData, focused: bool) !void {
    //
    // PORT NOTE (core C-T un-defer): guard-less C0 renderer lock; the mode is
    // read through the shared `renderer_state.terminal` pointer (which aliases
    // `self.terminal`), exactly as the reference. The `focus.encode` uses a
    // stack `FixedWriter` (the `std.Io.Writer.fixed` analogue).
    pub fn focusGained(&mut self, td: &mut ThreadData<M>, focused: bool) -> Result<(), Error>
    where
        M: 'static,
    {
        // self.renderer_state.mutex.lock();
        //
        // SAFETY: guard-less shared renderer-state lock (see `changeConfig`);
        // `renderer_state.terminal` is the live terminal pointer.
        unsafe { (*(*self.renderer_state).mutex).lock() };
        // const focus_event = self.renderer_state.terminal.modes.get(.focus_event);
        let focus_event = unsafe {
            (*(*self.renderer_state).terminal)
                .modes
                .get(Mode::FocusEvent)
        };
        // self.renderer_state.mutex.unlock();
        unsafe { (*(*self.renderer_state).mutex).unlock() };

        // If we have focus events enabled, we send the focus event.
        // if (focus_event) {
        if focus_event {
            // var buf: [terminalpkg.focus.max_encode_size]u8 = undefined;
            // var writer: std.Io.Writer = .fixed(&buf);
            let mut buf = [0u8; focus::MAX_ENCODE_SIZE];
            let mut writer = FixedWriter::new(&mut buf);
            // terminalpkg.focus.encode(&writer, if (focused) .gained else .lost) catch |err| {
            //     log.err("error encoding focus event err={}", .{err});
            //     return;
            // };
            let event = if focused {
                focus::Event::Gained
            } else {
                focus::Event::Lost
            };
            if focus::encode(&mut writer, event).is_err() {
                // log.err("error encoding focus event err={}", .{err});
                return Ok(());
            }
            // try self.queueWrite(td, writer.buffered(), false);
            self.queueWrite(td, writer.buffered(), false)?;
        }

        // We always notify our backend of focus changes.
        // try self.backend.focusGained(td, focused);
        self.backend.focusGained(td, focused)
    }

    /// Process output from the pty. This is the manual API that users can
    /// call with pty data but it is also called by the read thread when using
    /// an exec subprocess.
    //
    // PORTED (xev B3, [ADV-FIX F2], 0 reference tests — R1-safe). The reference
    // threads a RAW `self: *Termio`; the port keeps `io: *mut Termio<M>` (NOT
    // `&mut self`) because the read thread calls this concurrently with the
    // writer thread's `threadMain` — a `&mut self` arg is a protected `Unique`
    // retag for the whole call that would alias the writer + the parent
    // (`rust-mut-args-strongly-protected`). We form only transient,
    // non-overlapping borrows under the renderer mutex. Preserved verbatim:
    //
    // pub fn processOutput(self: *Termio, buf: []const u8) void {
    //     // We are modifying terminal state from here on out and we need
    //     // the lock to grab our read data.
    //     self.renderer_state.mutex.lock();
    //     defer self.renderer_state.mutex.unlock();
    //     self.processOutputLocked(buf);
    // }
    ///
    /// # Safety
    /// `io` must be a valid `Termio<M>` pointer whose `renderer_state` is a live
    /// shared render `State`; the read thread is its sole `processOutput` toucher
    /// for the pointer's lifetime.
    pub unsafe fn processOutput(io: *mut Termio<M>, buf: &[u8]) {
        // TEST-ONLY instrumentation (WS10 T20). The pipeline's throughput
        // property is "every byte the producer wrote reaches processOutput
        // exactly once", and an 80x24 screen physically cannot retain 8 MiB, so
        // the screen alone cannot witness it. This tally is the witness. It is
        // `#[cfg(test)]`, so it does not exist in any shipped build.
        #[cfg(test)]
        {
            crate::PROCESSED_BYTES.fetch_add(buf.len(), core::sync::atomic::Ordering::Relaxed);
            crate::PROCESSED_CALLS.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        }

        // We are modifying terminal state from here on out and we need
        // the lock to grab our read data.
        // self.renderer_state.mutex.lock();
        unsafe { (*(*(*io).renderer_state).mutex).lock() };
        // defer self.renderer_state.mutex.unlock();
        // self.processOutputLocked(buf);
        unsafe { Self::processOutputLocked(io, buf) };
        // (the deferred renderer-state unlock — the guard-less zigstd Mutex model,
        // matching `colorSchemeReport`'s explicit lock/call/unlock precedent; the
        // `defer` fires after the call returns.)
        unsafe { (*(*(*io).renderer_state).mutex).unlock() };
    }

    // PORTED (xev B3, with `processOutput` above — was DEFERRED). The verbatim
    // reference is preserved below; the ported body follows it:
    //
    // /// Process output from readdata but the lock is already held.
    // fn processOutputLocked(self: *Termio, buf: []const u8) void {
    //     // Schedule a render. We can call this first because we have the lock.
    //     self.terminal_stream.handler.queueRender() catch unreachable;
    //
    //     // Whenever a character is typed, we ensure the cursor is in the
    //     // non-blink state so it is rendered if visible. If we're under
    //     // HEAVY read load, we don't want to send a ton of these so we
    //     // use a timer under the covers
    //     if (std.time.Instant.now()) |now| cursor_reset: {
    //         if (self.last_cursor_reset) |last| {
    //             if (now.since(last) <= (500 * std.time.ns_per_ms)) {
    //                 break :cursor_reset;
    //             }
    //         }
    //
    //         self.last_cursor_reset = now;
    //         _ = self.renderer_mailbox.push(.{
    //             .reset_cursor_blink = {},
    //         }, .{ .instant = {} });
    //     } else |err| {
    //         log.warn("failed to get current time err={}", .{err});
    //     }
    //
    //     // If we have an inspector, we enter SLOW MODE because we need to
    //     // process a byte at a time alternating between the inspector handler
    //     // and the termio handler. This is very slow compared to our optimizations
    //     // below but at least users only pay for it if they're using the inspector.
    //     if (self.renderer_state.inspector) |insp| {
    //         for (buf, 0..) |byte, i| {
    //             insp.recordPtyRead(
    //                 self.alloc,
    //                 &self.terminal,
    //                 buf[i .. i + 1],
    //             ) catch |err| {
    //                 log.err("error recording pty read in inspector err={}", .{err});
    //             };
    //
    //             self.terminal_stream.next(byte);
    //         }
    //     } else {
    //         self.terminal_stream.nextSlice(buf);
    //     }
    //
    //     // If our stream handling caused messages to be sent to the mailbox
    //     // thread, then we need to wake it up so that it processes them.
    //     if (self.terminal_stream.handler.termio_messaged) {
    //         self.terminal_stream.handler.termio_messaged = false;
    //         self.mailbox.notify();
    //     }
    // }
    // PORTED (xev B3, [ADV-FIX F2], 0 reference tests — R1-safe): now live (the
    // sole reference caller, `processOutput` above, is live). Raw `io: *mut
    // Termio<M>` (NOT `&mut self`) for the same protected-aliasing reason as
    // `processOutput`; forms only transient, non-overlapping borrows under the
    // caller-held renderer mutex.
    //
    // # Safety: `io` must be a valid `Termio<M>` pointer with the renderer mutex
    // held by the caller (`processOutput`).
    unsafe fn processOutputLocked(io: *mut Termio<M>, buf: &[u8]) {
        // Schedule a render. We can call this first because we have the lock.
        // self.terminal_stream.handler.queueRender() catch unreachable;
        unsafe { (*io).terminal_stream.handler.queueRender() }.unwrap();

        // Whenever a character is typed, we ensure the cursor is in the
        // non-blink state so it is rendered if visible. If we're under
        // HEAVY read load, we don't want to send a ton of these so we
        // use a timer under the covers
        // if (std.time.Instant.now()) |now| cursor_reset: {
        //     if (self.last_cursor_reset) |last| {
        //         if (now.since(last) <= (500 * std.time.ns_per_ms)) {
        //             break :cursor_reset;
        //         }
        //     }
        //
        //     self.last_cursor_reset = now;
        //     _ = self.renderer_mailbox.push(.{
        //         .reset_cursor_blink = {},
        //     }, .{ .instant = {} });
        // } else |err| {
        //     log.warn("failed to get current time err={}", .{err});
        // }
        //
        // (PORT NOTE: Rust's `Instant::now()` is infallible, so the `else |err|` warn
        // arm never fires; `since` -> `duration_since`, `500 * ns_per_ms` ->
        // `Duration::from_millis(500)`.)
        let now = std::time::Instant::now();
        'cursor_reset: {
            // if (self.last_cursor_reset) |last| {
            if let Some(last) = unsafe { (*io).last_cursor_reset } {
                // if (now.since(last) <= (500 * std.time.ns_per_ms)) break :cursor_reset;
                if now.duration_since(last) <= std::time::Duration::from_millis(500) {
                    break 'cursor_reset;
                }
            }
            // self.last_cursor_reset = now;
            unsafe { (*io).last_cursor_reset = Some(now) };
            // _ = self.renderer_mailbox.push(.{ .reset_cursor_blink = {} }, .{ .instant = {} });
            let _ = unsafe { &*(*io).renderer_mailbox }
                .push(RendererMessage::ResetCursorBlink, Timeout::Instant);
        }

        // If we have an inspector, we enter SLOW MODE because we need to
        // process a byte at a time alternating between the inspector handler
        // and the termio handler. This is very slow compared to our optimizations
        // below but at least users only pay for it if they're using the inspector.
        // if (self.renderer_state.inspector) |insp| {
        //     for (buf, 0..) |byte, i| {
        //         insp.recordPtyRead(self.alloc, &self.terminal, buf[i .. i + 1]) catch |err| {
        //             log.err("error recording pty read in inspector err={}", .{err});
        //         };
        //         self.terminal_stream.next(byte);
        //     }
        // } else {
        //     self.terminal_stream.nextSlice(buf);
        // }
        //
        // PORT NOTE (INV-8, `rustty_renderer::state::State::inspector_ptr` PORT
        // NOTE): `renderer_state.inspector` is only the narrow `*mut dyn
        // InspectorOverlay` fat pointer (the `Inspector.gui.renderer_info`
        // sub-object) — it cannot reach the sibling `vt_stream` field
        // `recordPtyRead` needs. `renderer_state.inspector_ptr` carries the SAME
        // pointer core computed in `activateInspector`, but type-erased to
        // `*mut ()` so rustty-renderer never has to name a rustty-inspector
        // type; here (rustty-termio, which — like rustty-inspector — sits above
        // rustty-renderer) it is cast back to the concrete `*mut
        // rustty_inspector::Inspector` to call the now-live `recordPtyRead`.
        if let Some(insp_ptr) = unsafe { (*(*io).renderer_state).inspector_ptr } {
            // SAFETY: `inspector_ptr` is set/cleared in lock-step with
            // `renderer_state.inspector` by core's activateInspector/
            // deactivateInspector under `renderer_state.mutex` (already held by
            // this function's caller, `processOutput`), and always holds a live
            // `*mut rustty_inspector::Inspector` when `Some`.
            let insp: *mut rustty_inspector::Inspector =
                insp_ptr as *mut rustty_inspector::Inspector;
            // for (buf, 0..) |byte, i| {
            for &byte in buf {
                // insp.recordPtyRead(self.alloc, &self.terminal, buf[i .. i + 1]) catch |err| {
                //     log.err("error recording pty read in inspector err={}", .{err});
                // };
                //
                // PORT NOTE: `rustty_inspector::Inspector::recordPtyRead` is
                // infallible in the port (Task 16 design decision — the OSC
                // parser allocator collapses under Rust's infallible global
                // allocator), unlike the reference's `catch |err| log.err`; the
                // error-logging arm never fires so it has no Rust equivalent.
                unsafe {
                    (*insp).recordPtyRead(
                        (*io).alloc,
                        &raw mut (*io).terminal,
                        std::slice::from_ref(&byte),
                    )
                };
                // self.terminal_stream.next(byte);
                unsafe { (*io).terminal_stream.next(byte) };
            }
        } else {
            // self.terminal_stream.nextSlice(buf);
            unsafe { (*io).terminal_stream.next_slice(buf) };
        }

        // If our stream handling caused messages to be sent to the mailbox
        // thread, then we need to wake it up so that it processes them.
        // if (self.terminal_stream.handler.termio_messaged) {
        if unsafe { (*io).terminal_stream.handler.termio_messaged } {
            // self.terminal_stream.handler.termio_messaged = false;
            unsafe { (*io).terminal_stream.handler.termio_messaged = false };
            // self.mailbox.notify();
            unsafe { (*io).mailbox.notify() };
        }
    }

    /// Sends a DSR response for the current color scheme to the pty.
    // pub fn colorSchemeReport(self: *Termio, td: *ThreadData, force: bool) !void {
    //
    // PORT NOTE (core C-T un-defer): guard-less C0 renderer lock.
    pub fn colorSchemeReport(&mut self, td: &mut ThreadData<M>, force: bool) -> Result<(), Error> {
        // self.renderer_state.mutex.lock();
        // defer self.renderer_state.mutex.unlock();
        //
        // SAFETY: guard-less shared renderer-state lock (see `changeConfig`).
        unsafe { (*(*self.renderer_state).mutex).lock() };
        // try self.colorSchemeReportLocked(td, force);
        let result = self.colorSchemeReportLocked(td, force);
        // (the deferred renderer-state unlock — fires on the `try` error too.)
        unsafe { (*(*self.renderer_state).mutex).unlock() };
        result
    }

    // pub fn colorSchemeReportLocked(self: *Termio, td: *ThreadData, force: bool) !void {
    pub fn colorSchemeReportLocked(
        &mut self,
        td: &mut ThreadData<M>,
        force: bool,
    ) -> Result<(), Error> {
        // if (!force and !self.renderer_state.terminal.modes.get(.report_color_scheme)) {
        //     return;
        // }
        //
        // SAFETY: `renderer_state.terminal` is the live shared terminal pointer;
        // the read happens under the caller-held lock (colorSchemeReport).
        if !force
            && !unsafe {
                (*(*self.renderer_state).terminal)
                    .modes
                    .get(Mode::ReportColorScheme)
            }
        {
            return Ok(());
        }
        // const output = switch (self.config.conditional_state.theme) {
        //     .light => "\x1B[?997;2n",
        //     .dark => "\x1B[?997;1n",
        // };
        let scheme = match self.config.conditional_state.theme {
            conditional::Theme::Light => device_status::ColorScheme::Light,
            conditional::Theme::Dark => device_status::ColorScheme::Dark,
        };
        let mut output = Vec::new();
        device_status::encode_color_scheme_report(&mut output, scheme);
        // try self.queueWrite(td, output, false);
        self.queueWrite(td, &output, false)
    }

    /// Get information about the process(es) attached to the backend. Returns
    /// `null` if there was an error getting the information or the information is
    /// not available on a particular platform.
    //
    // PORT NOTE: reference `pub fn getProcessInfo(self: *Termio, comptime info:
    // ProcessInfo) ?ProcessInfo.Type(info) { return self.backend.getProcessInfo(info); }`
    // — split into the two concrete accessors (rustty-os `pty.rs` precedent;
    // see `Backend::getForegroundPid`).
    pub fn getForegroundPid(&mut self) -> Option<u64> {
        self.backend.getForegroundPid()
    }

    /// See [`Termio::getForegroundPid`] (the `.tty_name` half of the reference
    /// `getProcessInfo`).
    pub fn getTtyName(&mut self) -> Option<&std::ffi::CStr> {
        self.backend.getTtyName()
    }
}

/// ThreadData is the data created and stored in the termio thread
/// when the thread is started and destroyed when the thread is
/// stopped.
///
/// All of the fields in this struct should only be read/written by
/// the termio thread. As such, a lock is not necessary.
pub struct ThreadData<M: AppMailbox> {
    /// Allocator used for the event data
    pub alloc: Allocator,

    /// The event loop associated with this thread. This is owned by
    /// the Thread but we have a pointer so we can queue new work to it.
    //
    // PORT NOTE: reference field name `loop` is a Rust keyword -> `loop_`
    // (cf_release_thread precedent). `xev.Loop` is the D6 deferred seam.
    pub loop_: *mut xev::Loop,

    /// The shared render state
    pub renderer_state: *mut State,

    /// Mailboxes for different threads
    pub surface_mailbox: M,

    /// Data associated with the backend implementation (i.e. pty/exec state)
    pub backend: backend::ThreadData,
    pub mailbox: *mut Mailbox,
}

impl<M: AppMailbox> ThreadData<M> {
    pub fn deinit(&mut self) {
        self.backend.deinit(self.alloc);
        // self.* = undefined;
        // (PORT NOTE: the reference poisons the struct; Rust ownership leaves the
        // value in place — callers must not reuse it after deinit.)
    }
}

/// PORTS: `std.Io.Writer.fixed(&buf)` + `writer.buffered()` — a stack-buffered
/// `core::fmt::Write` sink used by `sizeReportLocked`/`focusGained` to encode a
/// small response WITHOUT heap allocation (the reference's `var buf: [N]u8 =
/// undefined; var writer: std.Io.Writer = .fixed(&buf);`). Like `.fixed`,
/// `write_str` ERRORS (rather than truncating) if the output would overflow the
/// buffer, so the encoders' `try` propagates; `buffered()` is the written prefix.
struct FixedWriter<'b> {
    buf: &'b mut [u8],
    len: usize,
}

impl<'b> FixedWriter<'b> {
    fn new(buf: &'b mut [u8]) -> FixedWriter<'b> {
        FixedWriter { buf, len: 0 }
    }

    // writer.buffered()
    fn buffered(&self) -> &[u8] {
        &self.buf[..self.len]
    }
}

impl core::fmt::Write for FixedWriter<'_> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let bytes = s.as_bytes();
        if bytes.len() > self.buf.len() - self.len {
            // error.NoSpaceLeft — the reference `.fixed` writer's overflow error.
            return Err(core::fmt::Error);
        }
        self.buf[self.len..self.len + bytes.len()].copy_from_slice(bytes);
        self.len += bytes.len();
        Ok(())
    }
}

// PORT NOTE: the `log.warn` call site in `prepareInput`, preserved verbatim
// (cf_release log-helper precedent).
fn log_warn_input_open_failed(_path: &[u8], _err: &std::io::Error) {
    // log.warn("failed to open input file={s} err={}", .{ path, err });
}

/// PORT NOTE: `std.fs.File.readToEndAlloc(alloc, max_bytes)` — reads the whole file
/// into an owned buffer, erroring if it exceeds `max_bytes` (the reference's
/// `error.FileTooBig`). Consumed by `Termio::threadEnter`'s input-file arm,
/// whose caller maps any error to `error.InputFailed`.
fn read_input_file(f: std::fs::File, max_bytes: usize) -> std::io::Result<Vec<u8>> {
    use std::io::Read;
    // Read up to `max_bytes + 1` so an over-limit file is detectable.
    let mut buf = Vec::new();
    f.take(max_bytes as u64 + 1).read_to_end(&mut buf)?;
    if buf.len() > max_bytes {
        return Err(std::io::Error::other("input file exceeds 10 MiB limit"));
    }
    Ok(buf)
}

/// Convert raw path bytes to an `OsStr`.
//
// PORT NOTE: `path` is raw bytes; on POSIX they map directly to an `OsStr`
// (rustty-config `file_load.rs` precedent).
#[cfg(unix)]
fn os_str_from_bytes(bytes: &[u8]) -> &std::ffi::OsStr {
    use std::os::unix::ffi::OsStrExt;
    std::ffi::OsStr::from_bytes(bytes)
}

#[cfg(not(unix))]
fn os_str_from_bytes(bytes: &[u8]) -> &std::ffi::OsStr {
    std::ffi::OsStr::new(std::str::from_utf8(bytes).unwrap_or(""))
}
