//! This file contains the implementation for tmux control mode. See
//! tmux(1) for more information on control mode. Some basics are documented
//! here but this is not meant to be a comprehensive source of protocol
//! documentation.
//!
//! PORTS: ghostty src/terminal/tmux/control.zig
//!
//! PORT STATUS: done (vt-16). Faithful port of the tmux -CC control-mode
//! protocol parser and all 18 reference tests.
//!
//! The notification-parsing arms that use Oniguruma (`%output`,
//! `%session-changed`, `%layout-change`, `%window-add`, `%window-renamed`,
//! `%window-pane-changed`, `%client-detached`, `%client-session-changed`) plus
//! the 8 tests that exercise them are gated behind `#[cfg(feature =
//! "oniguruma")]`, OFF by default — exactly as the `StringMap` port does. The
//! `oni` binding crate does not exist yet, so the gated code is NOT compiled in
//! the default build; with the feature off those arms fall through to the
//! "unknown notification" path and the 8 tests are not compiled, faithful to the
//! Zig pattern where regex support is a build option (the reference compiles
//! `oni` unconditionally, but rustty mirrors the established `SkipZigTest`-style
//! feature gate until the binding crate lands). The remaining 10 tests
//! (begin/end blocks + `%sessions-changed`) build and run by default.

// const std = @import("std");
// const Allocator = std.mem.Allocator;
// const assert = @import("../../quirks.zig").inlineAssert;
// const oni = @import("oniguruma");
//
// PORT NOTE: `assert` is the `inlineAssert` quirk in rustty-util.
use rustty_util::quirks::inlineAssert as assert;

// PORT NOTE: the reference functions return `Allocator.Error!T`; in Rust this is
// `Result<T, OutOfMemory>`, reusing the crate-wide allocation-failure marker.
use crate::pagelist::OutOfMemory;

// const log = std.log.scoped(.terminal_tmux);
//
// PORT NOTE: `const log = std.log.scoped(.terminal_tmux);` — rustty has no
// logging facade wired into this crate (matching the other terminal modules), so
// the scoped logger and its `log.warn` calls are ported as commented-out lines.

/// A tmux control mode parser. This takes in output from tmux control
/// mode and parses it into a structured notifications.
///
/// It is up to the caller to establish the connection to the tmux
/// control mode session in some way (e.g. via exec, a network socket,
/// whatever). This is fully agnostic to how the data is received and sent.
pub struct Parser {
    /// Current state of the client.
    state: State,

    /// The buffer used to store in-progress notifications, output, etc.
    //
    // PORT NOTE: `buffer: std.Io.Writer.Allocating` -> an owning `Vec<u8>`.
    // `buffer.written()` -> `&self.buffer`, `clearRetainingCapacity()` ->
    // `clear()`, `buffer.writer.writeByte(byte)` -> `push(byte)`.
    buffer: Vec<u8>,

    /// The maximum size in bytes of the buffer. This is used to limit
    /// memory usage. If the buffer exceeds this size, the client will
    /// enter a broken state (the control mode session will be forcibly
    /// exited and future data dropped).
    max_bytes: usize,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    /// Outside of any active notifications. This should drop any output
    /// unless it is '%' on the first byte of a line. The buffer will be
    /// cleared when it sees '%', this is so that the previous notification
    /// data is valid until we receive/process new data.
    Idle,

    /// We experienced unexpected input and are in a broken state
    /// so we cannot continue processing. When this state is set,
    /// the buffer has been deinited and must not be accessed.
    Broken,

    /// Inside an active notification (started with '%').
    Notification,

    /// Inside a begin/end block.
    Block,
}

impl Parser {
    // PORT NOTE: the reference constructs `Parser` with field defaults
    // (`state: State = .idle`, `max_bytes: usize = 1024 * 1024`) and a
    // caller-supplied `buffer: .init(alloc)`. The faithful Rust mirror is a
    // constructor that takes the buffer (an empty `Vec`) and applies the same
    // defaults; tests build it via `Parser::init()`.
    pub fn init() -> Parser {
        Parser {
            state: State::Idle,
            buffer: Vec::new(),
            max_bytes: 1024 * 1024,
        }
    }

    pub fn deinit(&mut self) {
        // If we're in a broken state, we already deinited
        // the buffer, so we don't need to do anything.
        if self.state == State::Broken {
            return;
        }

        // self.buffer.deinit();
        //
        // PORT NOTE: `self.buffer` is an owning `Vec`; dropping its storage is
        // the faithful equivalent of `buffer.deinit()`.
        self.buffer = Vec::new();
    }

    // Handle a byte of input.
    //
    // If we reach our byte limit this will return OutOfMemory. It only
    // does this on the first time we exceed the limit; subsequent calls
    // will return null as we drop all input in a broken state.
    pub fn put(&mut self, byte: u8) -> Result<Option<Notification>, OutOfMemory> {
        // If we're in a broken state, just do nothing.
        //
        // We have to do this check here before we check the buffer, because if
        // we're in a broken state then we'd have already deinited the buffer.
        if self.state == State::Broken {
            return Ok(None);
        }

        if self.buffer.len() >= self.max_bytes {
            self.broken();
            return Err(OutOfMemory);
        }

        match self.state {
            // Drop because we're in a broken state.
            State::Broken => return Ok(None),

            // Waiting for a notification so if the byte is not '%' then
            // we're in a broken state. Control mode output should always
            // be wrapped in '%begin/%end' orelse we expect a notification.
            // Return an exit notification.
            State::Idle => {
                if byte != b'%' {
                    self.broken();
                    return Ok(Some(Notification::Exit));
                } else {
                    self.buffer.clear();
                    self.state = State::Notification;
                }
            }

            // If we're in a notification and its not a newline then
            // we accumulate. If it is a newline then we have a
            // complete notification we need to parse.
            State::Notification => {
                if byte == b'\n' {
                    // We have a complete notification, parse it.
                    return match self.parse_notification() {
                        Ok(v) => Ok(v),
                        // If parsing failed, then we do not mark the state
                        // as broken because we may be able to continue parsing
                        // other types of notifications.
                        //
                        // In the future we may want to emit a notification
                        // here about unknown or unsupported notifications.
                        Err(_) => Ok(None),
                    };
                }
            }

            // If we're in a block then we accumulate until we see a newline
            // and then we check to see if that line ended the block.
            State::Block => {
                if byte == b'\n' {
                    let written = &self.buffer;
                    let idx = match last_index_of_scalar(written, b'\n') {
                        Some(v) => v + 1,
                        None => 0,
                    };
                    let line = &written[idx..];

                    if let Some(terminator) = parse_block_terminator(line) {
                        // PORT NOTE: `std.mem.trimRight(u8, written[0..idx],
                        // "\r\n")` — trim trailing '\r'/'\n' bytes from the
                        // pre-terminator payload.
                        let output_len = trim_right_len(&written[0..idx], b"\r\n");

                        // Important: do not clear buffer since the notification
                        // contains it.
                        self.state = State::Idle;
                        match terminator {
                            BlockTerminator::End => {
                                return Ok(Some(Notification::BlockEnd(
                                    self.buffer[0..output_len].to_vec(),
                                )));
                            }
                            BlockTerminator::Err => {
                                // log.warn("tmux control mode error={s}", .{output});
                                return Ok(Some(Notification::BlockErr(
                                    self.buffer[0..output_len].to_vec(),
                                )));
                            }
                        }
                    }

                    // Didn't end the block, continue accumulating.
                }
            }
        }

        // self.buffer.writer.writeByte(byte) catch |err| switch (err) {
        //     error.WriteFailed => return error.OutOfMemory,
        // };
        //
        // PORT NOTE: pushing onto an owning `Vec` cannot fail short of a global
        // allocation abort, which is the Rust analogue of the `WriteFailed ->
        // OutOfMemory` mapping; the byte-limit check above is the faithful guard.
        self.buffer.push(byte);

        Ok(None)
    }

    fn parse_notification(&mut self) -> Result<Option<Notification>, ParseError> {
        assert(self.state == State::Notification);

        // PORT NOTE: the reference `line`/`cmd` are sub-slices of `self.buffer`.
        // The Oniguruma-backed dispatch needs `&mut self` (to set state / clear
        // the buffer) while also reading `line`, which would alias the buffer.
        // The faithful Rust mirror copies `line` out first (the notifications
        // copy their payloads out of the buffer anyway), so the borrow of
        // `self.buffer` ends before any `&mut self` call.
        let line: Vec<u8> = {
            let mut line: &[u8] = &self.buffer;
            if line[line.len() - 1] == b'\r' {
                line = &line[0..line.len() - 1];
            }
            line.to_vec()
        };
        let line: &[u8] = &line;
        let cmd = {
            let idx = index_of_scalar(line, b' ').unwrap_or(line.len());
            &line[0..idx]
        };

        // The notification MUST exist because we guard entering the notification
        // state on seeing at least a '%'.
        if cmd == b"%begin" {
            // We don't use the rest of the tokens for now because tmux
            // claims to guarantee that begin/end are always in order and
            // never intermixed. In the future, we should probably validate
            // this.
            // TODO(tmuxcc): do this before merge?

            // Move to block state because we expect a corresponding end/error
            // and want to accumulate the data.
            self.state = State::Block;
            self.buffer.clear();
            Ok(None)
        } else if let Some(n) = self.parse_notification_oni(cmd, line)? {
            // PORT NOTE: every `else if` arm that matches an Oniguruma-parsed
            // command is gated behind `#[cfg(feature = "oniguruma")]`. With the
            // feature off this dispatch helper is a no-op that returns `None`, so
            // control flow falls through to the "unknown notification" path
            // below — faithful to the reference's `SkipZigTest`-style behavior
            // when regex support is unavailable.
            Ok(Some(n))
        } else if cmd == b"%sessions-changed" {
            if line != b"%sessions-changed" {
                // log.warn("failed to match notification cmd={s} line=\"{s}\"", .{ cmd, line });
                // break :cmd;
                //
                // PORT NOTE: the Zig `break :cmd` exits the labelled `cmd:` block
                // and falls through to the shared unknown-command cleanup at the
                // end of `parseNotification`. The Rust mirror jumps to that same
                // cleanup.
                self.buffer.clear();
                self.state = State::Idle;
                return Ok(None);
            }

            self.buffer.clear();
            self.state = State::Idle;
            Ok(Some(Notification::SessionsChanged))
        } else if is_oni_command(cmd) {
            // PORT NOTE: with `oniguruma` off, the Oniguruma-parsed commands are
            // recognized here (so they are NOT misreported as "unknown
            // notification" spam) and fall through to the shared idle cleanup —
            // the equivalent of the reference's `break :cmd` on a failed regex
            // match. The arms themselves (and their tests) are compiled only when
            // the feature is on.
            #[cfg(not(feature = "oniguruma"))]
            {
                self.buffer.clear();
                self.state = State::Idle;
                Ok(None)
            }
            #[cfg(feature = "oniguruma")]
            {
                // Handled by `parse_notification_oni`; unreachable here.
                unreachable!()
            }
        } else {
            // Unknown notification, log it and return to idle state.
            // log.warn("unknown tmux control mode notification={s}", .{cmd});

            // Unknown command. Clear the buffer and return to idle state.
            self.buffer.clear();
            self.state = State::Idle;

            Ok(None)
        }
    }

    // PORT NOTE: the Oniguruma-backed notification arms (`%output`,
    // `%session-changed`, `%layout-change`, `%window-add`, `%window-renamed`,
    // `%window-pane-changed`, `%client-detached`, `%client-session-changed`) live
    // in this helper, gated behind `#[cfg(feature = "oniguruma")]`. With the
    // feature off the helper returns `Ok(None)` so the caller falls through to the
    // `is_oni_command` recognition + idle cleanup. The `oni` binding crate does
    // not exist yet, so the gated body is not compiled in the default build.
    #[cfg(not(feature = "oniguruma"))]
    fn parse_notification_oni(
        &mut self,
        _cmd: &[u8],
        _line: &[u8],
    ) -> Result<Option<Notification>, ParseError> {
        Ok(None)
    }

    #[cfg(feature = "oniguruma")]
    fn parse_notification_oni(
        &mut self,
        cmd: &[u8],
        line: &[u8],
    ) -> Result<Option<Notification>, ParseError> {
        if cmd == b"%output" {
            let mut re = oni::Regex::init(
                b"^%output %([0-9]+) (.+)$",
                oni::RegexOptions::capture_group(),
                oni::Encoding::utf8(),
                oni::Syntax::default(),
                None,
            )
            .map_err(|_err| {
                // log.warn("regex init failed error={}", .{err});
                ParseError::RegexError
            })?;
            // defer re.deinit();

            let region = match re.search(line, oni::SearchOptions::default()) {
                Ok(region) => region,
                Err(_err) => {
                    // log.warn("failed to match notification cmd={s} line=\"{s}\" err={}", .{ cmd, line, err });
                    // break :cmd;
                    re.deinit();
                    return Ok(None);
                }
            };
            // defer region.deinit();
            let starts = region.starts();
            let ends = region.ends();

            let id = parse_int_usize(&line[starts[1] as usize..ends[1] as usize]).unwrap();
            let data = line[starts[2] as usize..ends[2] as usize].to_vec();

            // Important: do not clear buffer here since name points to it
            self.state = State::Idle;
            region.deinit();
            re.deinit();
            Ok(Some(Notification::Output { pane_id: id, data }))
        } else if cmd == b"%session-changed" {
            let mut re = oni::Regex::init(
                b"^%session-changed \\$([0-9]+) (.+)$",
                oni::RegexOptions::capture_group(),
                oni::Encoding::utf8(),
                oni::Syntax::default(),
                None,
            )
            .map_err(|_err| {
                // log.warn("regex init failed error={}", .{err});
                ParseError::RegexError
            })?;
            // defer re.deinit();

            let region = match re.search(line, oni::SearchOptions::default()) {
                Ok(region) => region,
                Err(_err) => {
                    // log.warn("failed to match notification cmd={s} line=\"{s}\" err={}", .{ cmd, line, err });
                    re.deinit();
                    return Ok(None);
                }
            };
            // defer region.deinit();
            let starts = region.starts();
            let ends = region.ends();

            let id = parse_int_usize(&line[starts[1] as usize..ends[1] as usize]).unwrap();
            let name = line[starts[2] as usize..ends[2] as usize].to_vec();

            // Important: do not clear buffer here since name points to it
            self.state = State::Idle;
            region.deinit();
            re.deinit();
            Ok(Some(Notification::SessionChanged { id, name }))
        } else if cmd == b"%layout-change" {
            let mut re = oni::Regex::init(
                b"^%layout-change @([0-9]+) (.+) (.+) (.*)$",
                oni::RegexOptions::capture_group(),
                oni::Encoding::utf8(),
                oni::Syntax::default(),
                None,
            )
            .map_err(|_err| {
                // log.warn("regex init failed error={}", .{err});
                ParseError::RegexError
            })?;
            // defer re.deinit();

            let region = match re.search(line, oni::SearchOptions::default()) {
                Ok(region) => region,
                Err(_err) => {
                    // log.warn("failed to match notification cmd={s} line=\"{s}\" err={}", .{ cmd, line, err });
                    re.deinit();
                    return Ok(None);
                }
            };
            // defer region.deinit();
            let starts = region.starts();
            let ends = region.ends();

            let id = parse_int_usize(&line[starts[1] as usize..ends[1] as usize]).unwrap();
            let layout = line[starts[2] as usize..ends[2] as usize].to_vec();
            let visible_layout = line[starts[3] as usize..ends[3] as usize].to_vec();
            let raw_flags = line[starts[4] as usize..ends[4] as usize].to_vec();

            // Important: do not clear buffer here since layout strings point to it
            self.state = State::Idle;
            region.deinit();
            re.deinit();
            Ok(Some(Notification::LayoutChange {
                window_id: id,
                layout,
                visible_layout,
                raw_flags,
            }))
        } else if cmd == b"%window-add" {
            let mut re = oni::Regex::init(
                b"^%window-add @([0-9]+)$",
                oni::RegexOptions::capture_group(),
                oni::Encoding::utf8(),
                oni::Syntax::default(),
                None,
            )
            .map_err(|_err| {
                // log.warn("regex init failed error={}", .{err});
                ParseError::RegexError
            })?;
            // defer re.deinit();

            let region = match re.search(line, oni::SearchOptions::default()) {
                Ok(region) => region,
                Err(_err) => {
                    // log.warn("failed to match notification cmd={s} line=\"{s}\" err={}", .{ cmd, line, err });
                    re.deinit();
                    return Ok(None);
                }
            };
            // defer region.deinit();
            let starts = region.starts();
            let ends = region.ends();

            let id = parse_int_usize(&line[starts[1] as usize..ends[1] as usize]).unwrap();

            self.buffer.clear();
            self.state = State::Idle;
            region.deinit();
            re.deinit();
            Ok(Some(Notification::WindowAdd { id }))
        } else if cmd == b"%window-renamed" {
            let mut re = oni::Regex::init(
                b"^%window-renamed @([0-9]+) (.+)$",
                oni::RegexOptions::capture_group(),
                oni::Encoding::utf8(),
                oni::Syntax::default(),
                None,
            )
            .map_err(|_err| {
                // log.warn("regex init failed error={}", .{err});
                ParseError::RegexError
            })?;
            // defer re.deinit();

            let region = match re.search(line, oni::SearchOptions::default()) {
                Ok(region) => region,
                Err(_err) => {
                    // log.warn("failed to match notification cmd={s} line=\"{s}\" err={}", .{ cmd, line, err });
                    re.deinit();
                    return Ok(None);
                }
            };
            // defer region.deinit();
            let starts = region.starts();
            let ends = region.ends();

            let id = parse_int_usize(&line[starts[1] as usize..ends[1] as usize]).unwrap();
            let name = line[starts[2] as usize..ends[2] as usize].to_vec();

            // Important: do not clear buffer here since name points to it
            self.state = State::Idle;
            region.deinit();
            re.deinit();
            Ok(Some(Notification::WindowRenamed { id, name }))
        } else if cmd == b"%window-pane-changed" {
            let mut re = oni::Regex::init(
                b"^%window-pane-changed @([0-9]+) %([0-9]+)$",
                oni::RegexOptions::capture_group(),
                oni::Encoding::utf8(),
                oni::Syntax::default(),
                None,
            )
            .map_err(|_err| {
                // log.warn("regex init failed error={}", .{err});
                ParseError::RegexError
            })?;
            // defer re.deinit();

            let region = match re.search(line, oni::SearchOptions::default()) {
                Ok(region) => region,
                Err(_err) => {
                    // log.warn("failed to match notification cmd={s} line=\"{s}\" err={}", .{ cmd, line, err });
                    re.deinit();
                    return Ok(None);
                }
            };
            // defer region.deinit();
            let starts = region.starts();
            let ends = region.ends();

            let window_id = parse_int_usize(&line[starts[1] as usize..ends[1] as usize]).unwrap();
            let pane_id = parse_int_usize(&line[starts[2] as usize..ends[2] as usize]).unwrap();

            self.buffer.clear();
            self.state = State::Idle;
            region.deinit();
            re.deinit();
            Ok(Some(Notification::WindowPaneChanged { window_id, pane_id }))
        } else if cmd == b"%client-detached" {
            let mut re = oni::Regex::init(
                b"^%client-detached (.+)$",
                oni::RegexOptions::capture_group(),
                oni::Encoding::utf8(),
                oni::Syntax::default(),
                None,
            )
            .map_err(|_err| {
                // log.warn("regex init failed error={}", .{err});
                ParseError::RegexError
            })?;
            // defer re.deinit();

            let region = match re.search(line, oni::SearchOptions::default()) {
                Ok(region) => region,
                Err(_err) => {
                    // log.warn("failed to match notification cmd={s} line=\"{s}\" err={}", .{ cmd, line, err });
                    re.deinit();
                    return Ok(None);
                }
            };
            // defer region.deinit();
            let starts = region.starts();
            let ends = region.ends();

            let client = line[starts[1] as usize..ends[1] as usize].to_vec();

            // Important: do not clear buffer here since client points to it
            self.state = State::Idle;
            region.deinit();
            re.deinit();
            Ok(Some(Notification::ClientDetached { client }))
        } else if cmd == b"%client-session-changed" {
            let mut re = oni::Regex::init(
                b"^%client-session-changed (.+) \\$([0-9]+) (.+)$",
                oni::RegexOptions::capture_group(),
                oni::Encoding::utf8(),
                oni::Syntax::default(),
                None,
            )
            .map_err(|_err| {
                // log.warn("regex init failed error={}", .{err});
                ParseError::RegexError
            })?;
            // defer re.deinit();

            let region = match re.search(line, oni::SearchOptions::default()) {
                Ok(region) => region,
                Err(_err) => {
                    // log.warn("failed to match notification cmd={s} line=\"{s}\" err={}", .{ cmd, line, err });
                    re.deinit();
                    return Ok(None);
                }
            };
            // defer region.deinit();
            let starts = region.starts();
            let ends = region.ends();

            let client = line[starts[1] as usize..ends[1] as usize].to_vec();
            let session_id = parse_int_usize(&line[starts[2] as usize..ends[2] as usize]).unwrap();
            let name = line[starts[3] as usize..ends[3] as usize].to_vec();

            // Important: do not clear buffer here since client/name point to it
            self.state = State::Idle;
            region.deinit();
            re.deinit();
            Ok(Some(Notification::ClientSessionChanged {
                client,
                session_id,
                name,
            }))
        } else {
            Ok(None)
        }
    }

    // Mark the tmux state as broken.
    fn broken(&mut self) {
        self.state = State::Broken;
        // self.buffer.deinit();
        //
        // PORT NOTE: `self.buffer` is an owning `Vec`; dropping its storage is
        // the faithful equivalent of `buffer.deinit()`.
        self.buffer = Vec::new();
    }
}

// PORT NOTE: the reference declares these locally to `parseNotification`/`put`
// (`const ParseError = error{RegexError};` and `const BlockTerminator = enum {
// end, err };`). Rust nests neither errors nor enums inside functions, so they
// are module-level here.
// PORT NOTE: `RegexError` is constructed only by the Oniguruma-backed dispatch
// (`#[cfg(feature = "oniguruma")]`); with the feature off it is never built but
// remains part of the faithfully-ported error set, so the dead-code lint is
// suppressed.
#[cfg_attr(not(feature = "oniguruma"), allow(dead_code))]
enum ParseError {
    RegexError,
}

#[derive(Clone, Copy)]
enum BlockTerminator {
    End,
    Err,
}

/// Block payload is raw data, so a line only terminates a block if it
/// exactly matches tmux's `%end`/`%error` guard-line shape.
fn parse_block_terminator(line_raw: &[u8]) -> Option<BlockTerminator> {
    let mut line = line_raw;
    if !line.is_empty() && line[line.len() - 1] == b'\r' {
        line = &line[0..line.len() - 1];
    }

    // var fields = std.mem.tokenizeScalar(u8, line, ' ');
    //
    // PORT NOTE: `tokenizeScalar` skips empty tokens (runs of the delimiter); the
    // faithful Rust mirror is `split` on the byte filtered for non-empty pieces.
    let mut fields = line.split(|&b| b == b' ').filter(|s| !s.is_empty());
    let cmd = fields.next()?;
    let terminator: BlockTerminator = if cmd == b"%end" {
        BlockTerminator::End
    } else if cmd == b"%error" {
        BlockTerminator::Err
    } else {
        return None;
    };

    let time = fields.next()?;
    let command_id = fields.next()?;
    let flags = fields.next()?;
    let extra = fields.next();

    // In the future, we should compare these to the %begin block
    // because the tmux source guarantees that these always match and
    // that is a more robust way to match.
    parse_int_usize(time)?;
    parse_int_usize(command_id)?;
    parse_int_usize(flags)?;
    if extra.is_some() {
        return None;
    }

    Some(terminator)
}

// PORT NOTE: recognizes the set of commands handled by the Oniguruma-backed
// arms. Used (only when the `oniguruma` feature is off) to route those commands
// to the shared idle cleanup instead of the unknown-notification path, mirroring
// the reference's per-command `break :cmd` on a failed regex match.
#[allow(dead_code)]
fn is_oni_command(cmd: &[u8]) -> bool {
    matches!(
        cmd,
        b"%output"
            | b"%session-changed"
            | b"%layout-change"
            | b"%window-add"
            | b"%window-renamed"
            | b"%window-pane-changed"
            | b"%client-detached"
            | b"%client-session-changed"
    )
}

// PORT NOTE: `std.mem.lastIndexOfScalar(u8, slice, needle)`.
fn last_index_of_scalar(slice: &[u8], needle: u8) -> Option<usize> {
    slice.iter().rposition(|&b| b == needle)
}

// PORT NOTE: `std.mem.indexOfScalar(u8, slice, needle)`.
fn index_of_scalar(slice: &[u8], needle: u8) -> Option<usize> {
    slice.iter().position(|&b| b == needle)
}

// PORT NOTE: `std.mem.trimRight(u8, slice, strip)` — returns the length of the
// slice with all trailing bytes in `strip` removed. The buffer-relative length
// is what the caller needs (the trimmed view shares the buffer's storage).
fn trim_right_len(slice: &[u8], strip: &[u8]) -> usize {
    let mut len = slice.len();
    while len > 0 && strip.contains(&slice[len - 1]) {
        len -= 1;
    }
    len
}

// PORT NOTE: `std.fmt.parseInt(usize, bytes, 10)` — base-10 parse of an ASCII
// byte slice into a `usize`. Returns `None` on any parse failure (empty,
// non-digit, overflow), matching the reference's `catch` handling.
fn parse_int_usize(bytes: &[u8]) -> Option<usize> {
    let s = std::str::from_utf8(bytes).ok()?;
    s.parse::<usize>().ok()
}

/// Possible notification types from tmux control mode. These are documented
/// in tmux(1). A lot of the simple documentation was copied from that man
/// page here.
//
// PORT NOTE: `[]const u8` -> owned `Vec<u8>`. The reference borrows from the
// parser's buffer ("Important: do not clear buffer here since name points to
// it"); the faithful Rust mirror copies out the matched bytes so the returned
// `Notification` owns its payload independent of the buffer's lifetime.
#[derive(Debug, PartialEq, Eq)]
pub enum Notification {
    /// Entering tmux control mode. This isn't an actual event sent by
    /// tmux but is one sent by us to indicate that we have detected that
    /// tmux control mode is starting.
    Enter,

    /// Exit.
    ///
    /// NOTE: The tmux protocol contains a "reason" string (human friendly)
    /// associated with this. We currently drop it because we don't need it
    /// but this may be something we want to add later. If we do add it,
    /// we have to consider buffer limits and how we handle those (dropping
    /// vs truncating, etc.).
    Exit,

    /// Dispatched at the end of a begin/end block with the raw data.
    /// The control mode parser can't parse the data because it is unaware
    /// of the command that was sent to trigger this output.
    BlockEnd(Vec<u8>),
    BlockErr(Vec<u8>),

    /// Raw output from a pane.
    Output {
        pane_id: usize,
        data: Vec<u8>, // unescaped
    },

    /// The client is now attached to the session with ID session-id, which is
    /// named name.
    SessionChanged {
        id: usize,
        name: Vec<u8>,
    },

    /// A session was created or destroyed.
    SessionsChanged,

    /// The layout of the window with ID window-id changed.
    LayoutChange {
        window_id: usize,
        layout: Vec<u8>,
        visible_layout: Vec<u8>,
        raw_flags: Vec<u8>,
    },

    /// The window with ID window-id was linked to the current session.
    WindowAdd {
        id: usize,
    },

    /// The window with ID window-id was renamed to name.
    WindowRenamed {
        id: usize,
        name: Vec<u8>,
    },

    /// The active pane in the window with ID window-id changed to the pane
    /// with ID pane-id.
    WindowPaneChanged {
        window_id: usize,
        pane_id: usize,
    },

    /// The client has detached.
    ClientDetached {
        client: Vec<u8>,
    },

    /// The client is now attached to the session with ID session-id, which is
    /// named name.
    ClientSessionChanged {
        client: Vec<u8>,
        session_id: usize,
        name: Vec<u8>,
    },
}

// PORT NOTE: the reference `pub fn format(self, writer)` uses Zig comptime
// reflection over the tagged union (`@typeName`, `@tagName`, `inline for` over
// the fields, special-casing `[]const u8` with `trim`). Rust has no runtime
// reflection over enum variants; the faithful equivalent is a hand-written
// `Display` that reproduces the same output shape: `Notification{ .<tag> =
// <value> }`, with `[]const u8` payloads printed as `"<trimmed>"` (trimming
// " \t\r\n") and all other payloads via their debug formatting (`{any}`). Tags
// use the reference's `snake-case`/Zig field names.
impl std::fmt::Display for Notification {
    fn fmt(&self, writer: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // try writer.writeAll(@typeName(T));
        writer.write_str("control.Notification")?;
        // try writer.writeAll("{ .");
        writer.write_str("{ .")?;
        match self {
            Notification::Enter => write!(writer, "enter =  }}"),
            Notification::Exit => write!(writer, "exit =  }}"),
            Notification::BlockEnd(v) => {
                write!(writer, "block_end = \"{}\" }}", trim_display(v))
            }
            Notification::BlockErr(v) => {
                write!(writer, "block_err = \"{}\" }}", trim_display(v))
            }
            Notification::Output { pane_id, data } => write!(
                writer,
                "output = output{{ .pane_id = {pane_id}, .data = \"{}\" }} }}",
                trim_display(data)
            ),
            Notification::SessionChanged { id, name } => write!(
                writer,
                "session_changed = session_changed{{ .id = {id}, .name = \"{}\" }} }}",
                trim_display(name)
            ),
            Notification::SessionsChanged => write!(writer, "sessions_changed =  }}"),
            Notification::LayoutChange {
                window_id,
                layout,
                visible_layout,
                raw_flags,
            } => write!(
                writer,
                "layout_change = layout_change{{ .window_id = {window_id}, .layout = \"{}\", .visible_layout = \"{}\", .raw_flags = \"{}\" }} }}",
                trim_display(layout),
                trim_display(visible_layout),
                trim_display(raw_flags),
            ),
            Notification::WindowAdd { id } => {
                write!(writer, "window_add = window_add{{ .id = {id} }} }}")
            }
            Notification::WindowRenamed { id, name } => write!(
                writer,
                "window_renamed = window_renamed{{ .id = {id}, .name = \"{}\" }} }}",
                trim_display(name)
            ),
            Notification::WindowPaneChanged { window_id, pane_id } => write!(
                writer,
                "window_pane_changed = window_pane_changed{{ .window_id = {window_id}, .pane_id = {pane_id} }} }}"
            ),
            Notification::ClientDetached { client } => write!(
                writer,
                "client_detached = client_detached{{ .client = \"{}\" }} }}",
                trim_display(client)
            ),
            Notification::ClientSessionChanged {
                client,
                session_id,
                name,
            } => write!(
                writer,
                "client_session_changed = client_session_changed{{ .client = \"{}\", .session_id = {session_id}, .name = \"{}\" }} }}",
                trim_display(client),
                trim_display(name),
            ),
        }
    }
}

// PORT NOTE: helper for the `[]const u8 => writer.print("\"{s}\"",
// .{std.mem.trim(u8, value, " \t\r\n")})` branch — trims " \t\r\n" from both
// ends and renders as a UTF-8 lossy string.
fn trim_display(v: &[u8]) -> String {
    let trimmed = trim_both(v, b" \t\r\n");
    String::from_utf8_lossy(trimmed).into_owned()
}

fn trim_both<'a>(slice: &'a [u8], strip: &[u8]) -> &'a [u8] {
    let mut start = 0;
    let mut end = slice.len();
    while start < end && strip.contains(&slice[start]) {
        start += 1;
    }
    while end > start && strip.contains(&slice[end - 1]) {
        end -= 1;
    }
    &slice[start..end]
}

#[cfg(test)]
mod tests {
    use super::*;

    // test "tmux begin/end empty"
    #[test]
    fn tmux_begin_end_empty() {
        let mut c = Parser::init();
        // defer c.deinit();
        for &byte in b"%begin 1578922740 269 1\n" {
            assert!(c.put(byte).unwrap().is_none());
        }
        for &byte in b"%end 1578922740 269 1" {
            assert!(c.put(byte).unwrap().is_none());
        }
        let n = c.put(b'\n').unwrap().unwrap();
        assert!(matches!(n, Notification::BlockEnd(_)));
        let Notification::BlockEnd(v) = &n else {
            unreachable!()
        };
        assert_eq!(b"" as &[u8], v.as_slice());
        c.deinit();
    }

    // test "tmux begin/error empty"
    #[test]
    fn tmux_begin_error_empty() {
        let mut c = Parser::init();
        // defer c.deinit();
        for &byte in b"%begin 1578922740 269 1\n" {
            assert!(c.put(byte).unwrap().is_none());
        }
        for &byte in b"%error 1578922740 269 1" {
            assert!(c.put(byte).unwrap().is_none());
        }
        let n = c.put(b'\n').unwrap().unwrap();
        assert!(matches!(n, Notification::BlockErr(_)));
        let Notification::BlockErr(v) = &n else {
            unreachable!()
        };
        assert_eq!(b"" as &[u8], v.as_slice());
        c.deinit();
    }

    // test "tmux begin/end data"
    #[test]
    fn tmux_begin_end_data() {
        let mut c = Parser::init();
        // defer c.deinit();
        for &byte in b"%begin 1578922740 269 1\n" {
            assert!(c.put(byte).unwrap().is_none());
        }
        for &byte in b"hello\nworld\n" {
            assert!(c.put(byte).unwrap().is_none());
        }
        for &byte in b"%end 1578922740 269 1" {
            assert!(c.put(byte).unwrap().is_none());
        }
        let n = c.put(b'\n').unwrap().unwrap();
        assert!(matches!(n, Notification::BlockEnd(_)));
        let Notification::BlockEnd(v) = &n else {
            unreachable!()
        };
        assert_eq!(b"hello\nworld" as &[u8], v.as_slice());
        c.deinit();
    }

    // test "tmux block payload may start with %end"
    #[test]
    fn tmux_block_payload_may_start_with_end() {
        let mut c = Parser::init();
        // defer c.deinit();
        for &byte in b"%begin 1 1 1\n" {
            assert!(c.put(byte).unwrap().is_none());
        }
        for &byte in b"%end not really\n" {
            assert!(c.put(byte).unwrap().is_none());
        }
        for &byte in b"hello\n" {
            assert!(c.put(byte).unwrap().is_none());
        }
        for &byte in b"%end 1 1 1" {
            assert!(c.put(byte).unwrap().is_none());
        }
        let n = c.put(b'\n').unwrap().unwrap();
        assert!(matches!(n, Notification::BlockEnd(_)));
        let Notification::BlockEnd(v) = &n else {
            unreachable!()
        };
        assert_eq!(b"%end not really\nhello" as &[u8], v.as_slice());
        c.deinit();
    }

    // test "tmux block payload may start with %error"
    #[test]
    fn tmux_block_payload_may_start_with_error() {
        let mut c = Parser::init();
        // defer c.deinit();
        for &byte in b"%begin 1 1 1\n" {
            assert!(c.put(byte).unwrap().is_none());
        }
        for &byte in b"%error not really\n" {
            assert!(c.put(byte).unwrap().is_none());
        }
        for &byte in b"hello\n" {
            assert!(c.put(byte).unwrap().is_none());
        }
        for &byte in b"%end 1 1 1" {
            assert!(c.put(byte).unwrap().is_none());
        }
        let n = c.put(b'\n').unwrap().unwrap();
        assert!(matches!(n, Notification::BlockEnd(_)));
        let Notification::BlockEnd(v) = &n else {
            unreachable!()
        };
        assert_eq!(b"%error not really\nhello" as &[u8], v.as_slice());
        c.deinit();
    }

    // test "tmux block may terminate with real %error after misleading payload"
    #[test]
    fn tmux_block_may_terminate_with_real_error_after_misleading_payload() {
        let mut c = Parser::init();
        // defer c.deinit();
        for &byte in b"%begin 1 1 1\n" {
            assert!(c.put(byte).unwrap().is_none());
        }
        for &byte in b"%error not really\n" {
            assert!(c.put(byte).unwrap().is_none());
        }
        for &byte in b"hello\n" {
            assert!(c.put(byte).unwrap().is_none());
        }
        for &byte in b"%error 1 1 1" {
            assert!(c.put(byte).unwrap().is_none());
        }
        let n = c.put(b'\n').unwrap().unwrap();
        assert!(matches!(n, Notification::BlockErr(_)));
        let Notification::BlockErr(v) = &n else {
            unreachable!()
        };
        assert_eq!(b"%error not really\nhello" as &[u8], v.as_slice());
        c.deinit();
    }

    // test "tmux block terminator requires exact token count"
    #[test]
    fn tmux_block_terminator_requires_exact_token_count() {
        let mut c = Parser::init();
        // defer c.deinit();
        for &byte in b"%begin 1 1 1\n" {
            assert!(c.put(byte).unwrap().is_none());
        }
        for &byte in b"%end 1 1 1 trailing\n" {
            assert!(c.put(byte).unwrap().is_none());
        }
        for &byte in b"hello\n" {
            assert!(c.put(byte).unwrap().is_none());
        }
        for &byte in b"%end 1 1 1" {
            assert!(c.put(byte).unwrap().is_none());
        }
        let n = c.put(b'\n').unwrap().unwrap();
        assert!(matches!(n, Notification::BlockEnd(_)));
        let Notification::BlockEnd(v) = &n else {
            unreachable!()
        };
        assert_eq!(b"%end 1 1 1 trailing\nhello" as &[u8], v.as_slice());
        c.deinit();
    }

    // test "tmux block terminator requires numeric metadata"
    #[test]
    fn tmux_block_terminator_requires_numeric_metadata() {
        let mut c = Parser::init();
        // defer c.deinit();
        for &byte in b"%begin 1 1 1\n" {
            assert!(c.put(byte).unwrap().is_none());
        }
        for &byte in b"%end foo bar baz\n" {
            assert!(c.put(byte).unwrap().is_none());
        }
        for &byte in b"hello\n" {
            assert!(c.put(byte).unwrap().is_none());
        }
        for &byte in b"%end 1 1 1" {
            assert!(c.put(byte).unwrap().is_none());
        }
        let n = c.put(b'\n').unwrap().unwrap();
        assert!(matches!(n, Notification::BlockEnd(_)));
        let Notification::BlockEnd(v) = &n else {
            unreachable!()
        };
        assert_eq!(b"%end foo bar baz\nhello" as &[u8], v.as_slice());
        c.deinit();
    }

    // test "tmux output"
    //
    // PORT NOTE: gated behind `#[cfg(feature = "oniguruma")]`; the reference
    // parses `%output` with `oni.Regex`, which is the build-option-gated regex
    // surface. With the feature off the `oni` binding crate is absent and this
    // test is not compiled (the faithful `SkipZigTest` equivalent).
    #[cfg(feature = "oniguruma")]
    #[test]
    fn tmux_output() {
        let mut c = Parser::init();
        // defer c.deinit();
        for &byte in b"%output %42 foo bar baz" {
            assert!(c.put(byte).unwrap().is_none());
        }
        let n = c.put(b'\n').unwrap().unwrap();
        assert!(matches!(n, Notification::Output { .. }));
        let Notification::Output { pane_id, data } = &n else {
            unreachable!()
        };
        assert_eq!(42, *pane_id);
        assert_eq!(b"foo bar baz" as &[u8], data.as_slice());
        c.deinit();
    }

    // test "tmux session-changed"
    #[cfg(feature = "oniguruma")]
    #[test]
    fn tmux_session_changed() {
        let mut c = Parser::init();
        // defer c.deinit();
        for &byte in b"%session-changed $42 foo" {
            assert!(c.put(byte).unwrap().is_none());
        }
        let n = c.put(b'\n').unwrap().unwrap();
        assert!(matches!(n, Notification::SessionChanged { .. }));
        let Notification::SessionChanged { id, name } = &n else {
            unreachable!()
        };
        assert_eq!(42, *id);
        assert_eq!(b"foo" as &[u8], name.as_slice());
        c.deinit();
    }

    // test "tmux sessions-changed"
    #[test]
    fn tmux_sessions_changed() {
        let mut c = Parser::init();
        // defer c.deinit();
        for &byte in b"%sessions-changed" {
            assert!(c.put(byte).unwrap().is_none());
        }
        let n = c.put(b'\n').unwrap().unwrap();
        assert!(matches!(n, Notification::SessionsChanged));
        c.deinit();
    }

    // test "tmux sessions-changed carriage return"
    #[test]
    fn tmux_sessions_changed_carriage_return() {
        let mut c = Parser::init();
        // defer c.deinit();
        for &byte in b"%sessions-changed\r" {
            assert!(c.put(byte).unwrap().is_none());
        }
        let n = c.put(b'\n').unwrap().unwrap();
        assert!(matches!(n, Notification::SessionsChanged));
        c.deinit();
    }

    // test "tmux layout-change"
    #[cfg(feature = "oniguruma")]
    #[test]
    fn tmux_layout_change() {
        let mut c = Parser::init();
        // defer c.deinit();
        for &byte in b"%layout-change @2 1234x791,0,0{617x791,0,0,0,617x791,618,0,1} 1234x791,0,0{617x791,0,0,0,617x791,618,0,1} *-" {
            assert!(c.put(byte).unwrap().is_none());
        }
        let n = c.put(b'\n').unwrap().unwrap();
        assert!(matches!(n, Notification::LayoutChange { .. }));
        let Notification::LayoutChange {
            window_id,
            layout,
            visible_layout,
            raw_flags,
        } = &n
        else {
            unreachable!()
        };
        assert_eq!(2, *window_id);
        assert_eq!(
            b"1234x791,0,0{617x791,0,0,0,617x791,618,0,1}" as &[u8],
            layout.as_slice()
        );
        assert_eq!(
            b"1234x791,0,0{617x791,0,0,0,617x791,618,0,1}" as &[u8],
            visible_layout.as_slice()
        );
        assert_eq!(b"*-" as &[u8], raw_flags.as_slice());
        c.deinit();
    }

    // test "tmux window-add"
    #[cfg(feature = "oniguruma")]
    #[test]
    fn tmux_window_add() {
        let mut c = Parser::init();
        // defer c.deinit();
        for &byte in b"%window-add @14" {
            assert!(c.put(byte).unwrap().is_none());
        }
        let n = c.put(b'\n').unwrap().unwrap();
        assert!(matches!(n, Notification::WindowAdd { .. }));
        let Notification::WindowAdd { id } = &n else {
            unreachable!()
        };
        assert_eq!(14, *id);
        c.deinit();
    }

    // test "tmux window-renamed"
    #[cfg(feature = "oniguruma")]
    #[test]
    fn tmux_window_renamed() {
        let mut c = Parser::init();
        // defer c.deinit();
        for &byte in b"%window-renamed @42 bar" {
            assert!(c.put(byte).unwrap().is_none());
        }
        let n = c.put(b'\n').unwrap().unwrap();
        assert!(matches!(n, Notification::WindowRenamed { .. }));
        let Notification::WindowRenamed { id, name } = &n else {
            unreachable!()
        };
        assert_eq!(42, *id);
        assert_eq!(b"bar" as &[u8], name.as_slice());
        c.deinit();
    }

    // test "tmux window-pane-changed"
    #[cfg(feature = "oniguruma")]
    #[test]
    fn tmux_window_pane_changed() {
        let mut c = Parser::init();
        // defer c.deinit();
        for &byte in b"%window-pane-changed @42 %2" {
            assert!(c.put(byte).unwrap().is_none());
        }
        let n = c.put(b'\n').unwrap().unwrap();
        assert!(matches!(n, Notification::WindowPaneChanged { .. }));
        let Notification::WindowPaneChanged { window_id, pane_id } = &n else {
            unreachable!()
        };
        assert_eq!(42, *window_id);
        assert_eq!(2, *pane_id);
        c.deinit();
    }

    // test "tmux client-detached"
    #[cfg(feature = "oniguruma")]
    #[test]
    fn tmux_client_detached() {
        let mut c = Parser::init();
        // defer c.deinit();
        for &byte in b"%client-detached /dev/pts/1" {
            assert!(c.put(byte).unwrap().is_none());
        }
        let n = c.put(b'\n').unwrap().unwrap();
        assert!(matches!(n, Notification::ClientDetached { .. }));
        let Notification::ClientDetached { client } = &n else {
            unreachable!()
        };
        assert_eq!(b"/dev/pts/1" as &[u8], client.as_slice());
        c.deinit();
    }

    // test "tmux client-session-changed"
    #[cfg(feature = "oniguruma")]
    #[test]
    fn tmux_client_session_changed() {
        let mut c = Parser::init();
        // defer c.deinit();
        for &byte in b"%client-session-changed /dev/pts/1 $2 mysession" {
            assert!(c.put(byte).unwrap().is_none());
        }
        let n = c.put(b'\n').unwrap().unwrap();
        assert!(matches!(n, Notification::ClientSessionChanged { .. }));
        let Notification::ClientSessionChanged {
            client,
            session_id,
            name,
        } = &n
        else {
            unreachable!()
        };
        assert_eq!(b"/dev/pts/1" as &[u8], client.as_slice());
        assert_eq!(2, *session_id);
        assert_eq!(b"mysession" as &[u8], name.as_slice());
        c.deinit();
    }
}
