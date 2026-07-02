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
//! The live IO-thread execution paths (`init`/`threadEnter`/`changeConfig`/
//! `resize`/`sizeReport*`/`resetSynchronizedOutput`/`clearScreen`/
//! `scrollViewport`/`jumpToPrompt`/`focusGained`/`processOutput*`/
//! `colorSchemeReport*`) are DEFERRED with faithful signatures + verbatim
//! reference bodies in comments + `unimplemented!()` markers (0 reference
//! tests — R1-safe): they execute on the termio thread against the live
//! renderer mutex / xev loop / StreamHandler, i.e. the D6 xev decision, the
//! renderer-mutex modeling, and stage-4 stream_handler.zig. NOT fabricated
//! logic. See .port/phase-2/DEFERRALS.md ("rustty-termio stage 2").

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
use rustty_renderer::size::Size;
use rustty_renderer::state::State;
use rustty_renderer::thread;
use rustty_vt::color;
use rustty_vt::cursor::Style as CursorStyle;
use rustty_vt::terminal::Terminal;

use crate::backend;
use crate::mailbox::Mailbox;
use crate::message::Message;
use crate::options::Options;
use crate::thread::Thread;
use crate::xev_seam as xev;
use crate::Backend;
use crate::Error;

// const log = std.log.scoped(.io_exec);
//
// PORT: routed through named log helpers below that preserve the reference's
// messages verbatim (rustty-os cf_release_thread precedent).

/// PORT: the reference `terminal_stream` field type `StreamHandler.Stream`
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
        // (PORT: collapses into the owned Box/Vec allocations below.)

        // Allocate our ThreadEnterState
        // (PORT: the `arena_alloc.create(ThreadEnterState)` is the Box below.)

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
        // (PORT: the arena collapses to Rust ownership — the `dupe` below is a
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
            // (PORT: the `arena` field collapsed — see the struct PORT NOTE.)
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
    //             .max_scrollback = opts.full_config.@"scrollback-limit",
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
    pub fn init(_alloc: Allocator, _opts: Options<'_, M>) -> Result<Termio<M>, Error> {
        unimplemented!(
            "termio.Termio.init deferred (stage-4 stream_handler + D6); verbatim body above"
        )
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
    pub fn threadEnter(
        &mut self,
        _thread: &mut Thread,
        _data: &mut ThreadData<M>,
    ) -> Result<(), Error> {
        unimplemented!("termio.Termio.threadEnter deferred (D6); verbatim body above")
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
    //
    // DEFERRED (renderer-mutex modeling + stage-4 stream handler, 0 tests —
    // R1-safe): the body swaps the config under the renderer lock and updates
    // the stream handler (`self.terminal_stream.handler.changeConfig`, stage
    // 4). Preserved verbatim:
    //
    // pub fn changeConfig(self: *Termio, td: *ThreadData, config: *DerivedConfig) !void {
    //     // The remainder of this function is modifying terminal state or
    //     // the read thread data, all of which requires holding the renderer
    //     // state lock.
    //     self.renderer_state.mutex.lock();
    //     defer self.renderer_state.mutex.unlock();
    //
    //     // Deinit our old config. We do this in the lock because the
    //     // stream handler may be referencing the old config (i.e. enquiry resp)
    //     self.config.deinit();
    //     self.config = config.*;
    //
    //     // Update our stream handler. The stream handler uses the same
    //     // renderer mutex so this is safe to do despite being executed
    //     // from another thread.
    //     self.terminal_stream.handler.changeConfig(&self.config);
    //     td.backend.changeConfig(&self.config);
    //
    //     // Update the configuration that we know about.
    //     //
    //     // Specific things we don't update:
    //     //   - command, working-directory: we never restart the underlying
    //     //   process so we don't care or need to know about these.
    //
    //     // Update the default palette.
    //     self.terminal.colors.palette.changeDefault(config.palette);
    //     self.terminal.flags.dirty.palette = true;
    //
    //     // Update all our other colors
    //     self.terminal.colors.background.default = config.background.toTerminalRGB();
    //     self.terminal.colors.foreground.default = config.foreground.toTerminalRGB();
    //     self.terminal.colors.cursor.default = cursor: {
    //         const color = config.cursor_color orelse break :cursor null;
    //         break :cursor color.toTerminalRGB() orelse break :cursor null;
    //     };
    //
    //     // Set the image limits
    //     try self.terminal.setKittyGraphicsSizeLimit(self.alloc, config.image_storage_limit);
    //     self.terminal.setKittyGraphicsLoadingLimits(.all);
    // }
    pub fn changeConfig(
        &mut self,
        _td: &mut ThreadData<M>,
        _config: &mut DerivedConfig,
    ) -> Result<(), Error> {
        unimplemented!("termio.Termio.changeConfig deferred (renderer-mutex modeling + stage-4 stream handler); verbatim body above")
    }

    /// Resize the terminal.
    //
    // DEFERRED (D6 xev wakeup + renderer-mutex modeling, 0 tests — R1-safe).
    // Preserved verbatim:
    //
    // pub fn resize(
    //     self: *Termio,
    //     td: *ThreadData,
    //     size: renderer.Size,
    // ) !void {
    //     self.size = size;
    //     const grid_size = size.grid();
    //
    //     // Update the size of our pty.
    //     try self.backend.resize(grid_size, size.terminal());
    //
    //     // Enter the critical area that we want to keep small
    //     {
    //         self.renderer_state.mutex.lock();
    //         defer self.renderer_state.mutex.unlock();
    //
    //         // Update the size of our terminal state
    //         try self.terminal.resize(
    //             self.alloc,
    //             grid_size.columns,
    //             grid_size.rows,
    //         );
    //
    //         // Update our pixel sizes
    //         self.terminal.width_px = grid_size.columns * self.size.cell.width;
    //         self.terminal.height_px = grid_size.rows * self.size.cell.height;
    //
    //         // Disable synchronized output mode so that we show changes
    //         // immediately for a resize. This is allowed by the spec.
    //         self.terminal.modes.set(.synchronized_output, false);
    //
    //         // If we have size reporting enabled we need to send a report.
    //         if (self.terminal.modes.get(.in_band_size_reports)) {
    //             try self.sizeReportLocked(td, .mode_2048);
    //         }
    //     }
    //
    //     // Mail the renderer so that it can update the GPU and re-render
    //     _ = self.renderer_mailbox.push(.{ .resize = size }, .{ .forever = {} });
    //     self.renderer_wakeup.notify() catch {};
    // }
    pub fn resize(&mut self, _td: &mut ThreadData<M>, _size: Size) -> Result<(), Error> {
        unimplemented!(
            "termio.Termio.resize deferred (D6 + renderer-mutex modeling); verbatim body above"
        )
    }

    /// Make a size report.
    //
    // DEFERRED (renderer-mutex modeling, 0 tests — R1-safe). Preserved
    // verbatim:
    //
    // pub fn sizeReport(self: *Termio, td: *ThreadData, style: termio.Message.SizeReport) !void {
    //     self.renderer_state.mutex.lock();
    //     defer self.renderer_state.mutex.unlock();
    //     try self.sizeReportLocked(td, style);
    // }
    pub fn sizeReport(
        &mut self,
        _td: &mut ThreadData<M>,
        _style: crate::message::SizeReport,
    ) -> Result<(), Error> {
        unimplemented!(
            "termio.Termio.sizeReport deferred (renderer-mutex modeling); verbatim body above"
        )
    }

    // DEFERRED (with sizeReport above). Preserved verbatim:
    //
    // fn sizeReportLocked(self: *Termio, td: *ThreadData, style: termio.Message.SizeReport) !void {
    //     const grid_size = self.size.grid();
    //     const report_size: terminalpkg.size_report.Size = .{
    //         .rows = grid_size.rows,
    //         .columns = grid_size.columns,
    //         .cell_width = self.size.cell.width,
    //         .cell_height = self.size.cell.height,
    //     };
    //
    //     // 1024 bytes should be enough for size report since report
    //     // in columns and pixels.
    //     var buf: [1024]u8 = undefined;
    //     var writer: std.Io.Writer = .fixed(&buf);
    //     try terminalpkg.size_report.encode(
    //         &writer,
    //         style,
    //         report_size,
    //     );
    //
    //     try self.queueWrite(td, writer.buffered(), false);
    // }
    // (dead_code: the only reference caller, `sizeReport`, is deferred above.)
    #[allow(dead_code)]
    fn sizeReportLocked(
        &mut self,
        _td: &mut ThreadData<M>,
        _style: crate::message::SizeReport,
    ) -> Result<(), Error> {
        unimplemented!("termio.Termio.sizeReportLocked deferred (renderer-mutex modeling); verbatim body above")
    }

    /// Reset the synchronized output mode. This is usually called by timer
    /// expiration from the termio thread.
    //
    // DEFERRED (D6 xev wakeup + renderer-mutex modeling, 0 tests — R1-safe).
    // Preserved verbatim:
    //
    // pub fn resetSynchronizedOutput(self: *Termio) void {
    //     self.renderer_state.mutex.lock();
    //     defer self.renderer_state.mutex.unlock();
    //     self.terminal.modes.set(.synchronized_output, false);
    //     self.renderer_wakeup.notify() catch {};
    // }
    pub fn resetSynchronizedOutput(&mut self) {
        unimplemented!("termio.Termio.resetSynchronizedOutput deferred (D6 + renderer-mutex modeling); verbatim body above")
    }

    /// Clear the screen.
    //
    // DEFERRED (renderer-mutex modeling + the Zig-aliasing kitty delete call,
    // 0 tests — R1-safe; see memory `rust-mut-args-strongly-protected`).
    // Preserved verbatim:
    //
    // pub fn clearScreen(self: *Termio, td: *ThreadData, history: bool) !void {
    //     {
    //         self.renderer_state.mutex.lock();
    //         defer self.renderer_state.mutex.unlock();
    //
    //         // If we're on the alternate screen, we do not clear. Since this is an
    //         // emulator-level screen clear, this messes up the running programs
    //         // knowledge of where the cursor is and causes rendering issues. So,
    //         // for alt screen, we do nothing.
    //         if (self.terminal.screens.active_key == .alternate) return;
    //
    //         // Clear our selection
    //         self.terminal.screens.active.clearSelection();
    //
    //         // Clear our scrollback
    //         if (history) self.terminal.eraseDisplay(.scrollback, false);
    //
    //         // If we're not at a prompt, we just delete above the cursor.
    //         if (!self.terminal.cursorIsAtPrompt()) {
    //             if (self.terminal.screens.active.cursor.y > 0) {
    //                 self.terminal.screens.active.eraseActive(
    //                     self.terminal.screens.active.cursor.y - 1,
    //                 );
    //             }
    //
    //             // Clear all Kitty graphics state for this screen. This copies
    //             // Kitty's behavior when Cmd+K deletes all Kitty graphics. I
    //             // didn't spend time researching whether it only deletes Kitty
    //             // graphics that are placed above the cursor or if it deletes
    //             // all of them. We delete all of them for now but if this behavior
    //             // isn't fully correct we should fix this later.
    //             self.terminal.screens.active.kitty_images.delete(
    //                 self.terminal.screens.active.alloc,
    //                 &self.terminal,
    //                 .{ .all = true },
    //             );
    //
    //             return;
    //         }
    //
    //         // At a prompt, we want to first fully clear the screen, and then after
    //         // send a FF (0x0C) to the shell so that it can repaint the screen.
    //         // Mark the current row as a not a prompt so we can properly
    //         // clear the full screen in the next eraseDisplay call.
    //         // TODO: fix this
    //         // self.terminal.markSemanticPrompt(.command);
    //         // assert(!self.terminal.cursorIsAtPrompt());
    //         self.terminal.eraseDisplay(.complete, false);
    //     }
    //
    //     // If we reached here it means we're at a prompt, so we send a form-feed.
    //     try self.queueWrite(td, &[_]u8{0x0C}, false);
    // }
    pub fn clearScreen(&mut self, _td: &mut ThreadData<M>, _history: bool) -> Result<(), Error> {
        unimplemented!(
            "termio.Termio.clearScreen deferred (renderer-mutex modeling); verbatim body above"
        )
    }

    /// Scroll the viewport
    //
    // DEFERRED (renderer-mutex modeling, 0 tests — R1-safe). Preserved
    // verbatim:
    //
    // pub fn scrollViewport(
    //     self: *Termio,
    //     scroll: terminalpkg.Terminal.ScrollViewport,
    // ) void {
    //     self.renderer_state.mutex.lock();
    //     defer self.renderer_state.mutex.unlock();
    //     self.terminal.scrollViewport(scroll);
    // }
    pub fn scrollViewport(&mut self, _scroll: rustty_vt::terminal::ScrollViewport) {
        unimplemented!(
            "termio.Termio.scrollViewport deferred (renderer-mutex modeling); verbatim body above"
        )
    }

    /// Jump the viewport to the prompt.
    //
    // DEFERRED (D6 xev wakeup + renderer-mutex modeling, 0 tests — R1-safe).
    // Preserved verbatim:
    //
    // pub fn jumpToPrompt(self: *Termio, delta: isize) !void {
    //     {
    //         self.renderer_state.mutex.lock();
    //         defer self.renderer_state.mutex.unlock();
    //         self.terminal.screens.active.scroll(.{ .delta_prompt = delta });
    //     }
    //
    //     try self.renderer_wakeup.notify();
    // }
    pub fn jumpToPrompt(&mut self, _delta: isize) -> Result<(), Error> {
        unimplemented!("termio.Termio.jumpToPrompt deferred (D6 + renderer-mutex modeling); verbatim body above")
    }

    /// Called when focus is gained or lost (when focus events are enabled)
    //
    // DEFERRED (renderer-mutex modeling, 0 tests — R1-safe). Preserved
    // verbatim:
    //
    // pub fn focusGained(self: *Termio, td: *ThreadData, focused: bool) !void {
    //     self.renderer_state.mutex.lock();
    //     const focus_event = self.renderer_state.terminal.modes.get(.focus_event);
    //     self.renderer_state.mutex.unlock();
    //
    //     // If we have focus events enabled, we send the focus event.
    //     if (focus_event) {
    //         var buf: [terminalpkg.focus.max_encode_size]u8 = undefined;
    //         var writer: std.Io.Writer = .fixed(&buf);
    //         terminalpkg.focus.encode(&writer, if (focused) .gained else .lost) catch |err| {
    //             log.err("error encoding focus event err={}", .{err});
    //             return;
    //         };
    //         try self.queueWrite(td, writer.buffered(), false);
    //     }
    //
    //     // We always notify our backend of focus changes.
    //     try self.backend.focusGained(td, focused);
    // }
    pub fn focusGained(&mut self, _td: &mut ThreadData<M>, _focused: bool) -> Result<(), Error> {
        unimplemented!(
            "termio.Termio.focusGained deferred (renderer-mutex modeling); verbatim body above"
        )
    }

    /// Process output from the pty. This is the manual API that users can
    /// call with pty data but it is also called by the read thread when using
    /// an exec subprocess.
    //
    // DEFERRED (renderer-mutex modeling + stage-4 stream handler, 0 tests —
    // R1-safe). Preserved verbatim:
    //
    // pub fn processOutput(self: *Termio, buf: []const u8) void {
    //     // We are modifying terminal state from here on out and we need
    //     // the lock to grab our read data.
    //     self.renderer_state.mutex.lock();
    //     defer self.renderer_state.mutex.unlock();
    //     self.processOutputLocked(buf);
    // }
    pub fn processOutput(&mut self, _buf: &[u8]) {
        unimplemented!("termio.Termio.processOutput deferred (renderer-mutex modeling + stage-4 stream handler); verbatim body above")
    }

    // DEFERRED (with processOutput above). Preserved verbatim:
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
    // (dead_code: the only reference caller, `processOutput`, is deferred above.)
    #[allow(dead_code)]
    fn processOutputLocked(&mut self, _buf: &[u8]) {
        unimplemented!("termio.Termio.processOutputLocked deferred (renderer-mutex modeling + stage-4 stream handler); verbatim body above")
    }

    /// Sends a DSR response for the current color scheme to the pty.
    //
    // DEFERRED (renderer-mutex modeling, 0 tests — R1-safe). Preserved
    // verbatim:
    //
    // pub fn colorSchemeReport(self: *Termio, td: *ThreadData, force: bool) !void {
    //     self.renderer_state.mutex.lock();
    //     defer self.renderer_state.mutex.unlock();
    //
    //     try self.colorSchemeReportLocked(td, force);
    // }
    pub fn colorSchemeReport(
        &mut self,
        _td: &mut ThreadData<M>,
        _force: bool,
    ) -> Result<(), Error> {
        unimplemented!("termio.Termio.colorSchemeReport deferred (renderer-mutex modeling); verbatim body above")
    }

    // DEFERRED (with colorSchemeReport above). Preserved verbatim:
    //
    // pub fn colorSchemeReportLocked(self: *Termio, td: *ThreadData, force: bool) !void {
    //     if (!force and !self.renderer_state.terminal.modes.get(.report_color_scheme)) {
    //         return;
    //     }
    //     const output = switch (self.config.conditional_state.theme) {
    //         .light => "\x1B[?997;2n",
    //         .dark => "\x1B[?997;1n",
    //     };
    //     try self.queueWrite(td, output, false);
    // }
    pub fn colorSchemeReportLocked(
        &mut self,
        _td: &mut ThreadData<M>,
        _force: bool,
    ) -> Result<(), Error> {
        unimplemented!("termio.Termio.colorSchemeReportLocked deferred (renderer-mutex modeling); verbatim body above")
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
        // (PORT: the reference poisons the struct; Rust ownership leaves the
        // value in place — callers must not reuse it after deinit.)
    }
}

// PORT: the `log.warn` call site in `prepareInput`, preserved verbatim
// (cf_release log-helper precedent).
fn log_warn_input_open_failed(_path: &[u8], _err: &std::io::Error) {
    // log.warn("failed to open input file={s} err={}", .{ path, err });
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
