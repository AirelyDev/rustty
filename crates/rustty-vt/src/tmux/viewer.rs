//! rustty-vt tmux/viewer
//!
//! PORTS: ghostty src/terminal/tmux/viewer.zig
//!
//! PORT STATUS: done (vt-16). The tmux control-mode viewer/multiplexer that
//! drives a per-pane `Terminal`. Faithful port of all 9 reference tests.
//
// const std = @import("std");
// const Allocator = std.mem.Allocator;
// const ArenaAllocator = std.heap.ArenaAllocator;
// const testing = std.testing;
// const assert = @import("../../quirks.zig").inlineAssert;
// const size = @import("../size.zig");
// const CircBuf = @import("../../datastruct/main.zig").CircBuf;
// const CursorStyle = @import("../cursor.zig").Style;
// const Screen = @import("../Screen.zig");
// const ScreenSet = @import("../ScreenSet.zig");
// const Terminal = @import("../Terminal.zig");
// const Layout = @import("layout.zig").Layout;
// const control = @import("control.zig");
// const output = @import("output.zig");
//
// PORT NOTE: `assert` is the `inlineAssert` quirk in rustty-util. `size` types
// are hoisted into rustty-render-types (re-exported as `crate::size`).
// `CursorStyle` is `crate::cursor::Style`. The `Allocator` is the crate's
// `Global` seam, matching the rest of the crate.
//
// PORT NOTE (command queue): the reference `CommandQueue` is a `CircBuf(Command,
// undefined)`. The Rustty `CircBuf` requires `T: Copy`, and `Command` owns a
// `Vec<u8>` (the `.user` arm), so it cannot back the queue. A `VecDeque<Command>`
// reproduces the EXACT behavior the viewer relies on — FIFO `append`
// (`push_back`), `first` (`front`), `deleteOldest(1)` (`pop_front`), `len`,
// `empty`/`is_empty`, forward iteration on deinit — without the `Copy` bound. The
// `COMMAND_QUEUE_INITIAL` initial capacity is preserved via `with_capacity`.
//
// PORT NOTE (action arena): the reference accumulates return `Action`s into an
// `ArenaAllocator` whose lifetime is reset each `next()` call, returning a
// `[]const Action` that borrows from that arena (plus a pre-allocated
// `action_single` for infallible single-action returns). In Rust the faithful
// equivalent is an owned `Vec<Action>` stored on the `Viewer` (`self.actions`),
// cleared at the start of each `next()` and returned as `&[Action]` borrowing
// from `self`. `Action` owns its payload (`Command(Vec<u8>)`, `Windows(Vec<
// Window>)`), so there is no allocator to fail on for single-action returns —
// the `action_single` infallibility guarantee is structural here. Because the
// data is owned, the `.windows` action carries a CLONE of the window list rather
// than a borrow (`[]const Window`); behavior (the caller sees the current window
// set, empty on session change) is identical.

use std::collections::VecDeque;

use rustty_util::quirks::inlineAssert as assert;

use crate::cursor::Style as CursorStyle;
use crate::screen::Screen;
use crate::screen_set;
use crate::size;
use crate::terminal::{self, Terminal};
use allocator_api2::alloc::Global as Allocator;
use rustty_zigstd::array_hash_map::AutoArrayHashMapUnmanaged;

use super::control;
use super::layout::Layout;
use super::output;
use super::output::{Value, Variable};

// const log = std.log.scoped(.terminal_tmux_viewer);
const LOG_SCOPE: &str = "terminal_tmux_viewer";

// TODO: A list of TODOs as I think about them.
// - We need to make startup more robust so session and block can happen
//   out of order.
// - We need to ignore `output` for panes that aren't yet initialized
//   (until capture-panes are complete).
// - We should note what the active window pane is on the tmux side;
//   we can use this at least for initial focus.

// NOTE: There is some fragility here that can possibly break if tmux
// changes their implementation. In particular, the order of notifications
// and assurances about what is sent when are based on reading the tmux
// source code as of Dec, 2025. These aren't documented as fixed.
//
// I've tried not to depend on anything that seems like it'd change
// in the future. For example, it seems reasonable that command output
// always comes before session attachment. But, I am noting this here
// in case something breaks in the future we can consider it. We should
// be able to easily unit test all variations seen in the real world.

/// The initial capacity of the command queue. We dynamically resize
/// as necessary so the initial value isn't that important, but if we
/// want to feel good about it we should make it large enough to support
/// our most realistic use cases without resizing.
const COMMAND_QUEUE_INITIAL: usize = 8;

/// A viewer is a tmux control mode client that attempts to create
/// a remote view of a tmux session, including providing the ability to send
/// new input to the session.
///
/// This is the primary use case for tmux control mode, but technically
/// tmux control mode clients can do anything a normal tmux client can do,
/// so the `control.zig` and other files in this folder are more general
/// purpose.
///
/// This struct helps move through a state machine of connecting to a tmux
/// session, negotiating capabilities, listing window state, etc.
///
/// ## Viewer Lifecycle
///
/// The viewer progresses through several states from initial connection
/// to steady-state operation. Here is the full flow:
///
/// ```text
///                              ┌─────────────────────────────────────────────┐
///                              │           TMUX CONTROL MODE START           │
///                              │         (DCS 1000p received by host)        │
///                              └─────────────────┬───────────────────────────┘
///                                                │
///                                                ▼
///                              ┌─────────────────────────────────────────────┐
///                              │            startup_block                    │
///                              │                                             │
///                              │  Wait for initial %begin/%end block from    │
///                              │  tmux. This is the response to the initial  │
///                              │  command (e.g., "attach -t 0").             │
///                              └─────────────────┬───────────────────────────┘
///                                                │ %end / %error
///                                                ▼
///                              ┌─────────────────────────────────────────────┐
///                              │           startup_session                   │
///                              │                                             │
///                              │  Wait for %session-changed notification     │
///                              │  to get the initial session ID.             │
///                              └─────────────────┬───────────────────────────┘
///                                                │ %session-changed
///                                                ▼
///                              ┌─────────────────────────────────────────────┐
///                              │           command_queue                     │
///                              │                                             │
///                              │  Main operating state. Process commands     │
///                              │  sequentially and handle notifications.     │
///                              └─────────────────────────────────────────────┘
///                                                │
///                    ┌───────────────────────────┼───────────────────────────┐
///                    │                           │                           │
///                    ▼                           ▼                           ▼
///     ┌──────────────────────────┐ ┌──────────────────────────┐ ┌────────────────────────┐
///     │     tmux_version         │ │     list_windows         │ │   %output / %layout-   │
///     │                          │ │                          │ │   change / etc.        │
///     │  Query tmux version for  │ │  Get all windows in the  │ │                        │
///     │  compatibility checks.   │ │  current session.        │ │  Handle live updates   │
///     └──────────────────────────┘ └────────────┬─────────────┘ │  from tmux server.     │
///                                               │               └────────────────────────┘
///                                               ▼
///                              ┌─────────────────────────────────────────────┐
///                              │          syncLayouts                        │
///                              │                                             │
///                              │  For each window, parse layout and sync     │
///                              │  panes. New panes trigger capture commands. │
///                              └─────────────────┬───────────────────────────┘
///                                                │
///                    ┌───────────────────────────┴───────────────────────────┐
///                    │                  For each new pane:                   │
///                    ▼                                                       ▼
///     ┌──────────────────────────┐                            ┌──────────────────────────┐
///     │     pane_history         │                            │     pane_visible         │
///     │     (primary screen)     │                            │     (primary screen)     │
///     │                          │                            │                          │
///     │  Capture scrollback      │                            │  Capture visible area    │
///     │  history into terminal.  │                            │  into terminal.          │
///     └──────────────────────────┘                            └──────────────────────────┘
///                    │                                                       │
///                    ▼                                                       ▼
///     ┌──────────────────────────┐                            ┌──────────────────────────┐
///     │     pane_history         │                            │     pane_visible         │
///     │     (alternate screen)   │                            │     (alternate screen)   │
///     └──────────────────────────┘                            └──────────────────────────┘
///                    │                                                       │
///                    └───────────────────────────┬───────────────────────────┘
///                                                ▼
///                              ┌─────────────────────────────────────────────┐
///                              │          pane_state                         │
///                              │                                             │
///                              │  Query cursor position, cursor style,       │
///                              │  and alternate screen mode for all panes.   │
///                              └─────────────────────────────────────────────┘
///                                                │
///                                                ▼
///                              ┌─────────────────────────────────────────────┐
///                              │        READY FOR OPERATION                  │
///                              │                                             │
///                              │  Panes are populated with content. The      │
///                              │  viewer handles %output for live updates,   │
///                              │  %layout-change for pane changes, and       │
///                              │  %session-changed for session switches.     │
///                              └─────────────────────────────────────────────┘
/// ```
///
/// ## Error Handling
///
/// At any point, if an unrecoverable error occurs or tmux sends `%exit`,
/// the viewer transitions to the `defunct` state and emits an `.exit` action.
///
/// ## Session Changes
///
/// When `%session-changed` is received during `command_queue` state, the
/// viewer resets itself completely: clears all windows/panes, emits an
/// empty windows action, and restarts the `list_windows` flow for the new
/// session.
///
pub struct Viewer {
    /// Allocator used for all internal state.
    alloc: Allocator,

    /// Current state of the state machine.
    state: State,

    /// The current session ID we're attached to.
    session_id: usize,

    /// The tmux server version string (e.g., "3.5a"). We capture this
    /// on startup because it will allow us to change behavior between
    /// versions as necessary.
    tmux_version: Vec<u8>,

    /// The list of commands we've sent that we want to send and wait
    /// for a response for. We only send one command at a time just
    /// to avoid any possible confusion around ordering.
    command_queue: CommandQueue,

    /// The windows in the current session.
    windows: Vec<Window>,

    /// The panes in the current session, mapped by pane ID.
    panes: PanesMap,

    /// The arena used for the prior action allocated state. This contains
    /// the contents for the actions as well as the actions slice itself.
    // action_arena: ArenaAllocator.State,
    //
    /// A single action pre-allocated that we use for single-action
    /// returns (common). This ensures that we can never get allocation
    /// errors on single-action returns, especially those such as `.exit`.
    // action_single: [1]Action,
    //
    /// The accumulated actions returned by the most recent `next()` call.
    ///
    /// PORT NOTE: this owns what the reference's `action_arena` /
    /// `action_single` allocate. It is cleared at the start of each `next()`
    /// and returned as `&[Action]`.
    actions: Vec<Action>,
}

// pub const CommandQueue = CircBuf(Command, undefined);
//
// PORT NOTE: `Command` is file-private (reference `const Command`); the alias is
// kept crate-internal to honor that (the reference's `pub const CommandQueue`
// over a private element type is a Zig-ism with no Rust equivalent visibility).
type CommandQueue = VecDeque<Command>;
// pub const PanesMap = std.AutoArrayHashMapUnmanaged(usize, Pane);
pub type PanesMap = AutoArrayHashMapUnmanaged<usize, Pane>;

/// PORTS: ghostty `Viewer.Action`.
//
// PORT NOTE: the reference `command: []const u8` / `windows: []const Window`
// borrow from the action arena; the owned port carries `Vec<u8>` / `Vec<Window>`
// (see the action-arena PORT NOTE on `Viewer`).
pub enum Action {
    /// Tmux has closed the control mode connection, we should end
    /// our viewer session in some way.
    Exit,

    /// Send a command to tmux, e.g. `list-windows`. The caller
    /// should not worry about parsing this or reading what command
    /// it is; just send it to tmux as-is. This will include the
    /// trailing newline so you can send it directly.
    Command(Vec<u8>),

    /// Windows changed. This may add, remove or change windows. The
    /// caller is responsible for diffing the new window list against
    /// the prior one. Remember that for a given Viewer, window IDs
    /// are guaranteed to be stable. Additionally, tmux (as of Dec 2025)
    /// never reuses window IDs within a server process lifetime.
    Windows(Vec<Window>),
}

impl Action {
    /// The tag of this action, used by the test harness's `contains_tags`
    /// checks (`action == tag`).
    //
    // PORT NOTE: the reference's `std.meta.Tag(Action)` is reproduced as an
    // explicit `ActionTag` enum; `Action::tag()` is `@as(TagType, self)`.
    pub fn tag(&self) -> ActionTag {
        match self {
            Action::Exit => ActionTag::Exit,
            Action::Command(_) => ActionTag::Command,
            Action::Windows(_) => ActionTag::Windows,
        }
    }
}

/// PORTS: `std.meta.Tag(Viewer.Action)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionTag {
    Exit,
    Command,
    Windows,
}

// PORT NOTE: the reference `Action.format` uses Zig comptime reflection over the
// tagged union to print `Action{ .<tag> = <value> }`, special-casing
// `[]const u8` payloads with a `trim`. Rust has no such reflection; a
// hand-written `Display` reproduces the same shape. Only used for diagnostics
// (the reference logs it in `testViewer`), so it is faithful to the output
// shape, not exercised by any assertion.
impl std::fmt::Display for Action {
    fn fmt(&self, writer: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // try writer.writeAll(@typeName(T));
        writer.write_str("viewer.Viewer.Action")?;
        // try writer.writeAll("{ .");
        writer.write_str("{ .")?;
        match self {
            // try writer.writeAll(@tagName(...)); try writer.writeAll(" = ");
            // []const u8 => try writer.print("\"{s}\"", .{trim(...)}),
            Action::Exit => write!(writer, "exit =  }}"),
            Action::Command(v) => {
                write!(writer, "command = \"{}\" }}", trim_display(v))
            }
            // else => try writer.print("{any}", .{value}),
            Action::Windows(v) => write!(writer, "windows = {v:?} }}"),
        }
    }
}

/// PORTS: ghostty `Viewer.Input`.
pub enum Input {
    /// Data from tmux was received that needs to be processed.
    Tmux(control::Notification),
}

/// PORTS: ghostty `Viewer.Window`.
//
// PORT NOTE: the reference holds `layout_arena: ArenaAllocator.State` purely to
// free the `Layout` tree in one shot; the Rustty `Layout` owns its subtree
// (`Vec<Layout>`), so dropping `Window` frees the layout — the `layout_arena`
// field and the explicit `deinit` are unnecessary and dropped (see the layout.rs
// PORT NOTE).
#[derive(Debug, Clone)]
pub struct Window {
    pub id: usize,
    pub width: usize,
    pub height: usize,
    pub layout: Layout,
}

/// PORTS: ghostty `Viewer.Pane`.
pub struct Pane {
    pub terminal: Terminal,
}

impl Pane {
    // pub fn deinit(self: *Pane, alloc: Allocator) void {
    pub fn deinit(&mut self, alloc: Allocator) {
        // self.terminal.deinit(alloc);
        self.terminal.deinit(alloc);
    }
}

impl Viewer {
    /// Initialize a new viewer.
    ///
    /// The given allocator is used for all internal state. You must
    /// call deinit when you're done with the viewer to free it.
    // pub fn init(alloc: Allocator) Allocator.Error!Viewer {
    pub fn init(alloc: Allocator) -> Result<Viewer, OutOfMemory> {
        // Create our initial command queue
        // var command_queue: CommandQueue = try .init(alloc, COMMAND_QUEUE_INITIAL);
        // errdefer command_queue.deinit(alloc);
        let command_queue: CommandQueue = VecDeque::with_capacity(COMMAND_QUEUE_INITIAL);

        Ok(Viewer {
            alloc,
            state: State::StartupBlock,
            // The default value here is meaningless. We don't get started
            // until we receive a session-changed notification which will
            // set this to a real value.
            session_id: 0,
            tmux_version: Vec::new(),
            command_queue,
            // .windows = .empty,
            windows: Vec::new(),
            // .panes = .empty,
            panes: PanesMap::new(),
            // .action_arena = .{},
            // .action_single = undefined,
            actions: Vec::new(),
        })
    }

    // pub fn deinit(self: *Viewer) void {
    pub fn deinit(&mut self) {
        {
            // for (self.windows.items) |*window| window.deinit(self.alloc);
            // self.windows.deinit(self.alloc);
            //
            // PORT NOTE: `Window`/`Layout` own their subtree; dropping the Vec
            // frees the windows and their layouts. No explicit per-window deinit.
            self.windows.clear();
        }
        {
            // var it = self.command_queue.iterator(.forward);
            // while (it.next()) |command| command.deinit(self.alloc);
            // self.command_queue.deinit(self.alloc);
            for command in self.command_queue.iter() {
                command.deinit(self.alloc);
            }
            self.command_queue.clear();
        }
        {
            // var it = self.panes.iterator();
            // while (it.next()) |kv| kv.value_ptr.deinit(self.alloc);
            // self.panes.deinit(self.alloc);
            for pane in self.panes.values_mut() {
                pane.deinit(self.alloc);
            }
            self.panes.clear_retaining_capacity();
        }
        // if (self.tmux_version.len > 0) self.alloc.free(self.tmux_version);
        //
        // PORT NOTE: `tmux_version` is an owned `Vec<u8>`; dropping it frees the
        // string (the `len > 0` guard is irrelevant for an owned vector).
        // self.action_arena.promote(self.alloc).deinit();
    }

    /// Send in an input event (such as a tmux protocol notification,
    /// keyboard input for a pane, etc.) and process it. The returned
    /// list is a set of actions to take as a result of the input prior
    /// to the next input. This list may be empty.
    // pub fn next(self: *Viewer, input: Input) []const Action {
    pub fn next(&mut self, input: Input) -> &[Action] {
        // Developer note: this function must never return an error. If
        // an error occurs we must go into a defunct state or some other
        // state to gracefully handle it.
        // return switch (input) {
        //     .tmux => self.nextTmux(input.tmux),
        // };
        match input {
            Input::Tmux(n) => self.next_tmux(n),
        }
    }

    fn next_tmux(&mut self, n: control::Notification) -> &[Action] {
        // return switch (self.state) {
        match self.state {
            // .defunct => defunct: {
            //     log.info("received notification in defunct state, ignoring", .{});
            //     break :defunct &.{};
            // },
            State::Defunct => {
                log::info!(target: LOG_SCOPE, "received notification in defunct state, ignoring");
                self.actions.clear();
                &self.actions
            }

            // .startup_block => self.nextStartupBlock(n),
            State::StartupBlock => self.next_startup_block(n),
            // .startup_session => self.nextStartupSession(n),
            State::StartupSession => self.next_startup_session(n),
            // .command_queue => self.nextCommand(n),
            State::CommandQueue => self.next_command(n),
        }
    }

    fn next_startup_block(&mut self, n: control::Notification) -> &[Action] {
        // assert(self.state == .startup_block);
        assert(self.state == State::StartupBlock);

        // switch (n) {
        match n {
            // This is only sent by the DCS parser when we first get
            // DCS 1000p, it should never reach us here.
            // .enter => unreachable,
            control::Notification::Enter => unreachable!(),

            // I don't think this is technically possible (reading the
            // tmux source code), but if we see an exit we can semantically
            // handle this without issue.
            // .exit => return self.defunct(),
            control::Notification::Exit => self.defunct(),

            // Any begin and end (even error) is fine! Now we wait for
            // session-changed to get the initial session ID. session-changed
            // is guaranteed to come after the initial command output
            // since if the initial command is `attach` tmux will run that,
            // queue the notification, then do notificatins.
            // .block_end, .block_err => {
            control::Notification::BlockEnd(_) | control::Notification::BlockErr(_) => {
                // self.state = .startup_session;
                self.state = State::StartupSession;
                // return &.{};
                self.actions.clear();
                &self.actions
            }

            // I don't like catch-all else branches but startup is such
            // a special case of looking for very specific things that
            // are unlikely to expand.
            // else => return &.{},
            _ => {
                self.actions.clear();
                &self.actions
            }
        }
    }

    fn next_startup_session(&mut self, n: control::Notification) -> &[Action] {
        // assert(self.state == .startup_session);
        assert(self.state == State::StartupSession);

        // switch (n) {
        match n {
            // .enter => unreachable,
            control::Notification::Enter => unreachable!(),

            // .exit => return self.defunct(),
            control::Notification::Exit => self.defunct(),

            // .session_changed => |info| {
            control::Notification::SessionChanged { id, .. } => {
                // self.session_id = info.id;
                self.session_id = id;

                // var arena = self.action_arena.promote(self.alloc);
                // defer self.action_arena = arena.state;
                // _ = arena.reset(.free_all);
                self.actions.clear();

                // return self.enterCommandQueue(
                //     arena.allocator(),
                //     &.{ .tmux_version, .list_windows },
                // ) catch {
                //     log.warn("failed to queue command, becoming defunct", .{});
                //     return self.defunct();
                // };
                match self.enter_command_queue(&[Command::TmuxVersion, Command::ListWindows]) {
                    Ok(()) => &self.actions,
                    Err(OutOfMemory) => {
                        // log.warn("failed to queue command, becoming defunct", .{});
                        log::warn!(target: LOG_SCOPE, "failed to queue command, becoming defunct");
                        self.defunct()
                    }
                }
            }

            // else => return &.{},
            _ => {
                self.actions.clear();
                &self.actions
            }
        }
    }

    // PORT NOTE: `nextIdle` references the `.idle` state which is not present
    // in the `State` enum the reference defines (the `idle` case is dead code in
    // the reference too — nothing transitions into it and `nextIdle` is never
    // called). It is dropped along with the nonexistent `.idle` state.

    fn next_command(&mut self, n: control::Notification) -> &[Action] {
        // We have to be in a command queue, but the command queue MAY
        // be empty. If it is empty, then receivedCommandOutput will
        // handle it by ignoring any command output. That's okay!
        // assert(self.state == .command_queue);
        assert(self.state == State::CommandQueue);

        // Clear our prior arena so it is ready to be used for any
        // actions immediately.
        // {
        //     var arena = self.action_arena.promote(self.alloc);
        //     _ = arena.reset(.free_all);
        //     self.action_arena = arena.state;
        // }
        //
        // Setup our empty actions list that commands can populate.
        // var actions: std.ArrayList(Action) = .empty;
        //
        // PORT NOTE: `self.actions` is the owned actions buffer; clearing it is
        // the arena reset, and it is the `actions` list commands populate.
        self.actions.clear();

        // Track whether the in-flight command slot is available. Starts true
        // if queue is empty (no command in flight). Set to true when a command
        // completes (block_end/block_err) or the queue is reset (session_changed).
        // var command_consumed = self.command_queue.empty();
        let mut command_consumed = self.command_queue.is_empty();

        // switch (n) {
        match n {
            // .enter => unreachable,
            control::Notification::Enter => unreachable!(),
            // .exit => return self.defunct(),
            control::Notification::Exit => return self.defunct(),

            // inline .block_end, .block_err => |content, tag| {
            control::Notification::BlockEnd(content) => {
                // self.receivedCommandOutput(&actions, content, tag == .block_err) catch {
                //     log.warn("failed to process command output, becoming defunct", .{});
                //     return self.defunct();
                // };
                if self.received_command_output(&content, false).is_err() {
                    // log.warn("failed to process command output, becoming defunct", .{});
                    log::warn!(
                        target: LOG_SCOPE,
                        "failed to process command output, becoming defunct"
                    );
                    return self.defunct();
                }
                // Command is consumed since a block end/err is the output
                // from a command.
                // command_consumed = true;
                command_consumed = true;
            }
            control::Notification::BlockErr(content) => {
                // PORT NOTE: the reference handles `.block_end` and `.block_err`
                // in a single `inline` switch prong, so this arm carries the
                // same diagnostic as the `BlockEnd` arm above.
                // log.warn("failed to process command output, becoming defunct", .{});
                if self.received_command_output(&content, true).is_err() {
                    log::warn!(
                        target: LOG_SCOPE,
                        "failed to process command output, becoming defunct"
                    );
                    return self.defunct();
                }
                command_consumed = true;
            }

            // .output => |out| self.receivedOutput(out.pane_id, out.data) catch |err| {
            control::Notification::Output { pane_id, data } => {
                if let Err(err) = self.received_output(pane_id, &data) {
                    log::warn!(
                        target: LOG_SCOPE,
                        "failed to process output for pane id={pane_id}: {err:?}"
                    );
                }
            }

            // Session changed means we switched to a different tmux session.
            // We need to reset our state and start fresh with list-windows.
            // This completely replaces the viewer, so treat it like a fresh start.
            // .session_changed => |info| {
            control::Notification::SessionChanged { id, .. } => {
                // self.sessionChanged(&actions, info.id) catch {
                //     log.warn("failed to handle session change, becoming defunct", .{});
                //     return self.defunct();
                // };
                if self.session_changed(id).is_err() {
                    // log.warn("failed to handle session change, becoming defunct", .{});
                    log::warn!(
                        target: LOG_SCOPE,
                        "failed to handle session change, becoming defunct"
                    );
                    return self.defunct();
                }
                // Command is consumed because sessionChanged resets
                // our entire viewer.
                // command_consumed = true;
                command_consumed = true;
            }

            // Layout changed of a single window.
            // .layout_change => |info| self.layoutChanged(
            //     &actions, info.window_id, info.layout,
            // ) catch {
            control::Notification::LayoutChange {
                window_id, layout, ..
            } => {
                if self.layout_changed(window_id, &layout).is_err() {
                    // Note: in the future, we can probably handle a failure
                    // here with a fallback to remove this one window, list
                    // windows again, and try again.
                    // log.warn("failed to handle layout change, becoming defunct", .{});
                    log::warn!(
                        target: LOG_SCOPE,
                        "failed to handle layout change, becoming defunct"
                    );
                    return self.defunct();
                }
            }

            // A window was added to this session.
            // .window_add => |info| self.windowAdd(info.id) catch {
            control::Notification::WindowAdd { id } => {
                if self.window_add(id).is_err() {
                    // log.warn("failed to handle window add, becoming defunct", .{});
                    log::warn!(target: LOG_SCOPE, "failed to handle window add, becoming defunct");
                    return self.defunct();
                }
            }

            // The active pane changed. We don't care about this because
            // we handle our own focus.
            // .window_pane_changed => {},
            control::Notification::WindowPaneChanged { .. } => {}

            // We ignore this one. It means a session was created or
            // destroyed. If it was our own session we will get an exit
            // notification very soon. If it is another session we don't
            // care.
            // .sessions_changed => {},
            control::Notification::SessionsChanged => {}

            // We don't use window names for anything, currently.
            // .window_renamed => {},
            control::Notification::WindowRenamed { .. } => {}

            // This is for other clients, which we don't do anything about.
            // For us, we'll get `exit` or `session_changed`, respectively.
            // .client_detached, .client_session_changed => {},
            control::Notification::ClientDetached { .. }
            | control::Notification::ClientSessionChanged { .. } => {}
        }

        // After processing commands, we add our next command to
        // execute if we have one. We do this last because command
        // processing may itself queue more commands. We only emit a
        // command if a prior command was consumed (or never existed).
        // if (self.state == .command_queue and command_consumed) {
        if self.state == State::CommandQueue && command_consumed {
            // if (self.command_queue.first()) |next_command| {
            if let Some(next_command) = self.command_queue.front() {
                // We should not have any commands, because our nextCommand
                // always queues them.
                // if (comptime std.debug.runtime_safety) {
                //     for (actions.items) |action| {
                //         if (action == .command) assert(false);
                //     }
                // }
                if cfg!(debug_assertions) {
                    for action in &self.actions {
                        if action.tag() == ActionTag::Command {
                            assert(false);
                        }
                    }
                }

                // var arena = self.action_arena.promote(self.alloc);
                // defer self.action_arena = arena.state;
                // const arena_alloc = arena.allocator();
                //
                // var builder: std.Io.Writer.Allocating = .init(arena_alloc);
                // next_command.formatCommand(&builder.writer) catch
                //     return self.defunct();
                let mut builder: Vec<u8> = Vec::new();
                if next_command.format_command(&mut builder).is_err() {
                    return self.defunct();
                }
                // actions.append(arena_alloc, .{ .command = builder.writer.buffered() }) catch
                //     return self.defunct();
                self.actions.push(Action::Command(builder));
            }
        }

        // return actions.items;
        &self.actions
    }

    /// When the layout changes for a single window, a pane may be added
    /// or removed that we've never seen, in addition to the layout itself
    /// physically changing.
    ///
    /// To handle this, its similar to list-windows except we expect the
    /// window to already exist. We update the layout, do the initLayout
    /// call for any diffs, setup commands to capture any new panes,
    /// prune any removed panes.
    // fn layoutChanged(
    //     self: *Viewer,
    //     actions: *std.ArrayList(Action),
    //     window_id: usize,
    //     layout_str: []const u8,
    // ) !void {
    fn layout_changed(&mut self, window_id: usize, layout_str: &[u8]) -> Result<(), VtError> {
        // Find the window this layout change is for.
        // const window: *Window = window: for (self.windows.items) |*w| {
        //     if (w.id == window_id) break :window w;
        // } else {
        //     log.info("layout change for unknown window id={}", .{window_id});
        //     return;
        // };
        let window_idx = match self.windows.iter().position(|w| w.id == window_id) {
            Some(idx) => idx,
            None => {
                // log.info("layout change for unknown window id={}", .{window_id});
                log::info!(target: LOG_SCOPE, "layout change for unknown window id={window_id}");
                return Ok(());
            }
        };

        // Clear our prior window arena and setup our layout
        // window.layout = layout: {
        //     var arena = window.layout_arena.promote(self.alloc);
        //     defer window.layout_arena = arena.state;
        //     _ = arena.reset(.retain_capacity);
        //     break :layout Layout.parseWithChecksum(arena.allocator(), layout_str) catch |err| {
        //         log.info("failed to parse window layout id={} layout={s}", .{ window_id, layout_str });
        //         return err;
        //     };
        // };
        //
        // PORT NOTE: `parseWithChecksum` takes `&str`; the layout bytes are
        // always valid UTF-8 (tmux layout strings are ASCII). A non-UTF-8 byte
        // surfaces as a SyntaxError-equivalent parse failure.
        let layout_str_s = std::str::from_utf8(layout_str).map_err(|_| VtError::SyntaxError)?;
        let new_layout = match Layout::parse_with_checksum(layout_str_s) {
            Ok(l) => l,
            Err(_err) => {
                // log.info(
                //     "failed to parse window layout id={} layout={s}",
                //     .{ window_id, layout_str },
                // );
                log::info!(
                    target: LOG_SCOPE,
                    "failed to parse window layout id={window_id} layout={layout_str_s}"
                );
                return Err(VtError::SyntaxError);
            }
        };
        self.windows[window_idx].layout = new_layout;

        // Reset our arena so we can build up actions.
        // var arena = self.action_arena.promote(self.alloc);
        // defer self.action_arena = arena.state;
        // const arena_alloc = arena.allocator();
        //
        // Our initial action is to definitely let the caller know that
        // some windows changed.
        // try actions.append(arena_alloc, .{ .windows = self.windows.items });
        self.actions.push(Action::Windows(self.windows.clone()));

        // Sync up our panes
        // try self.syncLayouts(self.windows.items);
        //
        // PORT NOTE: the reference passes `self.windows.items` (a borrow of its
        // own window list) into `syncLayouts`. The owned port clones the window
        // list so `syncLayouts` does not alias `self.windows` while mutating it;
        // the pointer-equality fast path in `syncLayouts` is preserved by a
        // separate flag (see its PORT NOTE).
        let windows = self.windows.clone();
        self.sync_layouts(windows, true)?;
        Ok(())
    }

    /// When a window is added to the session, we need to refresh our window
    /// list to get the new window's information.
    // fn windowAdd(self: *Viewer, window_id: usize) !void {
    fn window_add(&mut self, window_id: usize) -> Result<(), OutOfMemory> {
        // _ = window_id; // We refresh all windows via list-windows
        let _ = window_id; // We refresh all windows via list-windows

        // Queue list-windows to get the updated window list
        // try self.queueCommands(&.{.list_windows});
        self.queue_commands(&[Command::ListWindows]);
        Ok(())
    }

    // fn syncLayouts(self: *Viewer, windows: []const Window) !void {
    //
    // PORT NOTE: `windows` is taken by value (owned). `windows_same` replaces
    // the reference's `windows.ptr != self.windows.items.ptr` check: it is true
    // when `windows` is a clone of `self.windows` (layout-change path) and false
    // when `windows` is a freshly built list (list-windows path).
    fn sync_layouts(&mut self, windows: Vec<Window>, windows_same: bool) -> Result<(), VtError> {
        // Go through the window layout and setup all our panes. We move
        // this into a new panes map so that we can easily prune our old
        // list.
        // var panes: PanesMap = .empty;
        // errdefer { ... }
        //
        // PORT NOTE: the reference `errdefer` deinits panes that are NOT shared
        // with `self.panes`. Here every fallible step below `Ok`s before mutating
        // `panes`, and the only fallible operations are `initLayout` (whose own
        // failure path is handled inline) and `queueCommands`/`ensure...` which
        // cannot fail with the owned VecDeque. We thread a manual cleanup on the
        // single fallible `init_layout` call to preserve the errdefer semantics.
        //
        // PORT NOTE (soundness): the reference's `initLayout` writes directly into
        // the new `panes` map, copying an existing `Pane` BITWISE from `panes_old`
        // (`*const PanesMap`) — the two maps then alias the same `Terminal` until
        // the final `self.panes = panes` swap. Rust cannot alias an owned
        // `Terminal`, and bitwise-moving one out from behind a shared `&PanesMap`
        // is unsound (raw-ptr discipline). We split the work: `init_layout`
        // produces the ordered set of leaf pane DESCRIPTORS (id + dims) without
        // touching the old panes; the construction below then builds `panes` by
        // MOVING each still-present pane out of `self.panes` (via
        // `fetch_swap_remove`) or constructing a fresh `Terminal`. The
        // get-or-init order, the new-pane command queueing, and the removed-pane
        // pruning are byte-faithful to the reference.
        let mut panes: PanesMap = PanesMap::new();

        // The pane ids present BEFORE this sync (used to distinguish carried-over
        // panes from brand-new ones — see the "added" loop below).
        let prev_ids: Vec<usize> = self.panes.keys().copied().collect();

        // Collect the leaf pane descriptors in layout order.
        // for (windows) |window| try initLayout(self.alloc, &self.panes, &panes, window.layout);
        let mut descriptors: Vec<PaneDescriptor> = Vec::new();
        for window in &windows {
            Self::init_layout(&mut descriptors, &window.layout);
        }

        // The set of pane ids present in the new layout (deduplicated, in
        // first-seen order — the get-or-init contract of the reference's
        // `initLayout`).
        let mut new_ids: Vec<usize> = Vec::new();
        for d in &descriptors {
            if !new_ids.contains(&d.id) {
                new_ids.push(d.id);
            }
        }

        // Build up the list of removed panes.
        // var removed: std.ArrayList(usize) = removed: { ... };
        // defer removed.deinit(self.alloc);
        let removed: Vec<usize> = {
            let mut removed: Vec<usize> = Vec::new();
            // var panes_it = self.panes.iterator();
            // while (panes_it.next()) |kv| {
            //     if (panes.contains(kv.key_ptr.*)) continue;
            //     try removed.append(self.alloc, kv.key_ptr.*);
            // }
            for key in self.panes.keys() {
                if new_ids.contains(key) {
                    continue;
                }
                removed.push(*key);
            }
            removed
        };

        // Ensure we can add the windows
        // try self.windows.ensureTotalCapacity(self.alloc, windows.len);
        self.windows
            .reserve(windows.len().saturating_sub(self.windows.len()));

        // Construct the new panes map: move each still-present pane out of
        // `self.panes`, or construct a fresh `Terminal` for genuinely new panes.
        // (This is the safe realization of the reference's `initLayout`
        // get-or-copy — see the soundness PORT NOTE above.)
        // .pane => |id| pane: {
        //     const gop = try panes_new.getOrPut(gpa_alloc, id);
        //     if (gop.found_existing) break :pane;
        //     errdefer _ = panes_new.swapRemove(gop.key_ptr.*);
        //     if (panes_old.getEntry(id)) |entry| { gop.value_ptr.* = entry.value_ptr.*; break :pane; }
        //     var t: Terminal = try .init(gpa_alloc, .{ .cols, .rows });
        //     errdefer t.deinit(gpa_alloc);
        //     gop.value_ptr.* = .{ .terminal = t };
        // },
        for d in &descriptors {
            // if (gop.found_existing) break :pane;
            if panes.contains(&d.id) {
                continue;
            }
            // If we already have this pane, it is already initialized
            // so just copy it over.
            // if (panes_old.getEntry(id)) |entry| { gop.value_ptr.* = entry.value_ptr.*; break :pane; }
            if let Some(entry) = self.panes.fetch_swap_remove(&d.id) {
                panes.put_no_clobber(d.id, entry.value);
                continue;
            }
            // TODO: We need to gracefully handle overflow of our
            // max cols/width here. In practice we shouldn't hit this
            // so we cast but its not safe.
            // var t: Terminal = try .init(gpa_alloc, .{ .cols = @intCast(layout.width), .rows = @intCast(layout.height) });
            // errdefer t.deinit(gpa_alloc);
            //
            // PORT NOTE (S209-F1): `@intCast` is a CHECKED narrowing in every
            // safe build mode — an out-of-range layout dimension is illegal
            // behavior detected, not a silent wrap. Bare `as` wrapped modulo
            // 2^16 and handed `Terminal::init` a wrong (possibly 0) size, which
            // the TODO above does NOT sanction: it says the cast "is not safe",
            // which is exactly the reference's trap. PORT_RULES R7:
            // `@intCast` -> `try_from(..).unwrap()`.
            let cols = size::CellCountInt::try_from(d.width).unwrap_or_else(|_| {
                panic!("@intCast: tmux pane layout width {} > max cols", d.width)
            });
            let rows = size::CellCountInt::try_from(d.height).unwrap_or_else(|_| {
                panic!("@intCast: tmux pane layout height {} > max rows", d.height)
            });
            let t: Terminal = match Terminal::init(self.alloc, terminal_opts(cols, rows)) {
                Ok(t) => t,
                Err(_) => {
                    // errdefer: clear out all the new panes not shared with
                    // self.panes. The still-present panes already moved into
                    // `panes` were taken from `self.panes` (owned), so deiniting
                    // every new-map entry here is correct (no aliasing).
                    //
                    // Clear out all the new panes.
                    for pane in panes.values_mut() {
                        pane.deinit(self.alloc);
                    }
                    return Err(VtError::OutOfMemory);
                }
            };
            // gop.value_ptr.* = .{ .terminal = t };
            panes.put_no_clobber(d.id, Pane { terminal: t });
        }

        // Get our list of added panes and setup our command queue
        // to populate them.
        // TODO: errdefer cleanup
        {
            // var panes_it = panes.iterator();
            // var added: bool = false;
            let mut added: bool = false;
            // while (panes_it.next()) |kv| {
            //     const pane_id: usize = kv.key_ptr.*;
            //     if (self.panes.contains(pane_id)) continue;
            //     added = true;
            //     try self.queueCommands(&.{ ... });
            // }
            //
            // PORT NOTE: the reference tests `self.panes.contains(pane_id)` to
            // skip panes carried over from the OLD map. We already drained those
            // out of `self.panes` above, so the equivalent "is this a brand-new
            // pane" test is `!prev_ids.contains(pane_id)`: a pane is new iff it
            // is NOT one of the ids that existed before this sync (captured in
            // `prev_ids`).
            for pane_id in panes.keys().copied().collect::<Vec<usize>>() {
                if prev_ids.contains(&pane_id) {
                    continue;
                }
                added = true;
                self.queue_commands(&[
                    Command::PaneHistory(CapturePane {
                        id: pane_id,
                        screen_key: screen_set::Key::Primary,
                    }),
                    Command::PaneVisible(CapturePane {
                        id: pane_id,
                        screen_key: screen_set::Key::Primary,
                    }),
                    Command::PaneHistory(CapturePane {
                        id: pane_id,
                        screen_key: screen_set::Key::Alternate,
                    }),
                    Command::PaneVisible(CapturePane {
                        id: pane_id,
                        screen_key: screen_set::Key::Alternate,
                    }),
                ]);
            }

            // If we added any panes, then we also want to resync the pane
            // state (terminal modes and cursor positions and so on).
            // if (added) try self.queueCommands(&.{.pane_state});
            if added {
                self.queue_commands(&[Command::PaneState]);
            }
        }

        // No more errors after this point. We're about to replace all
        // our owned state with our temporary state, and our errdefers
        // above will double-free if there is an error.
        // errdefer comptime unreachable;

        // Replace our window list if it changed. We assume it didn't
        // change if our pointer is pointing to the same data.
        // if (windows.ptr != self.windows.items.ptr) {
        //     for (self.windows.items) |*window| window.deinit(self.alloc);
        //     self.windows.clearRetainingCapacity();
        //     self.windows.appendSliceAssumeCapacity(windows);
        // }
        if !windows_same {
            self.windows.clear();
            self.windows.extend(windows);
        }

        // Replace our panes
        {
            // First remove our old panes
            // for (removed.items) |id| if (self.panes.fetchSwapRemove(id)) |entry_const| {
            //     var entry = entry_const;
            //     entry.value.deinit(self.alloc);
            // };
            //
            // PORT NOTE: the panes still in `self.panes` at this point are exactly
            // the removed ones (the preserved panes were moved into `panes`
            // above). We deinit them here, matching the reference's removal of the
            // `removed` set.
            for id in removed {
                if let Some(mut entry) = self.panes.fetch_swap_remove(&id) {
                    entry.value.deinit(self.alloc);
                }
            }
            // We can now deinit self.panes because the existing
            // entries are preserved.
            // self.panes.deinit(self.alloc);
            // self.panes = panes;
            self.panes = panes;
        }
        Ok(())
    }

    /// When a session changes, we have to basically reset our whole state.
    /// To do this, we emit an empty windows event (so callers can clear all
    /// windows), reset ourself, and start all over.
    // fn sessionChanged(
    //     self: *Viewer,
    //     actions: *std.ArrayList(Action),
    //     session_id: usize,
    // ) (Allocator.Error || std.Io.Writer.Error)!void {
    fn session_changed(&mut self, session_id: usize) -> Result<(), VtError> {
        // Build up a new viewer. Its the easiest way to reset ourselves.
        // var replacement: Viewer = try .init(self.alloc);
        // errdefer replacement.deinit();
        let mut replacement: Viewer = Viewer::init(self.alloc)?;

        // Our actions must start out empty so we don't mix arenas
        // assert(actions.items.len == 0);
        // errdefer actions.* = .empty;
        assert(self.actions.is_empty());

        // Build actions: empty windows notification + list-windows command
        // var arena = replacement.action_arena.promote(replacement.alloc);
        // const arena_alloc = arena.allocator();
        // try actions.append(arena_alloc, .{ .windows = &.{} });
        self.actions.push(Action::Windows(Vec::new()));

        // Setup our command queue and put ourselves in the command queue
        // state.
        // try replacement.queueCommands(&.{.list_windows});
        replacement.queue_commands(&[Command::ListWindows]);
        // replacement.state = .command_queue;
        replacement.state = State::CommandQueue;

        // Transfer preserved version to replacement
        // replacement.tmux_version = try replacement.alloc.dupe(u8, self.tmux_version);
        replacement.tmux_version = self.tmux_version.clone();

        // Save arena state back before swap
        // replacement.action_arena = arena.state;

        // Swap our self, no more error handling after this.
        // errdefer comptime unreachable;
        // self.deinit();
        // self.* = replacement;
        //
        // PORT NOTE: the actions we just pushed live on `self.actions` (the
        // reference puts them on `actions` which `next` returns); we preserve
        // them across the reset by carrying them into the replacement before the
        // swap, since `next` returns `&self.actions`.
        let actions = std::mem::take(&mut self.actions);
        self.deinit();
        *self = replacement;
        self.actions = actions;

        // Set our session ID and jump directly to the list
        // self.session_id = session_id;
        self.session_id = session_id;

        // assert(self.state == .command_queue);
        assert(self.state == State::CommandQueue);
        Ok(())
    }

    // fn receivedCommandOutput(
    //     self: *Viewer,
    //     actions: *std.ArrayList(Action),
    //     content: []const u8,
    //     is_err: bool,
    // ) !void {
    fn received_command_output(&mut self, content: &[u8], is_err: bool) -> Result<(), VtError> {
        // Get the command we're expecting output for. We need to get the
        // non-pointer value because we are deleting it from the circular
        // buffer immediately. This shallow copy is all we need since
        // all the memory in Command is owned by GPA.
        // const command: Command = if (self.command_queue.first()) |ptr| switch (ptr.*) {
        //     // I truly can't explain this. A simple `ptr.*` copy will cause
        //     // our memory to become undefined when deleteOldest is called
        //     // below. I logged all the pointers and they don't match so I
        //     // don't know how its being set to undefined. But a copy like
        //     // this does work.
        //     inline else => |v, tag| @unionInit(Command, @tagName(tag), v),
        // } else {
        //     log.info("unexpected block output err={}", .{is_err});
        //     return;
        // };
        // self.command_queue.deleteOldest(1);
        // defer command.deinit(self.alloc);
        //
        // PORT NOTE: the reference's `inline else` copy-then-deleteOldest dance
        // works around a Zig-specific aliasing footgun. Owned `VecDeque`
        // `pop_front` moves the `Command` out, no aliasing: this is the faithful
        // equivalent of taking the front command and deleting it from the queue.
        // The trailing `command.deinit` frees the `.user` arm's Vec; here the
        // owned `command` is dropped at scope end (we call `deinit` explicitly to
        // keep the seam visible).
        let command: Command = match self.command_queue.pop_front() {
            Some(c) => c,
            None => {
                // If we have no pending commands, this is unexpected.
                log::info!(target: LOG_SCOPE, "unexpected block output err={is_err:?}");
                return Ok(());
            }
        };

        // We'll use our arena for the return value here so we can
        // easily accumulate actions.
        // var arena = self.action_arena.promote(self.alloc);
        // defer self.action_arena = arena.state;
        // const arena_alloc = arena.allocator();

        // Process our command
        // switch (command) {
        let result = match &command {
            // .user => {},
            Command::User(_) => Ok(()),

            // .pane_state => try self.receivedPaneState(content),
            Command::PaneState => self.received_pane_state(content),

            // .list_windows => try self.receivedListWindows(arena_alloc, actions, content),
            Command::ListWindows => self.received_list_windows(content),

            // .pane_history => |cap| try self.receivedPaneHistory(cap.screen_key, cap.id, content),
            Command::PaneHistory(cap) => {
                self.received_pane_history(cap.screen_key, cap.id, content)
            }

            // .pane_visible => |cap| try self.receivedPaneVisible(cap.screen_key, cap.id, content),
            Command::PaneVisible(cap) => {
                self.received_pane_visible(cap.screen_key, cap.id, content)
            }

            // .tmux_version => try self.receivedTmuxVersion(content),
            Command::TmuxVersion => self.received_tmux_version(content),
        };

        // defer command.deinit(self.alloc);
        command.deinit(self.alloc);
        result
    }

    // fn receivedTmuxVersion(self: *Viewer, content: []const u8) !void {
    fn received_tmux_version(&mut self, content: &[u8]) -> Result<(), VtError> {
        // const line = std.mem.trim(u8, content, " \t\r\n");
        let line = trim(content, b" \t\r\n");
        // if (line.len == 0) return;
        if line.is_empty() {
            return Ok(());
        }

        // const data = output.parseFormatStruct(
        //     Format.tmux_version.Struct(),
        //     line,
        //     Format.tmux_version.delim,
        // ) catch |err| {
        //     log.info("failed to parse tmux version: {s}", .{line});
        //     return err;
        // };
        let line_s = std::str::from_utf8(line).map_err(|_| VtError::FormatError)?;
        let data = match output::parse_format_struct(
            Format::TMUX_VERSION.vars,
            line_s,
            Format::TMUX_VERSION.delim,
        ) {
            Ok(d) => d,
            Err(_err) => {
                // log.info("failed to parse tmux version: {s}", .{line});
                log::info!(target: LOG_SCOPE, "failed to parse tmux version: {line_s}");
                return Err(VtError::FormatError);
            }
        };

        // if (self.tmux_version.len > 0) self.alloc.free(self.tmux_version);
        // self.tmux_version = try self.alloc.dupe(u8, data.version);
        //
        // PORT NOTE: `data.version` is the single `Version` field; it parses to a
        // `Value::Str`. Reassigning the owned `Vec` frees the prior version.
        let version = value_str(&data[0]);
        self.tmux_version = version.as_bytes().to_vec();
        Ok(())
    }

    // fn receivedListWindows(
    //     self: *Viewer,
    //     arena_alloc: Allocator,
    //     actions: *std.ArrayList(Action),
    //     content: []const u8,
    // ) !void {
    fn received_list_windows(&mut self, content: &[u8]) -> Result<(), VtError> {
        // If there is an error, reset our actions to what it was before.
        // errdefer actions.shrinkRetainingCapacity(actions.items.len);
        //
        // PORT NOTE: the reference `shrinkRetainingCapacity(actions.items.len)`
        // is a no-op (shrinking to the current length). It is preserved as a
        // comment; no behavior to model.

        // This stores our new window state from this list-windows output.
        // var windows: std.ArrayList(Window) = .empty;
        // defer windows.deinit(self.alloc);
        let mut windows: Vec<Window> = Vec::new();

        // Parse all our windows
        // var it = std.mem.splitScalar(u8, content, '\n');
        // while (it.next()) |line_raw| {
        let mut errored: Option<VtError> = None;
        'parse: for line_raw in split_scalar(content, b'\n') {
            // const line = std.mem.trim(u8, line_raw, " \t\r");
            let line = trim(line_raw, b" \t\r");
            // if (line.len == 0) continue;
            if line.is_empty() {
                continue;
            }
            // const data = output.parseFormatStruct(
            //     Format.list_windows.Struct(), line, Format.list_windows.delim,
            // ) catch |err| {
            //     log.info("failed to parse list-windows line: {s}", .{line});
            //     return err;
            // };
            let line_s = match std::str::from_utf8(line) {
                Ok(s) => s,
                Err(_) => {
                    errored = Some(VtError::FormatError);
                    break 'parse;
                }
            };
            let data = match output::parse_format_struct(
                Format::LIST_WINDOWS.vars,
                line_s,
                Format::LIST_WINDOWS.delim,
            ) {
                Ok(d) => d,
                Err(_err) => {
                    // log.info("failed to parse list-windows line: {s}", .{line});
                    log::info!(target: LOG_SCOPE, "failed to parse list-windows line: {line_s}");
                    errored = Some(VtError::FormatError);
                    break 'parse;
                }
            };

            // Parse the layout
            // var arena: ArenaAllocator = .init(self.alloc);
            // errdefer arena.deinit();
            // const window_alloc = arena.allocator();
            // const layout: Layout = Layout.parseWithChecksum(window_alloc, data.window_layout) catch |err| {
            //     log.info("failed to parse window layout id={} layout={s}", ...);
            //     return err;
            // };
            //
            // PORT NOTE: the per-window arena exists solely to free the layout
            // tree; the owned `Layout` frees itself when the `Window` drops.
            let window_layout = value_str(&data[4]);
            let layout: Layout = match Layout::parse_with_checksum(window_layout) {
                Ok(l) => l,
                Err(_err) => {
                    // log.info(
                    //     "failed to parse window layout id={} layout={s}",
                    //     .{ data.window_id, data.window_layout },
                    // );
                    log::info!(
                        target: LOG_SCOPE,
                        "failed to parse window layout id={} layout={window_layout}",
                        value_usize(&data[1]),
                    );
                    errored = Some(VtError::SyntaxError);
                    break 'parse;
                }
            };

            // try windows.append(self.alloc, .{
            //     .id = data.window_id,
            //     .width = data.window_width,
            //     .height = data.window_height,
            //     .layout_arena = arena.state,
            //     .layout = layout,
            // });
            windows.push(Window {
                id: value_usize(&data[1]),
                width: value_usize(&data[2]),
                height: value_usize(&data[3]),
                layout,
            });
        }

        if let Some(err) = errored {
            // errdefer (drop `windows`, which frees the parsed layouts), then
            // propagate. `next_command` turns this into `defunct`.
            return Err(err);
        }

        // Setup our windows action so the caller can process GUI
        // window changes.
        // try actions.append(arena_alloc, .{ .windows = windows.items });
        self.actions.push(Action::Windows(windows.clone()));

        // Sync up our layouts. This will populate unknown panes, prune, etc.
        // try self.syncLayouts(windows.items);
        self.sync_layouts(windows, false)?;
        Ok(())
    }

    // fn receivedPaneState(self: *Viewer, content: []const u8) !void {
    fn received_pane_state(&mut self, content: &[u8]) -> Result<(), VtError> {
        // var it = std.mem.splitScalar(u8, content, '\n');
        // while (it.next()) |line_raw| {
        for line_raw in split_scalar(content, b'\n') {
            // const line = std.mem.trim(u8, line_raw, " \t\r");
            let line = trim(line_raw, b" \t\r");
            // if (line.len == 0) continue;
            if line.is_empty() {
                continue;
            }

            // const data = output.parseFormatStruct(
            //     Format.list_panes.Struct(), line, Format.list_panes.delim,
            // ) catch |err| {
            //     log.info("failed to parse list-panes line: {s}", .{line});
            //     return err;
            // };
            let line_s = std::str::from_utf8(line).map_err(|_| VtError::FormatError)?;
            let data = match output::parse_format_struct(
                Format::LIST_PANES.vars,
                line_s,
                Format::LIST_PANES.delim,
            ) {
                Ok(d) => d,
                Err(_err) => {
                    // log.info("failed to parse list-panes line: {s}", .{line});
                    log::info!(target: LOG_SCOPE, "failed to parse list-panes line: {line_s}");
                    return Err(VtError::FormatError);
                }
            };
            let data = ListPanes::from_values(&data);

            // Get the pane for this ID
            // const entry = self.panes.getEntry(data.pane_id) orelse {
            //     log.info("received pane state for untracked pane id={}", .{data.pane_id});
            //     continue;
            // };
            // const pane: *Pane = entry.value_ptr;
            // const t: *Terminal = &pane.terminal;
            let pane: &mut Pane = match self.panes.get_ptr(&data.pane_id) {
                Some(p) => p,
                None => {
                    // log.info("received pane state for untracked pane id={}", .{data.pane_id});
                    log::info!(
                        target: LOG_SCOPE,
                        "received pane state for untracked pane id={}",
                        data.pane_id,
                    );
                    continue;
                }
            };
            let t: *mut Terminal = &mut pane.terminal;

            // Determine which screen to use based on alternate_on
            // const screen_key: ScreenSet.Key = if (data.alternate_on) .alternate else .primary;
            let screen_key: screen_set::Key = if data.alternate_on {
                screen_set::Key::Alternate
            } else {
                screen_set::Key::Primary
            };

            // SAFETY: `t` points at `pane.terminal`, a stable heap-owned field;
            // the methods below take `&mut Terminal`/`&mut Screen` and do not
            // touch `self.panes`. The screen pointers from `t.screens.get` alias
            // the same terminal's owned screens.
            let t: &mut Terminal = unsafe { &mut *t };

            // Set cursor position on the appropriate screen (tmux uses 0-based)
            // if (t.screens.get(screen_key)) |screen| {
            if let Some(screen) = t.screens.get(screen_key) {
                let screen: &mut Screen = unsafe { &mut *screen };
                // cursor: {
                'cursor: {
                    // const cursor_x = std.math.cast(size.CellCountInt, data.cursor_x) orelse break :cursor;
                    let cursor_x = match cast_cell(data.cursor_x) {
                        Some(v) => v,
                        None => break 'cursor,
                    };
                    // const cursor_y = std.math.cast(size.CellCountInt, data.cursor_y) orelse break :cursor;
                    let cursor_y = match cast_cell(data.cursor_y) {
                        Some(v) => v,
                        None => break 'cursor,
                    };
                    // if (cursor_x >= screen.pages.cols or cursor_y >= screen.pages.rows) break :cursor;
                    if cursor_x >= screen.pages.cols || cursor_y >= screen.pages.rows {
                        break 'cursor;
                    }
                    // screen.cursorAbsolute(cursor_x, cursor_y);
                    screen.cursor_absolute(cursor_x, cursor_y);
                }

                // Set cursor shape on this screen
                // if (data.cursor_shape.len > 0) {
                if !data.cursor_shape.is_empty() {
                    // if (std.mem.eql(u8, data.cursor_shape, "block")) {
                    //     screen.cursor.cursor_style = .block;
                    // } else if (...) { ... }
                    if data.cursor_shape == "block" {
                        screen.cursor.cursor_style = CursorStyle::Block;
                    } else if data.cursor_shape == "underline" {
                        screen.cursor.cursor_style = CursorStyle::Underline;
                    } else if data.cursor_shape == "bar" {
                        screen.cursor.cursor_style = CursorStyle::Bar;
                    }
                }
                // "default" or unknown: leave as-is
            }

            // Set alternate screen saved cursor position
            // if (t.screens.get(.alternate)) |alt_screen| cursor: {
            if let Some(alt_screen) = t.screens.get(screen_set::Key::Alternate) {
                let alt_screen: &mut Screen = unsafe { &mut *alt_screen };
                'cursor: {
                    // const alt_x = std.math.cast(size.CellCountInt, data.alternate_saved_x) orelse break :cursor;
                    let alt_x = match cast_cell(data.alternate_saved_x) {
                        Some(v) => v,
                        None => break 'cursor,
                    };
                    // const alt_y = std.math.cast(size.CellCountInt, data.alternate_saved_y) orelse break :cursor;
                    let alt_y = match cast_cell(data.alternate_saved_y) {
                        Some(v) => v,
                        None => break 'cursor,
                    };

                    // If our coordinates are outside our screen we ignore it.
                    // tmux actually sends MAX_INT for when there isn't a set
                    // cursor position, so this isn't theoretical.
                    // if (alt_x >= alt_screen.pages.cols or alt_y >= alt_screen.pages.rows) break :cursor;
                    if alt_x >= alt_screen.pages.cols || alt_y >= alt_screen.pages.rows {
                        break 'cursor;
                    }

                    // alt_screen.cursorAbsolute(alt_x, alt_y);
                    alt_screen.cursor_absolute(alt_x, alt_y);
                }
            }

            // Set cursor visibility
            // t.modes.set(.cursor_visible, data.cursor_flag);
            t.modes.set(modes::Mode::CursorVisible, data.cursor_flag);

            // Set cursor blinking
            // t.modes.set(.cursor_blinking, data.cursor_blinking);
            t.modes
                .set(modes::Mode::CursorBlinking, data.cursor_blinking);

            // Terminal modes
            // t.modes.set(.insert, data.insert_flag);
            t.modes.set(modes::Mode::Insert, data.insert_flag);
            // t.modes.set(.wraparound, data.wrap_flag);
            t.modes.set(modes::Mode::Wraparound, data.wrap_flag);
            // t.modes.set(.keypad_keys, data.keypad_flag);
            t.modes.set(modes::Mode::KeypadKeys, data.keypad_flag);
            // t.modes.set(.cursor_keys, data.keypad_cursor_flag);
            t.modes
                .set(modes::Mode::CursorKeys, data.keypad_cursor_flag);
            // t.modes.set(.origin, data.origin_flag);
            t.modes.set(modes::Mode::Origin, data.origin_flag);

            // Mouse modes
            // t.modes.set(.mouse_event_any, data.mouse_all_flag);
            t.modes.set(modes::Mode::MouseEventAny, data.mouse_all_flag);
            // t.modes.set(.mouse_event_button, data.mouse_any_flag);
            t.modes
                .set(modes::Mode::MouseEventButton, data.mouse_any_flag);
            // t.modes.set(.mouse_event_normal, data.mouse_button_flag);
            t.modes
                .set(modes::Mode::MouseEventNormal, data.mouse_button_flag);
            // t.modes.set(.mouse_event_x10, data.mouse_standard_flag);
            t.modes
                .set(modes::Mode::MouseEventX10, data.mouse_standard_flag);
            // t.modes.set(.mouse_format_utf8, data.mouse_utf8_flag);
            t.modes
                .set(modes::Mode::MouseFormatUtf8, data.mouse_utf8_flag);
            // t.modes.set(.mouse_format_sgr, data.mouse_sgr_flag);
            t.modes
                .set(modes::Mode::MouseFormatSgr, data.mouse_sgr_flag);

            // Focus and bracketed paste
            // t.modes.set(.focus_event, data.focus_flag);
            t.modes.set(modes::Mode::FocusEvent, data.focus_flag);
            // t.modes.set(.bracketed_paste, data.bracketed_paste);
            t.modes
                .set(modes::Mode::BracketedPaste, data.bracketed_paste);

            // Scroll region (tmux uses 0-based values)
            // scroll: {
            'scroll: {
                // const scroll_top = std.math.cast(size.CellCountInt, data.scroll_region_upper) orelse break :scroll;
                let scroll_top = match cast_cell(data.scroll_region_upper) {
                    Some(v) => v,
                    None => break 'scroll,
                };
                // const scroll_bottom = std.math.cast(size.CellCountInt, data.scroll_region_lower) orelse break :scroll;
                let scroll_bottom = match cast_cell(data.scroll_region_lower) {
                    Some(v) => v,
                    None => break 'scroll,
                };
                // t.scrolling_region.top = scroll_top;
                t.scrolling_region.top = scroll_top;
                // t.scrolling_region.bottom = scroll_bottom;
                t.scrolling_region.bottom = scroll_bottom;
            }

            // Tab stops - parse comma-separated list and set
            // t.tabstops.reset(0); // Clear all tabstops first
            t.tabstops.reset(0); // Clear all tabstops first
                                 // if (data.pane_tabs.len > 0) {
            if !data.pane_tabs.is_empty() {
                // var tabs_it = std.mem.splitScalar(u8, data.pane_tabs, ',');
                // while (tabs_it.next()) |tab_str| {
                for tab_str in data.pane_tabs.split(',') {
                    // const col = std.fmt.parseInt(usize, tab_str, 10) catch continue;
                    let col: usize = match tab_str.parse::<usize>() {
                        Ok(v) => v,
                        Err(_) => continue,
                    };
                    // const col_cell = std.math.cast(size.CellCountInt, col) orelse continue;
                    let col_cell = match cast_cell(col) {
                        Some(v) => v,
                        None => continue,
                    };
                    // if (col_cell >= t.cols) continue;
                    if col_cell >= t.cols {
                        continue;
                    }
                    // t.tabstops.set(col_cell);
                    t.tabstops.set(col_cell as usize);
                }
            }
        }
        Ok(())
    }

    // fn receivedPaneHistory(
    //     self: *Viewer,
    //     screen_key: ScreenSet.Key,
    //     id: usize,
    //     content: []const u8,
    // ) !void {
    fn received_pane_history(
        &mut self,
        screen_key: screen_set::Key,
        id: usize,
        content: &[u8],
    ) -> Result<(), VtError> {
        // Get our pane
        // const entry = self.panes.getEntry(id) orelse {
        //     log.info("received pane history for untracked pane id={}", .{id});
        //     return;
        // };
        // const pane: *Pane = entry.value_ptr;
        // const t: *Terminal = &pane.terminal;
        let pane: &mut Pane = match self.panes.get_ptr(&id) {
            Some(p) => p,
            None => {
                log::info!(target: LOG_SCOPE, "received pane history for untracked pane id={id}");
                return Ok(());
            }
        };
        let t: &mut Terminal = &mut pane.terminal;
        // _ = try t.switchScreen(screen_key);
        t.switch_screen(screen_key)
            .map_err(|_| VtError::OutOfMemory)?;
        // const screen: *Screen = t.screens.active;
        let screen: *mut Screen = t.screens.active;

        // Get a VT stream from the terminal so we can send data as-is into
        // it. This will populate the active area too so it won't be exactly
        // correct but we'll get the active contents soon.
        // var stream = t.vtStream();
        // defer stream.deinit();
        // stream.nextSlice(content);
        {
            let mut stream = t.vt_stream();
            stream.next_slice(content);
            stream.deinit();
        }

        // Populate the active area to be empty since this is only history.
        // We'll fill it with blanks and move the cursor to the top-left.
        // t.carriageReturn();
        t.carriage_return();
        // for (0..t.rows) |_| try t.index();
        for _ in 0..t.rows {
            t.index().map_err(|_| VtError::OutOfMemory)?;
        }
        // t.setCursorPos(1, 1);
        t.set_cursor_pos(1, 1);

        // Our active area should be empty
        // if (comptime std.debug.runtime_safety) {
        //     var discarding: std.Io.Writer.Discarding = .init(&.{});
        //     screen.dumpString(&discarding.writer, .{
        //         .tl = screen.pages.getTopLeft(.active),
        //         .unwrap = false,
        //     }) catch unreachable;
        //     assert(discarding.count == 0);
        // }
        if cfg!(debug_assertions) {
            let screen: &Screen = unsafe { &*screen };
            let mut discarding = CountingDiscard::default();
            screen
                .dump_string(
                    &mut discarding,
                    crate::screen::DumpStringOptions {
                        tl: screen.pages.get_top_left(crate::point::Tag::Active),
                        br: None,
                        unwrap: false,
                    },
                )
                .unwrap();
            assert(discarding.count == 0);
        }
        Ok(())
    }

    // fn receivedPaneVisible(
    //     self: *Viewer,
    //     screen_key: ScreenSet.Key,
    //     id: usize,
    //     content: []const u8,
    // ) !void {
    fn received_pane_visible(
        &mut self,
        screen_key: screen_set::Key,
        id: usize,
        content: &[u8],
    ) -> Result<(), VtError> {
        // Get our pane
        // const entry = self.panes.getEntry(id) orelse {
        //     log.info("received pane visible for untracked pane id={}", .{id});
        //     return;
        // };
        // const pane: *Pane = entry.value_ptr;
        // const t: *Terminal = &pane.terminal;
        let pane: &mut Pane = match self.panes.get_ptr(&id) {
            Some(p) => p,
            None => {
                // log.info("received pane visible for untracked pane id={}", .{id});
                log::info!(target: LOG_SCOPE, "received pane visible for untracked pane id={id}");
                return Ok(());
            }
        };
        let t: &mut Terminal = &mut pane.terminal;
        // _ = try t.switchScreen(screen_key);
        t.switch_screen(screen_key)
            .map_err(|_| VtError::OutOfMemory)?;

        // Erase the active area and reset the cursor to the top-left
        // before writing the visible content.
        // t.eraseDisplay(.complete, false);
        t.erase_display(csi::EraseDisplay::Complete, false);
        // t.setCursorPos(1, 1);
        t.set_cursor_pos(1, 1);

        // var stream = t.vtStream();
        // defer stream.deinit();
        // stream.nextSlice(content);
        let mut stream = t.vt_stream();
        stream.next_slice(content);
        stream.deinit();
        Ok(())
    }

    // fn receivedOutput(self: *Viewer, id: usize, data: []const u8) !void {
    fn received_output(&mut self, id: usize, data: &[u8]) -> Result<(), VtError> {
        // const entry = self.panes.getEntry(id) orelse {
        //     log.info("received output for untracked pane id={}", .{id});
        //     return;
        // };
        // const pane: *Pane = entry.value_ptr;
        // const t: *Terminal = &pane.terminal;
        let pane: &mut Pane = match self.panes.get_ptr(&id) {
            Some(p) => p,
            None => {
                // log.info("received output for untracked pane id={}", .{id});
                log::info!(target: LOG_SCOPE, "received output for untracked pane id={id}");
                return Ok(());
            }
        };
        let t: &mut Terminal = &mut pane.terminal;

        // var stream = t.vtStream();
        // defer stream.deinit();
        // stream.nextSlice(data);
        let mut stream = t.vt_stream();
        stream.next_slice(data);
        stream.deinit();
        Ok(())
    }

    // fn initLayout(
    //     gpa_alloc: Allocator,
    //     panes_old: *const PanesMap,
    //     panes_new: *PanesMap,
    //     layout: Layout,
    // ) !void {
    //
    // PORT NOTE (soundness): the reference walks the layout tree writing leaves
    // directly into `panes_new`, copying carried-over `Pane`s bitwise out of
    // `panes_old` (which aliases the owned `Terminal` — unsound in Rust). The
    // port instead COLLECTS the ordered leaf descriptors (id + dims) into
    // `descriptors`; `sync_layouts` then builds the new map by moving carried-over
    // panes out of `self.panes` or constructing fresh `Terminal`s. The walk order
    // and the leaf id/dim values are byte-faithful. `gpa_alloc`/`panes_old`/
    // `panes_new` collapse into the single `descriptors` out-param: there is no
    // allocation here (so no error), hence the `!void` reduces to `void`.
    fn init_layout(descriptors: &mut Vec<PaneDescriptor>, layout: &Layout) {
        // switch (layout.content) {
        match &layout.content {
            // Nested layouts, continue going.
            // .horizontal, .vertical => |layouts| {
            //     for (layouts) |l| try initLayout(gpa_alloc, panes_old, panes_new, l);
            // },
            super::layout::Content::Horizontal(layouts)
            | super::layout::Content::Vertical(layouts) => {
                for l in layouts {
                    Self::init_layout(descriptors, l);
                }
            }

            // A leaf! Initialize.
            // .pane => |id| pane: {
            super::layout::Content::Pane(id) => {
                // const gop = try panes_new.getOrPut(gpa_alloc, id);
                // if (gop.found_existing) break :pane;
                // errdefer _ = panes_new.swapRemove(gop.key_ptr.*);
                //
                // If we already have this pane, it is already initialized
                // so just copy it over.
                // if (panes_old.getEntry(id)) |entry| {
                //     gop.value_ptr.* = entry.value_ptr.*;
                //     break :pane;
                // }
                //
                // TODO: We need to gracefully handle overflow of our
                // max cols/width here. In practice we shouldn't hit this
                // so we cast but its not safe.
                // var t: Terminal = try .init(gpa_alloc, .{
                //     .cols = @intCast(layout.width),
                //     .rows = @intCast(layout.height),
                // });
                // errdefer t.deinit(gpa_alloc);
                //
                // gop.value_ptr.* = .{ .terminal = t };
                //
                // PORT NOTE: the get-or-init / get-or-copy / construct-fresh
                // decision is realized in `sync_layouts` (see its soundness PORT
                // NOTE); here we only record the leaf descriptor.
                descriptors.push(PaneDescriptor {
                    id: *id,
                    width: layout.width,
                    height: layout.height,
                });
            }
        }
    }

    /// Enters the command queue state from any other state, queueing
    /// the commands and returning an action to execute the first command.
    // fn enterCommandQueue(
    //     self: *Viewer,
    //     arena_alloc: Allocator,
    //     commands: []const Command,
    // ) Allocator.Error![]const Action {
    //
    // PORT NOTE: returns `Result<(), OutOfMemory>`; the produced action is pushed
    // onto `self.actions` (which `next` returns) rather than returned directly,
    // matching the owned-actions model.
    fn enter_command_queue(&mut self, commands: &[Command]) -> Result<(), OutOfMemory> {
        // assert(self.state != .command_queue);
        assert(self.state != State::CommandQueue);
        // assert(commands.len > 0);
        assert(!commands.is_empty());

        // Build our command string to send for the action.
        // var builder: std.Io.Writer.Allocating = .init(arena_alloc);
        // commands[0].formatCommand(&builder.writer) catch return error.OutOfMemory;
        // const action: Action = .{ .command = builder.writer.buffered() };
        let mut builder: Vec<u8> = Vec::new();
        commands[0]
            .format_command(&mut builder)
            .map_err(|_| OutOfMemory)?;
        let action: Action = Action::Command(builder);

        // Add our commands
        // try self.command_queue.ensureUnusedCapacity(self.alloc, commands.len);
        // for (commands) |cmd| self.command_queue.appendAssumeCapacity(cmd);
        self.command_queue.reserve(commands.len());
        for cmd in commands {
            self.command_queue.push_back(cmd.clone());
        }

        // Move into the command queue state
        // self.state = .command_queue;
        self.state = State::CommandQueue;

        // return self.singleAction(action);
        self.single_action(action);
        Ok(())
    }

    /// Queue multiple commands to execute. This doesn't add anything
    /// to the actions queue or return actions or anything because the
    /// command_queue state will automatically send the next command when
    /// it receives output.
    // fn queueCommands(self: *Viewer, commands: []const Command) Allocator.Error!void {
    fn queue_commands(&mut self, commands: &[Command]) {
        // try self.command_queue.ensureUnusedCapacity(self.alloc, commands.len);
        self.command_queue.reserve(commands.len());
        // for (commands) |command| self.command_queue.appendAssumeCapacity(command);
        for command in commands {
            self.command_queue.push_back(command.clone());
        }
    }

    /// Helper to return a single action. The input action may use the arena
    /// for allocated memory; this will not touch the arena.
    // fn singleAction(self: *Viewer, action: Action) []const Action {
    //
    // PORT NOTE: the reference writes into a pre-allocated `action_single[1]`
    // slot and returns it. The owned port replaces `self.actions` with the single
    // action (clearing any prior contents), which `next` returns. There is no
    // allocation to fail (the infallible single-action guarantee holds).
    fn single_action(&mut self, action: Action) {
        // Make our single action slice.
        // self.action_single[0] = action;
        // return &self.action_single;
        self.actions.clear();
        self.actions.push(action);
    }

    // fn defunct(self: *Viewer) []const Action {
    fn defunct(&mut self) -> &[Action] {
        // self.state = .defunct;
        self.state = State::Defunct;
        // return self.singleAction(.exit);
        self.single_action(Action::Exit);
        &self.actions
    }
}

impl Drop for Viewer {
    fn drop(&mut self) {
        self.deinit();
    }
}

/// PORTS: ghostty `State`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    /// We start in this state just after receiving the initial
    /// DCS 1000p opening sequence. We wait for an initial
    /// begin/end block that is guaranteed to be sent by tmux for
    /// the initial control mode command. (See tmux server-client.c
    /// where control mode starts).
    StartupBlock,

    /// After receiving the initial block, we wait for a session-changed
    /// notification to record the initial session ID.
    StartupSession,

    /// Tmux has closed the control mode connection
    Defunct,

    /// We're sitting on the command queue waiting for command output
    /// in the order provided in the `command_queue` field. This field
    /// isn't part of the state because it can be queued at any state.
    ///
    /// Precondition: if self.command_queue.len > 0, then the first
    /// command in the queue has already been sent to tmux (via a
    /// `command` Action). The next output is assumed to be the result
    /// of this command.
    ///
    /// To satisfy the above, any transitions INTO this state should
    /// send a command Action for the first command in the queue.
    CommandQueue,
}

/// PORTS: ghostty `Command`.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Command {
    /// List all windows so we can sync our window state.
    ListWindows,

    /// Capture history for the given pane ID.
    PaneHistory(CapturePane),

    /// Capture visible area for the given pane ID.
    PaneVisible(CapturePane),

    /// Capture the pane terminal state as best we can. The pane ID(s)
    /// are part of the output so we can map it back to our panes.
    PaneState,

    /// Get the tmux server version.
    TmuxVersion,

    /// User command. This is a command provided by the user. Since
    /// this is user provided, we can't be sure what it is.
    // user: []const u8,
    //
    // PORT NOTE: not constructed by the viewer's own flows (it is the
    // user-command escape hatch the reference defines for future use); retained
    // verbatim from the reference union.
    #[allow(dead_code)]
    User(Vec<u8>),
}

/// PORTS: ghostty `Command.CapturePane`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CapturePane {
    id: usize,
    screen_key: screen_set::Key,
}

impl Command {
    // pub fn deinit(self: Command, alloc: Allocator) void {
    fn deinit(&self, alloc: Allocator) {
        // return switch (self) {
        match self {
            // .list_windows, .pane_history, .pane_visible, .pane_state, .tmux_version => {},
            Command::ListWindows
            | Command::PaneHistory(_)
            | Command::PaneVisible(_)
            | Command::PaneState
            | Command::TmuxVersion => {}
            // .user => |v| alloc.free(v),
            //
            // PORT NOTE: `user` owns its `Vec<u8>`; it frees on drop. `alloc` is
            // unused (the global seam) but kept on the signature to mirror the
            // reference.
            Command::User(_) => {
                let _ = alloc;
            }
        }
    }

    /// Format the command into the command that should be executed
    /// by tmux. Trailing newlines are appended so this can be sent as-is
    /// to tmux.
    // pub fn formatCommand(self: Command, writer: *std.Io.Writer) std.Io.Writer.Error!void {
    //
    // PORT NOTE: the format strings deliberately end in a single `\n` (the
    // reference appends the trailing newline so the command can be sent to tmux
    // as-is). `writeln!` would emit an EXTRA newline, so `write!` with the
    // explicit `\n` is correct; the clippy lint is allowed here.
    #[allow(clippy::write_with_newline)]
    fn format_command(&self, writer: &mut Vec<u8>) -> std::io::Result<()> {
        use std::io::Write;
        // switch (self) {
        match self {
            // .list_windows => try writer.writeAll(std.fmt.comptimePrint(
            //     "list-windows -F '{s}'\n", .{comptime Format.list_windows.comptimeFormat()},
            // )),
            Command::ListWindows => write!(
                writer,
                "list-windows -F '{}'\n",
                Format::LIST_WINDOWS.comptime_format()
            ),

            // .pane_history => |cap| try writer.print(
            //     "capture-pane -p -e -q {s}-S - -E -1 -t %{d}\n",
            //     .{ if (cap.screen_key == .alternate) "-a " else "", cap.id },
            // ),
            Command::PaneHistory(cap) => write!(
                writer,
                // -p = output to stdout instead of buffer
                // -e = output escape sequences for SGR
                // -a = capture alternate screen (only valid for alternate)
                // -q = quiet, don't error if alternate screen doesn't exist
                // -S - = start at the top of history ("-")
                // -E -1 = end at the last line of history (1 before the
                //   visible area is -1).
                // -t %{d} = target a specific pane ID
                "capture-pane -p -e -q {}-S - -E -1 -t %{}\n",
                if cap.screen_key == screen_set::Key::Alternate {
                    "-a "
                } else {
                    ""
                },
                cap.id,
            ),

            // .pane_visible => |cap| try writer.print(
            //     "capture-pane -p -e -q {s}-t %{d}\n",
            //     .{ if (cap.screen_key == .alternate) "-a " else "", cap.id },
            // ),
            Command::PaneVisible(cap) => write!(
                writer,
                // -p = output to stdout instead of buffer
                // -e = output escape sequences for SGR
                // -a = capture alternate screen (only valid for alternate)
                // -q = quiet, don't error if alternate screen doesn't exist
                // -t %{d} = target a specific pane ID
                // (no -S/-E = capture visible area only)
                "capture-pane -p -e -q {}-t %{}\n",
                if cap.screen_key == screen_set::Key::Alternate {
                    "-a "
                } else {
                    ""
                },
                cap.id,
            ),

            // .pane_state => try writer.writeAll(std.fmt.comptimePrint(
            //     "list-panes -F '{s}'\n", .{comptime Format.list_panes.comptimeFormat()},
            // )),
            Command::PaneState => write!(
                writer,
                "list-panes -F '{}'\n",
                Format::LIST_PANES.comptime_format()
            ),

            // .tmux_version => try writer.writeAll(std.fmt.comptimePrint(
            //     "display-message -p '{s}'\n", .{comptime Format.tmux_version.comptimeFormat()},
            // )),
            Command::TmuxVersion => write!(
                writer,
                "display-message -p '{}'\n",
                Format::TMUX_VERSION.comptime_format()
            ),

            // .user => |v| try writer.writeAll(v),
            Command::User(v) => writer.write_all(v),
        }
    }
}

/// Format strings used for commands in our viewer.
/// PORTS: ghostty `Format`.
struct Format {
    /// The variables included in this format, in order.
    vars: &'static [Variable],

    /// The delimiter to use between variables. This must be a character
    /// guaranteed to not appear in any of the variable outputs.
    delim: u8,
}

impl Format {
    // const list_panes: Format = .{ .delim = ';', .vars = &.{ ... } };
    const LIST_PANES: Format = Format {
        delim: b';',
        vars: &[
            Variable::PaneId,
            // Cursor position & appearance
            Variable::CursorX,
            Variable::CursorY,
            Variable::CursorFlag,
            Variable::CursorShape,
            Variable::CursorColour,
            Variable::CursorBlinking,
            // Alternate screen
            Variable::AlternateOn,
            Variable::AlternateSavedX,
            Variable::AlternateSavedY,
            // Terminal modes
            Variable::InsertFlag,
            Variable::WrapFlag,
            Variable::KeypadFlag,
            Variable::KeypadCursorFlag,
            Variable::OriginFlag,
            // Mouse modes
            Variable::MouseAllFlag,
            Variable::MouseAnyFlag,
            Variable::MouseButtonFlag,
            Variable::MouseStandardFlag,
            Variable::MouseUtf8Flag,
            Variable::MouseSgrFlag,
            // Focus & special features
            Variable::FocusFlag,
            Variable::BracketedPaste,
            // Scroll region
            Variable::ScrollRegionUpper,
            Variable::ScrollRegionLower,
            // Tab stops
            Variable::PaneTabs,
        ],
    };

    // const list_windows: Format = .{ .delim = ' ', .vars = &.{ ... } };
    const LIST_WINDOWS: Format = Format {
        delim: b' ',
        vars: &[
            Variable::SessionId,
            Variable::WindowId,
            Variable::WindowWidth,
            Variable::WindowHeight,
            Variable::WindowLayout,
        ],
    };

    // const tmux_version: Format = .{ .delim = ' ', .vars = &.{.version} };
    const TMUX_VERSION: Format = Format {
        delim: b' ',
        vars: &[Variable::Version],
    };

    /// The format string, available at comptime.
    // pub fn comptimeFormat(comptime self: Format) []const u8 {
    //     return output.comptimeFormat(self.vars, self.delim);
    // }
    fn comptime_format(&self) -> String {
        output::comptime_format(self.vars, self.delim)
    }

    // The struct that can contain the parsed output.
    // pub fn Struct(comptime self: Format) type {
    //     return output.FormatStruct(self.vars);
    // }
    //
    // PORT NOTE: `Struct()` synthesizes a parse-target type. In the owned port
    // the parse target is the `&[Variable]` slice itself (`self.vars`), passed
    // directly to `parse_format_struct`; there is no type to synthesize.
}

// ---------------------------------------------------------------------------
// Port-local helpers
// ---------------------------------------------------------------------------

/// The faithful subset of the error set used by the receive helpers
/// (`Allocator.Error || std.Io.Writer.Error || Layout.ParseWithChecksumError ||
/// output.ParseError || Screen/Terminal switchScreen errors`). `next_command`
/// collapses any of these into a `defunct` transition, so only the
/// success/failure distinction is observable; the variants are retained for
/// traceability.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VtError {
    OutOfMemory,
    FormatError,
    SyntaxError,
}

impl From<OutOfMemory> for VtError {
    fn from(_: OutOfMemory) -> VtError {
        VtError::OutOfMemory
    }
}

/// A leaf-pane descriptor collected from a layout tree by `init_layout`: the
/// pane id and the dimensions a freshly constructed `Terminal` should use.
/// (See the soundness PORT NOTE on `init_layout`/`sync_layouts`.)
struct PaneDescriptor {
    id: usize,
    width: usize,
    height: usize,
}

/// Mirror of `std.math.cast(size.CellCountInt, value)`: `usize` -> `u16`,
/// returning `None` on overflow.
fn cast_cell(value: usize) -> Option<size::CellCountInt> {
    size::CellCountInt::try_from(value).ok()
}

/// Mirror of `Terminal.init(alloc, .{ .cols, .rows })` — the reference only
/// sets cols/rows; the remaining `Terminal.Options` fields take their struct
/// defaults (`max_scrollback_bytes = 10_000`, `colors = .default`, `default_modes =
/// .{}`, kitty limits per artifact).
fn terminal_opts(cols: size::CellCountInt, rows: size::CellCountInt) -> terminal::Options {
    terminal::Options {
        cols,
        rows,
        max_scrollback_bytes: Some(10_000),
        max_scrollback_lines: None,
        colors: terminal::Colors::default(),
        default_modes: modes::ModePacked::default(),
        default_cursor_style: crate::screen::CursorStyle::Block,
        default_cursor_blink: Some(false),
        kitty_image_storage_limit: 320 * 1000 * 1000,
        kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
    }
}

/// `std.mem.trim(u8, bytes, cutset)`.
fn trim<'a>(bytes: &'a [u8], cutset: &[u8]) -> &'a [u8] {
    let mut start = 0;
    let mut end = bytes.len();
    while start < end && cutset.contains(&bytes[start]) {
        start += 1;
    }
    while end > start && cutset.contains(&bytes[end - 1]) {
        end -= 1;
    }
    &bytes[start..end]
}

/// `std.mem.trim(u8, value, " \t\r\n")` for the `Display` of `Action`.
fn trim_display(bytes: &[u8]) -> String {
    String::from_utf8_lossy(trim(bytes, b" \t\r\n")).into_owned()
}

/// `std.mem.splitScalar(u8, bytes, sep)`.
fn split_scalar(bytes: &[u8], sep: u8) -> impl Iterator<Item = &[u8]> {
    bytes.split(move |&b| b == sep)
}

/// Extract the `&str` payload from a parsed `Value` for the string-typed
/// variables (`Version`, `WindowLayout`, `CursorShape`, `PaneTabs`,
/// `CursorColour`).
fn value_str<'a>(v: &Value<'a>) -> &'a str {
    match v {
        Value::Str(s) => s,
        _ => unreachable!("expected string variable"),
    }
}

/// Extract the `usize` payload from a parsed `Value`.
fn value_usize(v: &Value<'_>) -> usize {
    match v {
        Value::Usize(n) => *n,
        _ => unreachable!("expected usize variable"),
    }
}

/// Extract the `bool` payload from a parsed `Value`.
fn value_bool(v: &Value<'_>) -> bool {
    match v {
        Value::Bool(b) => *b,
        _ => unreachable!("expected bool variable"),
    }
}

/// The decoded `list-panes` row, mirroring the synthesized `Format.list_panes`
/// struct field-for-field (in `Format::LIST_PANES.vars` order).
struct ListPanes<'a> {
    pane_id: usize,
    cursor_x: usize,
    cursor_y: usize,
    cursor_flag: bool,
    cursor_shape: &'a str,
    cursor_blinking: bool,
    alternate_on: bool,
    alternate_saved_x: usize,
    alternate_saved_y: usize,
    insert_flag: bool,
    wrap_flag: bool,
    keypad_flag: bool,
    keypad_cursor_flag: bool,
    origin_flag: bool,
    mouse_all_flag: bool,
    mouse_any_flag: bool,
    mouse_button_flag: bool,
    mouse_standard_flag: bool,
    mouse_utf8_flag: bool,
    mouse_sgr_flag: bool,
    focus_flag: bool,
    bracketed_paste: bool,
    scroll_region_upper: usize,
    scroll_region_lower: usize,
    pane_tabs: &'a str,
}

impl<'a> ListPanes<'a> {
    fn from_values(v: &[Value<'a>]) -> ListPanes<'a> {
        // Order matches Format::LIST_PANES.vars (cursor_colour is parsed but
        // unused, just as the reference struct binds it without reading it).
        ListPanes {
            pane_id: value_usize(&v[0]),
            cursor_x: value_usize(&v[1]),
            cursor_y: value_usize(&v[2]),
            cursor_flag: value_bool(&v[3]),
            cursor_shape: value_str(&v[4]),
            // v[5] = cursor_colour (unused)
            cursor_blinking: value_bool(&v[6]),
            alternate_on: value_bool(&v[7]),
            alternate_saved_x: value_usize(&v[8]),
            alternate_saved_y: value_usize(&v[9]),
            insert_flag: value_bool(&v[10]),
            wrap_flag: value_bool(&v[11]),
            keypad_flag: value_bool(&v[12]),
            keypad_cursor_flag: value_bool(&v[13]),
            origin_flag: value_bool(&v[14]),
            mouse_all_flag: value_bool(&v[15]),
            mouse_any_flag: value_bool(&v[16]),
            mouse_button_flag: value_bool(&v[17]),
            mouse_standard_flag: value_bool(&v[18]),
            mouse_utf8_flag: value_bool(&v[19]),
            mouse_sgr_flag: value_bool(&v[20]),
            focus_flag: value_bool(&v[21]),
            bracketed_paste: value_bool(&v[22]),
            scroll_region_upper: value_usize(&v[23]),
            scroll_region_lower: value_usize(&v[24]),
            pane_tabs: value_str(&v[25]),
        }
    }
}

/// A `std.Io.Writer.Discarding` that counts bytes written (used by the
/// debug-only "active area should be empty" assertion).
#[derive(Default)]
struct CountingDiscard {
    count: usize,
}

impl std::io::Write for CountingDiscard {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.count += buf.len();
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

use crate::csi;
use crate::kitty;
use crate::modes;
use crate::pagelist::OutOfMemory;

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
//
// PORT NOTE: the reference `TestStep`'s `check`/`check_command` are
// `?*const fn(...) anyerror!void` (function pointers over anonymous struct
// methods). The faithful Rust equivalent is a boxed closure
// (`Box<dyn Fn(...) -> Result<(), TestError>>`). `testing.expect*` map to
// `assert!`/`assert_eq!` (a panic is the Rust analog of a failing
// `anyerror!void`). The `std.mem.startsWith`/`endsWith`/`containsAtLeast`
// helpers map to slice/`str` methods.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::tmux::control;
    use allocator_api2::alloc::Global;

    // Convenience: build an Input from a tmux notification.
    fn tmux(n: control::Notification) -> Input {
        Input::Tmux(n)
    }

    // PORT NOTE: tmux notifications are constructed directly in the reference
    // via the tagged-union literal (e.g. `.{ .block_end = "" }`). These
    // constructors mirror those, owning the byte payloads.
    fn block_end(s: &str) -> control::Notification {
        control::Notification::BlockEnd(s.as_bytes().to_vec())
    }

    type CheckFn = Box<dyn Fn(&mut Viewer, &[Action])>;
    type CheckCmdFn = Box<dyn Fn(&mut Viewer, &[u8])>;

    // PORTS: ghostty `TestStep`.
    struct TestStep {
        // input: Viewer.Input,
        input: Input,
        // contains_tags: []const std.meta.Tag(Viewer.Action) = &.{},
        contains_tags: Vec<ActionTag>,
        // contains_command: []const u8 = "",
        contains_command: &'static str,
        // check: ?*const fn (viewer: *Viewer, []const Viewer.Action) anyerror!void = null,
        check: Option<CheckFn>,
        // check_command: ?*const fn (viewer: *Viewer, []const u8) anyerror!void = null,
        check_command: Option<CheckCmdFn>,
    }

    impl TestStep {
        fn new(input: Input) -> TestStep {
            TestStep {
                input,
                contains_tags: Vec::new(),
                contains_command: "",
                check: None,
                check_command: None,
            }
        }

        // fn run(self: TestStep, viewer: *Viewer) !void {
        fn run(self, viewer: &mut Viewer) {
            // const actions = viewer.next(self.input);
            //
            // PORT NOTE: `actions` borrows `viewer`; the reference's `check`
            // closures take `*Viewer` AND the actions slice simultaneously. To
            // satisfy the borrow checker we collect the data the checks need from
            // `actions` (their tags + command bytes) into owned values, releasing
            // the borrow before invoking `check`/`check_command` with `&mut
            // viewer`. Behavior is identical: the checks see the same actions and
            // the same (post-`next`) viewer state.
            let TestStep {
                input,
                contains_tags,
                contains_command,
                check,
                check_command,
            } = self;
            let actions: Vec<ActionTag> = viewer.next(input).iter().map(|a| a.tag()).collect();
            // We re-borrow to read command bytes (immutable) for the checks.
            let actions_owned: Vec<OwnedAction> = {
                // SAFETY/PORT NOTE: re-run is not possible; instead snapshot the
                // actions returned by the single `next` call above. We cannot
                // hold the `&[Action]` borrow across the `&mut viewer` checks, so
                // snapshot into owned `OwnedAction`s here. `next` is idempotent
                // for reads — we read the SAME `self.actions` buffer it returned.
                viewer.actions.iter().map(OwnedAction::from).collect()
            };
            let _ = actions; // tags also available via actions_owned

            // Common mistake, forgetting the newline on a command.
            // for (actions) |action| {
            //     if (action == .command) {
            //         try testing.expect(std.mem.endsWith(u8, action.command, "\n"));
            //     }
            // }
            for action in &actions_owned {
                if let OwnedAction::Command(cmd) = action {
                    assert!(cmd.ends_with(b"\n"));
                }
            }

            // for (self.contains_tags) |tag| {
            //     var found = false;
            //     for (actions) |action| if (action == tag) { found = true; break; };
            //     try testing.expect(found);
            // }
            for tag in &contains_tags {
                let mut found = false;
                for action in &actions_owned {
                    if action.tag() == *tag {
                        found = true;
                        break;
                    }
                }
                assert!(found);
            }

            // if (self.contains_command.len > 0) {
            //     var found = false;
            //     for (actions) |action| {
            //         if (action == .command and std.mem.startsWith(u8, action.command, self.contains_command)) {
            //             found = true; break;
            //         }
            //     }
            //     try testing.expect(found);
            // }
            if !contains_command.is_empty() {
                let mut found = false;
                for action in &actions_owned {
                    if let OwnedAction::Command(cmd) = action {
                        if cmd.starts_with(contains_command.as_bytes()) {
                            found = true;
                            break;
                        }
                    }
                }
                assert!(found);
            }

            // if (self.check) |check_fn| try check_fn(viewer, actions);
            if let Some(check_fn) = check {
                let action_slice: Vec<Action> =
                    actions_owned.iter().map(|a| a.to_action()).collect();
                check_fn(viewer, &action_slice);
            }

            // if (self.check_command) |check_fn| {
            //     var found = false;
            //     for (actions) |action| {
            //         if (action == .command) { found = true; try check_fn(viewer, action.command); }
            //     }
            //     try testing.expect(found);
            // }
            if let Some(check_fn) = check_command {
                let mut found = false;
                let cmds: Vec<Vec<u8>> = actions_owned
                    .iter()
                    .filter_map(|a| match a {
                        OwnedAction::Command(c) => Some(c.clone()),
                        _ => None,
                    })
                    .collect();
                for cmd in cmds {
                    found = true;
                    check_fn(viewer, &cmd);
                }
                assert!(found);
            }
        }
    }

    // A snapshot of an Action sufficient for the test harness's checks
    // (tag + command bytes + windows-empty test).
    enum OwnedAction {
        Exit,
        Command(Vec<u8>),
        Windows(Vec<Window>),
    }

    impl OwnedAction {
        fn from(a: &Action) -> OwnedAction {
            match a {
                Action::Exit => OwnedAction::Exit,
                Action::Command(c) => OwnedAction::Command(c.clone()),
                Action::Windows(w) => OwnedAction::Windows(w.clone()),
            }
        }
        fn tag(&self) -> ActionTag {
            match self {
                OwnedAction::Exit => ActionTag::Exit,
                OwnedAction::Command(_) => ActionTag::Command,
                OwnedAction::Windows(_) => ActionTag::Windows,
            }
        }
        fn to_action(&self) -> Action {
            match self {
                OwnedAction::Exit => Action::Exit,
                OwnedAction::Command(c) => Action::Command(c.clone()),
                OwnedAction::Windows(w) => Action::Windows(w.clone()),
            }
        }
    }

    /// A helper to run a series of test steps against a viewer and assert
    /// that the expected actions are produced.
    ///
    /// I'm generally not a fan of these types of abstracted tests because
    /// it makes diagnosing failures harder, but being able to construct
    /// simulated tmux inputs and verify outputs is going to be extremely
    /// important since the tmux control mode protocol is very complex and
    /// fragile.
    // fn testViewer(viewer: *Viewer, steps: []const TestStep) !void {
    /// PORT NOTE: no reference analogue. The reference narrows the layout's
    /// `usize` width/height with `@intCast`, which is a CHECKED narrowing in
    /// every safe build mode — an out-of-range layout is illegal behavior
    /// detected, not a silent wrap. The port used bare `as`, which wraps modulo
    /// 2^16 and hands `Terminal::init` a wrong (possibly 0) size (S209-F1).
    /// Per PORT_RULES R7 `@intCast` ports as `try_from(..).unwrap()`, so this
    /// test asserts the trap.
    #[test]
    #[should_panic(expected = "@intCast: tmux pane layout width")]
    fn sync_layouts_pane_width_overflow_traps() {
        let mut viewer = Viewer::init(Global).unwrap();
        let window = Window {
            id: 0,
            width: 80,
            height: 24,
            layout: crate::tmux::layout::Layout {
                width: 70_000,
                height: 24,
                x: 0,
                y: 0,
                content: crate::tmux::layout::Content::Pane(1),
            },
        };
        let _ = viewer.sync_layouts(vec![window], false);
    }

    /// The positive control for the test above: the SAME code path with an
    /// in-range width does NOT panic and does create the pane. Without this, a
    /// `should_panic` test proves only that something panicked somewhere.
    #[test]
    fn sync_layouts_pane_width_in_range_is_created() {
        let mut viewer = Viewer::init(Global).unwrap();
        let window = Window {
            id: 0,
            width: 80,
            height: 24,
            layout: crate::tmux::layout::Layout {
                width: 80,
                height: 24,
                x: 0,
                y: 0,
                content: crate::tmux::layout::Content::Pane(1),
            },
        };
        viewer.sync_layouts(vec![window], false).unwrap();
        assert!(viewer.panes.contains(&1));
    }

    fn test_viewer(viewer: &mut Viewer, steps: Vec<TestStep>) {
        // for (steps, 0..) |step, i| {
        //     step.run(viewer) catch |err| {
        //         log.warn("testViewer step failed i={} step={}", .{ i, step });
        //         return err;
        //     };
        // }
        //
        // PORT NOTE (logging facade, S209-F2): this is the ONE `log.*` call in
        // `viewer.zig` that has no restorable counterpart here, and it is
        // deliberately left as a comment. Two independent reasons, either one
        // sufficient: (1) `TestStep::run` returns `()` and reports a failed step
        // by panicking (the port of the reference's `!void` + `try`-assertions),
        // so there is no error value to catch and log before propagating —
        // Rust's panic message already carries the diagnostic the reference's
        // `log.warn` exists to add; (2) `step={}` formats the `TestStep` value
        // itself, and `TestStep` holds `Box<dyn Fn>` check closures, which have
        // no `Debug`/`Display` — producing that argument would mean inventing a
        // representation the reference does not define. This is a test-only
        // helper (`#[cfg(test)]`); no production diagnostic is lost.
        for step in steps {
            step.run(viewer);
        }
    }

    // Helper to read the active screen content of a pane as a String.
    fn pane_dump(v: &mut Viewer, id: usize, tag: crate::point::Tag) -> String {
        let pane: &mut Pane = v.panes.get_ptr(&id).unwrap();
        let screen: &Screen = unsafe { &*pane.terminal.screens.active };
        let bytes = screen
            .dump_string_alloc(Global, point_for_tag(tag))
            .unwrap();
        String::from_utf8(bytes).unwrap()
    }

    fn point_for_tag(tag: crate::point::Tag) -> crate::point::Point {
        crate::point::Point::from_c(tag, crate::point::Coordinate { x: 0, y: 0 })
    }

    // test "immediate exit"
    #[test]
    fn immediate_exit() {
        // var viewer = try Viewer.init(testing.allocator);
        // defer viewer.deinit();
        let mut viewer = Viewer::init(Global).unwrap();

        test_viewer(
            &mut viewer,
            vec![
                TestStep {
                    contains_tags: vec![ActionTag::Exit],
                    ..TestStep::new(tmux(control::Notification::Exit))
                },
                TestStep {
                    check: Some(Box::new(|_v: &mut Viewer, actions: &[Action]| {
                        // try testing.expectEqual(0, actions.len);
                        assert_eq!(0, actions.len());
                    })),
                    ..TestStep::new(tmux(control::Notification::Exit))
                },
            ],
        );
    }

    // test "session changed resets state"
    #[test]
    fn session_changed_resets_state() {
        let mut viewer = Viewer::init(Global).unwrap();

        test_viewer(
            &mut viewer,
            vec![
                // Initial startup
                TestStep::new(tmux(block_end(""))),
                TestStep {
                    contains_command: "display-message",
                    ..TestStep::new(tmux(control::Notification::SessionChanged {
                        id: 1,
                        name: b"first".to_vec(),
                    }))
                },
                // Receive version response, which triggers list-windows
                TestStep {
                    contains_command: "list-windows",
                    ..TestStep::new(tmux(block_end("3.5a")))
                },
                // Receive window layout with two panes (same format as "initial flow" test)
                TestStep {
                    contains_tags: vec![ActionTag::Windows, ActionTag::Command],
                    check: Some(Box::new(|v: &mut Viewer, _: &[Action]| {
                        assert_eq!(1, v.session_id);
                        assert_eq!(1, v.windows.len());
                        assert_eq!(2, v.panes.count());
                        assert_eq!(b"3.5a".as_slice(), v.tmux_version.as_slice());
                    })),
                    ..TestStep::new(tmux(block_end(
                        "$1 @0 83 44 027b,83x44,0,0[83x20,0,0,0,83x23,0,21,1]",
                    )))
                },
                // Now session changes - should reset everything but keep version
                TestStep {
                    contains_tags: vec![ActionTag::Windows, ActionTag::Command],
                    contains_command: "list-windows",
                    check: Some(Box::new(|v: &mut Viewer, actions: &[Action]| {
                        // Session ID should be updated
                        assert_eq!(2, v.session_id);
                        // Windows should be cleared (empty windows action sent)
                        let mut found_empty_windows = false;
                        for action in actions {
                            if let Action::Windows(w) = action {
                                if w.is_empty() {
                                    found_empty_windows = true;
                                }
                            }
                        }
                        assert!(found_empty_windows);
                        // Old windows should be cleared
                        assert_eq!(0, v.windows.len());
                        // Old panes should be cleared
                        assert_eq!(0, v.panes.count());
                        // Version should still be preserved
                        assert_eq!(b"3.5a".as_slice(), v.tmux_version.as_slice());
                    })),
                    ..TestStep::new(tmux(control::Notification::SessionChanged {
                        id: 2,
                        name: b"second".to_vec(),
                    }))
                },
                // Receive new window layout for new session (same layout, different session/window)
                // Uses same pane IDs 0,1 - they should be re-created since old panes were cleared
                TestStep {
                    contains_tags: vec![ActionTag::Windows, ActionTag::Command],
                    check: Some(Box::new(|v: &mut Viewer, _: &[Action]| {
                        assert_eq!(2, v.session_id);
                        assert_eq!(1, v.windows.len());
                        assert_eq!(1, v.windows[0].id);
                        // Panes 0 and 1 should be created (fresh, since old ones were cleared)
                        assert_eq!(2, v.panes.count());
                    })),
                    ..TestStep::new(tmux(block_end(
                        "$2 @1 83 44 027b,83x44,0,0[83x20,0,0,0,83x23,0,21,1]",
                    )))
                },
                TestStep {
                    contains_tags: vec![ActionTag::Exit],
                    ..TestStep::new(tmux(control::Notification::Exit))
                },
            ],
        );
    }

    // test "initial flow"
    #[test]
    fn initial_flow() {
        let mut viewer = Viewer::init(Global).unwrap();

        test_viewer(
            &mut viewer,
            vec![
                TestStep::new(tmux(block_end(""))),
                TestStep {
                    contains_command: "display-message",
                    check: Some(Box::new(|v: &mut Viewer, _: &[Action]| {
                        assert_eq!(42, v.session_id);
                    })),
                    ..TestStep::new(tmux(control::Notification::SessionChanged {
                        id: 42,
                        name: b"main".to_vec(),
                    }))
                },
                // Receive version response, which triggers list-windows
                TestStep {
                    contains_command: "list-windows",
                    check: Some(Box::new(|v: &mut Viewer, _: &[Action]| {
                        assert_eq!(b"3.5a".as_slice(), v.tmux_version.as_slice());
                    })),
                    ..TestStep::new(tmux(block_end("3.5a")))
                },
                TestStep {
                    contains_tags: vec![ActionTag::Windows, ActionTag::Command],
                    contains_command: "capture-pane",
                    // pane_history for pane 0 (primary)
                    check_command: Some(Box::new(|_v: &mut Viewer, command: &[u8]| {
                        assert!(contains_at_least(command, 1, b"-t %0"));
                        assert!(!contains_at_least(command, 1, b"-a"));
                    })),
                    ..TestStep::new(tmux(block_end(
                        "$0 @0 83 44 027b,83x44,0,0[83x20,0,0,0,83x23,0,21,1]",
                    )))
                },
                TestStep {
                    // Moves on to pane_visible for pane 0 (primary)
                    contains_command: "capture-pane",
                    check_command: Some(Box::new(|_v: &mut Viewer, command: &[u8]| {
                        assert!(contains_at_least(command, 1, b"-t %0"));
                        assert!(!contains_at_least(command, 1, b"-a"));
                    })),
                    check: Some(Box::new(|v: &mut Viewer, _: &[Action]| {
                        {
                            let str = pane_dump(v, 0, crate::point::Tag::History);
                            assert_eq!("Hello, world!", str);
                        }
                        {
                            let str = pane_dump(v, 0, crate::point::Tag::Active);
                            assert_eq!("", str);
                        }
                    })),
                    ..TestStep::new(tmux(block_end("Hello, world!")))
                },
                TestStep {
                    // Moves on to pane_history for pane 0 (alternate)
                    contains_command: "capture-pane",
                    check_command: Some(Box::new(|_v: &mut Viewer, command: &[u8]| {
                        assert!(contains_at_least(command, 1, b"-t %0"));
                        assert!(contains_at_least(command, 1, b"-a"));
                    })),
                    ..TestStep::new(tmux(block_end("")))
                },
                TestStep {
                    // Moves on to pane_visible for pane 0 (alternate)
                    contains_command: "capture-pane",
                    check_command: Some(Box::new(|_v: &mut Viewer, command: &[u8]| {
                        assert!(contains_at_least(command, 1, b"-t %0"));
                        assert!(contains_at_least(command, 1, b"-a"));
                    })),
                    ..TestStep::new(tmux(block_end("")))
                },
                TestStep {
                    // Moves on to pane_history for pane 1 (primary)
                    contains_command: "capture-pane",
                    check_command: Some(Box::new(|_v: &mut Viewer, command: &[u8]| {
                        assert!(contains_at_least(command, 1, b"-t %1"));
                        assert!(!contains_at_least(command, 1, b"-a"));
                    })),
                    ..TestStep::new(tmux(block_end("")))
                },
                TestStep {
                    // Moves on to pane_visible for pane 1 (primary)
                    contains_command: "capture-pane",
                    check_command: Some(Box::new(|_v: &mut Viewer, command: &[u8]| {
                        assert!(contains_at_least(command, 1, b"-t %1"));
                        assert!(!contains_at_least(command, 1, b"-a"));
                    })),
                    ..TestStep::new(tmux(block_end("")))
                },
                TestStep {
                    // Moves on to pane_history for pane 1 (alternate)
                    contains_command: "capture-pane",
                    check_command: Some(Box::new(|_v: &mut Viewer, command: &[u8]| {
                        assert!(contains_at_least(command, 1, b"-t %1"));
                        assert!(contains_at_least(command, 1, b"-a"));
                    })),
                    ..TestStep::new(tmux(block_end("")))
                },
                TestStep {
                    // Moves on to pane_visible for pane 1 (alternate)
                    contains_command: "capture-pane",
                    check_command: Some(Box::new(|_v: &mut Viewer, command: &[u8]| {
                        assert!(contains_at_least(command, 1, b"-t %1"));
                        assert!(contains_at_least(command, 1, b"-a"));
                    })),
                    ..TestStep::new(tmux(block_end("")))
                },
                TestStep::new(tmux(block_end(""))),
                TestStep {
                    check: Some(Box::new(|v: &mut Viewer, actions: &[Action]| {
                        assert_eq!(0, actions.len());
                        let str = pane_dump(v, 0, crate::point::Tag::Active);
                        assert!(contains_at_least(str.as_bytes(), 1, b"new output"));
                    })),
                    ..TestStep::new(tmux(control::Notification::Output {
                        pane_id: 0,
                        data: b"new output".to_vec(),
                    }))
                },
                TestStep {
                    check: Some(Box::new(|_v: &mut Viewer, actions: &[Action]| {
                        assert_eq!(0, actions.len());
                    })),
                    ..TestStep::new(tmux(control::Notification::Output {
                        pane_id: 999,
                        data: b"ignored".to_vec(),
                    }))
                },
                TestStep {
                    contains_tags: vec![ActionTag::Exit],
                    ..TestStep::new(tmux(control::Notification::Exit))
                },
            ],
        );
    }

    // test "layout change"
    #[test]
    fn layout_change() {
        let mut viewer = Viewer::init(Global).unwrap();

        test_viewer(
            &mut viewer,
            vec![
                // Initial startup
                TestStep::new(tmux(block_end(""))),
                TestStep {
                    contains_command: "display-message",
                    ..TestStep::new(tmux(control::Notification::SessionChanged {
                        id: 1,
                        name: b"test".to_vec(),
                    }))
                },
                // Receive version response, which triggers list-windows
                TestStep {
                    contains_command: "list-windows",
                    ..TestStep::new(tmux(block_end("3.5a")))
                },
                // Receive initial window layout with one pane
                TestStep {
                    contains_tags: vec![ActionTag::Windows, ActionTag::Command],
                    check: Some(Box::new(|v: &mut Viewer, _: &[Action]| {
                        assert_eq!(1, v.windows.len());
                        assert_eq!(1, v.panes.count());
                        assert!(v.panes.contains(&0));
                    })),
                    ..TestStep::new(tmux(block_end("$0 @0 83 44 b7dd,83x44,0,0,0")))
                },
                // Complete all capture-pane commands for pane 0 (primary and alternate)
                // plus pane_state
                TestStep::new(tmux(block_end(""))),
                TestStep::new(tmux(block_end(""))),
                TestStep::new(tmux(block_end(""))),
                TestStep::new(tmux(block_end(""))),
                TestStep::new(tmux(block_end(""))),
                // Now send a layout_change that splits into two panes
                TestStep {
                    contains_tags: vec![ActionTag::Windows],
                    check: Some(Box::new(|v: &mut Viewer, _: &[Action]| {
                        // Should still have 1 window
                        assert_eq!(1, v.windows.len());
                        // Should now have 2 panes (0 and 2)
                        assert_eq!(2, v.panes.count());
                        assert!(v.panes.contains(&0));
                        assert!(v.panes.contains(&2));
                        // Commands should be queued for the new pane (4 capture-pane + 1 pane_state)
                        assert_eq!(5, v.command_queue.len());
                    })),
                    ..TestStep::new(tmux(control::Notification::LayoutChange {
                        window_id: 0,
                        layout: b"e07b,83x44,0,0[83x22,0,0,0,83x21,0,23,2]".to_vec(),
                        visible_layout: b"e07b,83x44,0,0[83x22,0,0,0,83x21,0,23,2]".to_vec(),
                        raw_flags: b"*".to_vec(),
                    }))
                },
                TestStep {
                    contains_tags: vec![ActionTag::Exit],
                    ..TestStep::new(tmux(control::Notification::Exit))
                },
            ],
        );
    }

    // test "layout_change does not return command when queue not empty"
    #[test]
    fn layout_change_does_not_return_command_when_queue_not_empty() {
        let mut viewer = Viewer::init(Global).unwrap();

        test_viewer(
            &mut viewer,
            vec![
                // Initial startup
                TestStep::new(tmux(block_end(""))),
                TestStep {
                    contains_command: "display-message",
                    ..TestStep::new(tmux(control::Notification::SessionChanged {
                        id: 1,
                        name: b"test".to_vec(),
                    }))
                },
                // Receive version response, which triggers list-windows
                TestStep {
                    contains_command: "list-windows",
                    ..TestStep::new(tmux(block_end("3.5a")))
                },
                // Receive initial window layout with one pane
                TestStep {
                    contains_tags: vec![ActionTag::Windows, ActionTag::Command],
                    check: Some(Box::new(|v: &mut Viewer, _: &[Action]| {
                        assert!(!v.command_queue.is_empty());
                    })),
                    ..TestStep::new(tmux(block_end("$0 @0 83 44 b7dd,83x44,0,0,0")))
                },
                // Do NOT complete capture-pane commands - queue still has commands.
                // Send a layout_change that splits into two panes.
                // This should NOT return a command action since queue was not empty.
                TestStep {
                    contains_tags: vec![ActionTag::Windows],
                    check: Some(Box::new(|v: &mut Viewer, actions: &[Action]| {
                        assert_eq!(2, v.panes.count());
                        // Should not contain a command action
                        for action in actions {
                            assert!(action.tag() != ActionTag::Command);
                        }
                    })),
                    ..TestStep::new(tmux(control::Notification::LayoutChange {
                        window_id: 0,
                        layout: b"e07b,83x44,0,0[83x22,0,0,0,83x21,0,23,2]".to_vec(),
                        visible_layout: b"e07b,83x44,0,0[83x22,0,0,0,83x21,0,23,2]".to_vec(),
                        raw_flags: b"*".to_vec(),
                    }))
                },
                TestStep {
                    contains_tags: vec![ActionTag::Exit],
                    ..TestStep::new(tmux(control::Notification::Exit))
                },
            ],
        );
    }

    // test "layout_change returns command when queue was empty"
    #[test]
    fn layout_change_returns_command_when_queue_was_empty() {
        let mut viewer = Viewer::init(Global).unwrap();

        test_viewer(
            &mut viewer,
            vec![
                // Initial startup
                TestStep::new(tmux(block_end(""))),
                TestStep {
                    contains_command: "display-message",
                    ..TestStep::new(tmux(control::Notification::SessionChanged {
                        id: 1,
                        name: b"test".to_vec(),
                    }))
                },
                // Receive version response, which triggers list-windows
                TestStep {
                    contains_command: "list-windows",
                    ..TestStep::new(tmux(block_end("3.5a")))
                },
                // Receive initial window layout with one pane
                TestStep {
                    contains_tags: vec![ActionTag::Windows, ActionTag::Command],
                    ..TestStep::new(tmux(block_end("$0 @0 83 44 b7dd,83x44,0,0,0")))
                },
                // Complete all capture-pane commands for pane 0
                TestStep::new(tmux(block_end(""))),
                TestStep::new(tmux(block_end(""))),
                TestStep::new(tmux(block_end(""))),
                TestStep::new(tmux(block_end(""))),
                // Queue should now be empty
                TestStep {
                    check: Some(Box::new(|v: &mut Viewer, _: &[Action]| {
                        assert!(v.command_queue.is_empty());
                    })),
                    ..TestStep::new(tmux(block_end("")))
                },
                // Now send a layout_change that splits into two panes.
                // This should return a command action since we're queuing commands
                // for the new pane and the queue was empty.
                TestStep {
                    contains_tags: vec![ActionTag::Windows, ActionTag::Command],
                    check: Some(Box::new(|v: &mut Viewer, _: &[Action]| {
                        assert_eq!(2, v.panes.count());
                        assert!(!v.command_queue.is_empty());
                    })),
                    ..TestStep::new(tmux(control::Notification::LayoutChange {
                        window_id: 0,
                        layout: b"e07b,83x44,0,0[83x22,0,0,0,83x21,0,23,2]".to_vec(),
                        visible_layout: b"e07b,83x44,0,0[83x22,0,0,0,83x21,0,23,2]".to_vec(),
                        raw_flags: b"*".to_vec(),
                    }))
                },
                TestStep {
                    contains_tags: vec![ActionTag::Exit],
                    ..TestStep::new(tmux(control::Notification::Exit))
                },
            ],
        );
    }

    // test "window_add queues list_windows when queue empty"
    #[test]
    fn window_add_queues_list_windows_when_queue_empty() {
        let mut viewer = Viewer::init(Global).unwrap();

        test_viewer(
            &mut viewer,
            vec![
                // Initial startup
                TestStep::new(tmux(block_end(""))),
                TestStep {
                    contains_command: "display-message",
                    ..TestStep::new(tmux(control::Notification::SessionChanged {
                        id: 1,
                        name: b"test".to_vec(),
                    }))
                },
                // Receive version response, which triggers list-windows
                TestStep {
                    contains_command: "list-windows",
                    ..TestStep::new(tmux(block_end("3.5a")))
                },
                // Receive initial window layout with one pane
                TestStep {
                    contains_tags: vec![ActionTag::Windows, ActionTag::Command],
                    ..TestStep::new(tmux(block_end("$0 @0 83 44 b7dd,83x44,0,0,0")))
                },
                // Complete all capture-pane commands for pane 0
                TestStep::new(tmux(block_end(""))),
                TestStep::new(tmux(block_end(""))),
                TestStep::new(tmux(block_end(""))),
                TestStep::new(tmux(block_end(""))),
                // Queue should now be empty
                TestStep {
                    check: Some(Box::new(|v: &mut Viewer, _: &[Action]| {
                        assert!(v.command_queue.is_empty());
                    })),
                    ..TestStep::new(tmux(block_end("")))
                },
                // Now send window_add - should trigger list-windows command
                TestStep {
                    contains_command: "list-windows",
                    check: Some(Box::new(|v: &mut Viewer, _: &[Action]| {
                        // Command queue should have list_windows
                        assert!(!v.command_queue.is_empty());
                        assert_eq!(1, v.command_queue.len());
                    })),
                    ..TestStep::new(tmux(control::Notification::WindowAdd { id: 1 }))
                },
                TestStep {
                    contains_tags: vec![ActionTag::Exit],
                    ..TestStep::new(tmux(control::Notification::Exit))
                },
            ],
        );
    }

    // test "window_add queues list_windows when queue not empty"
    #[test]
    fn window_add_queues_list_windows_when_queue_not_empty() {
        let mut viewer = Viewer::init(Global).unwrap();

        test_viewer(
            &mut viewer,
            vec![
                // Initial startup
                TestStep::new(tmux(block_end(""))),
                TestStep {
                    contains_command: "display-message",
                    ..TestStep::new(tmux(control::Notification::SessionChanged {
                        id: 1,
                        name: b"test".to_vec(),
                    }))
                },
                // Receive version response, which triggers list-windows
                TestStep {
                    contains_command: "list-windows",
                    ..TestStep::new(tmux(block_end("3.5a")))
                },
                // Receive initial window layout with one pane
                TestStep {
                    contains_tags: vec![ActionTag::Windows, ActionTag::Command],
                    check: Some(Box::new(|v: &mut Viewer, _: &[Action]| {
                        // Queue should have capture-pane commands
                        assert!(!v.command_queue.is_empty());
                    })),
                    ..TestStep::new(tmux(block_end("$0 @0 83 44 b7dd,83x44,0,0,0")))
                },
                // Do NOT complete capture-pane commands - queue still has commands.
                // Send window_add - should queue list-windows but NOT return command action
                TestStep {
                    check: Some(Box::new(|v: &mut Viewer, actions: &[Action]| {
                        // Should not contain a command action since queue was not empty
                        for action in actions {
                            assert!(action.tag() != ActionTag::Command);
                        }
                        // But list_windows should be in the queue
                        assert!(!v.command_queue.is_empty());
                    })),
                    ..TestStep::new(tmux(control::Notification::WindowAdd { id: 1 }))
                },
                TestStep {
                    contains_tags: vec![ActionTag::Exit],
                    ..TestStep::new(tmux(control::Notification::Exit))
                },
            ],
        );
    }

    // test "two pane flow with pane state"
    #[test]
    fn two_pane_flow_with_pane_state() {
        let mut viewer = Viewer::init(Global).unwrap();

        test_viewer(
            &mut viewer,
            vec![
                // Initial block_end from attach
                TestStep::new(tmux(block_end(""))),
                // Session changed notification
                TestStep {
                    contains_command: "display-message",
                    check: Some(Box::new(|v: &mut Viewer, _: &[Action]| {
                        assert_eq!(0, v.session_id);
                    })),
                    ..TestStep::new(tmux(control::Notification::SessionChanged {
                        id: 0,
                        name: b"0".to_vec(),
                    }))
                },
                // Receive version response, which triggers list-windows
                TestStep {
                    contains_command: "list-windows",
                    ..TestStep::new(tmux(block_end("3.5a")))
                },
                // list-windows output with 2 panes in a vertical split
                TestStep {
                    contains_tags: vec![ActionTag::Windows, ActionTag::Command],
                    check: Some(Box::new(|v: &mut Viewer, _: &[Action]| {
                        assert_eq!(1, v.windows.len());
                        let window = &v.windows[0];
                        assert_eq!(0, window.id);
                        assert_eq!(165, window.width);
                        assert_eq!(79, window.height);
                        assert_eq!(2, v.panes.count());
                        assert!(v.panes.contains(&0));
                        assert!(v.panes.contains(&4));
                    })),
                    ..TestStep::new(tmux(block_end(
                        "$0 @0 165 79 ca97,165x79,0,0[165x40,0,0,0,165x38,0,41,4]",
                    )))
                },
                // capture-pane pane 0 primary history
                TestStep::new(tmux(block_end("prompt %\nprompt %"))),
                // capture-pane pane 0 primary visible
                TestStep {
                    check: Some(Box::new(|v: &mut Viewer, _: &[Action]| {
                        {
                            let str = pane_dump(v, 0, crate::point::Tag::History);
                            // History has 2 lines with "prompt %" (padded to screen width)
                            assert!(contains_at_least(str.as_bytes(), 2, b"prompt %"));
                        }
                        {
                            let str = pane_dump(v, 0, crate::point::Tag::Active);
                            assert_eq!("prompt %", str);
                        }
                    })),
                    ..TestStep::new(tmux(block_end("prompt %")))
                },
                // capture-pane pane 0 alternate history (empty)
                TestStep::new(tmux(block_end(""))),
                // capture-pane pane 0 alternate visible (empty)
                TestStep::new(tmux(block_end(""))),
                // capture-pane pane 4 primary history
                TestStep::new(tmux(block_end("prompt %"))),
                // capture-pane pane 4 primary visible
                TestStep {
                    check: Some(Box::new(|v: &mut Viewer, _: &[Action]| {
                        {
                            let str = pane_dump(v, 4, crate::point::Tag::History);
                            assert_eq!("prompt %", str);
                        }
                        {
                            let str = pane_dump(v, 4, crate::point::Tag::Active);
                            // Active screen starts with "prompt %" at beginning
                            assert!(str.starts_with("prompt %"));
                        }
                    })),
                    ..TestStep::new(tmux(block_end("prompt %")))
                },
                // capture-pane pane 4 alternate history (empty)
                TestStep::new(tmux(block_end(""))),
                // capture-pane pane 4 alternate visible (empty)
                TestStep::new(tmux(block_end(""))),
                // list-panes output with terminal state
                TestStep {
                    check: Some(Box::new(|v: &mut Viewer, _: &[Action]| {
                        // Pane 0: cursor at (42, 0), cursor visible, wraparound on
                        {
                            let pane: &mut Pane = v.panes.get_ptr(&0).unwrap();
                            let t: *mut Terminal = &mut pane.terminal;
                            let t: &mut Terminal = unsafe { &mut *t };
                            let screen: &Screen =
                                unsafe { &*t.screens.get(screen_set::Key::Primary).unwrap() };
                            assert_eq!(42, screen.cursor.x);
                            assert_eq!(0, screen.cursor.y);
                            assert!(t.modes.get(modes::Mode::CursorVisible));
                            assert!(t.modes.get(modes::Mode::Wraparound));
                            assert!(!t.modes.get(modes::Mode::Insert));
                            assert!(!t.modes.get(modes::Mode::Origin));
                            assert!(!t.modes.get(modes::Mode::KeypadKeys));
                            assert!(!t.modes.get(modes::Mode::CursorKeys));
                        }
                        // Pane 4: cursor at (10, 5), cursor visible, wraparound on
                        {
                            let pane: &mut Pane = v.panes.get_ptr(&4).unwrap();
                            let t: *mut Terminal = &mut pane.terminal;
                            let t: &mut Terminal = unsafe { &mut *t };
                            let screen: &Screen =
                                unsafe { &*t.screens.get(screen_set::Key::Primary).unwrap() };
                            assert_eq!(10, screen.cursor.x);
                            assert_eq!(5, screen.cursor.y);
                            assert!(t.modes.get(modes::Mode::CursorVisible));
                            assert!(t.modes.get(modes::Mode::Wraparound));
                            assert!(!t.modes.get(modes::Mode::Insert));
                            assert!(!t.modes.get(modes::Mode::Origin));
                            assert!(!t.modes.get(modes::Mode::KeypadKeys));
                            assert!(!t.modes.get(modes::Mode::CursorKeys));
                        }
                    })),
                    ..TestStep::new(tmux(block_end(
                        "%0;42;0;1;;;;0;4294967295;4294967295;0;1;0;0;0;0;0;0;0;0;0;;;0;39;8,16,24,32,40,48,56,64,72,80,88,96,104,112,120,128,136,144,152,160\n%4;10;5;1;;;;0;4294967295;4294967295;0;1;0;0;0;0;0;0;0;0;0;;;0;37;8,16,24,32,40,48,56,64,72,80,88,96,104,112,120,128,136,144,152,160",
                    )))
                },
                TestStep {
                    contains_tags: vec![ActionTag::Exit],
                    ..TestStep::new(tmux(control::Notification::Exit))
                },
            ],
        );
    }

    /// `std.mem.containsAtLeast(u8, haystack, count, needle)`.
    fn contains_at_least(haystack: &[u8], count: usize, needle: &[u8]) -> bool {
        if needle.is_empty() {
            return true;
        }
        let mut found = 0usize;
        let mut i = 0usize;
        while i + needle.len() <= haystack.len() {
            if &haystack[i..i + needle.len()] == needle {
                found += 1;
                if found >= count {
                    return true;
                }
                i += needle.len();
            } else {
                i += 1;
            }
        }
        false
    }

    // S209-F2: `received_pane_history` logs `log.info("received pane history
    // for untracked pane id={}", .{id})` (viewer.zig:1049) when the history
    // arrives for a pane id this viewer never tracked.
    #[test]
    fn received_pane_history_for_untracked_pane_logs() {
        let mut viewer = Viewer::init(Global).unwrap();
        assert!(viewer.panes.get_ptr(&999).is_none());

        let cap = crate::log_capture::start();
        assert!(viewer
            .received_pane_history(screen_set::Key::Primary, 999, b"some history")
            .is_ok());
        let messages = cap.messages();
        assert!(
            messages
                .iter()
                .any(|(level, target, msg)| *level == log::Level::Info
                    && target == "terminal_tmux_viewer"
                    && *msg == "received pane history for untracked pane id=999"),
            "expected the untracked-pane-history diagnostic, got: {messages:?}"
        );
        drop(cap);
    }

    // S209-F2: a `layout_change` whose layout string does not parse drives the
    // viewer into the defunct state, and the reference warns on the way
    // (`log.warn("failed to handle layout change, becoming defunct", .{})`,
    // viewer.zig:502) as well as logging the parse failure that caused it
    // (`log.info("failed to parse window layout id={} layout={s}", ...)`,
    // viewer.zig:594).
    #[test]
    fn layout_change_failure_becomes_defunct_and_logs() {
        let mut viewer = Viewer::init(Global).unwrap();

        // Drive the standard startup handshake so window @0 exists and the
        // viewer is in the `command_queue` state.
        test_viewer(
            &mut viewer,
            vec![
                TestStep::new(tmux(block_end(""))),
                TestStep::new(tmux(control::Notification::SessionChanged {
                    id: 1,
                    name: b"test".to_vec(),
                })),
                TestStep::new(tmux(block_end("3.5a"))),
                TestStep::new(tmux(block_end("$0 @0 83 44 b7dd,83x44,0,0,0"))),
            ],
        );
        assert_eq!(1, viewer.windows.len());
        assert_eq!(State::CommandQueue, viewer.state);

        let cap = crate::log_capture::start();
        let _ = viewer.next(tmux(control::Notification::LayoutChange {
            window_id: 0,
            layout: b"not-a-layout".to_vec(),
            visible_layout: b"not-a-layout".to_vec(),
            raw_flags: b"*".to_vec(),
        }));
        let messages = cap.messages();
        assert_eq!(State::Defunct, viewer.state);
        assert!(
            messages
                .iter()
                .any(|(level, target, msg)| *level == log::Level::Warn
                    && target == "terminal_tmux_viewer"
                    && *msg == "failed to handle layout change, becoming defunct"),
            "expected the becoming-defunct warning, got: {messages:?}"
        );
        assert!(
            messages
                .iter()
                .any(|(level, target, msg)| *level == log::Level::Info
                    && target == "terminal_tmux_viewer"
                    && *msg == "failed to parse window layout id=0 layout=not-a-layout"),
            "expected the layout parse-failure diagnostic, got: {messages:?}"
        );
        drop(cap);
    }
}
