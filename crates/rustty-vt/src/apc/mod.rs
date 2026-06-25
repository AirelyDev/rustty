//! rustty-vt apc
//!
//! PORTS: ghostty src/terminal/apc.zig
//!
//! PORT STATUS: done

// const std = @import("std");
// const build_options = @import("terminal_options");
// const Allocator = std.mem.Allocator;
// PORT NOTE: `Allocator` is the project allocator type, threaded through the
// `feed`/`end` API for call-site fidelity even though neither the kitty nor the
// glyph parser takes an allocator in their Rust ports (both buffer into owned
// `Vec`s). The kitty `complete`/`kitty_graphics` path still consumes an
// allocator at `end`, so the parameter is real there.
use allocator_api2::alloc::Global as Allocator;

// pub const glyph = @import("apc/glyph.zig");
// const kitty_gfx = @import("kitty/graphics.zig");
pub mod glyph;
use crate::kitty::graphics as kitty_gfx;
use crate::options::KITTY_GRAPHICS;

// const log = std.log.scoped(.terminal_apc);

/// APC command handler. This should be hooked into a terminal.Stream handler.
/// The start/feed/end functions are meant to be called from the terminal.Stream
/// apcStart, apcPut, and apcEnd functions, respectively.
pub struct Handler {
    // state: State = .inactive,
    state: State,

    /// Maximum bytes each APC protocol can buffer. This is to prevent
    /// malicious input from causing us to allocate too much memory.
    /// If you want to be lazy and set a single value for all protocols,
    /// use `.initFull`.
    // max_bytes: std.EnumMap(Protocol, usize) = .initFullWith(.{
    //     .kitty = Protocol.defaultMaxBytes(.kitty),
    //     .glyph = Protocol.defaultMaxBytes(.glyph),
    // }),
    // PORT NOTE: `std.EnumMap(Protocol, usize)` ports to a per-key array indexed
    // by `Protocol as usize` (matching the `ScreenSet` EnumMap ports). The map is
    // always full here (every protocol has a value), so each slot is a bare
    // `usize` rather than an `Option<usize>`; the reference `.get(p) orelse
    // defaultMaxBytes(p)` is therefore exact.
    max_bytes: [usize; Protocol::COUNT],

    /// Protocols recognized by this APC handler. When a protocol is absent,
    /// matching APC sequences are ignored so callers see the same behavior as
    /// an unsupported protocol: no command execution and no response.
    // enabled: std.EnumSet(Protocol) = .initFull(),
    // PORT NOTE: `std.EnumSet(Protocol)` ports to a per-key `[bool; COUNT]`
    // presence array indexed by `Protocol as usize`; `.initFull()` is all-true.
    enabled: [bool; Protocol::COUNT],
}

impl Default for Handler {
    fn default() -> Handler {
        Handler {
            state: State::Inactive,
            max_bytes: [
                Protocol::default_max_bytes(Protocol::Kitty),
                Protocol::default_max_bytes(Protocol::Glyph),
            ],
            enabled: [true; Protocol::COUNT],
        }
    }
}

impl Handler {
    // pub fn deinit(self: *Handler) void {
    //     self.state.deinit();
    // }
    pub fn deinit(&mut self) {
        self.state.deinit();
    }

    // pub fn start(self: *Handler) void {
    //     self.state.deinit();
    //     self.state = .{ .identify = .{} };
    // }
    pub fn start(&mut self) {
        self.state.deinit();
        self.state = State::Identify(Identify::default());
    }

    /// Enable or disable APC protocol recognition for future APC sequences.
    /// This does not affect any APC command already being parsed.
    // pub fn enable(self: *Handler, protocol: Protocol, enabled: bool) void {
    //     self.enabled.setPresent(protocol, enabled);
    // }
    pub fn enable(&mut self, protocol: Protocol, enabled: bool) {
        self.enabled[protocol as usize] = enabled;
    }

    // pub fn feed(self: *Handler, alloc: Allocator, byte: u8) void {
    // PORT NOTE: `alloc` is threaded for call-site fidelity; the Rust kitty and
    // glyph parsers do not take an allocator on `init`/`feed` (they buffer into
    // owned `Vec`s), so it is unused here.
    pub fn feed(&mut self, alloc: Allocator, byte: u8) {
        let _ = alloc;
        match &mut self.state {
            // .inactive => unreachable,
            State::Inactive => unreachable!(),

            // We're ignoring this APC command, likely because we don't
            // recognize it so there is no need to store the data in memory.
            // .ignore => return,
            State::Ignore => (),

            // We identify the APC command by the first byte.
            // .identify => |*id| id: {
            State::Identify(id) => {
                // PORT NOTE: the reference `break :id;` labeled-block early exits
                // are reproduced by short-circuiting through nested branches; the
                // tail (buffer-append) only runs when none of the prior branches
                // matched, mirroring the `break :id` control flow exactly.

                // Kitty graphics is detected immediately on the `G` byte,
                // since commands begin immediately after with no termination
                // character after the 'G'.
                // if (comptime build_options.kitty_graphics) {
                //     if (id.len == 0 and byte == 'G' and
                //         self.enabled.contains(.kitty))
                //     {
                //         self.state = .{ .kitty = .init(alloc,
                //             self.max_bytes.get(.kitty) orelse
                //                 Protocol.defaultMaxBytes(.kitty)) };
                //         break :id;
                //     }
                // }
                if KITTY_GRAPHICS
                    && id.len == 0
                    && byte == b'G'
                    && self.enabled[Protocol::Kitty as usize]
                {
                    self.state = State::Kitty(kitty_gfx::CommandParser::init(
                        self.max_bytes[Protocol::Kitty as usize],
                    ));
                    return;
                }

                // If we hit `;` then identify...
                // if (byte == ';') {
                //     const str = id.buf[0..id.len];
                //     if (std.mem.eql(u8, str, "25a1") and
                //         self.enabled.contains(.glyph))
                //     {
                //         self.state = .{ .glyph = .init(alloc,
                //             self.max_bytes.get(.glyph) orelse
                //                 Protocol.defaultMaxBytes(.glyph)) };
                //     } else {
                //         self.state = .ignore;
                //     }
                //     break :id;
                // }
                if byte == b';' {
                    let str = &id.buf[0..id.len as usize];
                    if str == b"25a1" && self.enabled[Protocol::Glyph as usize] {
                        self.state = State::Glyph(glyph::CommandParser::init(
                            self.max_bytes[Protocol::Glyph as usize],
                        ));
                    } else {
                        self.state = State::Ignore;
                    }

                    return;
                }

                // If we're out of space to buffer then we're done.
                // if (id.len >= id.buf.len) {
                //     self.state = .ignore;
                //     break :id;
                // }
                if id.len as usize >= id.buf.len() {
                    self.state = State::Ignore;
                    return;
                }

                // id.buf[id.len] = byte;
                // id.len += 1;
                id.buf[id.len as usize] = byte;
                id.len += 1;
            }

            // .kitty => |*p| if (comptime build_options.kitty_graphics) {
            //     p.feed(byte) catch |err| {
            //         log.warn("kitty graphics protocol error: {}", .{err});
            //         p.deinit();
            //         self.state = .ignore;
            //     };
            // } else unreachable,
            State::Kitty(p) => {
                if KITTY_GRAPHICS {
                    if p.feed(byte).is_err() {
                        // log.warn("kitty graphics protocol error: {}", .{err});
                        // p.deinit();
                        // PORT NOTE: the Rust kitty `Parser` has no `deinit` (it
                        // frees its `data`/`kv` on drop); reassigning `self.state`
                        // drops the parser, reproducing the reference cleanup.
                        self.state = State::Ignore;
                    }
                } else {
                    unreachable!();
                }
            }

            // .glyph => |*p| p.feed(byte) catch |err| {
            //     log.warn("glyph protocol error: {}", .{err});
            //     p.deinit();
            //     self.state = .ignore;
            // },
            State::Glyph(p) => {
                if p.feed(byte).is_err() {
                    // log.warn("glyph protocol error: {}", .{err});
                    p.deinit();
                    self.state = State::Ignore;
                }
            }
        }
    }

    // pub fn end(self: *Handler) ?Command {
    //     defer {
    //         self.state.deinit();
    //         self.state = .inactive;
    //     }
    //     return switch (self.state) { ... };
    // }
    // PORT NOTE: the reference `defer { self.state.deinit(); self.state =
    // .inactive; }` deinits and resets state after the `switch` produces its
    // value. We take the state out (`std::mem::replace` with `.inactive`) first —
    // this both resets `self.state` to `.inactive` and gives us an owned `State`
    // to consume; the per-arm `deinit()` calls on the moved-out parsers reproduce
    // the deferred `state.deinit()` (the moved-out value drops at scope end either
    // way, so the explicit `deinit()` is the no-op call-site fidelity hook).
    // `alloc` is threaded for fidelity (kitty `complete` consumes it via the
    // arena `child_allocator` in the reference; the Rust parser ignores it).
    pub fn end(&mut self, alloc: Allocator) -> Option<Command> {
        let _ = alloc;
        let state = core::mem::replace(&mut self.state, State::Inactive);
        match state {
            // .inactive => unreachable,
            State::Inactive => unreachable!(),
            // .ignore, .identify => null,
            State::Ignore | State::Identify(_) => None,
            // .kitty => |*p| kitty: {
            //     if (comptime !build_options.kitty_graphics) unreachable;
            //     const alloc = p.arena.child_allocator;
            //     const command = p.complete(alloc) catch |err| {
            //         log.warn("kitty graphics protocol error: {}", .{err});
            //         break :kitty null;
            //     };
            //     break :kitty .{ .kitty = command };
            // },
            State::Kitty(mut p) => {
                if !KITTY_GRAPHICS {
                    unreachable!();
                }

                // Use the same allocator that was used to create the parser.
                match p.complete() {
                    Ok(command) => Some(Command::Kitty(command)),
                    Err(_) => {
                        // log.warn("kitty graphics protocol error: {}", .{err});
                        None
                    }
                }
            }

            // .glyph => |*p| glyph_cmd: {
            //     const command = p.complete(p.alloc) catch |err| {
            //         log.warn("glyph protocol error: {}", .{err});
            //         break :glyph_cmd null;
            //     };
            //     break :glyph_cmd .{ .glyph = command };
            // },
            State::Glyph(mut p) => match p.complete() {
                Ok(command) => Some(Command::Glyph(command)),
                Err(_) => {
                    // log.warn("glyph protocol error: {}", .{err});
                    None
                }
            },
        }
    }
}

// pub const State = union(enum) { ... };
pub enum State {
    /// We're not in the middle of an APC command yet.
    // inactive,
    Inactive,

    /// We got an unrecognized APC sequence or the APC sequence we
    /// recognized became invalid. We're just dropping bytes.
    // ignore,
    Ignore,

    /// We're waiting to identify the APC sequence. The way this is done
    /// is pretty fluid depending on supported APC protocols, but for now
    /// our rule is:
    ///
    ///  * 'G' - immediate transition to Kitty graphics protocol
    ///  * Buffer up to `;` and the bytes before dictate the protocol.
    ///    If we overflow then we're immediately invalid because we don't
    ///    support anything longer than this.
    ///
    // identify: struct {
    //     len: u3 = 0,
    //     buf: [4]u8 = undefined,
    // },
    Identify(Identify),

    /// Kitty graphics protocol
    // kitty: if (build_options.kitty_graphics)
    //     kitty_gfx.CommandParser
    // else
    //     void,
    // PORT NOTE: the `if (build_options.kitty_graphics) ... else void` comptime
    // type select collapses to always carrying the parser (Rust has no comptime
    // type-of-variant select); the `!KITTY_GRAPHICS` arms keep their `unreachable`
    // guards from the reference so the protocol is dead on those targets.
    Kitty(kitty_gfx::CommandParser),

    /// Glyph protocol
    // glyph: glyph.CommandParser,
    Glyph(glyph::CommandParser),
}

impl State {
    // pub fn deinit(self: *State) void {
    //     switch (self.*) {
    //         .inactive, .ignore, .identify => {},
    //         .glyph => |*v| v.deinit(),
    //         .kitty => |*v| if (comptime build_options.kitty_graphics)
    //             v.deinit()
    //         else
    //             unreachable,
    //     }
    // }
    pub fn deinit(&mut self) {
        match self {
            State::Inactive | State::Ignore | State::Identify(_) => {}
            State::Glyph(v) => v.deinit(),
            State::Kitty(_v) => {
                if KITTY_GRAPHICS {
                    // v.deinit();
                    // PORT NOTE: the Rust kitty `Parser` frees on drop; no
                    // explicit `deinit` (see the feed-error note above).
                } else {
                    unreachable!();
                }
            }
        }
    }
}

/// The identify-phase buffer that accumulates the protocol prefix up to `;`.
// PORT NOTE: reference `identify: struct { len: u3 = 0, buf: [4]u8 = undefined }`.
// `len: u3` (0..=4 needed) ports to `u8`; `buf: [4]u8 = undefined` starts zeroed
// (only `buf[0..len]` is ever read, so the initial fill is unobservable).
#[derive(Default)]
pub struct Identify {
    len: u8,
    buf: [u8; 4],
}

/// Possible APC command types.
// pub const Protocol = enum { kitty, glyph, ... };
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Protocol {
    Kitty = 0,
    Glyph = 1,
}

impl Protocol {
    /// Number of protocols; sizes the per-key EnumMap/EnumSet storage.
    pub const COUNT: usize = 2;

    /// Returns the default maximum bytes for the given protocol.
    // pub fn defaultMaxBytes(self: Protocol) usize {
    //     return switch (self) {
    //         .kitty => 65 * 1024 * 1024,
    //         .glyph => 1 * 1024 * 1024,
    //     };
    // }
    pub fn default_max_bytes(self) -> usize {
        match self {
            // Kitty graphics payloads can be very large (e.g. full images
            // encoded as base64), so the default is set to 65 MiB.
            Protocol::Kitty => 65 * 1024 * 1024,
            // Glyph protocol messages carry single glyf outlines which
            // are small, but base64 encoding inflates them. 1 MiB is
            // generous for any single simple-glyph record.
            Protocol::Glyph => 1024 * 1024,
        }
    }
}

/// Possible APC commands.
// pub const Command = union(Protocol) {
//     kitty: if (build_options.kitty_graphics) kitty_gfx.Command else void,
//     glyph: glyph.Request,
//     ...
// };
pub enum Command {
    Kitty(kitty_gfx::Command),
    Glyph(glyph::Request),
}

impl Command {
    // pub fn deinit(self: *Command, alloc: Allocator) void {
    //     switch (self.*) {
    //         .kitty => |*v| if (comptime build_options.kitty_graphics)
    //             v.deinit(alloc)
    //         else
    //             unreachable,
    //         .glyph => |*v| v.deinit(alloc),
    //     }
    // }
    // PORT NOTE: both the kitty `Command` and the glyph `Request` own their bytes
    // via `Vec`s and free them on drop, so the per-arm `deinit(alloc)` calls are
    // no-ops retained for call-site fidelity (`alloc` is unused).
    pub fn deinit(&mut self, alloc: Allocator) {
        let _ = alloc;
        match self {
            Command::Kitty(_v) => {
                if !KITTY_GRAPHICS {
                    unreachable!();
                }
                // v.deinit(alloc) — Rust drops the owned data; no-op.
            }
            // v.deinit(alloc) — `Request::deinit` frees on drop; no-op.
            Command::Glyph(v) => v.deinit(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use allocator_api2::alloc::Global;

    // test "unknown APC command"
    #[test]
    fn unknown_apc_command() {
        let alloc = Global;

        let mut h: Handler = Handler::default();
        h.start();
        for c in *b"Xabcdef1234" {
            h.feed(alloc, c);
        }
        assert!(h.end(alloc).is_none());
    }

    // test "garbage Kitty command"
    #[test]
    fn garbage_kitty_command() {
        // if (comptime !build_options.kitty_graphics) return error.SkipZigTest;
        if !KITTY_GRAPHICS {
            return;
        }

        let alloc = Global;

        let mut h: Handler = Handler::default();
        h.start();
        for c in *b"Gabcdef1234" {
            h.feed(alloc, c);
        }
        assert!(h.end(alloc).is_none());
    }

    // test "Kitty command with overflow u32"
    #[test]
    fn kitty_command_with_overflow_u32() {
        // if (comptime !build_options.kitty_graphics) return error.SkipZigTest;
        if !KITTY_GRAPHICS {
            return;
        }

        let alloc = Global;

        let mut h: Handler = Handler::default();
        h.start();
        for c in *b"Ga=p,i=10000000000" {
            h.feed(alloc, c);
        }
        assert!(h.end(alloc).is_none());
    }

    // test "Kitty command with overflow i32"
    #[test]
    fn kitty_command_with_overflow_i32() {
        // if (comptime !build_options.kitty_graphics) return error.SkipZigTest;
        if !KITTY_GRAPHICS {
            return;
        }

        let alloc = Global;

        let mut h: Handler = Handler::default();
        h.start();
        for c in *b"Ga=p,i=1,z=-9999999999" {
            h.feed(alloc, c);
        }
        assert!(h.end(alloc).is_none());
    }

    // test "kitty feed error deinits parser"
    #[test]
    fn kitty_feed_error_deinits_parser() {
        // if (comptime !build_options.kitty_graphics) return error.SkipZigTest;
        if !KITTY_GRAPHICS {
            return;
        }

        let alloc = Global;

        // Feed a valid kitty command start to allocate parser state, then
        // trigger an error during feed via an integer overflow. The testing
        // allocator will detect leaks if deinit is not called.
        let mut h: Handler = Handler::default();
        // defer h.deinit();
        h.start();
        for c in *b"Ga=p,i=10000000000;" {
            h.feed(alloc, c);
        }
        // try testing.expect(h.state == .ignore);
        assert!(matches!(h.state, State::Ignore));
        h.deinit();
    }

    // test "kitty max bytes exceeded"
    #[test]
    fn kitty_max_bytes_exceeded() {
        // if (comptime !build_options.kitty_graphics) return error.SkipZigTest;
        if !KITTY_GRAPHICS {
            return;
        }

        let alloc = Global;

        // var h: Handler = .{ .max_bytes = .init(.{ .kitty = 4 }) };
        let mut h: Handler = Handler {
            max_bytes: {
                let mut mb = [0usize; Protocol::COUNT];
                mb[Protocol::Kitty as usize] = 4;
                mb
            },
            ..Handler::default()
        };
        // defer h.deinit();
        h.start();
        // 'G' identifies kitty, 'a=t;' moves to data state, then feed exceeds max_bytes.
        for c in *b"Ga=t;" {
            h.feed(alloc, c);
        }
        // try testing.expect(h.state != .ignore);
        assert!(!matches!(h.state, State::Ignore));
        for c in *b"abcd" {
            h.feed(alloc, c);
        }
        // try testing.expect(h.state != .ignore);
        assert!(!matches!(h.state, State::Ignore));
        // The 5th data byte exceeds the 4-byte limit.
        h.feed(alloc, b'e');
        // try testing.expect(h.state == .ignore);
        assert!(matches!(h.state, State::Ignore));
        h.deinit();
    }

    // test "valid Kitty command"
    #[test]
    fn valid_kitty_command() {
        // if (comptime !build_options.kitty_graphics) return error.SkipZigTest;
        if !KITTY_GRAPHICS {
            return;
        }

        let alloc = Global;

        let mut h: Handler = Handler::default();
        h.start();
        let input = *b"Gf=24,s=10,v=20,hello=world";
        for c in input {
            h.feed(alloc, c);
        }

        // var cmd = h.end().?;
        let mut cmd = h.end(alloc).unwrap();
        // defer cmd.deinit(alloc);
        // try testing.expect(cmd == .kitty);
        assert!(matches!(cmd, Command::Kitty(_)));
        cmd.deinit(alloc);
    }

    // test "identify with unrecognized command"
    #[test]
    fn identify_with_unrecognized_command() {
        let alloc = Global;

        let mut h: Handler = Handler::default();
        h.start();
        for c in *b"abcd;payload" {
            h.feed(alloc, c);
        }
        assert!(h.end(alloc).is_none());
    }

    // test "identify buffer overflow"
    #[test]
    fn identify_buffer_overflow() {
        let alloc = Global;

        let mut h: Handler = Handler::default();
        h.start();
        for c in *b"abcde;payload" {
            h.feed(alloc, c);
        }
        assert!(h.end(alloc).is_none());
    }

    // test "identify with no input"
    #[test]
    fn identify_with_no_input() {
        let alloc = Global;

        let mut h: Handler = Handler::default();
        h.start();
        assert!(h.end(alloc).is_none());
    }

    // test "identify with unknown partial input"
    #[test]
    fn identify_with_unknown_partial_input() {
        let alloc = Global;

        let mut h: Handler = Handler::default();
        h.start();
        for c in *b"25a" {
            h.feed(alloc, c);
        }
        assert!(h.end(alloc).is_none());
    }

    // test "garbage glyph command"
    #[test]
    fn garbage_glyph_command() {
        let alloc = Global;

        let mut h: Handler = Handler::default();
        h.start();
        for c in *b"25a1;X" {
            h.feed(alloc, c);
        }

        assert!(h.end(alloc).is_none());
    }

    // test "valid glyph command"
    #[test]
    fn valid_glyph_command() {
        let alloc = Global;

        let mut h: Handler = Handler::default();
        h.start();
        for c in *b"25a1;q;cp=E0A0" {
            h.feed(alloc, c);
        }

        // var cmd = h.end().?;
        let mut cmd = h.end(alloc).unwrap();
        // defer cmd.deinit(alloc);
        // try testing.expect(cmd == .glyph);
        assert!(matches!(cmd, Command::Glyph(_)));
        // try testing.expect(cmd.glyph == .query);
        assert!(matches!(cmd, Command::Glyph(glyph::Request::Query(_))));
        cmd.deinit(alloc);
    }

    // test "disabled glyph command is ignored"
    #[test]
    fn disabled_glyph_command_is_ignored() {
        let alloc = Global;

        let mut h: Handler = Handler::default();
        h.enable(Protocol::Glyph, false);
        h.start();
        for c in *b"25a1;q;cp=e0a0" {
            h.feed(alloc, c);
        }
        assert!(h.end(alloc).is_none());
    }
}
