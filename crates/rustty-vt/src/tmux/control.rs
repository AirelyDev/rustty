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
//! "oniguruma")]`, OFF by default — exactly as the `StringMap` port does. With
//! the feature off those arms fall through to the "unknown notification" path and
//! the 8 tests are not compiled; with `--features oniguruma` (the G-ONIG lane,
//! `.port/tools/gates/check_oniguruma_lane.sh`) they are. The remaining 10 tests
//! (begin/end blocks + `%sessions-changed`) build and run by default.
//!
//! CORRECTION (task-18, closes S208-F3). The text here used to say "The `oni`
//! binding crate does not exist yet, so the gated code is NOT compiled in the
//! default build … until the binding crate lands". The crate DOES exist —
//! `crates/rustty-oniguruma`, wired as `rustty-vt`'s optional `oniguruma`
//! dependency (`Cargo.toml:113,221`). Note what did NOT change: the feature is
//! still default-OFF, so "not compiled in the default build" was and remains
//! true. Only the REASON was stale, which is the kind of half-false note that
//! survives a reading — the conclusion still looked right. Whether to make the
//! feature default-on is a separate question from this row and is not decided
//! here; the reference compiles `oni` unconditionally — that divergence is now
//! FILED as `NEW-FINDINGS.md` **N34** (eight notification arms, `%output`
//! among them, dead in the default build). Closing S208-F3 on this prose fix
//! did NOT retire it. If you are here because the tmux `%output` arm is not
//! firing, N34 is your row, not this note.

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
// PORT NOTE (task-14e-2): the note that stood here said Rustty had "no logging
// facade wired into this crate", so the scoped logger and its `log.warn` calls
// were ported as commented-out lines. That has not been true since task 14b —
// `rustty-vt` carries `log = { workspace = true }`. The reference's scope tag is
// carried through the facade as `target: LOG_SCOPE` (DECISION-logging.md §6r4).
const LOG_SCOPE: &str = "terminal_tmux";

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

    // PORT NOTE: `dcs.zig`'s tmux hook constructs the parser as a struct
    // literal overriding the field defaults:
    //   .{ .max_bytes = self.max_bytes,
    //      .buffer = try .initCapacity(alloc, 128) }
    // `Parser`'s fields are private to this module, so the DCS handler uses
    // this constructor mirror. This Parser's buffer is an owning
    // global-allocator `Vec` (see the `buffer` field PORT NOTE above), so the
    // allocator does not thread through — but the `try` DOES: the reference's
    // `initCapacity` is a fallible allocation whose `error.OutOfMemory`
    // propagates out of `dcs.hook`, so we reserve with `try_reserve_exact`
    // (like `dcs::Allocating::init_capacity`) instead of the aborting
    // `Vec::with_capacity`, and return `OutOfMemory` to the DCS hook site.
    pub fn init_options(max_bytes: usize, capacity: usize) -> Result<Parser, OutOfMemory> {
        let mut buffer: Vec<u8> = Vec::new();
        buffer
            .try_reserve_exact(capacity)
            .map_err(|_| OutOfMemory)?;
        Ok(Parser {
            state: State::Idle,
            buffer,
            max_bytes,
        })
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
                                log::warn!(
                                    target: LOG_SCOPE,
                                    "tmux control mode error={}",
                                    String::from_utf8_lossy(&self.buffer[0..output_len]),
                                );
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
                log::warn!(
                    target: LOG_SCOPE,
                    "failed to match notification cmd={} line=\"{}\"",
                    String::from_utf8_lossy(cmd),
                    String::from_utf8_lossy(line),
                );
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
            // PORT NOTE: this arm is the reference's `break :cmd` for the
            // Oniguruma-parsed commands, and it is LIVE IN BOTH BUILDS.
            //
            //  - With `oniguruma` OFF the commands are recognized here (so they
            //    are NOT misreported as "unknown notification" spam) and fall
            //    through to the shared idle cleanup.
            //  - With `oniguruma` ON `parse_notification_oni` returns `Ok(None)`
            //    exactly when its `re.search` FAILED, which is the reference's
            //    `catch |err| { log.warn(...); break :cmd; }` (control.zig:226-229
            //    and the seven sibling arms at :259, :299, :337, :367, :398, :433,
            //    :459). The reference falls out of the labelled `cmd:` block into
            //    the shared cleanup at control.zig:481-485
            //    (`buffer.clearRetainingCapacity(); state = .idle; return null`),
            //    so this arm must do the same. It previously panicked here, which
            //    turned a logged, recoverable no-op on a malformed `%output` /
            //    `%window-add` line from the PTY into an abort driven by
            //    untrusted input.
            self.buffer.clear();
            self.state = State::Idle;
            Ok(None)
        } else {
            // Unknown notification, log it and return to idle state.
            // log.warn("unknown tmux control mode notification={s}", .{cmd});
            log::warn!(
                target: LOG_SCOPE,
                "unknown tmux control mode notification={}",
                String::from_utf8_lossy(cmd),
            );

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
    // `is_oni_command` recognition + idle cleanup. The gated body is compiled
    // only with the feature (CORRECTION: this used to say the `oni` binding
    // crate did not exist yet; it is `rustty-oniguruma`).
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
        // PORT NOTE (N102): `oni.Regex.init` -> `onig_new`, which initializes
        // Oniguruma's process-global tables LAZILY on first use
        // (`onig_reg_init` -> `onig_initialize`), without synchronization. The
        // reference is only ever single-threaded there: the app runs
        // `oni.init(&.{oni.Encoding.utf8})` at startup (`global.zig:215`), and
        // `zig test` runs these tests serially. Parsers on several threads that
        // reach their first regex together (Rust's parallel test harness; any
        // multi-threaded host of rustty-vt that has not initialized Oniguruma)
        // raced inside `onig_initialize` and corrupted the C heap. So the
        // process-global init runs first, exactly once and thread-safely
        // (`ensure_init` is the same UTF-8-only init the app performs and the
        // lazy path would; after the app's own call it is a no-op). Its error
        // is ignored: `Regex::init` below still reports any init failure
        // through the reference's own `regex init failed` path.
        let _ = oni::testing::ensure_init();

        if cmd == b"%output" {
            let mut re = oni::Regex::init(
                b"^%output %([0-9]+) (.+)$",
                oni::RegexOptions {
                    capture_group: true,
                    ..Default::default()
                },
                oni::Encoding::utf8(),
                oni::Syntax::default(),
                None,
            )
            .map_err(|err| {
                // log.warn("regex init failed error={}", .{err});
                //
                // PORT NOTE (N11): Zig's `{}` on an error renders `error.<Name>`;
                // `{err:?}` renders the Rust error's own Debug.
                log::warn!(target: LOG_SCOPE, "regex init failed error={err:?}");
                ParseError::RegexError
            })?;
            // defer re.deinit();

            let mut region = match re.search(line, oni::SearchOptions::default()) {
                Ok(region) => region,
                Err(err) => {
                    // log.warn("failed to match notification cmd={s} line=\"{s}\" err={}", .{ cmd, line, err });
                    log::warn!(
                        target: LOG_SCOPE,
                        "failed to match notification cmd={} line=\"{}\" err={err:?}",
                        String::from_utf8_lossy(cmd),
                        String::from_utf8_lossy(line),
                    );
                    // break :cmd;
                    re.deinit();
                    return Ok(None);
                }
            };
            // defer region.deinit();
            let starts = region.starts();
            let ends = region.ends();

            // `catch unreachable` in the reference: R13, see `parse_capture_usize`.
            let Some(id) =
                parse_capture_usize(cmd, line, &line[starts[1] as usize..ends[1] as usize])
            else {
                region.deinit();
                re.deinit();
                return Ok(None);
            };
            let data = line[starts[2] as usize..ends[2] as usize].to_vec();

            // Important: do not clear buffer here since name points to it
            self.state = State::Idle;
            region.deinit();
            re.deinit();
            Ok(Some(Notification::Output { pane_id: id, data }))
        } else if cmd == b"%session-changed" {
            let mut re = oni::Regex::init(
                b"^%session-changed \\$([0-9]+) (.+)$",
                oni::RegexOptions {
                    capture_group: true,
                    ..Default::default()
                },
                oni::Encoding::utf8(),
                oni::Syntax::default(),
                None,
            )
            .map_err(|err| {
                // log.warn("regex init failed error={}", .{err});
                //
                // PORT NOTE (N11): Zig's `{}` on an error renders `error.<Name>`;
                // `{err:?}` renders the Rust error's own Debug.
                log::warn!(target: LOG_SCOPE, "regex init failed error={err:?}");
                ParseError::RegexError
            })?;
            // defer re.deinit();

            let mut region = match re.search(line, oni::SearchOptions::default()) {
                Ok(region) => region,
                Err(err) => {
                    // log.warn("failed to match notification cmd={s} line=\"{s}\" err={}", .{ cmd, line, err });
                    log::warn!(
                        target: LOG_SCOPE,
                        "failed to match notification cmd={} line=\"{}\" err={err:?}",
                        String::from_utf8_lossy(cmd),
                        String::from_utf8_lossy(line),
                    );
                    re.deinit();
                    return Ok(None);
                }
            };
            // defer region.deinit();
            let starts = region.starts();
            let ends = region.ends();

            // `catch unreachable` in the reference: R13, see `parse_capture_usize`.
            let Some(id) =
                parse_capture_usize(cmd, line, &line[starts[1] as usize..ends[1] as usize])
            else {
                region.deinit();
                re.deinit();
                return Ok(None);
            };
            let name = line[starts[2] as usize..ends[2] as usize].to_vec();

            // Important: do not clear buffer here since name points to it
            self.state = State::Idle;
            region.deinit();
            re.deinit();
            Ok(Some(Notification::SessionChanged { id, name }))
        } else if cmd == b"%layout-change" {
            let mut re = oni::Regex::init(
                b"^%layout-change @([0-9]+) (.+) (.+) (.*)$",
                oni::RegexOptions {
                    capture_group: true,
                    ..Default::default()
                },
                oni::Encoding::utf8(),
                oni::Syntax::default(),
                None,
            )
            .map_err(|err| {
                // log.warn("regex init failed error={}", .{err});
                //
                // PORT NOTE (N11): Zig's `{}` on an error renders `error.<Name>`;
                // `{err:?}` renders the Rust error's own Debug.
                log::warn!(target: LOG_SCOPE, "regex init failed error={err:?}");
                ParseError::RegexError
            })?;
            // defer re.deinit();

            let mut region = match re.search(line, oni::SearchOptions::default()) {
                Ok(region) => region,
                Err(err) => {
                    // log.warn("failed to match notification cmd={s} line=\"{s}\" err={}", .{ cmd, line, err });
                    log::warn!(
                        target: LOG_SCOPE,
                        "failed to match notification cmd={} line=\"{}\" err={err:?}",
                        String::from_utf8_lossy(cmd),
                        String::from_utf8_lossy(line),
                    );
                    re.deinit();
                    return Ok(None);
                }
            };
            // defer region.deinit();
            let starts = region.starts();
            let ends = region.ends();

            // `catch unreachable` in the reference: R13, see `parse_capture_usize`.
            let Some(id) =
                parse_capture_usize(cmd, line, &line[starts[1] as usize..ends[1] as usize])
            else {
                region.deinit();
                re.deinit();
                return Ok(None);
            };
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
                oni::RegexOptions {
                    capture_group: true,
                    ..Default::default()
                },
                oni::Encoding::utf8(),
                oni::Syntax::default(),
                None,
            )
            .map_err(|err| {
                // log.warn("regex init failed error={}", .{err});
                //
                // PORT NOTE (N11): Zig's `{}` on an error renders `error.<Name>`;
                // `{err:?}` renders the Rust error's own Debug.
                log::warn!(target: LOG_SCOPE, "regex init failed error={err:?}");
                ParseError::RegexError
            })?;
            // defer re.deinit();

            let mut region = match re.search(line, oni::SearchOptions::default()) {
                Ok(region) => region,
                Err(err) => {
                    // log.warn("failed to match notification cmd={s} line=\"{s}\" err={}", .{ cmd, line, err });
                    log::warn!(
                        target: LOG_SCOPE,
                        "failed to match notification cmd={} line=\"{}\" err={err:?}",
                        String::from_utf8_lossy(cmd),
                        String::from_utf8_lossy(line),
                    );
                    re.deinit();
                    return Ok(None);
                }
            };
            // defer region.deinit();
            let starts = region.starts();
            let ends = region.ends();

            // `catch unreachable` in the reference: R13, see `parse_capture_usize`.
            let Some(id) =
                parse_capture_usize(cmd, line, &line[starts[1] as usize..ends[1] as usize])
            else {
                region.deinit();
                re.deinit();
                return Ok(None);
            };

            self.buffer.clear();
            self.state = State::Idle;
            region.deinit();
            re.deinit();
            Ok(Some(Notification::WindowAdd { id }))
        } else if cmd == b"%window-renamed" {
            let mut re = oni::Regex::init(
                b"^%window-renamed @([0-9]+) (.+)$",
                oni::RegexOptions {
                    capture_group: true,
                    ..Default::default()
                },
                oni::Encoding::utf8(),
                oni::Syntax::default(),
                None,
            )
            .map_err(|err| {
                // log.warn("regex init failed error={}", .{err});
                //
                // PORT NOTE (N11): Zig's `{}` on an error renders `error.<Name>`;
                // `{err:?}` renders the Rust error's own Debug.
                log::warn!(target: LOG_SCOPE, "regex init failed error={err:?}");
                ParseError::RegexError
            })?;
            // defer re.deinit();

            let mut region = match re.search(line, oni::SearchOptions::default()) {
                Ok(region) => region,
                Err(err) => {
                    // log.warn("failed to match notification cmd={s} line=\"{s}\" err={}", .{ cmd, line, err });
                    log::warn!(
                        target: LOG_SCOPE,
                        "failed to match notification cmd={} line=\"{}\" err={err:?}",
                        String::from_utf8_lossy(cmd),
                        String::from_utf8_lossy(line),
                    );
                    re.deinit();
                    return Ok(None);
                }
            };
            // defer region.deinit();
            let starts = region.starts();
            let ends = region.ends();

            // `catch unreachable` in the reference: R13, see `parse_capture_usize`.
            let Some(id) =
                parse_capture_usize(cmd, line, &line[starts[1] as usize..ends[1] as usize])
            else {
                region.deinit();
                re.deinit();
                return Ok(None);
            };
            let name = line[starts[2] as usize..ends[2] as usize].to_vec();

            // Important: do not clear buffer here since name points to it
            self.state = State::Idle;
            region.deinit();
            re.deinit();
            Ok(Some(Notification::WindowRenamed { id, name }))
        } else if cmd == b"%window-pane-changed" {
            let mut re = oni::Regex::init(
                b"^%window-pane-changed @([0-9]+) %([0-9]+)$",
                oni::RegexOptions {
                    capture_group: true,
                    ..Default::default()
                },
                oni::Encoding::utf8(),
                oni::Syntax::default(),
                None,
            )
            .map_err(|err| {
                // log.warn("regex init failed error={}", .{err});
                //
                // PORT NOTE (N11): Zig's `{}` on an error renders `error.<Name>`;
                // `{err:?}` renders the Rust error's own Debug.
                log::warn!(target: LOG_SCOPE, "regex init failed error={err:?}");
                ParseError::RegexError
            })?;
            // defer re.deinit();

            let mut region = match re.search(line, oni::SearchOptions::default()) {
                Ok(region) => region,
                Err(err) => {
                    // log.warn("failed to match notification cmd={s} line=\"{s}\" err={}", .{ cmd, line, err });
                    log::warn!(
                        target: LOG_SCOPE,
                        "failed to match notification cmd={} line=\"{}\" err={err:?}",
                        String::from_utf8_lossy(cmd),
                        String::from_utf8_lossy(line),
                    );
                    re.deinit();
                    return Ok(None);
                }
            };
            // defer region.deinit();
            let starts = region.starts();
            let ends = region.ends();

            // `catch unreachable` in the reference: R13, see `parse_capture_usize`.
            let Some(window_id) =
                parse_capture_usize(cmd, line, &line[starts[1] as usize..ends[1] as usize])
            else {
                region.deinit();
                re.deinit();
                return Ok(None);
            };
            // `catch unreachable` in the reference: R13, see `parse_capture_usize`.
            let Some(pane_id) =
                parse_capture_usize(cmd, line, &line[starts[2] as usize..ends[2] as usize])
            else {
                region.deinit();
                re.deinit();
                return Ok(None);
            };

            self.buffer.clear();
            self.state = State::Idle;
            region.deinit();
            re.deinit();
            Ok(Some(Notification::WindowPaneChanged { window_id, pane_id }))
        } else if cmd == b"%client-detached" {
            let mut re = oni::Regex::init(
                b"^%client-detached (.+)$",
                oni::RegexOptions {
                    capture_group: true,
                    ..Default::default()
                },
                oni::Encoding::utf8(),
                oni::Syntax::default(),
                None,
            )
            .map_err(|err| {
                // log.warn("regex init failed error={}", .{err});
                //
                // PORT NOTE (N11): Zig's `{}` on an error renders `error.<Name>`;
                // `{err:?}` renders the Rust error's own Debug.
                log::warn!(target: LOG_SCOPE, "regex init failed error={err:?}");
                ParseError::RegexError
            })?;
            // defer re.deinit();

            let mut region = match re.search(line, oni::SearchOptions::default()) {
                Ok(region) => region,
                Err(err) => {
                    // log.warn("failed to match notification cmd={s} line=\"{s}\" err={}", .{ cmd, line, err });
                    log::warn!(
                        target: LOG_SCOPE,
                        "failed to match notification cmd={} line=\"{}\" err={err:?}",
                        String::from_utf8_lossy(cmd),
                        String::from_utf8_lossy(line),
                    );
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
                oni::RegexOptions {
                    capture_group: true,
                    ..Default::default()
                },
                oni::Encoding::utf8(),
                oni::Syntax::default(),
                None,
            )
            .map_err(|err| {
                // log.warn("regex init failed error={}", .{err});
                //
                // PORT NOTE (N11): Zig's `{}` on an error renders `error.<Name>`;
                // `{err:?}` renders the Rust error's own Debug.
                log::warn!(target: LOG_SCOPE, "regex init failed error={err:?}");
                ParseError::RegexError
            })?;
            // defer re.deinit();

            let mut region = match re.search(line, oni::SearchOptions::default()) {
                Ok(region) => region,
                Err(err) => {
                    // log.warn("failed to match notification cmd={s} line=\"{s}\" err={}", .{ cmd, line, err });
                    log::warn!(
                        target: LOG_SCOPE,
                        "failed to match notification cmd={} line=\"{}\" err={err:?}",
                        String::from_utf8_lossy(cmd),
                        String::from_utf8_lossy(line),
                    );
                    re.deinit();
                    return Ok(None);
                }
            };
            // defer region.deinit();
            let starts = region.starts();
            let ends = region.ends();

            let client = line[starts[1] as usize..ends[1] as usize].to_vec();
            // `catch unreachable` in the reference: R13, see `parse_capture_usize`.
            let Some(session_id) =
                parse_capture_usize(cmd, line, &line[starts[2] as usize..ends[2] as usize])
            else {
                region.deinit();
                re.deinit();
                return Ok(None);
            };
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
    parse_int_usize(time).ok()?;
    parse_int_usize(command_id).ok()?;
    parse_int_usize(flags).ok()?;
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

// PORT NOTE: `std.fmt.parseInt(usize, bytes, 10)`, through the port of the std
// function in `crate::zig_parse_int`. The error is `std.fmt.ParseIntError`
// (`Overflow` / `InvalidCharacter`) so a caller that logs it renders the
// reference's error name.
// CORRECTION (parity N100): this used `str::parse::<usize>()` with a local
// copy of `ParseIntError`; that rejects the `_` digit separators and `-0` that
// `std.fmt.parseInt` accepts, which `parse_block_terminator`'s unconstrained
// fields can carry.
fn parse_int_usize(bytes: &[u8]) -> Result<usize, crate::zig_parse_int::ParseIntError> {
    crate::zig_parse_int::parse::<usize>(bytes, 10)
}

// PORT NOTE: reference defect (R13, UPSTREAM-DEFECTS.md UD7). Every id capture
// in the regex arms is an unbounded `([0-9]+)`, and the reference parses it with
// `std.fmt.parseInt(usize, ..) catch unreachable`
// (`control.zig:238,269,309,347,377,408,413,470`). Twenty or more digits
// overflow `usize`: a panic in ReleaseSafe, undefined behaviour in ReleaseFast.
// Anything written to the pty inside `ESC P 1000 p` reaches it. The port takes
// the arm's own match-failure path instead: the same `failed to match
// notification` warning (with the parse error as `err`), then the shared
// clear-buffer, back-to-idle cleanup, so the line yields no notification and
// the parser keeps going. Test:
// `an_overflowing_notification_id_warns_and_the_parser_continues`.
#[cfg(feature = "oniguruma")]
fn parse_capture_usize(cmd: &[u8], line: &[u8], digits: &[u8]) -> Option<usize> {
    match parse_int_usize(digits) {
        Ok(v) => Some(v),
        Err(err) => {
            log::warn!(
                target: LOG_SCOPE,
                "failed to match notification cmd={} line=\"{}\" err={err:?}",
                String::from_utf8_lossy(cmd),
                String::from_utf8_lossy(line),
            );
            None
        }
    }
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

// pub fn format(self: Notification, writer: *std.Io.Writer) !void {
//     const T = Notification;
//     const info = @typeInfo(T).@"union";
//
//     try writer.writeAll(@typeName(T));
//     if (info.tag_type) |TagType| {
//         try writer.writeAll("{ .");
//         try writer.writeAll(@tagName(@as(TagType, self)));
//         try writer.writeAll(" = ");
//
//         inline for (info.fields) |u_field| {
//             if (self == @field(TagType, u_field.name)) {
//                 const value = @field(self, u_field.name);
//                 switch (u_field.type) {
//                     []const u8 => try writer.print("\"{s}\"", .{std.mem.trim(u8, value, " \t\r\n")}),
//                     else => try writer.print("{any}", .{value}),
//                 }
//             }
//         }
//
//         try writer.writeAll(" }");
//     }
// }
//
// PORT NOTE: Rust has no comptime reflection over the union, so each arm
// writes what the reflection produces for that field type, as MEASURED from
// zig 0.16 (the strings are pinned by
// `notification_display_matches_the_reference_format`): `@typeName` is
// `terminal.tmux.control.Notification`; a `void` payload prints `void` under
// `{any}`; a top-level `[]const u8` payload takes the trim-and-quote arm; a
// struct payload prints under `{any}` as `.{ .field = value, ... }`, where a
// `usize` is decimal and a `[]const u8` member is its bytes in decimal
// (`{ 104, 105 }`, and `{  }` when empty). `{s}` writes raw bytes but a
// `fmt::Formatter` takes `str`, so the trimmed quoted payload goes through
// `from_utf8_lossy`: a byte sequence that is not UTF-8 prints U+FFFD here.
// CORRECTION (parity S208-F1): this used to print the type name as
// `control.Notification`, nothing for a `void` payload, and struct payloads
// as `<tag>{ .field = "<text>" }`.
impl std::fmt::Display for Notification {
    fn fmt(&self, writer: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // try writer.writeAll(@typeName(T));
        writer.write_str("terminal.tmux.control.Notification")?;
        // try writer.writeAll("{ .");
        writer.write_str("{ .")?;
        // try writer.writeAll(@tagName(@as(TagType, self)));
        writer.write_str(self.tag_name())?;
        // try writer.writeAll(" = ");
        writer.write_str(" = ")?;
        match self {
            Notification::Enter | Notification::Exit | Notification::SessionsChanged => {
                writer.write_str("void")?
            }
            Notification::BlockEnd(v) | Notification::BlockErr(v) => {
                write!(writer, "\"{}\"", trim_display(v))?
            }
            Notification::Output { pane_id, data } => {
                write!(writer, ".{{ .pane_id = {pane_id}, .data = ")?;
                write_any_bytes(writer, data)?;
                writer.write_str(" }")?;
            }
            Notification::SessionChanged { id, name } => {
                write!(writer, ".{{ .id = {id}, .name = ")?;
                write_any_bytes(writer, name)?;
                writer.write_str(" }")?;
            }
            Notification::LayoutChange {
                window_id,
                layout,
                visible_layout,
                raw_flags,
            } => {
                write!(writer, ".{{ .window_id = {window_id}, .layout = ")?;
                write_any_bytes(writer, layout)?;
                writer.write_str(", .visible_layout = ")?;
                write_any_bytes(writer, visible_layout)?;
                writer.write_str(", .raw_flags = ")?;
                write_any_bytes(writer, raw_flags)?;
                writer.write_str(" }")?;
            }
            Notification::WindowAdd { id } => write!(writer, ".{{ .id = {id} }}")?,
            Notification::WindowRenamed { id, name } => {
                write!(writer, ".{{ .id = {id}, .name = ")?;
                write_any_bytes(writer, name)?;
                writer.write_str(" }")?;
            }
            Notification::WindowPaneChanged { window_id, pane_id } => write!(
                writer,
                ".{{ .window_id = {window_id}, .pane_id = {pane_id} }}"
            )?,
            Notification::ClientDetached { client } => {
                writer.write_str(".{ .client = ")?;
                write_any_bytes(writer, client)?;
                writer.write_str(" }")?;
            }
            Notification::ClientSessionChanged {
                client,
                session_id,
                name,
            } => {
                writer.write_str(".{ .client = ")?;
                write_any_bytes(writer, client)?;
                write!(writer, ", .session_id = {session_id}, .name = ")?;
                write_any_bytes(writer, name)?;
                writer.write_str(" }")?;
            }
        }
        // try writer.writeAll(" }");
        writer.write_str(" }")
    }
}

impl Notification {
    /// `@tagName` of the active tag.
    fn tag_name(&self) -> &'static str {
        match self {
            Notification::Enter => "enter",
            Notification::Exit => "exit",
            Notification::BlockEnd(_) => "block_end",
            Notification::BlockErr(_) => "block_err",
            Notification::Output { .. } => "output",
            Notification::SessionChanged { .. } => "session_changed",
            Notification::SessionsChanged => "sessions_changed",
            Notification::LayoutChange { .. } => "layout_change",
            Notification::WindowAdd { .. } => "window_add",
            Notification::WindowRenamed { .. } => "window_renamed",
            Notification::WindowPaneChanged { .. } => "window_pane_changed",
            Notification::ClientDetached { .. } => "client_detached",
            Notification::ClientSessionChanged { .. } => "client_session_changed",
        }
    }
}

/// `{any}` over a `[]const u8`: `{ ` + the bytes in decimal joined by `, ` +
/// ` }`.
fn write_any_bytes(writer: &mut std::fmt::Formatter<'_>, bytes: &[u8]) -> std::fmt::Result {
    writer.write_str("{ ")?;
    for (i, b) in bytes.iter().enumerate() {
        if i > 0 {
            writer.write_str(", ")?;
        }
        write!(writer, "{b}")?;
    }
    writer.write_str(" }")
}

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

    // Parity S208-F1 (no reference test): `Notification.format`'s output,
    // each string MEASURED from zig 0.16 over a verbatim copy of the union
    // (probe at the matching `terminal/tmux/control.zig` path so `@typeName`
    // agrees). Payload-less tags print `void`; top-level `[]const u8`
    // payloads are trimmed and quoted; struct payloads go through `{any}`,
    // which prints a `[]const u8` member as its bytes in decimal.
    #[test]
    fn notification_display_matches_the_reference_format() {
        let cases: Vec<(Notification, &str)> = vec![
            (Notification::Enter, "terminal.tmux.control.Notification{ .enter = void }"),
            (Notification::Exit, "terminal.tmux.control.Notification{ .exit = void }"),
            (
                Notification::BlockEnd(b" hello\n".to_vec()),
                "terminal.tmux.control.Notification{ .block_end = \"hello\" }",
            ),
            (
                Notification::BlockErr(b"err".to_vec()),
                "terminal.tmux.control.Notification{ .block_err = \"err\" }",
            ),
            (
                Notification::Output { pane_id: 42, data: b"hi\x1b".to_vec() },
                "terminal.tmux.control.Notification{ .output = .{ .pane_id = 42, .data = { 104, 105, 27 } } }",
            ),
            (
                Notification::SessionChanged { id: 3, name: b"main".to_vec() },
                "terminal.tmux.control.Notification{ .session_changed = .{ .id = 3, .name = { 109, 97, 105, 110 } } }",
            ),
            (
                Notification::SessionsChanged,
                "terminal.tmux.control.Notification{ .sessions_changed = void }",
            ),
            (
                Notification::LayoutChange {
                    window_id: 2,
                    layout: b"b25d,80x24,0,0,0".to_vec(),
                    visible_layout: b"v".to_vec(),
                    raw_flags: b"*".to_vec(),
                },
                "terminal.tmux.control.Notification{ .layout_change = .{ .window_id = 2, .layout = { 98, 50, 53, 100, 44, 56, 48, 120, 50, 52, 44, 48, 44, 48, 44, 48 }, .visible_layout = { 118 }, .raw_flags = { 42 } } }",
            ),
            (
                Notification::WindowAdd { id: 7 },
                "terminal.tmux.control.Notification{ .window_add = .{ .id = 7 } }",
            ),
            (
                Notification::WindowRenamed { id: 7, name: b"vim".to_vec() },
                "terminal.tmux.control.Notification{ .window_renamed = .{ .id = 7, .name = { 118, 105, 109 } } }",
            ),
            (
                Notification::WindowPaneChanged { window_id: 1, pane_id: 5 },
                "terminal.tmux.control.Notification{ .window_pane_changed = .{ .window_id = 1, .pane_id = 5 } }",
            ),
            (
                Notification::ClientDetached { client: b"/dev/ttys001".to_vec() },
                "terminal.tmux.control.Notification{ .client_detached = .{ .client = { 47, 100, 101, 118, 47, 116, 116, 121, 115, 48, 48, 49 } } }",
            ),
            (
                Notification::ClientSessionChanged {
                    client: b"c".to_vec(),
                    session_id: 9,
                    name: b"s".to_vec(),
                },
                "terminal.tmux.control.Notification{ .client_session_changed = .{ .client = { 99 }, .session_id = 9, .name = { 115 } } }",
            ),
            // `{any}` over an EMPTY slice is `{  }`, two spaces.
            (
                Notification::Output { pane_id: 0, data: Vec::new() },
                "terminal.tmux.control.Notification{ .output = .{ .pane_id = 0, .data = {  } } }",
            ),
        ];
        for (n, want) in cases {
            assert_eq!(want, format!("{n}"));
        }
    }

    // Parity N100 (no reference test): the guard line's three numbers are
    // checked with `std.fmt.parseInt(usize, .., 10)`, which accepts `_` digit
    // separators and `-0`.
    #[test]
    fn block_terminator_numbers_use_zig_parse_int() {
        assert!(matches!(
            parse_block_terminator(b"%end 1_578 2_69 -0"),
            Some(BlockTerminator::End)
        ));
        assert!(parse_block_terminator(b"%end 1578_ 269 1").is_none());
        assert!(parse_block_terminator(b"%end 1578 269 +").is_none());
    }

    // PORT NOTE: no reference test; this pins the fallible-allocation contract
    // of `init_options`, the mirror of the reference's
    // `.buffer = try .initCapacity(alloc, 128)` in `dcs.zig`'s tmux hook. An
    // aborting `Vec::with_capacity` would kill the process instead of
    // returning `error.OutOfMemory` to `dcs.hook`.
    #[test]
    fn tmux_init_options_reports_allocation_failure() {
        // The nominal capacity succeeds and the options are applied.
        let mut p = Parser::init_options(4096, 128).unwrap();
        assert_eq!(4096, p.max_bytes);
        assert!(p.buffer.capacity() >= 128);
        p.deinit();

        // A capacity that cannot be satisfied is reported, not aborted.
        assert_eq!(
            Err(OutOfMemory),
            Parser::init_options(4096, usize::MAX / 2).map(|_| ())
        );
    }

    // ---- task-14e-2: the restored `tmux/control.zig` diagnostics ------------
    //
    // `assert_eq!` on the FULL captured record vector, never `.any()`.

    fn tmux_warn(msg: &str) -> crate::log_capture::CapturedRecord {
        (
            log::Level::Warn,
            "terminal_tmux".to_string(),
            msg.to_string(),
        )
    }

    fn feed(input: &[u8]) -> (Vec<crate::log_capture::CapturedRecord>, usize) {
        let mut c = Parser::init();
        let cap = crate::log_capture::start();
        let mut notifications = 0usize;
        for &byte in input {
            if c.put(byte).unwrap().is_some() {
                notifications += 1;
            }
        }
        let messages = cap.messages();
        drop(cap);
        c.deinit();
        (messages, notifications)
    }

    /// `control.zig:478` — a notification this parser does not know.
    #[test]
    fn an_unknown_notification_warns_with_the_command() {
        let (messages, notifications) = feed(b"%bogus-notification arg\n");
        assert_eq!(0, notifications);
        assert_eq!(
            vec![tmux_warn(
                "unknown tmux control mode notification=%bogus-notification"
            )],
            messages
        );
    }

    /// `control.zig:277` — the one non-regex match failure: `%sessions-changed`
    /// takes no arguments, so a line carrying any is a match failure.
    #[test]
    fn a_sessions_changed_line_with_arguments_warns() {
        let (messages, notifications) = feed(b"%sessions-changed extra\n");
        assert_eq!(0, notifications);
        assert_eq!(
            vec![tmux_warn(
                "failed to match notification cmd=%sessions-changed line=\"%sessions-changed extra\""
            )],
            messages
        );
    }

    /// `control.zig:132` — a `%error`-terminated block.
    #[test]
    fn an_error_block_warns_with_its_output() {
        let (messages, notifications) =
            feed(b"%begin 1578922740 269 1\nbad thing\n%error 1578922740 269 1\n");
        // Positive reach: the block still produces its `BlockErr` notification.
        assert_eq!(1, notifications);
        assert_eq!(
            vec![tmux_warn("tmux control mode error=bad thing")],
            messages
        );
    }

    /// The absence half: a well-formed block emits NOTHING. Positive reach is
    /// the `BlockEnd` notification, which only exists if the block completed.
    #[test]
    fn a_well_formed_block_is_silent() {
        let (messages, notifications) = feed(b"%begin 1578922740 269 1\n%end 1578922740 269 1\n");
        assert_eq!(
            1, notifications,
            "the block never completed, so this test never reached the code \
             whose silence it asserts"
        );
        assert_eq!(Vec::<crate::log_capture::CapturedRecord>::new(), messages);
    }

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

    // REGRESSION (P3 unreachable audit): a malformed line whose command IS one of
    // the Oniguruma-parsed notifications but whose regex does NOT match must be a
    // logged, recoverable no-op that returns to `.idle` — the reference's
    // `re.search(...) catch |err| { log.warn(...); break :cmd; }` falling into the
    // shared cleanup at `control.zig:481-485`. This arm used to `unreachable!()`
    // under `--features oniguruma`, aborting on untrusted PTY input.
    //
    // Not gated on the feature: the behavior must be identical in both builds.
    #[test]
    fn tmux_oni_command_regex_mismatch_returns_to_idle() {
        // `%output` with no pane id / body: recognized command, no regex match.
        for line in [
            b"%output" as &[u8],
            b"%output nope",
            b"%window-add",
            b"%client-session-changed bogus",
        ] {
            let mut c = Parser::init();
            for &byte in line {
                assert!(c.put(byte).unwrap().is_none());
            }
            assert!(
                c.put(b'\n').unwrap().is_none(),
                "malformed {:?} must yield no notification",
                core::str::from_utf8(line).unwrap()
            );
            // Back to idle with an empty buffer, ready for the next line.
            assert!(c.buffer.is_empty());
            assert!(matches!(c.state, State::Idle));

            // And the parser is still usable afterwards.
            for &byte in b"%sessions-changed" {
                assert!(c.put(byte).unwrap().is_none());
            }
            assert!(matches!(
                c.put(b'\n').unwrap(),
                Some(Notification::SessionsChanged)
            ));
            c.deinit();
        }
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

    // N102 (no reference test): the first `oni::Regex::init` in a process
    // makes `onig_new` initialize Oniguruma's process-global tables, lazily and
    // without synchronization, so several threads parsing their first
    // notification at once corrupted the C heap (the intermittent SIGTRAP /
    // SIGABRT / SIGSEGV of this module's tests under the parallel harness).
    // The child process starts 16 threads that all parse their first
    // notification together; the parent runs it 40 times.
    #[cfg(feature = "oniguruma")]
    #[test]
    fn concurrent_first_notifications_do_not_race_oniguruma_init() {
        const CHILD: &str = "RUSTTY_VT_ONIG_INIT_RACE_CHILD";
        if std::env::var_os(CHILD).is_some() {
            let barrier = std::sync::Arc::new(std::sync::Barrier::new(16));
            let threads: Vec<_> = (0..16)
                .map(|_| {
                    let barrier = barrier.clone();
                    std::thread::spawn(move || {
                        let mut c = Parser::init();
                        barrier.wait();
                        for &byte in b"%client-session-changed /dev/pts/1 $2 mysession" {
                            assert!(c.put(byte).unwrap().is_none());
                        }
                        let n = c.put(b'\n').unwrap().unwrap();
                        assert!(matches!(n, Notification::ClientSessionChanged { .. }));
                        c.deinit();
                    })
                })
                .collect();
            for t in threads {
                t.join().unwrap();
            }
            println!("ONIG_INIT_RACE_CHILD_OK");
            return;
        }
        for run in 0..40 {
            let out = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "tmux::control::tests::concurrent_first_notifications_do_not_race_oniguruma_init",
                    "--nocapture",
                    "--test-threads=1",
                ])
                .env(CHILD, "1")
                .output()
                .unwrap();
            let stdout = String::from_utf8_lossy(&out.stdout);
            assert!(
                out.status.success() && stdout.contains("ONIG_INIT_RACE_CHILD_OK"),
                "child run {run} failed: {:?}\nstdout: {stdout}\nstderr: {}",
                out.status,
                String::from_utf8_lossy(&out.stderr)
            );
        }
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

    /// R13 (task R13-A, UPSTREAM-DEFECTS UD7). No reference test. Every numeric
    /// capture in the regex arms is an unbounded `([0-9]+)`, and the reference
    /// parses each with `parseInt(usize, ..) catch unreachable`
    /// (`control.zig:238,269,309,347,377,408,413,470`), so a 20-digit id from
    /// the pty panics (ReleaseSafe) or is undefined behaviour (ReleaseFast).
    /// The port takes the arms' own match-failure path instead. The bytes go
    /// through the VT stream and the DCS handler, and each case checks three
    /// things: the one warning, that no notification came out of the bad line,
    /// and that the parser still produces the valid `%output` sent after it.
    #[cfg(feature = "oniguruma")]
    #[test]
    fn an_overflowing_notification_id_warns_and_the_parser_continues() {
        const BIG: &str = "99999999999999999999";
        let cases: [(&str, String); 8] = [
            ("%output", format!("%output %{BIG} data")),
            ("%session-changed", format!("%session-changed ${BIG} name")),
            ("%layout-change", format!("%layout-change @{BIG} a b c")),
            ("%window-add", format!("%window-add @{BIG}")),
            ("%window-renamed", format!("%window-renamed @{BIG} name")),
            (
                "%window-pane-changed",
                format!("%window-pane-changed @{BIG} %2"),
            ),
            (
                "%window-pane-changed",
                format!("%window-pane-changed @2 %{BIG}"),
            ),
            (
                "%client-session-changed",
                format!("%client-session-changed /dev/pts/1 ${BIG} name"),
            ),
        ];
        for (cmd, line) in cases {
            let mut s = crate::tmux::test_host::stream();
            let cap = crate::log_capture::start();
            s.next_slice(b"\x1bP1000p");
            s.next_slice(line.as_bytes());
            s.next_slice(b"\n%output %1 still-parsing\n\x1b\\");
            let warnings: Vec<_> = cap
                .messages()
                .into_iter()
                .filter(|(level, target, _)| {
                    *level == log::Level::Warn && target == "terminal_tmux"
                })
                .collect();
            drop(cap);
            assert_eq!(
                vec![tmux_warn(&format!(
                    "failed to match notification cmd={cmd} line=\"{line}\" err=Overflow"
                ))],
                warnings,
                "{line}"
            );
            assert_eq!(
                vec!["enter", "output", "exit"],
                s.handler.notifications,
                "{line}: the bad line yields nothing, the next line still parses"
            );
            s.deinit();
        }
    }
}
