//! rustty-vt apc
//!
//! PORTS: ghostty src/terminal/apc.zig
//!
//! PORT STATUS: done

// const std = @import("std");
// const build_options = @import("terminal_options");
// const Allocator = std.mem.Allocator;
// PORT NOTE (parity tail S7-4): `Allocator` is the runtime allocator handle.
// The kitty and glyph parsers and the unknown-capture builder are created with
// the `alloc` passed to `feed` (the stream handler's is the terminal's
// `gpa()`) and buffer through it, fallibly; `end` completes each parser with
// its own allocator, as the reference does.
use crate::alloc::{Allocator, Bytes, CopyVecExt};

// pub const glyph = @import("apc/glyph.zig");
// const kitty_gfx = @import("kitty/graphics.zig");
pub mod glyph;
use crate::kitty::graphics as kitty_gfx;
use crate::options::{GLYPH_PROTOCOL, KITTY_GRAPHICS};

// const log = std.log.scoped(.terminal_apc);
const LOG_SCOPE: &str = "terminal_apc";

/// The identify arm's `comptime build_options.glyph_protocol` gate.
///
/// PORT NOTE (parity S149-F1): in every non-test build this is exactly the
/// const `options::GLYPH_PROTOCOL`. The `cfg(test)` arm is a test seam: this
/// crate's tests never build with the `glyph-protocol` feature off
/// (rustty-synthetic's dev-dependency unifies the default features back on,
/// and `check_vt_features.py` records that a trimmed build is a build
/// configuration, not a test one), so a thread-local lets
/// `glyph_protocol_build_option_off_ignores_glyph_command` take the gate's off
/// arm. It gates only the identify arm; with it off no `State::Glyph` is ever
/// entered, so the other arms' `GLYPH_PROTOCOL` guards are not reached.
#[inline(always)]
fn glyph_protocol_build_option() -> bool {
    #[cfg(test)]
    if tests::GLYPH_PROTOCOL_FORCED_OFF.with(|off| off.get()) {
        return false;
    }
    GLYPH_PROTOCOL
}

/// APC command handler. This should be hooked into a terminal.Stream handler.
/// The start/feed/end functions are meant to be called from the terminal.Stream
/// apcStart, apcPut, and apcEnd functions, respectively.
pub struct Handler {
    // state: State = .inactive,
    state: State,

    /// Maximum content bytes retained for unsupported APC identifiers. Zero
    /// drops and ignores unknown APC values.
    // unknown_max_bytes: usize = 0,
    pub unknown_max_bytes: usize,

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
    // PORT NOTE: `pub` to mirror the reference (a public struct field accessed
    // directly by the C wrapper, e.g. `apc_handler.max_bytes`).
    pub max_bytes: [usize; Protocol::COUNT],

    /// Protocols recognized by this APC handler. When a protocol is absent,
    /// matching APC sequences are ignored and are not reported as unknown.
    // enabled: std.EnumSet(Protocol) = .initFull(),
    // PORT NOTE: `std.EnumSet(Protocol)` ports to a per-key `[bool; COUNT]`
    // presence array indexed by `Protocol as usize`; `.initFull()` is all-true.
    // `pub` to mirror the reference (a public struct field; the C wrapper's tests
    // read `apc_handler.enabled.contains(.glyph)`).
    pub enabled: [bool; Protocol::COUNT],
}

impl Default for Handler {
    fn default() -> Handler {
        Handler {
            state: State::Inactive,
            unknown_max_bytes: 0,
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
    pub fn feed(&mut self, alloc: Allocator, byte: u8) {
        match &mut self.state {
            // .inactive => unreachable,
            State::Inactive => unreachable!(),

            // We're ignoring this APC command, likely because we don't
            // recognize it so there is no need to store the data in memory.
            // .ignore => return,
            State::Ignore => (),

            // Unsupported APC content is retained only when enabled.
            // .unknown => |*unknown| unknown.append(&.{byte}),
            State::Unknown(unknown) => unknown.append(&[byte]),

            // We identify the APC command by the first byte.
            // .identify => |*id| id: {
            State::Identify(id) => {
                // PORT NOTE: the reference `break :id;` labeled-block early exits
                // are reproduced by short-circuiting through nested branches; the
                // tail (buffer-append + prefix check) only runs when none of the
                // prior branches matched, mirroring the `break :id` control flow
                // exactly. The `beginUnknown` calls take a copied prefix (the
                // identify buffer is 4 bytes, `Copy`) because `id` borrows
                // `self.state`, the very union `beginUnknown` overwrites — the
                // same in-place hazard the reference's "Build the replacement
                // before overwriting identify" comment documents.

                // Kitty graphics is detected immediately on the `G` byte,
                // since commands begin immediately after with no termination
                // character after the 'G'.
                // if (id.len == 0 and byte == 'G') {
                //     if (comptime build_options.kitty_graphics) {
                //         if (self.enabled.contains(.kitty)) { ... }
                //         else { self.state = .ignore; }
                //     } else {
                //         self.state = .ignore;
                //     }
                //     break :id;
                // }
                if id.len == 0 && byte == b'G' {
                    if KITTY_GRAPHICS {
                        if self.enabled[Protocol::Kitty as usize] {
                            self.state = State::Kitty(kitty_gfx::CommandParser::init(
                                alloc,
                                self.max_bytes[Protocol::Kitty as usize],
                            ));
                        } else {
                            self.state = State::Ignore;
                        }
                    } else {
                        self.state = State::Ignore;
                    }
                    return;
                }

                // If we hit `;` then identify...
                // if (byte == ';') {
                //     const str = id.buf[0..id.len];
                //     if (std.mem.eql(u8, str, glyph.identifier)) {
                //         if (comptime build_options.glyph_protocol) {
                //             if (self.enabled.contains(.glyph)) { ... }
                //             else { self.state = .ignore; }
                //         } else {
                //             self.state = .ignore;
                //         }
                //     } else {
                //         self.beginUnknown(alloc, str, &.{byte});
                //     }
                //     break :id;
                // }
                if byte == b';' {
                    let prefix_buf = id.buf;
                    let prefix_len = id.len as usize;
                    let str = &prefix_buf[0..prefix_len];
                    if str == glyph::IDENTIFIER {
                        // CORRECTION (parity S149-F1): the build-option gate
                        // was missing here and in the glyph arms below.
                        if glyph_protocol_build_option() {
                            if self.enabled[Protocol::Glyph as usize] {
                                self.state = State::Glyph(glyph::CommandParser::init(
                                    alloc,
                                    self.max_bytes[Protocol::Glyph as usize],
                                ));
                            } else {
                                self.state = State::Ignore;
                            }
                        } else {
                            self.state = State::Ignore;
                        }
                    } else {
                        self.begin_unknown(alloc, str, &[byte]);
                    }

                    return;
                }

                // If we're out of identification space, the identifier is
                // unsupported. Preserve the buffered prefix before replacing
                // the identify union state.
                // if (id.len >= id.buf.len) {
                //     self.beginUnknown(alloc, id.buf[0..id.len], &.{byte});
                //     break :id;
                // }
                if id.len as usize >= id.buf.len() {
                    let prefix_buf = id.buf;
                    let prefix_len = id.len as usize;
                    self.begin_unknown(alloc, &prefix_buf[0..prefix_len], &[byte]);
                    return;
                }

                // const expected_idx: usize = id.len;
                // id.buf[id.len] = byte;
                // id.len += 1;
                let expected_idx: usize = id.len as usize;
                id.buf[id.len as usize] = byte;
                id.len += 1;

                // Once the buffered input is no longer a prefix of a known
                // protocol, it is an unsupported identifier.
                // if (self.unknown_max_bytes > 0 and
                //     byte != glyph.identifier[expected_idx])
                // {
                //     self.beginUnknown(alloc, id.buf[0..id.len], &.{});
                // }
                if self.unknown_max_bytes > 0 && byte != glyph::IDENTIFIER[expected_idx] {
                    let prefix_buf = id.buf;
                    let prefix_len = id.len as usize;
                    self.begin_unknown(alloc, &prefix_buf[0..prefix_len], &[]);
                }
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
                    if let Err(err) = p.feed(byte) {
                        log::warn!(target: LOG_SCOPE, "kitty graphics protocol error: {err:?}");
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

            // .glyph => |*p| if (comptime build_options.glyph_protocol) {
            //     p.feed(byte) catch |err| {
            //         log.warn("glyph protocol error: {}", .{err});
            //         p.deinit();
            //         self.state = .ignore;
            //     };
            // } else unreachable,
            State::Glyph(p) => {
                if GLYPH_PROTOCOL {
                    if let Err(err) = p.feed(byte) {
                        log::warn!(target: LOG_SCOPE, "glyph protocol error: {err:?}");
                        p.deinit();
                        self.state = State::Ignore;
                    }
                } else {
                    unreachable!();
                }
            }
        }
    }

    /// Transition from protocol identification to bounded unknown capture.
    // fn beginUnknown(
    //     self: *Handler,
    //     alloc: Allocator,
    //     prefix: []const u8,
    //     suffix: []const u8,
    // ) void {
    fn begin_unknown(&mut self, alloc: Allocator, prefix: &[u8], suffix: &[u8]) {
        // const max_bytes = self.unknown_max_bytes;
        let max_bytes = self.unknown_max_bytes;
        // if (max_bytes == 0) { self.state = .ignore; return; }
        if max_bytes == 0 {
            self.state = State::Ignore;
            return;
        }

        // Build the replacement before overwriting identify because prefix
        // points into that union field.
        // PORT NOTE: the Rust callers copy the 4-byte identify buffer out
        // before calling (see the `feed` PORT NOTE), so `prefix` here never
        // aliases `self.state`.
        let mut unknown = UnknownBuilder::init(alloc, max_bytes);
        unknown.append(prefix);
        unknown.append(suffix);
        self.state = State::Unknown(unknown);
    }

    /// Feed a slice of bytes to the handler. This is equivalent to
    /// calling feed for each byte in order, but protocol payload bytes
    /// are passed through in bulk so large payloads (e.g. Kitty graphics
    /// images) avoid per-byte dispatch overhead.
    //
    // PORT NOTE (D-terminal-subsystems.md §4.6): the reference borrows the
    // union payload (`.kitty => |*p|`) and reassigns the union
    // (`self.state = .ignore`) from inside that borrow — legal in Zig,
    // UB-adjacent in Rust. Here the payload borrow (`p`) ENDS at the
    // `is_err()` call, so the `self.state = State::Ignore` assignment that
    // follows is a fresh, exclusive borrow of `self.state` and the old
    // parser is dropped by the assignment (which is also what reproduces the
    // reference's `p.deinit()` for the kitty parser). This is the same shape
    // `feed` above already uses; NLL, not luck, and `apc_feed_slice_*` plus
    // the Miri run over this module are the proof.
    // pub fn feedSlice(self: *Handler, alloc: Allocator, bytes: []const u8) void
    pub fn feed_slice(&mut self, alloc: Allocator, bytes: &[u8]) {
        // var rem = bytes;
        let mut rem = bytes;
        // while (rem.len > 0) {
        while !rem.is_empty() {
            match &mut self.state {
                // .inactive => unreachable,
                State::Inactive => unreachable!(),

                // We're ignoring this APC command; drop the whole slice.
                // .ignore => return,
                State::Ignore => return,

                // We're capturing an unknown APC command, so store it.
                // .unknown => |*unknown| {
                //     unknown.append(rem);
                //     return;
                // },
                State::Unknown(unknown) => {
                    unknown.append(rem);
                    return;
                }

                // Identification consumes at most a few bytes; step
                // through them one at a time until the state changes.
                // .identify => { self.feed(alloc, rem[0]); rem = rem[1..]; },
                State::Identify(_) => {
                    self.feed(alloc, rem[0]);
                    rem = &rem[1..];
                }

                // .kitty => |*p| if (comptime build_options.kitty_graphics) { ... } else unreachable,
                State::Kitty(p) => {
                    if KITTY_GRAPHICS {
                        if let Err(err) = p.feed_slice(rem) {
                            log::warn!(target: LOG_SCOPE, "kitty graphics protocol error: {err:?}");
                            // p.deinit();
                            self.state = State::Ignore;
                        }
                        return;
                    } else {
                        unreachable!();
                    }
                }

                // .glyph => |*p| if (comptime build_options.glyph_protocol) { ... } else unreachable,
                State::Glyph(p) => {
                    if GLYPH_PROTOCOL {
                        if let Err(err) = p.feed_slice(rem) {
                            log::warn!(target: LOG_SCOPE, "glyph protocol error: {err:?}");
                            p.deinit();
                            self.state = State::Ignore;
                        }
                        return;
                    } else {
                        unreachable!();
                    }
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
    // `alloc` (a port-only parameter) is the allocator the APC was fed with;
    // each parser completes with its own allocator (`p.alloc`), as in the
    // reference, and `alloc` is debug-asserted to be it.
    //
    /// Complete the current APC. The caller owns a returned result and must
    /// call `Command.deinit` with the allocator used while feeding the APC.
    pub fn end(&mut self, alloc: Allocator) -> Option<Command> {
        let state = core::mem::replace(&mut self.state, State::Inactive);
        match state {
            // .inactive => unreachable,
            State::Inactive => unreachable!(),
            // .ignore, .identify => null,
            State::Ignore | State::Identify(_) => None,
            // .unknown => |*unknown| .{ .unknown = unknown.toOwned() },
            State::Unknown(mut unknown) => Some(Command::Unknown(unknown.to_owned())),
            // .kitty => |*p| kitty: {
            //     if (comptime !build_options.kitty_graphics) unreachable;
            //     const alloc = p.alloc;
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
                // const alloc = p.alloc;
                let parser_alloc = p.allocator();
                parser_alloc.debug_assert_same(alloc);
                match p.complete(parser_alloc) {
                    Ok(command) => Some(Command::Kitty(command)),
                    Err(err) => {
                        log::warn!(target: LOG_SCOPE, "kitty graphics protocol error: {err:?}");
                        None
                    }
                }
            }

            // .glyph => |*p| glyph_cmd: {
            //     if (comptime !build_options.glyph_protocol) unreachable;
            //     const command = p.complete(p.alloc) catch |err| {
            //         log.warn("glyph protocol error: {}", .{err});
            //         break :glyph_cmd null;
            //     };
            //     break :glyph_cmd .{ .glyph = command };
            // },
            State::Glyph(mut p) => {
                if !GLYPH_PROTOCOL {
                    unreachable!();
                }

                let parser_alloc = p.allocator();
                parser_alloc.debug_assert_same(alloc);
                match p.complete(parser_alloc) {
                    Ok(command) => Some(Command::Glyph(command)),
                    Err(err) => {
                        log::warn!(target: LOG_SCOPE, "glyph protocol error: {err:?}");
                        None
                    }
                }
            }
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
    // glyph: if (build_options.glyph_protocol)
    //     glyph.CommandParser
    // else
    //     void,
    // PORT NOTE: always carries the parser, as `Kitty` does (see above); the
    // `!GLYPH_PROTOCOL` arms keep the reference's `unreachable` guards.
    Glyph(glyph::CommandParser),

    /// An unsupported APC retained for the optional unknown callback.
    /// Keep this after recognized protocol states so their tag values and
    /// generated dispatch stay stable when unknown capture is unused.
    // unknown: UnknownBuilder,
    Unknown(UnknownBuilder),
}

impl State {
    // pub fn deinit(self: *State) void {
    //     switch (self.*) {
    //         .inactive, .ignore, .identify => {},
    //         .unknown => |*v| v.deinit(),
    //         .glyph => |*v| if (comptime build_options.glyph_protocol)
    //             v.deinit()
    //         else
    //             unreachable,
    //         .kitty => |*v| if (comptime build_options.kitty_graphics)
    //             v.deinit()
    //         else
    //             unreachable,
    //     }
    // }
    pub fn deinit(&mut self) {
        match self {
            State::Inactive | State::Ignore | State::Identify(_) => {}
            State::Unknown(v) => v.deinit(),
            State::Glyph(v) => {
                if GLYPH_PROTOCOL {
                    v.deinit();
                } else {
                    unreachable!();
                }
            }
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
// PORT NOTE: reference `identify: struct { len: u3 = 0,
// buf: [glyph.identifier.len]u8 = undefined }`.
// `len: u3` (0..=4 needed) ports to `u8`; `buf = undefined` starts zeroed
// (only `buf[0..len]` is ever read, so the initial fill is unobservable).
#[derive(Default)]
pub struct Identify {
    len: u8,
    buf: [u8; glyph::IDENTIFIER.len()],
}

/// UnknownBuilder is responsible for accumulating bytes for an
/// unidentified APC command if unknown capture is enabled.
// const UnknownBuilder = struct { ... };
pub struct UnknownBuilder {
    // data: std.ArrayList(u8) = .empty,
    // alloc: Allocator,
    // PORT NOTE (parity tail S7-4): the list is allocated through `alloc`
    // (bound at `init`); a refused growth takes the reference's
    // `catch { self.truncated = true; return; }` arm.
    data: Bytes,
    max_bytes: usize,
    truncated: bool,
}

impl UnknownBuilder {
    // fn init(alloc: Allocator, max_bytes: usize) UnknownBuilder {
    fn init(alloc: Allocator, max_bytes: usize) -> UnknownBuilder {
        UnknownBuilder {
            data: Bytes::new_in(alloc),
            max_bytes,
            truncated: false,
        }
    }

    // fn deinit(self: *UnknownBuilder) void {
    //     self.data.deinit(self.alloc);
    //     self.data = .empty;
    // }
    fn deinit(&mut self) {
        let alloc = *self.data.allocator();
        self.data = Bytes::new_in(alloc);
    }

    // Append some bytes to the unknown capture. This flags as truncated
    // if allocation fails or we reach our byte limit, therefore
    // it can't fail.
    // fn append(self: *UnknownBuilder, bytes: []const u8) void {
    fn append(&mut self, bytes: &[u8]) {
        // if (bytes.len == 0) return;
        if bytes.is_empty() {
            return;
        }
        // const current = self.data.items.len;
        let current = self.data.len();

        // Determine how many bytes we can store in this append and
        // if it is less than our input, then we have to note we're
        // truncating.
        // const retained = @min(bytes.len, self.max_bytes -| current);
        // if (retained < bytes.len) self.truncated = true;
        let retained = core::cmp::min(bytes.len(), self.max_bytes.saturating_sub(current));
        if retained < bytes.len() {
            self.truncated = true;
        }

        // If we require more bytes than our capacity allows then we
        // need to grow.
        // const required = current + retained;
        // if (required > self.data.capacity) {
        //     const capacity = @min(
        //         self.max_bytes,
        //         @max(required, @max(self.data.capacity *| 2, 1)),
        //     );
        //     self.data.ensureTotalCapacityPrecise(self.alloc, capacity) catch {
        //         self.truncated = true;
        //         return;
        //     };
        // }
        let required = current + retained;
        if required > self.data.capacity() {
            let capacity = core::cmp::min(
                self.max_bytes,
                core::cmp::max(
                    required,
                    core::cmp::max(self.data.capacity().saturating_mul(2), 1),
                ),
            );
            if self
                .data
                .try_reserve_exact(capacity - self.data.len())
                .is_err()
            {
                self.truncated = true;
                return;
            }
        }

        // self.data.appendSliceAssumeCapacity(bytes[0..retained]);
        self.data.extend_copy(&bytes[0..retained]);
    }

    /// Convert the current capture state to an Unknown where allocator
    /// ownership shifts to Unknown. Removes any accumulated unknown
    /// capture in this struct.
    ///
    /// This can't fail because if there is an allocator issue we return
    /// an empty truncate-flagged Unknown.
    // fn toOwned(self: *UnknownBuilder) Unknown {
    // PORT NOTE: `toOwnedSlice` hands the list over (not a shrunk copy, so
    // its failure arm cannot occur; a T3 call-shape residual). The inherent
    // name shadows `ToOwned::to_owned` deliberately — the receiver is
    // `&mut self`, so there is no ambiguity in practice.
    #[allow(clippy::should_implement_trait, clippy::wrong_self_convention)]
    fn to_owned(&mut self) -> Unknown {
        // toOwnedSlice allows us to reuse data but this makes error
        // handling a little simpler.
        let alloc = *self.data.allocator();
        let content = core::mem::replace(&mut self.data, Bytes::new_in(alloc));

        Unknown {
            content,
            truncated: self.truncated,
        }
    }
}

/// An unsupported APC returned by `Handler.end`.
// pub const Unknown = struct {
//     content: []u8,
//     truncated: bool,
//     ...
// };
pub struct Unknown {
    pub content: Bytes,
    pub truncated: bool,
}

impl Unknown {
    // pub fn deinit(self: *Unknown, alloc: Allocator) void {
    //     if (self.content.len > 0) alloc.free(self.content);
    //     self.* = undefined;
    // }
    // PORT NOTE: frees the content (through the allocator it was allocated
    // with, checked against `alloc`).
    pub fn deinit(&mut self, alloc: Allocator) {
        if self.content.capacity() != 0 {
            self.content.allocator().debug_assert_same(alloc);
        }
        self.content = Bytes::new_in(alloc);
    }
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

    /// Return the largest default buffer limit across every APC protocol.
    /// Consumers that must retain any unfinished APC can derive their limit
    /// here instead of duplicating a particular protocol's current default.
    // pub fn maxDefaultBytes() usize {
    //     var result: usize = 0;
    //     for (std.enums.values(Protocol)) |protocol| {
    //         result = @max(result, protocol.defaultMaxBytes());
    //     }
    //     return result;
    // }
    pub fn max_default_bytes() -> usize {
        let mut result: usize = 0;
        // `std.enums.values(Protocol)` — every variant, in declaration order.
        for protocol in [Protocol::Kitty, Protocol::Glyph] {
            result = core::cmp::max(result, protocol.default_max_bytes());
        }
        result
    }
}

/// A recognized or unsupported APC command.
// pub const Command = union(enum) {
//     kitty: if (build_options.kitty_graphics) kitty_gfx.Command else void,
//     glyph: if (build_options.glyph_protocol) glyph.Request else void,
//     unknown: Unknown,
//     ...
// };
pub enum Command {
    Kitty(kitty_gfx::Command),
    Glyph(glyph::Request),
    Unknown(Unknown),
}

impl Command {
    // pub fn deinit(self: *Command, alloc: Allocator) void {
    //     switch (self.*) {
    //         .kitty => |*v| if (comptime build_options.kitty_graphics)
    //             v.deinit(alloc)
    //         else
    //             unreachable,
    //         .glyph => |*v| if (comptime build_options.glyph_protocol)
    //             v.deinit(alloc)
    //         else
    //             unreachable,
    //         .unknown => |*v| v.deinit(alloc),
    //     }
    // }
    // PORT NOTE: the kitty `Command` and the glyph `Request` own their bytes,
    // allocated through `alloc`; each arm frees them now (and checks it).
    pub fn deinit(&mut self, alloc: Allocator) {
        match self {
            Command::Kitty(v) => {
                if !KITTY_GRAPHICS {
                    unreachable!();
                }
                // v.deinit(alloc)
                if v.data.capacity() != 0 {
                    v.data.allocator().debug_assert_same(alloc);
                }
                v.data = Bytes::new_in(alloc);
            }
            // v.deinit(alloc)
            Command::Glyph(v) => {
                if !GLYPH_PROTOCOL {
                    unreachable!();
                }
                v.deinit(alloc);
            }
            // .unknown => |*v| v.deinit(alloc),
            Command::Unknown(v) => v.deinit(alloc),
        }
    }
}

#[cfg(test)]
mod tests {
    thread_local! {
        /// Test seam for `glyph_protocol_build_option` (parity S149-F1).
        pub(super) static GLYPH_PROTOCOL_FORCED_OFF: core::cell::Cell<bool> =
            const { core::cell::Cell::new(false) };
    }

    /// Forces the glyph build option off on this test thread until dropped.
    struct ForceGlyphProtocolOff;

    impl ForceGlyphProtocolOff {
        fn new() -> Self {
            GLYPH_PROTOCOL_FORCED_OFF.with(|off| off.set(true));
            ForceGlyphProtocolOff
        }
    }

    impl Drop for ForceGlyphProtocolOff {
        fn drop(&mut self) {
            GLYPH_PROTOCOL_FORCED_OFF.with(|off| off.set(false));
        }
    }

    use super::*;
    use crate::alloc::Global;

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

    // S149-F2: glyph-protocol parse failures at `end()` log
    // `log.warn("glyph protocol error: {}", .{err})` (apc.zig:241-244).
    #[test]
    #[cfg_attr(
        not(feature = "glyph-protocol"),
        ignore = "SkipZigTest: needs options::GLYPH_PROTOCOL (R14)"
    )]
    fn glyph_protocol_error_at_end_logs() {
        // The glyph parser only exists with `build_options.glyph_protocol`.

        let alloc = Global;
        let mut h: Handler = Handler::default();
        h.start();
        // `glyph::IDENTIFIER` ("25a1") routes into the glyph protocol state;
        // "x" is not a well-formed glyph request, so `complete()` fails with
        // `CommandParserError::InvalidFormat`.
        h.feed_slice(alloc, b"25a1;x");

        let cap = crate::log_capture::start();
        assert!(h.end(alloc).is_none());
        let messages = cap.messages();
        assert!(
            messages
                .iter()
                .any(|(level, target, msg)| *level == log::Level::Warn
                    && target == "terminal_apc"
                    && msg.starts_with("glyph protocol error: ")),
            "expected the glyph-protocol-error diagnostic, got: {messages:?}"
        );
        drop(cap);

        h.deinit();
    }

    // test "capture unknown APC command"
    #[test]
    fn capture_unknown_apc_command() {
        let alloc = Global;

        // var h: Handler = .{ .unknown_max_bytes = 5 };
        let mut h: Handler = Handler {
            unknown_max_bytes: 5,
            ..Handler::default()
        };
        // defer h.deinit();
        h.start();
        h.feed_slice(alloc, b"abcd;payload");

        // var result = h.end().?;
        let mut result = h.end(alloc).unwrap();
        // defer result.deinit(alloc);
        let Command::Unknown(unknown) = &result else {
            panic!("expected unknown");
        };
        assert_eq!(b"abcd;".as_slice(), unknown.content.as_slice());
        assert!(unknown.truncated);
        result.deinit(alloc);
        h.deinit();
    }

    // test "capture short unknown APC command"
    #[test]
    fn capture_short_unknown_apc_command() {
        let alloc = Global;

        // var h: Handler = .{ .unknown_max_bytes = 16 };
        let mut h: Handler = Handler {
            unknown_max_bytes: 16,
            ..Handler::default()
        };
        // defer h.deinit();
        h.start();
        h.feed(alloc, b'X');

        let mut result = h.end(alloc).unwrap();
        {
            let Command::Unknown(unknown) = &result else {
                panic!("expected unknown");
            };
            assert_eq!(b"X".as_slice(), unknown.content.as_slice());
            assert!(!unknown.truncated);
        }
        result.deinit(alloc);

        h.unknown_max_bytes = 1;
        h.start();
        h.feed_slice(alloc, b"XYZ");
        result = h.end(alloc).unwrap();
        {
            let Command::Unknown(truncated) = &result else {
                panic!("expected unknown");
            };
            assert_eq!(b"X".as_slice(), truncated.content.as_slice());
            assert!(truncated.truncated);
        }
        result.deinit(alloc);
        h.deinit();
    }

    // test "disabled known APC protocol is not unknown"
    #[test]
    fn disabled_known_apc_protocol_is_not_unknown() {
        let alloc = Global;

        // var h: Handler = .{ .unknown_max_bytes = 64 };
        let mut h: Handler = Handler {
            unknown_max_bytes: 64,
            ..Handler::default()
        };
        // defer h.deinit();
        h.enable(Protocol::Glyph, false);
        h.start();
        h.feed_slice(alloc, b"25a1;q;cp=E0A0");
        assert!(h.end(alloc).is_none());

        // An incomplete known protocol identifier is malformed, not unknown.
        h.start();
        h.feed_slice(alloc, b"25a");
        assert!(h.end(alloc).is_none());
        h.deinit();
    }

    // test "garbage Kitty command"
    #[test]
    #[cfg_attr(
        not(all(feature = "kitty-graphics", not(target_os = "none"))),
        ignore = "SkipZigTest: needs options::KITTY_GRAPHICS (R14)"
    )]
    fn garbage_kitty_command() {
        // if (comptime !build_options.kitty_graphics) return error.SkipZigTest;

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
    #[cfg_attr(
        not(all(feature = "kitty-graphics", not(target_os = "none"))),
        ignore = "SkipZigTest: needs options::KITTY_GRAPHICS (R14)"
    )]
    fn kitty_command_with_overflow_u32() {
        // if (comptime !build_options.kitty_graphics) return error.SkipZigTest;

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
    #[cfg_attr(
        not(all(feature = "kitty-graphics", not(target_os = "none"))),
        ignore = "SkipZigTest: needs options::KITTY_GRAPHICS (R14)"
    )]
    fn kitty_command_with_overflow_i32() {
        // if (comptime !build_options.kitty_graphics) return error.SkipZigTest;

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
    #[cfg_attr(
        not(all(feature = "kitty-graphics", not(target_os = "none"))),
        ignore = "SkipZigTest: needs options::KITTY_GRAPHICS (R14)"
    )]
    fn kitty_feed_error_deinits_parser() {
        // if (comptime !build_options.kitty_graphics) return error.SkipZigTest;

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
    #[cfg_attr(
        not(all(feature = "kitty-graphics", not(target_os = "none"))),
        ignore = "SkipZigTest: needs options::KITTY_GRAPHICS (R14)"
    )]
    fn kitty_max_bytes_exceeded() {
        // if (comptime !build_options.kitty_graphics) return error.SkipZigTest;

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
    #[cfg_attr(
        not(all(feature = "kitty-graphics", not(target_os = "none"))),
        ignore = "SkipZigTest: needs options::KITTY_GRAPHICS (R14)"
    )]
    fn valid_kitty_command() {
        // if (comptime !build_options.kitty_graphics) return error.SkipZigTest;

        let alloc = Global;

        let mut h: Handler = Handler::default();
        h.start();
        let input = *b"Gf=24,s=10,v=20,hello=world";
        for c in input {
            h.feed(alloc, c);
        }

        // var result = h.end().?;
        let mut result = h.end(alloc).unwrap();
        // defer result.deinit(alloc);
        // try testing.expect(result == .kitty);
        assert!(matches!(result, Command::Kitty(_)));
        result.deinit(alloc);
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
    #[cfg_attr(
        not(feature = "glyph-protocol"),
        ignore = "SkipZigTest: needs options::GLYPH_PROTOCOL (R14)"
    )]
    fn valid_glyph_command() {
        // if (comptime !build_options.glyph_protocol) return error.SkipZigTest;

        let alloc = Global;

        let mut h: Handler = Handler::default();
        h.start();
        for c in *b"25a1;q;cp=E0A0" {
            h.feed(alloc, c);
        }

        // var result = h.end().?;
        let mut result = h.end(alloc).unwrap();
        // defer result.deinit(alloc);
        // try testing.expect(result == .glyph);
        assert!(matches!(result, Command::Glyph(_)));
        // try testing.expect(result.glyph == .query);
        assert!(matches!(result, Command::Glyph(glyph::Request::Query(_))));
        result.deinit(alloc);
    }

    // Parity S149-F1 (no reference test): with `build_options.glyph_protocol`
    // off, the reference's identify arm takes `self.state = .ignore` for the
    // glyph identifier even while `.glyph` is enabled at runtime, so the APC
    // ends with no command. The build option is forced off through the
    // `glyph_protocol_build_option` test seam, so this runs in every lane (in a
    // genuinely trimmed build the const is already false).
    #[test]
    fn glyph_protocol_build_option_off_ignores_glyph_command() {
        // Control: without the seam the same bytes identify as glyph exactly
        // when the build option is on.
        {
            let alloc = Global;
            let mut h: Handler = Handler::default();
            h.start();
            h.feed_slice(alloc, b"25a1;");
            assert_eq!(GLYPH_PROTOCOL, matches!(h.state, State::Glyph(_)));
            h.deinit();
        }

        let _off = ForceGlyphProtocolOff::new();
        assert!(!super::glyph_protocol_build_option());
        let alloc = Global;
        let mut h: Handler = Handler::default();
        assert!(h.enabled[Protocol::Glyph as usize]);
        h.start();
        for c in *b"25a1;" {
            h.feed(alloc, c);
        }
        assert!(matches!(h.state, State::Ignore));
        h.feed_slice(alloc, b"q;cp=E0A0");
        assert!(h.end(alloc).is_none());

        h.start();
        h.feed_slice(alloc, b"25a1;q;cp=E0A0");
        assert!(matches!(h.state, State::Ignore));
        assert!(h.end(alloc).is_none());
    }

    // test "feedSlice valid Kitty command"
    #[test]
    #[cfg_attr(
        not(all(feature = "kitty-graphics", not(target_os = "none"))),
        ignore = "SkipZigTest: needs options::KITTY_GRAPHICS (R14)"
    )]
    fn feed_slice_valid_kitty_command() {
        // if (comptime !build_options.kitty_graphics) return error.SkipZigTest;

        let alloc = Global;

        let mut h: Handler = Handler::default();
        h.start();
        h.feed_slice(alloc, b"Gf=24,s=10,v=20;aGVsbG8=");

        let mut result = h.end(alloc).unwrap();
        assert!(matches!(result, Command::Kitty(_)));

        // The payload is base64-decoded by the parser on completion.
        if let Command::Kitty(ref k) = result {
            assert_eq!(b"hello".as_slice(), k.data.as_slice());
        }
        result.deinit(alloc);
    }

    // test "feedSlice identify split across slices"
    #[test]
    #[cfg_attr(
        not(all(feature = "kitty-graphics", not(target_os = "none"))),
        ignore = "SkipZigTest: needs options::KITTY_GRAPHICS (R14)"
    )]
    fn feed_slice_identify_split_across_slices() {
        let alloc = Global;

        let mut h: Handler = Handler::default();
        h.start();
        h.feed_slice(alloc, b"G");
        h.feed_slice(alloc, b"f=24,s=10,");
        h.feed_slice(alloc, b"v=20;aGVsbG8=");

        let mut result = h.end(alloc).unwrap();
        assert!(matches!(result, Command::Kitty(_)));

        // The payload is base64-decoded by the parser on completion.
        if let Command::Kitty(ref k) = result {
            assert_eq!(b"hello".as_slice(), k.data.as_slice());
        }
        result.deinit(alloc);
    }

    // test "feedSlice unknown APC command is ignored"
    #[test]
    fn feed_slice_unknown_apc_command_is_ignored() {
        let alloc = Global;

        let mut h: Handler = Handler::default();
        h.start();
        h.feed_slice(alloc, b"Xabcdef1234");
        assert!(matches!(h.state, State::Ignore));
        h.feed_slice(alloc, b"more data that is dropped");
        assert!(h.end(alloc).is_none());
    }

    // test "feedSlice valid glyph command"
    #[test]
    #[cfg_attr(
        not(feature = "glyph-protocol"),
        ignore = "SkipZigTest: needs options::GLYPH_PROTOCOL (R14)"
    )]
    fn feed_slice_valid_glyph_command() {
        // if (comptime !build_options.glyph_protocol) return error.SkipZigTest;

        let alloc = Global;

        let mut h: Handler = Handler::default();
        h.start();
        h.feed_slice(alloc, b"25a1;q;cp=E0A0");

        let mut result = h.end(alloc).unwrap();
        assert!(matches!(result, Command::Glyph(_)));
        assert!(matches!(result, Command::Glyph(glyph::Request::Query(_))));
        result.deinit(alloc);
    }

    // test "feedSlice kitty max bytes exceeded"
    #[test]
    #[cfg_attr(
        not(all(feature = "kitty-graphics", not(target_os = "none"))),
        ignore = "SkipZigTest: needs options::KITTY_GRAPHICS (R14)"
    )]
    fn feed_slice_kitty_max_bytes_exceeded() {
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
        h.start();
        h.feed_slice(alloc, b"Ga=t;abcd");
        assert!(!matches!(h.state, State::Ignore));
        h.feed_slice(alloc, b"e");
        assert!(matches!(h.state, State::Ignore));
        h.deinit();
    }

    /// RUSTTY-SIDE (WS10 T17): `feed_slice` must be observationally identical
    /// to `feed`-per-byte for every chunking of the same input — that is the
    /// whole claim of `f6f79acce` ("equivalent to calling feed for each byte in
    /// order"), and it is the property the union-payload borrow reshape could
    /// break silently.
    #[test]
    fn feed_slice_matches_per_byte_for_every_chunking() {
        let alloc = Global;
        let inputs: &[&[u8]] = &[
            b"Gf=24,s=10,v=20;aGVsbG8=",
            b"25a1;q;cp=E0A0",
            b"Xabcdef1234",
            b"G",
            b"25a1;",
            b"",
            b"Ga=t;abcdefghijklmnop",
        ];

        for input in inputs {
            let per_byte_end = {
                let mut h: Handler = Handler::default();
                h.start();
                for &c in *input {
                    h.feed(alloc, c);
                }
                let e = h.end(alloc);
                format!("{}", e.is_some())
            };

            // Every split point, as two slices.
            for split in 0..=input.len() {
                let mut h: Handler = Handler::default();
                h.start();
                h.feed_slice(alloc, &input[..split]);
                h.feed_slice(alloc, &input[split..]);
                let e = h.end(alloc);
                assert_eq!(
                    per_byte_end,
                    format!("{}", e.is_some()),
                    "input={:?} split={split}",
                    String::from_utf8_lossy(input)
                );
            }
        }
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

    // Parity tail S7-4 (routing evidence, no reference counterpart): the
    // parsers and the unknown capture are created with the `feed` allocator
    // and complete with their own, so every buffer and every completed
    // command's bytes go to it; the glyph registration's decoded outline
    // (its contours and points) is allocated through the registering
    // allocator. `deinit` returns every byte.
    #[test]
    fn apc_builders_route_through_the_allocator_handle() {
        use super::*;
        use crate::alloc::testing::CountingAllocator;
        let counting = CountingAllocator::over_heap();
        let c = counting.allocator();
        crate::alloc::testing::with_handle(&c, |alloc| {
            let mut h = Handler::default();

            // Kitty: the payload list grows through `alloc` and the command
            // owns it.
            h.start();
            h.feed_slice(alloc, b"Ga=t,f=24,s=1,v=1;/wAA");
            assert_eq!(1, counting.alloc_calls.get(), "the kitty payload list");
            let mut cmd = h.end(alloc).unwrap();
            assert!(matches!(&cmd, Command::Kitty(k) if k.data.as_slice() == [255, 0, 0]));
            cmd.deinit(alloc);
            assert_eq!(0, counting.live_bytes.get());

            // Glyph: the request buffer, then the decoded outline.
            h.start();
            h.feed_slice(
                alloc,
                b"25a1;r;cp=e0a0;AAEAAAAAAGQAZAACAAABAQEAAABk/84AAAAAAGQ=",
            );
            assert!(counting.alloc_calls.get() > 1, "the glyph request buffer");
            let mut cmd = h.end(alloc).unwrap();
            let Command::Glyph(glyph::Request::Register(reg)) = &cmd else {
                panic!("a glyph registration");
            };
            let c0 = counting.alloc_calls.get();
            let entry = glyph::glossary::Entry::init(alloc, reg).unwrap();
            assert_eq!(
                c0 + 2,
                counting.alloc_calls.get(),
                "the outline's contours and points"
            );
            assert_eq!(
                Some((
                    3 * core::mem::size_of::<rustty_font::opentype::glyf::Point>(),
                    core::mem::align_of::<rustty_font::opentype::glyf::Point>().trailing_zeros()
                        as u8
                )),
                counting.last_alloc.get(),
                "the three points"
            );
            drop(entry);
            cmd.deinit(alloc);
            assert_eq!(0, counting.live_bytes.get());

            // Unknown capture (when enabled) grows through `alloc` too.
            h.unknown_max_bytes = 64;
            h.start();
            h.feed_slice(alloc, b"Xunknown");
            let mut cmd = h.end(alloc).unwrap();
            assert!(matches!(&cmd, Command::Unknown(u) if u.content.as_slice() == b"Xunknown"));
            assert!(
                counting.live_bytes.get() > 0,
                "the capture is on the handle"
            );
            cmd.deinit(alloc);
            h.deinit();
            assert_eq!(0, counting.live_bytes.get(), "every byte is returned");
        });
    }

    // Parity tail S7-4b (N278): a glyph registration decodes its payload on
    // a 64 KiB `stackFallback` scratch, as the reference does. Run end to end
    // (APC feed, request, `Entry::init`) on a thread with a 128 KiB stack
    // (1/16 of Rust's 2 MiB default, 1/128 of Zig's 16 MiB). Measured on
    // aarch64 macOS: a debug build needs about 80 KiB with the scratch built
    // in place (`StackFallback::new_in_place`), and overflowed 192 KiB with a
    // by-value `new`; a release build fits in 64 KiB.
    #[test]
    fn glyph_registration_fits_a_256k_stack() {
        let stack = std::env::var("RUSTTY_GLYPH_STACK")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(128 * 1024);
        std::thread::Builder::new()
            .stack_size(stack)
            .spawn(|| {
                use super::*;
                let alloc = crate::alloc::Global;
                let mut h = Handler::default();
                h.start();
                h.feed_slice(
                    alloc,
                    b"25a1;r;cp=e0a0;AAEAAAAAAGQAZAACAAABAQEAAABk/84AAAAAAGQ=",
                );
                let mut cmd = h.end(alloc).unwrap();
                let Command::Glyph(glyph::Request::Register(reg)) = &cmd else {
                    panic!("a glyph registration");
                };
                let entry = glyph::glossary::Entry::init(alloc, reg).unwrap();
                drop(entry);
                cmd.deinit(alloc);
                h.deinit();
            })
            .unwrap()
            .join()
            .unwrap();
    }
}
