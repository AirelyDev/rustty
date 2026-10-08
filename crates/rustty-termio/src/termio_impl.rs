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

// CORRECTION (parity tail S7-7): a process-static `GlobalZigAllocator`
// (`GLOBAL_ZIG_ALLOCATOR_FOR_OSC`) used to back the terminal `Stream`'s OSC
// parser, because `stream::Options.allocator` wanted a `ZigAllocatorRef`. It
// takes the runtime allocator handle now, and `init` passes the Rust heap,
// the application's `alloc` (`terminal_stream = .init(.{ .allocator = alloc,
// .handler = handler })`).

// const log = std.log.scoped(.io_exec);
//
// PORT NOTE (DECISION-logging.md §6/§6r4): `LOG_SCOPE` is the `@tagName` of the
// reference's `std.log.scoped(.io_exec)` — `Termio.zig` shares the scope with
// `Exec.zig`. Restored sites call the `log` facade with `target: LOG_SCOPE`;
// this replaces the EMPTY `log_warn_input_open_failed` stub the file used to
// carry (an empty `log_*` helper is a dropped diagnostic, not a facade).
const LOG_SCOPE: &str = "io_exec";

/// PORT NOTE: the reference `terminal_stream` field type `StreamHandler.Stream`
/// (= `terminal.Stream(StreamHandler)`, src/termio/stream_handler.zig — landed
/// stage 4). The vt Stream's OSC-parser allocator lifetime is `'static` here
/// (the field owns its handler for the Termio lifetime; only the deferred
/// `Termio::init` constructs it — `.init(.{ .allocator = alloc, .handler = handler })`).
pub type StreamHandlerStream<M> = crate::stream_handler::Stream<M>;

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
        // var inputs: std.ArrayList(Input) = try .initCapacity(
        //     alloc,
        //     self.input.list.items.len,
        // );
        //
        // PERF(port): `appendAssumeCapacity` — `with_capacity` + plain `push`.
        let mut inputs = Vec::with_capacity(self.input.list.len());
        // errdefer for (inputs.items) |item| item.deinit();
        //
        // (PORT NOTE (9566a1a87): N-A by ownership — the error return below
        // drops the partially-built `Vec`, and `Input::File`'s `std::fs::File`
        // closes on Drop; `Input.deinit` is Rust Drop.)
        for item in &self.input.list {
            inputs.push(match item {
                // .raw => |v| .{ .string = v },
                //
                // PERF(port) (9566a1a87): upstream stopped arena-duping the raw
                // bytes (`.string = v` borrows the ThreadEnterState arena); the
                // port keeps the owned clone — a borrow into
                // `(*self_).thread_enter_state` would be held across the
                // `(*self_).queueWrite(&mut ...)` calls in `threadEnter`
                // (SB hazard, memory `rust-mut-args-strongly-protected`).
                rustty_config::io::ReadableIO::Raw(v) => Input::String(v.clone()),
                // .path => |path| file: { ... },
                rustty_config::io::ReadableIO::Path(path) => {
                    let f = match std::fs::File::open(os_str_from_bytes(path)) {
                        Ok(f) => f,
                        Err(err) => {
                            // log.warn("failed to open input file={s} err={}", .{
                            //     path,
                            //     err,
                            // });
                            log::warn!(
                                target: LOG_SCOPE,
                                "failed to open input file={} err={err:?}",
                                String::from_utf8_lossy(path)
                            );
                            return Err(PrepareInputError::InputNotFound);
                        }
                    };

                    Input::File(f)
                }
            });
        }

        Ok(inputs)
    }
}

// PORT NOTE: reference file-private `const Input = union(enum)`; `pub` here
// because it is `prepareInput`'s element type.
pub enum Input {
    // string: []const u8,
    //
    // PORT NOTE: the reference `string` is the raw byte slice -> owned
    // `Vec<u8>` (R1: byte slices stay raw bytes, never a lossy String; see the
    // PERF(port) note at the `.raw` arm above).
    String(Vec<u8>),
    // file: std.Io.File,
    File(std::fs::File),
}

// fn deinit(self: Input) void {
//     switch (self) {
//         .string => {},
//         .file => |f| f.close(global.io()),
//     }
// }
//
// (PORT NOTE (9566a1a87): `Input.deinit` is Rust Drop — `.string` drops its
// `Vec` and `.file`'s `std::fs::File` closes the descriptor on drop.)

/// The error set of [`ThreadEnterState::prepareInput`] (see its PORT NOTE).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrepareInputError {
    InputNotFound,
}

// The configuration for this IO that is derived from the main
// configuration. This must be exported so that we don't need to
// pass around Config pointers which makes memory management a pain.
//
// PORT NOTE (00a19db51): the reference inserted `xtChecksumFlags` between this
// doc comment and `DerivedConfig`, so the comment sits above the function
// there. Rust keeps it, in upstream order, as a plain comment and repeats it
// as the doc comment of `DerivedConfig` below.

/// Convert the vt-xt-checksum-extension config, whose values are named for
/// what they turn on, to the XTCHECKSUM bits, which turn things off.
// fn xtChecksumFlags(
//     ext: configpkg.Config.XtChecksumExtension,
// ) terminalpkg.xt_checksum.Flags {
fn xt_checksum_flags(ext: rustty_config::XtChecksumExtension) -> rustty_vt::xt_checksum::Flags {
    // return .{
    //     .positive = !ext.negate,
    //     .no_attributes = !ext.attributes,
    //     .no_trim = !ext.trim,
    //     .undrawn = ext.undrawn,
    //     .full = ext.full,
    // };
    rustty_vt::xt_checksum::Flags::default()
        .with_positive(!ext.negate)
        .with_no_attributes(!ext.attributes)
        .with_no_trim(!ext.trim)
        .with_undrawn(ext.undrawn)
        .with_full(ext.full)
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
    // clipboard_write_limit: usize,
    pub clipboard_write_limit: usize,
    // PORT NOTE: `enquiry_response: []const u8` -> owned `Vec<u8>` (R1 raw
    // bytes; arena-`dupe`d in the reference).
    pub enquiry_response: Vec<u8>,
    // xt_checksum_report: bool,
    pub xt_checksum_report: bool,
    // xt_checksum_extension: terminalpkg.xt_checksum.Flags,
    pub xt_checksum_extension: rustty_vt::xt_checksum::Flags,
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
            // .clipboard_write_limit = config.@"clipboard-write-limit-bytes".value,
            clipboard_write_limit: config.clipboard_write_limit_bytes.value,
            enquiry_response: config.enquiry_response.clone(),
            // .xt_checksum_report = config.@"vt-xt-checksum-report",
            xt_checksum_report: config.vt_xt_checksum_report,
            // .xt_checksum_extension = xtChecksumFlags(config.@"vt-xt-checksum-extension"),
            xt_checksum_extension: xt_checksum_flags(config.vt_xt_checksum_extension),
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
    //         .xt_checksum_report = opts.config.xt_checksum_report,
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
            alloc.into(),
            rustty_vt::terminal::Options {
                cols: grid_size.columns,
                rows: grid_size.rows,
                max_scrollback_bytes: scrollback_limits(opts.full_config).0,
                max_scrollback_lines: scrollback_limits(opts.full_config).1,
                default_modes,
                // .default_cursor_style = opts.config.cursor_style,
                // .default_cursor_blink = opts.config.cursor_blink,
                default_cursor_style: opts.config.cursor_style,
                default_cursor_blink: opts.config.cursor_blink,
                // .default_xt_checksum = opts.config.xt_checksum_extension,
                default_xt_checksum: opts.config.xt_checksum_extension,
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
                    // .palette = .default,
                    palette: color::DynamicPalette::default_palette(),
                },
                kitty_image_storage_limit: opts.config.image_storage_limit,
                // .kitty_image_loading_limits = .allWithTempDir(global.tmpDirPath()),
                // PORT NOTE (oracle `e8525c0fd`, `P3-W11 T1` / `P3-W6 T4`
                // Step 4): `Limits.all` no longer exists — enabling the kitty
                // temporary-file medium REQUIRES naming the directory it is
                // restricted to. This is the only producer of that directory in
                // the app: `rustty_global::tmp_dir_path()` is
                // `global.tmpDirPath()`, published once by the app initializer
                // from the CAPTURED environment. `rustty-vt` (the libghostty-vt
                // artifact) may not reach `global` itself, which is exactly why
                // upstream threads it in from here.
                //
                // SAFETY: `rustty_global::tmp_dir_path()` returns
                // `&'static [u8]` (published once, never freed), so the
                // borrow `Limits::all_with_temp_dir` takes is live for
                // the whole process (P7-W2 T3, ruling 44).
                kitty_image_loading_limits: unsafe {
                    Limits::all_with_temp_dir(rustty_global::tmp_dir_path())
                },
            },
        )
        // (PORT NOTE: `Terminal::init`'s `OutOfMemory` maps to the opaque termio
        // `crate::Error` — the reference `try` bubbles the alloc failure.)
        .map_err(|_| Error)?;
        // errdefer term.deinit(alloc);
        // (PORT NOTE: `Terminal::init` and the `changeDefault` below are the
        // fallible steps, and both run BEFORE any write into `*self_`; the
        // errdefer is the explicit `term.deinit` on the `changeDefault` error.)

        // The default palette may be an allocator-owned copy, so it is set
        // once the terminal owns its memory and can release it on deinit.
        // try term.colors.palette.changeDefault(alloc, opts.config.palette);
        if term
            .colors
            .palette
            .change_default(alloc.into(), opts.config.palette)
            .is_err()
        {
            term.deinit(alloc.into());
            return Err(Error);
        }

        // PORT NOTE (oracle c594031d5): the "Set our default cursor style"
        // poke at `term.screens.active.cursor.cursor_style` that used to live
        // here is DELETED. The defaults are passed into `Terminal::init` above
        // (`default_cursor_style` / `default_cursor_blink`) and `init` applies
        // them via `setCursorStyle(.default)`.

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
        // .clipboard_write_limit = opts.config.clipboard_write_limit,
        let handler_clipboard_write_limit = opts.config.clipboard_write_limit;
        let handler_enquiry_response = opts.config.enquiry_response.clone();
        // .xt_checksum_report = opts.config.xt_checksum_report,
        let handler_xt_checksum_report = opts.config.xt_checksum_report;

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
            // (`clone`: the Zig value copy. `xev::Async` is `Copy` on every
            // backend but Windows', whose guarded state is only `Clone`.)
            #[allow(clippy::clone_on_copy)]
            (&raw mut (*self_).renderer_wakeup).write(renderer_wakeup.clone());
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
            // .clipboard_write_limit = opts.config.clipboard_write_limit,
            clipboard_write_limit: handler_clipboard_write_limit,
            // (reference field defaults: `kitty_clipboard_grants = .{}`,
            // `kitty_clipboard_write = null`.)
            kitty_clipboard_grants: rustty_vt::kitty::clipboard::Grants::default(),
            kitty_clipboard_write: None,
            // .enquiry_response = opts.config.enquiry_response,
            enquiry_response: handler_enquiry_response,
            // .xt_checksum_report = opts.config.xt_checksum_report,
            xt_checksum_report: handler_xt_checksum_report,
            // (reference field defaults for the handler fields not in the literal:
            // `apc = .{}`, `dcs = .{}`, `tmux_viewer = null`,
            // `termio_messaged = false`, `seen_title = false`.)
            apc: rustty_vt::apc::Handler::default(),
            dcs: rustty_vt::dcs::Handler::default(),
            tmux_viewer: None,
            termio_messaged: false,
            seen_title: false,
        };

        // .terminal_stream = .init(.{
        //     .allocator = alloc,
        //     .handler = handler,
        // }),
        //
        // SAFETY: `terminal_stream` is the last unwritten field of `*self_`; the
        // handler it takes references the already-written self fields above.
        let terminal_stream: StreamHandlerStream<M> =
            crate::stream_handler::Stream::init(rustty_vt::stream::Options {
                // (parity tail S7-7: `Options.allocator` is the runtime
                // handle; the application's `alloc` is the Rust heap.)
                allocator: Some(rustty_vt::alloc::Global),
                handler,
                continuation_max_bytes: None,
            });
        unsafe { (&raw mut (*self_).terminal_stream).write(terminal_stream) };

        Ok(())
    }

    // pub fn deinit(self: *Termio) void — consuming `self` (the reference
    // invalidates the struct; cf_release `Thread::deinit` precedent).
    pub fn deinit(mut self) {
        self.backend.deinit();
        self.terminal.deinit(self.alloc.into());
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
    //     defer if (inputs) |items| {
    //         for (items) |input| input.deinit();
    //     };
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
    //         .file => |f| {
    //             const contents = compat_file.readToEndAlloc(
    //                 f,
    //                 self.alloc,
    //                 10 * 1024 * 1024, // 10 MiB max
    //             ) catch |err| {
    //                 log.warn("failed to read input file err={}", .{err});
    //                 return error.InputFailed;
    //             };
    //             defer self.alloc.free(contents);
    //
    //             self.queueWrite(data, contents, false) catch |err| {
    //                 log.warn("failed to queue input file err={}", .{err});
    //                 return error.InputFailed;
    //             };
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
                // (PORT NOTE: `try v.prepareInput()` bubbles `error.InputNotFound`
                // to `threadMain`'s error switch. S212-F1: it is now
                // `Error::InputNotFound`; it used to be the opaque unit `Error`.)
                Some(v) => Some(v.prepareInput().map_err(|e| match e {
                    PrepareInputError::InputNotFound => Error::InputNotFound,
                })?),
                None => None,
            };
            // defer if (inputs) |items| {
            //     for (items) |input| input.deinit();
            // };
            //
            // (PORT NOTE (9566a1a87): N-A by ownership — `inputs` is consumed by
            // `into_iter().flatten()` below, dropping each `Input` (the File
            // closes) at the end of its iteration; any error return drops the
            // remainder. Rust Drop IS the reference `defer`.)

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
            // SAFETY: hands the raw `&raw mut (*self_).backend` receiver (N449:
            // `Backend::threadEnter` is raw-self, so no reference to the whole
            // backend is formed), the raw whole-Termio `self_` as `io` and the
            // raw `data` — matching the reference `self.backend.threadEnter(
            // self.alloc, self, data)`.
            unsafe {
                Backend::threadEnter(&raw mut (*self_).backend, (*self_).alloc, self_, &mut *data)
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
                    Input::String(v) => {
                        let r = unsafe { Self::queueWrite(self_, &mut *data, &v, false) };
                        if let Err(err) = &r {
                            log::warn!(
                                target: LOG_SCOPE,
                                "failed to queue input string err={err:?}"
                            );
                        }
                        r
                    }
                    // .file => |f| {
                    //     const contents = compat_file.readToEndAlloc(
                    //         f,
                    //         self.alloc,
                    //         10 * 1024 * 1024, // 10 MiB max
                    //     ) catch |err| {
                    //         log.warn("failed to read input file err={}", .{err});
                    //         return error.InputFailed;
                    //     };
                    //     defer self.alloc.free(contents);
                    //
                    //     self.queueWrite(data, contents, false) catch |err| {
                    //         log.warn("failed to queue input file err={}", .{err});
                    //         return error.InputFailed;
                    //     };
                    // },
                    //
                    // (the `defer self.alloc.free(contents)` is `bytes` dropping
                    // at the end of this arm.)
                    Input::File(f) => match read_input_file(f, 10 * 1024 * 1024) {
                        Ok(bytes) => {
                            let r = unsafe { Self::queueWrite(self_, &mut *data, &bytes, false) };
                            if let Err(err) = &r {
                                log::warn!(
                                    target: LOG_SCOPE,
                                    "failed to queue input file err={err:?}"
                                );
                            }
                            r
                        }
                        Err(err) => {
                            // log.warn("failed to read input file err={}", .{err});
                            log::warn!(
                                target: LOG_SCOPE,
                                "failed to read input file err={err:?}"
                            );
                            Err(Error)
                        }
                    },
                };
                if queued.is_err() {
                    // (each failing arm above logged its own reference message;
                    // all three return `error.InputFailed`.)
                    // errdefer self.backend.threadExit(data);
                    //
                    // SAFETY: `data` is fully initialized (backend.threadEnter
                    // succeeded); the read thread has NOT yet been joined, so
                    // `threadExit` (which writes the quit pipe + joins) is the
                    // correct teardown.
                    unsafe { Backend::threadExit(&raw mut (*self_).backend, &mut *data) };

                    // PORT NOTE: reference defect (R13), UPSTREAM-DEFECTS UD290.
                    // The reference stops at the `errdefer` above. By now
                    // `backend.threadEnter` (and any input already queued) has
                    // registered completions on the thread's loop that live in
                    // `data`: Exec's `process_wait_c`, `termios_timer_c` and the
                    // write-stream requests. `data` is `cb.data` in
                    // `Thread.threadMain_`'s frame, which is gone once this error
                    // returns there, and `Thread.threadMain` then runs
                    // `loop.run(.until_done)` to drain: the loop starts those
                    // completions from freed stack memory (a use-after-free,
                    // reachable from config: an `input` file over 10 MiB or one
                    // that cannot be read; measured on Linux as a SIGSEGV in the
                    // epoll backend's submission queue). `data` is also never
                    // deinitialized, which leaks the read-thread pipe, the process
                    // watcher and the write stream. The loop has not run yet, so
                    // nothing was started: stop it (a stopped loop never ticks
                    // again, and `threadMain` skips its drain; the drain had
                    // nothing to do here, since the mailbox and stop waits are
                    // registered only after `threadEnter` succeeds), then release
                    // `data` as the normal exit does (`threadMain_`'s
                    // `data.deinit()` + drop). The one visible difference: the
                    // "abrupt io thread exit detected" warning is not logged.
                    unsafe {
                        (*thread).loop_.stop();
                        (*data).deinit();
                        core::ptr::drop_in_place(data);
                    }

                    // return error.InputFailed;  (S212-F1: was the opaque `Error`.)
                    return Err(Error::InputFailed);
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

    // pub fn threadExit(self: *Termio, data: *ThreadData) void {
    //
    // PORT NOTE (N449, P7-W7 T5d): raw-self, the reference's `self: *Termio`,
    // like every io writer-thread method below (`queueWrite`, `changeConfig`,
    // `resize`, `sizeReport`, `resetSynchronizedOutput`, `clearScreen`,
    // `scrollViewport`, `jumpToPrompt`, `focusGained`, `kittyClipboardGrant`,
    // `colorSchemeReport`, `visibilityReport`). The writer thread runs them
    // while the io read thread writes `terminal` under the renderer mutex
    // (`processOutput`, raw-self), the render and main threads reach the
    // terminal by place under the same mutex, and the main thread reads
    // `mailbox` and `backend`. A `&mut self` is a protected Unique over the
    // whole `Termio`, terminal included, which races with each of them
    // (`.port/phase-2/mirihar-n444` `n449_writer_termio_mut` and
    // `n449_shipped`, UB under SB and TB). Each method reaches only the fields
    // the reference touches, by place: the terminal and the stream handler
    // only inside the renderer-mutex section, the backend only through the
    // raw-self `Backend` methods. `threadExit` runs after the writer loop but
    // before it joins the read thread.
    ///
    /// # Safety
    /// `self_` must point at a live, initialized `Termio<M>`; the caller is
    /// the io writer thread.
    pub unsafe fn threadExit(self_: *mut Termio<M>, data: &mut ThreadData<M>) {
        // self.backend.threadExit(data);
        unsafe { Backend::threadExit(&raw mut (*self_).backend, data) };
    }

    /// Send a message to the mailbox. Depending on the mailbox type in use
    /// this may process now or it may just enqueue and process later.
    ///
    /// This will also notify the mailbox thread to process the message. If
    /// you're sending a lot of messages, it may be more efficient to use
    /// the mailbox directly and then call notify separately.
    //
    // pub fn queueMessage(
    //     self: *Termio,
    //     msg: termio.Message,
    //     mutex: MutexState,
    // ) void {
    //
    // PORT NOTE (N444, P7-W7 T5c): raw-self, the reference's `self: *Termio`.
    // The main thread calls this (core `Surface::queueIo`) while the io read
    // thread writes `terminal` under the renderer mutex; a `&mut self` would be
    // a protected Unique over the whole `Termio`, terminal included
    // (`.port/phase-2/mirihar-n444` `n444_boxed_queue_io_termio_mut`, UB under
    // SB and TB). Only the two fields the reference touches are reached, by
    // place, and the mailbox only through `&self` (`Mailbox::send`'s note).
    ///
    /// # Safety
    /// `self_` must point at a live, initialized `Termio<M>` whose
    /// `renderer_state` is the live shared render state.
    pub unsafe fn queueMessage(self_: *mut Termio<M>, msg: Message<'static>, mutex: MutexState) {
        // PORT NOTE: `self.renderer_state.mutex` is the raw shared-state read
        // (renderer state.rs precedent); the `*mut Mutex` is handed to the
        // mailbox untouched.
        let mutex_ptr = match mutex {
            // .locked => self.renderer_state.mutex,
            MutexState::Locked => Some(unsafe { (*(*self_).renderer_state).mutex }),
            // .unlocked => null,
            MutexState::Unlocked => None,
        };
        // self.mailbox.send(msg, switch (mutex) { .. });
        unsafe { (*self_).mailbox.send(msg, mutex_ptr) };
        // self.mailbox.notify();
        unsafe { (*self_).mailbox.notify() };
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
    //
    // PORT NOTE (N449, P7-W7 T5d): raw-self (see `threadExit`).
    ///
    /// # Safety
    /// As [`Termio::threadExit`].
    #[inline]
    pub unsafe fn queueWrite(
        self_: *mut Termio<M>,
        td: &mut ThreadData<M>,
        data: &[u8],
        linefeed: bool,
    ) -> Result<(), Error> {
        // try self.backend.queueWrite(self.alloc, td, data, linefeed);
        unsafe {
            Backend::queueWrite(
                &raw mut (*self_).backend,
                (*self_).alloc,
                td,
                data,
                linefeed,
            )
        }
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
    //
    // PORT NOTE (N449, P7-W7 T5d): raw-self (see `threadExit`). `config`,
    // `terminal_stream` and `terminal` are reached by place, inside the
    // renderer-mutex section, as disjoint field borrows.
    ///
    /// # Safety
    /// As [`Termio::threadExit`].
    pub unsafe fn changeConfig(
        self_: *mut Termio<M>,
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
        unsafe { (*(*(*self_).renderer_state).mutex).lock() };

        // SAFETY: under the renderer mutex; the field borrows below are
        // disjoint places of the live `Termio`, and none outlives the unlock.
        // The terminal is borrowed only after the stream handler's
        // `changeConfig`, which writes it through the handler's own pointer.
        let this_config = unsafe { &mut (*self_).config };
        let terminal_stream = unsafe { &mut (*self_).terminal_stream };
        let alloc = unsafe { (*self_).alloc };

        // Deinit our old config. We do this in the lock because the
        // stream handler may be referencing the old config (i.e. enquiry resp)
        // self.config.deinit();
        this_config.deinit();
        // self.config = config.*;
        *this_config = config;

        // Update our stream handler. The stream handler uses the same
        // renderer mutex so this is safe to do despite being executed
        // from another thread.
        // self.terminal_stream.handler.changeConfig(&self.config);
        terminal_stream.handler.changeConfig(this_config);
        // td.backend.changeConfig(&self.config);
        td.backend.changeConfig(this_config);
        let terminal = unsafe { &mut (*self_).terminal };

        // Update the configuration that we know about.
        //
        // Specific things we don't update:
        //   - command, working-directory: we never restart the underlying
        //   process so we don't care or need to know about these.

        // Update the default palette. A config change must not fail here, so
        // if we can't allocate the copy of the configured palette we fall back
        // to the built-in default, which never allocates.
        // self.terminal.colors.palette.changeDefault(
        //     self.alloc,
        //     config.palette,
        // ) catch |err| {
        //     log.warn("error changing default palette, using built-in default err={}", .{err});
        //     self.terminal.colors.palette.resetDefault(self.alloc);
        // };
        if let Err(err) = terminal
            .colors
            .palette
            .change_default(alloc.into(), this_config.palette)
        {
            log::warn!(
                target: LOG_SCOPE,
                "error changing default palette, using built-in default err={err:?}"
            );
            terminal.colors.palette.reset_default(alloc.into());
        }
        // self.terminal.flags.dirty.palette = true;
        terminal.flags.dirty.palette = true;

        // Update all our other colors
        // self.terminal.colors.background.default = config.background.toTerminalRGB();
        terminal.colors.background.default = Some(this_config.background.to_terminal_rgb());
        // self.terminal.colors.foreground.default = config.foreground.toTerminalRGB();
        terminal.colors.foreground.default = Some(this_config.foreground.to_terminal_rgb());
        // self.terminal.colors.cursor.default = cursor: {
        //     const color = config.cursor_color orelse break :cursor null;
        //     break :cursor color.toTerminalRGB() orelse break :cursor null;
        // };
        terminal.colors.cursor.default = this_config
            .cursor_color
            .and_then(|color| color.to_terminal_rgb());

        // Set the image limits
        // self.terminal.setKittyGraphicsSizeLimit(self.alloc, config.image_storage_limit);
        // (`4371871bc`: eviction no longer allocates, so this cannot fail — the
        // pre-range `try` and its early-return gate are gone upstream.)
        terminal.set_kitty_graphics_size_limit(alloc.into(), this_config.image_storage_limit);
        // self.terminal.setKittyGraphicsLoadingLimits(.allWithTempDir(global.tmpDirPath()));
        // SAFETY: `rustty_global::tmp_dir_path()` is `&'static [u8]` — see
        // the `new`-time call site's note above.
        terminal.set_kitty_graphics_loading_limits(unsafe {
            Limits::all_with_temp_dir(rustty_global::tmp_dir_path())
        });

        // (the deferred renderer-state unlock)
        unsafe { (*(*(*self_).renderer_state).mutex).unlock() };
        Ok(())
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
    //
    // PORT NOTE (N449, P7-W7 T5d): raw-self (see `threadExit`). `size` is
    // written by place inside the mutex section (UD368, below); the pty
    // through the raw-self `Backend::resize`; `terminal` only inside the mutex
    // section.
    ///
    /// # Safety
    /// As [`Termio::threadExit`].
    pub unsafe fn resize(
        self_: *mut Termio<M>,
        td: &mut ThreadData<M>,
        size: Size,
    ) -> Result<(), Error> {
        // self.size = size;
        //
        // PORT NOTE: reference defect (R13), UD368. The reference writes
        // `self.size` here, before it takes the renderer mutex, while the io
        // read thread reads it under that mutex through the stream handler's
        // `size` pointer (`.enable_mode_3`): program output that sets mode 3
        // during a window resize races the write. The store moves into the
        // mutex section below. Nothing between here and there reads
        // `self.size` (`backend.resize` takes the sizes as arguments), and a
        // failing `backend.resize` still stores it before returning, as the
        // reference does (`.port/phase-2/mirihar-n444`
        // `ud368_size_before_lock` UB under SB and TB, `ud368_size_under_lock`
        // CLEAN).
        //
        // const grid_size = size.grid();
        let grid_size = size.grid();

        // Update the size of our pty.
        // try self.backend.resize(grid_size, size.terminal());
        let backend_resized =
            unsafe { Backend::resize(&raw mut (*self_).backend, grid_size, size.terminal()) };

        // Enter the critical area that we want to keep small
        // {
        //     self.renderer_state.mutex.lock();
        //     defer self.renderer_state.mutex.unlock();
        //
        // SAFETY: `renderer_state` points at the live shared render State for
        // the Termio's lifetime; `.mutex` is the guard-less shared lock.
        unsafe { (*(*(*self_).renderer_state).mutex).lock() };
        // (UD368: the reference's `self.size = size`, under the mutex.)
        unsafe { (*self_).size = size };
        if let Err(e) = backend_resized {
            // (the reference's `try self.backend.resize(..)` returned here,
            // before the critical area.)
            unsafe { (*(*(*self_).renderer_state).mutex).unlock() };
            return Err(e);
        }
        let locked: Result<(), Error> = 'locked: {
            // Update the size of our terminal state
            // try self.terminal.resize(self.alloc, .{
            //     .cols = grid_size.columns,
            //     .rows = grid_size.rows,
            //     .cell_size_px = .{
            //         .width = self.size.cell.width,
            //         .height = self.size.cell.height,
            //     },
            // });
            //
            // PORT NOTE (oracle 89b103dd5, area E8): the pixel-geometry
            // assignment and the `modes.set(.synchronized_output, false)` that
            // used to live HERE both moved INTO `Terminal::resize`. Do not
            // re-add them: `Terminal::resize` applies the geometry even when the
            // grid dimensions are unchanged, and the clear must happen exactly
            // once (guarded by `modes::sync_output_clear_count`).
            let cell_size = rustty_vt::terminal::ResizeCellSize {
                width: unsafe { (*self_).size.cell.width },
                height: unsafe { (*self_).size.cell.height },
            };
            // SAFETY: under the renderer mutex; a transient borrow of the
            // `terminal` place only.
            if unsafe { &mut (*self_).terminal }
                .resize(
                    unsafe { (*self_).alloc }.into(),
                    rustty_vt::terminal::Resize {
                        cols: grid_size.columns,
                        rows: grid_size.rows,
                        cell_size_px: Some(cell_size),
                    },
                )
                .is_err()
            {
                break 'locked Err(Error);
            }

            // If we have size reporting enabled we need to send a report.
            // if (self.terminal.modes.get(.in_band_size_reports)) {
            //     try self.sizeReportLocked(td, .mode_2048);
            // }
            if unsafe { (*self_).terminal.modes.get(Mode::InBandSizeReports) } {
                if let Err(e) =
                    unsafe { Self::sizeReportLocked(self_, td, size_report::Style::Mode2048) }
                {
                    break 'locked Err(e);
                }
            }

            Ok(())
        };
        // }
        // (the deferred renderer-state unlock)
        unsafe { (*(*(*self_).renderer_state).mutex).unlock() };
        locked?;

        // Mail the renderer so that it can update the GPU and re-render
        // _ = self.renderer_mailbox.push(.{ .resize = size }, .{ .forever = {} });
        //
        // SAFETY: `renderer_mailbox` points at the live renderer thread mailbox.
        let _ = unsafe { &*(*self_).renderer_mailbox }
            .push(RendererMessage::Resize(size), Timeout::Forever);
        // self.renderer_wakeup.notify() catch {};
        let _ = unsafe { (*self_).renderer_wakeup.notify() };
        Ok(())
    }

    /// Make a size report.
    // pub fn sizeReport(self: *Termio, td: *ThreadData, style: termio.Message.SizeReport) !void {
    //
    // PORT NOTE (N449, P7-W7 T5d): raw-self (see `threadExit`).
    ///
    /// # Safety
    /// As [`Termio::threadExit`].
    pub unsafe fn sizeReport(
        self_: *mut Termio<M>,
        td: &mut ThreadData<M>,
        style: crate::message::SizeReport,
    ) -> Result<(), Error> {
        // self.renderer_state.mutex.lock();
        // defer self.renderer_state.mutex.unlock();
        //
        // SAFETY: guard-less shared renderer-state lock (see `changeConfig`).
        unsafe { (*(*(*self_).renderer_state).mutex).lock() };
        // try self.sizeReportLocked(td, style);
        let result = unsafe { Self::sizeReportLocked(self_, td, style) };
        // (the deferred renderer-state unlock — fires on the `try` error too.)
        unsafe { (*(*(*self_).renderer_state).mutex).unlock() };
        result
    }

    // fn sizeReportLocked(self: *Termio, td: *ThreadData, style: termio.Message.SizeReport) !void {
    //
    // (N449: raw-self; `size` is copied out by place.)
    unsafe fn sizeReportLocked(
        self_: *mut Termio<M>,
        td: &mut ThreadData<M>,
        style: crate::message::SizeReport,
    ) -> Result<(), Error> {
        let size = unsafe { (*self_).size };
        // const grid_size = self.size.grid();
        let grid_size = size.grid();
        // const report_size: terminalpkg.size_report.Size = .{
        //     .rows = grid_size.rows,
        //     .columns = grid_size.columns,
        //     .cell_width = self.size.cell.width,
        //     .cell_height = self.size.cell.height,
        // };
        let report_size = size_report::Size {
            rows: grid_size.rows,
            columns: grid_size.columns,
            cell_width: size.cell.width,
            cell_height: size.cell.height,
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
        unsafe { Self::queueWrite(self_, td, writer.buffered(), false) }
    }

    /// Reset the synchronized output mode. This is usually called by timer
    /// expiration from the termio thread.
    // pub fn resetSynchronizedOutput(self: *Termio) void {
    //
    // PORT NOTE (core C-T un-defer): guard-less C0 renderer lock; the final
    // `renderer_wakeup.notify()` is the xev placeholder leaf (D6, live-now).
    //
    // PORT NOTE (N449, P7-W7 T5d): raw-self (see `threadExit`).
    ///
    /// # Safety
    /// As [`Termio::threadExit`].
    pub unsafe fn resetSynchronizedOutput(self_: *mut Termio<M>) {
        // self.renderer_state.mutex.lock();
        // defer self.renderer_state.mutex.unlock();
        //
        // SAFETY: guard-less shared renderer-state lock (see `changeConfig`).
        unsafe { (*(*(*self_).renderer_state).mutex).lock() };
        // self.terminal.modes.set(.synchronized_output, false);
        unsafe { (*self_).terminal.modes.set(Mode::SynchronizedOutput, false) };
        // self.renderer_wakeup.notify() catch {};
        let _ = unsafe { (*self_).renderer_wakeup.notify() };
        // (the deferred renderer-state unlock)
        unsafe { (*(*(*self_).renderer_state).mutex).unlock() };
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
    //
    // PORT NOTE (N449, P7-W7 T5d): raw-self (see `threadExit`). The locked
    // section borrows the `terminal` place once (`terminal` below) and ends
    // that borrow at the unlock.
    ///
    /// # Safety
    /// As [`Termio::threadExit`].
    pub unsafe fn clearScreen(
        self_: *mut Termio<M>,
        td: &mut ThreadData<M>,
        history: bool,
    ) -> Result<(), Error> {
        // {
        //     self.renderer_state.mutex.lock();
        //     defer self.renderer_state.mutex.unlock();
        //
        // SAFETY: guard-less shared renderer-state lock (see `changeConfig`).
        unsafe { (*(*(*self_).renderer_state).mutex).lock() };
        let send_form_feed: bool = 'locked: {
            // SAFETY: under the renderer mutex; the `terminal` place only.
            let terminal = unsafe { &mut (*self_).terminal };
            // If we're on the alternate screen, we do not clear. Since this is an
            // emulator-level screen clear, this messes up the running programs
            // knowledge of where the cursor is and causes rendering issues. So,
            // for alt screen, we do nothing.
            // if (self.terminal.screens.active_key == .alternate) return;
            if terminal.screens.active_key == rustty_vt::screen_set::Key::Alternate {
                break 'locked false;
            }

            // Clear our selection
            // self.terminal.screens.active.clearSelection();
            //
            // SAFETY: `screens.active` is the live active `*mut Screen` owned by
            // the terminal's screen set; the deref is short-lived.
            unsafe { &mut *terminal.screens.active }.clear_selection();

            // Clear our scrollback
            // if (history) self.terminal.eraseDisplay(.scrollback, false);
            if history {
                terminal.erase_display(rustty_vt::csi::EraseDisplay::Scrollback, false);
            }

            // If we're not at a prompt, we just delete above the cursor.
            // if (!self.terminal.cursorIsAtPrompt()) {
            if !terminal.cursor_is_at_prompt() {
                // if (self.terminal.screens.active.cursor.y > 0) {
                //     self.terminal.screens.active.eraseActive(
                //         self.terminal.screens.active.cursor.y - 1,
                //     );
                // }
                let cursor_y = unsafe { (*terminal.screens.active).cursor.y };
                if cursor_y > 0 {
                    unsafe { &mut *terminal.screens.active }.erase_active(cursor_y - 1);
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
                let active = terminal.screens.active;
                let alloc = unsafe { (*active).alloc };
                let mut storage = core::mem::take(unsafe { &mut (*active).kitty_images });
                storage.delete(
                    alloc,
                    terminal,
                    rustty_vt::kitty::graphics_command::DeleteAction::All(true),
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
            terminal.erase_display(rustty_vt::csi::EraseDisplay::Complete, false);

            // (fall-through: at a prompt — send the form-feed below)
            true
        };
        // }
        // (the deferred renderer-state unlock)
        unsafe { (*(*(*self_).renderer_state).mutex).unlock() };

        // If we reached here it means we're at a prompt, so we send a form-feed.
        // try self.queueWrite(td, &[_]u8{0x0C}, false);
        if send_form_feed {
            unsafe { Self::queueWrite(self_, td, &[0x0C], false) }?;
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
    //
    // PORT NOTE (N449, P7-W7 T5d): raw-self (see `threadExit`).
    ///
    /// # Safety
    /// As [`Termio::threadExit`].
    pub unsafe fn scrollViewport(
        self_: *mut Termio<M>,
        scroll: rustty_vt::terminal::ScrollViewport,
    ) {
        // self.renderer_state.mutex.lock();
        // defer self.renderer_state.mutex.unlock();
        //
        // SAFETY: guard-less shared renderer-state lock (see `changeConfig`).
        unsafe { (*(*(*self_).renderer_state).mutex).lock() };
        // self.terminal.scrollViewport(scroll);
        unsafe { (*self_).terminal.scroll_viewport(scroll) };
        // (the deferred renderer-state unlock)
        unsafe { (*(*(*self_).renderer_state).mutex).unlock() };
    }

    /// Jump the viewport to the prompt.
    // pub fn jumpToPrompt(self: *Termio, delta: isize) !void {
    //
    // PORT NOTE (core C-T un-defer): guard-less C0 renderer lock; the final
    // `renderer_wakeup.notify()` is the xev placeholder leaf (D6, live-now).
    //
    // PORT NOTE (N449, P7-W7 T5d): raw-self (see `threadExit`).
    ///
    /// # Safety
    /// As [`Termio::threadExit`].
    pub unsafe fn jumpToPrompt(self_: *mut Termio<M>, delta: isize) -> Result<(), Error> {
        // {
        //     self.renderer_state.mutex.lock();
        //     defer self.renderer_state.mutex.unlock();
        //
        // SAFETY: guard-less shared renderer-state lock (see `changeConfig`).
        unsafe { (*(*(*self_).renderer_state).mutex).lock() };
        //     self.terminal.screens.active.scroll(.{ .delta_prompt = delta });
        //
        // SAFETY: `screens.active` is the live active `*mut Screen`.
        unsafe { &mut *(*self_).terminal.screens.active }
            .scroll(rustty_vt::screen::Scroll::DeltaPrompt(delta));
        // }
        // (the deferred renderer-state unlock)
        unsafe { (*(*(*self_).renderer_state).mutex).unlock() };

        // try self.renderer_wakeup.notify();
        unsafe { (*self_).renderer_wakeup.notify() }.map_err(|_| Error)
    }

    /// Called when focus is gained or lost (when focus events are enabled)
    // pub fn focusGained(self: *Termio, td: *ThreadData, focused: bool) !void {
    //
    // PORT NOTE (core C-T un-defer): guard-less C0 renderer lock; the mode is
    // read through the shared `renderer_state.terminal` pointer (which aliases
    // `self.terminal`), exactly as the reference. The `focus.encode` uses a
    // stack `FixedWriter` (the `std.Io.Writer.fixed` analogue).
    //
    // PORT NOTE (N449, P7-W7 T5d): raw-self (see `threadExit`).
    ///
    /// # Safety
    /// As [`Termio::threadExit`].
    pub unsafe fn focusGained(
        self_: *mut Termio<M>,
        td: &mut ThreadData<M>,
        focused: bool,
    ) -> Result<(), Error>
    where
        M: 'static,
    {
        // self.renderer_state.mutex.lock();
        //
        // SAFETY: guard-less shared renderer-state lock (see `changeConfig`);
        // `renderer_state.terminal` is the live terminal pointer.
        let renderer_state = unsafe { (*self_).renderer_state };
        unsafe { (*(*renderer_state).mutex).lock() };
        // const focus_event = self.renderer_state.terminal.modes.get(.focus_event);
        let focus_event = unsafe { (*(*renderer_state).terminal).modes.get(Mode::FocusEvent) };
        // self.renderer_state.mutex.unlock();
        unsafe { (*(*renderer_state).mutex).unlock() };

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
            if let Err(err) = focus::encode(&mut writer, event) {
                // log.err("error encoding focus event err={}", .{err});
                log::error!(target: LOG_SCOPE, "error encoding focus event err={err:?}");
                return Ok(());
            }
            // try self.queueWrite(td, writer.buffered(), false);
            unsafe { Self::queueWrite(self_, td, writer.buffered(), false) }?;
        }

        // We always notify our backend of focus changes.
        // try self.backend.focusGained(td, focused);
        Backend::focusGained(unsafe { &raw mut (*self_).backend }, td, focused)
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
                // CORRECTION (task S-1d, review S-1c F9): the arm cannot fire in
                // the REFERENCE either. `Stream.recordPtyRead`
                // (inspector/widgets/termio.zig:50-62) has no `try`, so its
                // inferred error set is empty and so is `Inspector.recordPtyRead`'s;
                // `Termio.zig:705` is a dead-branch row in LOG-DEAD-SITES.tsv.
                //
                // PORT NOTE (G-LOGFILE / task-14e-1, 2026-09-10): this is
                // `Termio.zig:705`, the ONE site of this file's six that stays
                // UNRESTORED, and it stays that way deliberately. There is no
                // failure arm to hang it on; emitting it unconditionally would
                // put a diagnostic on a success path (the bug this plan already
                // shipped once). `LOG-PORTMAP.tsv`'s `src/termio/Termio.zig`
                // allowance is held at 1 for exactly this site, with the reason
                // in its `reason` column; see `ADJUDICATION.md`.
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

    /// Record a Kitty clipboard protocol session grant so future requests
    /// carrying the password skip the permission prompt.
    // pub fn kittyClipboardGrant(
    //     self: *Termio,
    //     pw: []const u8,
    //     dir: terminalpkg.kitty.clipboard.Grants.Direction,
    // ) error{OutOfMemory}!void {
    //     self.renderer_state.mutex.lockUncancelable(global.io());
    //     defer self.renderer_state.mutex.unlock(global.io());
    //     try self.terminal_stream.handler.kittyClipboardGrant(pw, dir);
    // }
    //
    // PORT NOTE: same raw shared-state seam as `colorSchemeReport` (guard-less
    // renderer lock). `error{OutOfMemory}` is `Error::OutOfMemory`; the unlock
    // runs before it propagates.
    //
    // PORT NOTE (N449, P7-W7 T5d): raw-self (see `threadExit`); the stream
    // handler is reached by place under the mutex.
    ///
    /// # Safety
    /// As [`Termio::threadExit`].
    pub unsafe fn kittyClipboardGrant(
        self_: *mut Termio<M>,
        pw: &[u8],
        dir: rustty_vt::kitty::clipboard::Direction,
    ) -> Result<(), crate::Error> {
        // SAFETY: guard-less shared renderer-state lock (see `changeConfig`).
        unsafe { (*(*(*self_).renderer_state).mutex).lock() };
        let result = unsafe {
            (*self_)
                .terminal_stream
                .handler
                .kittyClipboardGrant(pw, dir)
        };
        unsafe { (*(*(*self_).renderer_state).mutex).unlock() };
        result.map_err(|_| crate::Error::OutOfMemory)
    }

    /// Sends a DSR response for the current color scheme to the pty.
    // pub fn colorSchemeReport(self: *Termio, td: *ThreadData, force: bool) !void {
    //
    // PORT NOTE (core C-T un-defer): guard-less C0 renderer lock.
    //
    // PORT NOTE (N449, P7-W7 T5d): raw-self (see `threadExit`).
    ///
    /// # Safety
    /// As [`Termio::threadExit`].
    pub unsafe fn colorSchemeReport(
        self_: *mut Termio<M>,
        td: &mut ThreadData<M>,
        force: bool,
    ) -> Result<(), Error> {
        // self.renderer_state.mutex.lock();
        // defer self.renderer_state.mutex.unlock();
        //
        // SAFETY: guard-less shared renderer-state lock (see `changeConfig`).
        unsafe { (*(*(*self_).renderer_state).mutex).lock() };
        // try self.colorSchemeReportLocked(td, force);
        let result = unsafe { Self::colorSchemeReportLocked(self_, td, force) };
        // (the deferred renderer-state unlock — fires on the `try` error too.)
        unsafe { (*(*(*self_).renderer_state).mutex).unlock() };
        result
    }

    // pub fn colorSchemeReportLocked(self: *Termio, td: *ThreadData, force: bool) !void {
    //
    // (N449: raw-self, as `colorSchemeReport`.)
    ///
    /// # Safety
    /// As [`Termio::threadExit`].
    pub unsafe fn colorSchemeReportLocked(
        self_: *mut Termio<M>,
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
                (*(*(*self_).renderer_state).terminal)
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
        let scheme = match unsafe { (*self_).config.conditional_state.theme } {
            conditional::Theme::Light => device_status::ColorScheme::Light,
            conditional::Theme::Dark => device_status::ColorScheme::Dark,
        };
        // PORT NOTE (oracle f00e90694, T7 Step 6): this call site ALREADY
        // routed through the shared `encode_color_scheme_report` rather than
        // hand-writing "\x1B[?997;1n"/";2n"; the only delta was that the buffer
        // did not name the encoder's advertised maximum. It does now, so the
        // constant and its call site cannot drift apart silently.
        let mut output = Vec::with_capacity(device_status::MAX_COLOR_SCHEME_REPORT_ENCODE_SIZE);
        device_status::encode_color_scheme_report(&mut output, scheme);
        // try self.queueWrite(td, output, false);
        unsafe { Self::queueWrite(self_, td, &output, false) }
    }

    /// Sends a visibility report to the pty. Unforced reports are only sent
    /// while DEC mode 2033 is enabled.
    // pub fn visibilityReport(self: *Termio, td: *ThreadData, visible: bool, force: bool) !void {
    //
    // PORT NOTE (N449, P7-W7 T5d): raw-self (see `threadExit`).
    ///
    /// # Safety
    /// As [`Termio::threadExit`].
    pub unsafe fn visibilityReport(
        self_: *mut Termio<M>,
        td: &mut ThreadData<M>,
        visible: bool,
        force: bool,
    ) -> Result<(), Error> {
        // self.renderer_state.mutex.lockUncancelable(global.io());
        // defer self.renderer_state.mutex.unlock(global.io());
        //
        // SAFETY: guard-less shared renderer-state lock (see `colorSchemeReport`).
        unsafe { (*(*(*self_).renderer_state).mutex).lock() };
        let result = unsafe { Self::visibilityReportLocked(self_, td, visible, force) };
        // (the deferred renderer-state unlock — fires on the `try` error too.)
        unsafe { (*(*(*self_).renderer_state).mutex).unlock() };
        result
    }

    // (The body of `visibilityReport` inside its lock; split out so the
    // guard-less unlock above is the single `defer`. N449: raw-self.)
    unsafe fn visibilityReportLocked(
        self_: *mut Termio<M>,
        td: &mut ThreadData<M>,
        visible: bool,
        force: bool,
    ) -> Result<(), Error> {
        // if (!force and !self.renderer_state.terminal.modes.get(.report_visibility)) return;
        //
        // SAFETY: `renderer_state.terminal` is the live shared terminal pointer;
        // the read happens under the caller-held lock.
        if !force
            && !unsafe {
                (*(*(*self_).renderer_state).terminal)
                    .modes
                    .get(Mode::ReportVisibility)
            }
        {
            return Ok(());
        }

        // var buf: [max_visibility_report_encode_size]u8 = undefined;
        // var writer: std.Io.Writer = .fixed(&buf);
        // try device_status.encodeVisibilityReport(&writer, ...);
        // try self.queueWrite(td, writer.buffered(), false);
        let mut buf = Vec::with_capacity(device_status::MAX_VISIBILITY_REPORT_ENCODE_SIZE);
        device_status::encode_visibility_report(
            &mut buf,
            if visible {
                device_status::Visibility::PotentiallyVisible
            } else {
                device_status::Visibility::NotVisible
            },
        );
        unsafe { Self::queueWrite(self_, td, &buf, false) }
    }

    /// Get information about the process(es) attached to the backend. Returns
    /// `null` if there was an error getting the information or the information is
    /// not available on a particular platform.
    //
    // PORT NOTE: reference `pub fn getProcessInfo(self: *Termio, comptime info:
    // ProcessInfo) ?ProcessInfo.Type(info) { return self.backend.getProcessInfo(info); }`
    // — split into the two concrete accessors (rustty-os `pty.rs` precedent;
    // see `Backend::getForegroundPid`).
    //
    // PORT NOTE (N444, P7-W7 T5c): raw-self, the reference's `self: *Termio`.
    // The main thread calls this (core `Surface::getForegroundPid`) while the
    // io read thread writes `terminal`; a `&mut self` would be a protected
    // Unique over the whole `Termio` (`.port/phase-2/mirihar-n444`
    // `n444_boxed_queue_io_termio_mut`, the same shape). Only `backend` is
    // reached, by place.
    ///
    /// # Safety
    /// `self_` must point at a live, initialized `Termio<M>`.
    pub unsafe fn getForegroundPid(self_: *mut Termio<M>) -> Option<u64> {
        // (N449: through the raw-self `Backend`, never a `&mut Backend`.)
        unsafe { Backend::getForegroundPid(&raw mut (*self_).backend) }
    }

    /// See [`Termio::getForegroundPid`] (the `.tty_name` half of the reference
    /// `getProcessInfo`).
    ///
    /// # Safety
    /// As [`Termio::getForegroundPid`]; the returned name borrows the
    /// backend's cached buffer, so the caller picks `'a` no longer than the
    /// `Termio` lives.
    pub unsafe fn getTtyName<'a>(self_: *mut Termio<M>) -> Option<&'a std::ffi::CStr> {
        // (N449: through the raw-self `Backend`, never a `&mut Backend`.)
        unsafe { Backend::getTtyName(&raw mut (*self_).backend) }
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

/// The `(bytes, lines)` scrollback limits `Termio::init` hands to
/// `terminal.Options`. `None` is unlimited on both axes.
//
// PORT NOTE: the reference writes these two expressions inline in
// `Termio.init`'s `opts:` block:
//
//     .max_scrollback_bytes = opts.full_config.@"scrollback-limit-bytes".optional(),
//     .max_scrollback_lines = opts.full_config.@"scrollback-limit-lines".optional(),
//
// They are hoisted into this helper ONLY so the config -> PageList handoff has
// a seam a test can drive without a pty (`P3-W3` T6 Step 4). `init` is the sole
// production caller, so the mapping still exists in exactly one place — a test
// that re-spelled `.optional()` itself would be vacuous.
pub(crate) fn scrollback_limits(full_config: &Config) -> (Option<usize>, Option<usize>) {
    (
        full_config.scrollback_limit_bytes.optional(),
        full_config.scrollback_limit_lines.optional(),
    )
}

// ===========================================================================
// `P3-W3` T6 Step 4 — the config -> PageList scrollback-limit handoff KAT.
// ===========================================================================
#[cfg(test)]
mod scrollback_limit_handoff_tests {
    use super::scrollback_limits;
    use allocator_api2::alloc::Global;
    use rustty_args::args::Target as _;
    use rustty_config::Config;
    use rustty_vt::terminal::Terminal;

    fn term_from_config(cfg: &Config, cols: u16, rows: u16) -> Terminal {
        let (bytes, lines) = scrollback_limits(cfg);
        Terminal::init(
            Global.into(),
            rustty_vt::terminal::Options {
                cols,
                rows,
                max_scrollback_bytes: bytes,
                max_scrollback_lines: lines,
                colors: rustty_vt::terminal::Colors {
                    background: rustty_vt::color::DynamicRGB::UNSET,
                    foreground: rustty_vt::color::DynamicRGB::UNSET,
                    cursor: rustty_vt::color::DynamicRGB::UNSET,
                    palette: rustty_vt::color::DynamicPalette::default_palette(),
                },
                default_modes: rustty_vt::modes::ModePacked::default(),
                default_cursor_style: rustty_vt::screen::CursorStyle::Block,
                default_cursor_blink: Some(false),
                default_xt_checksum: Default::default(),
                kitty_image_storage_limit: 0,
                kitty_image_loading_limits: rustty_vt::kitty::graphics_image::Limits::DIRECT,
            },
        )
        .unwrap()
    }

    fn history_rows(t: &Terminal) -> usize {
        // SAFETY: `screens.active` is a live heap Screen owned by the terminal.
        let pages = unsafe { &(*t.screens.active).pages };
        pages.total_rows - pages.rows as usize
    }

    /// `scrollback-limit-lines = n` must actually PRUNE at `n` after the
    /// terminal has been fed far more than `n` lines.
    ///
    /// This is deliberately a behaviour assertion, not a field-storage
    /// assertion: a test that only checks the option reached `Terminal::Options`
    /// would still pass with `Limits::enforce` never called, which is the exact
    /// failure mode this whole workstream exists to prevent.
    #[test]
    fn config_scrollback_limit_lines_prunes_the_pagelist() {
        let rows: u16 = 3;
        let mut cfg = Config::default();
        cfg.parse_into_field(b"scrollback-limit-lines", Some(b"64"))
            .unwrap();
        assert_eq!(Some(64usize), cfg.scrollback_limit_lines.optional());

        let mut t = term_from_config(&cfg, 80, rows);

        // The effective limit is raised to one standard page of rows, so
        // capture it rather than asserting the literal 64.
        let effective = {
            let pages = unsafe { &(*t.screens.active).pages };
            pages.limits.max(rustty_vt::pagelist::Key::Lines)
        };
        assert!(
            (64..usize::MAX).contains(&effective),
            "the configured line limit did not reach the PageList (effective={effective})"
        );

        // Feed FAR more lines than the limit.
        for _ in 0..(8 * effective) {
            t.linefeed().unwrap();
        }
        assert!(
            history_rows(&t) <= effective,
            "history {} > effective limit {}",
            history_rows(&t),
            effective
        );

        t.deinit(Global.into());
    }

    /// The default config leaves the line limit unlimited, so the same feed
    /// retains everything. Without this arm the test above could pass on a
    /// build where the line limit is hard-wired on.
    #[test]
    fn config_default_scrollback_limit_lines_is_unlimited() {
        let cfg = Config::default();
        assert_eq!(None, cfg.scrollback_limit_lines.optional());
        assert_eq!(Some(50_000_000usize), cfg.scrollback_limit_bytes.optional());

        let rows: u16 = 3;
        let mut t = term_from_config(&cfg, 80, rows);
        let feed = 8 * 595usize;
        for _ in 0..feed {
            t.linefeed().unwrap();
        }
        // The first `rows - 1` linefeeds only move the cursor down inside the
        // initial screen; everything after that becomes history.
        assert_eq!(feed - (rows as usize - 1), history_rows(&t));

        t.deinit(Global.into());
    }

    /// `unlimited` on the byte axis is distinct from the 50 MB default and
    /// reaches the PageList as "no explicit byte limit".
    #[test]
    fn config_scrollback_limit_bytes_unlimited_reaches_the_pagelist() {
        let mut cfg = Config::default();
        cfg.parse_into_field(b"scrollback-limit-bytes", Some(b"unlimited"))
            .unwrap();
        assert_eq!(None, cfg.scrollback_limit_bytes.optional());

        let mut t = term_from_config(&cfg, 80, 3);
        let pages = unsafe { &(*t.screens.active).pages };
        assert_eq!(usize::MAX, pages.limits.bytes.explicit);

        t.deinit(Global.into());
    }
}
