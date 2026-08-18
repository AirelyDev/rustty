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
                //         if (self.enabled.contains(.glyph)) { ... }
                //         else { self.state = .ignore; }
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
                        if self.enabled[Protocol::Glyph as usize] {
                            self.state = State::Glyph(glyph::CommandParser::init(
                                self.max_bytes[Protocol::Glyph as usize],
                            ));
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

    /// Transition from protocol identification to bounded unknown capture.
    // fn beginUnknown(
    //     self: *Handler,
    //     alloc: Allocator,
    //     prefix: []const u8,
    //     suffix: []const u8,
    // ) void {
    fn begin_unknown(&mut self, alloc: Allocator, prefix: &[u8], suffix: &[u8]) {
        let _ = alloc;
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
        let mut unknown = UnknownBuilder::init(max_bytes);
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
                        if p.feed_slice(rem).is_err() {
                            // log.warn("kitty graphics protocol error: {}", .{err});
                            // p.deinit();
                            self.state = State::Ignore;
                        }
                        return;
                    } else {
                        unreachable!();
                    }
                }

                // .glyph => |*p| { p.feedSlice(rem) catch ...; return; },
                State::Glyph(p) => {
                    if p.feed_slice(rem).is_err() {
                        // log.warn("glyph protocol error: {}", .{err});
                        p.deinit();
                        self.state = State::Ignore;
                    }
                    return;
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
    //
    /// Complete the current APC. The caller owns a returned result and must
    /// call `Command.deinit` with the allocator used while feeding the APC.
    pub fn end(&mut self, alloc: Allocator) -> Option<Command> {
        let _ = alloc;
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
            State::Unknown(v) => v.deinit(),
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
    data: Vec<u8>,
    // alloc: Allocator,
    // PORT NOTE: the `Vec` allocates on the global allocator (the crate's
    // established ArrayList mapping); the reference's allocation-failure arms
    // (`catch { self.truncated = true; return; }` / `toOwnedSlice` failure)
    // have no port-side analog because the global allocator aborts on OOM.
    max_bytes: usize,
    truncated: bool,
}

impl UnknownBuilder {
    // fn init(alloc: Allocator, max_bytes: usize) UnknownBuilder {
    fn init(max_bytes: usize) -> UnknownBuilder {
        UnknownBuilder {
            data: Vec::new(),
            max_bytes,
            truncated: false,
        }
    }

    // fn deinit(self: *UnknownBuilder) void {
    //     self.data.deinit(self.alloc);
    //     self.data = .empty;
    // }
    fn deinit(&mut self) {
        self.data = Vec::new();
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
            self.data.reserve_exact(capacity - self.data.len());
        }

        // self.data.appendSliceAssumeCapacity(bytes[0..retained]);
        self.data.extend_from_slice(&bytes[0..retained]);
    }

    /// Convert the current capture state to an Unknown where allocator
    /// ownership shifts to Unknown. Removes any accumulated unknown
    /// capture in this struct.
    ///
    /// This can't fail because if there is an allocator issue we return
    /// an empty truncate-flagged Unknown.
    // fn toOwned(self: *UnknownBuilder) Unknown {
    // PORT NOTE: `toOwnedSlice` → `mem::take` of the `Vec` (its failure arm
    // has no analog, see the `alloc` PORT NOTE above). The inherent name
    // shadows `ToOwned::to_owned` deliberately — the receiver is `&mut self`,
    // so there is no ambiguity in practice.
    #[allow(clippy::should_implement_trait, clippy::wrong_self_convention)]
    fn to_owned(&mut self) -> Unknown {
        // toOwnedSlice allows us to reuse data but this makes error
        // handling a little simpler.
        let content = core::mem::take(&mut self.data);

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
    pub content: Vec<u8>,
    pub truncated: bool,
}

impl Unknown {
    // pub fn deinit(self: *Unknown, alloc: Allocator) void {
    //     if (self.content.len > 0) alloc.free(self.content);
    //     self.* = undefined;
    // }
    // PORT NOTE: the owned `Vec` frees on drop; the explicit free is the
    // no-op call-site fidelity hook (`alloc` unused).
    pub fn deinit(&mut self, alloc: Allocator) {
        let _ = alloc;
        if !self.content.is_empty() {
            self.content = Vec::new();
        }
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
//     glyph: glyph.Request,
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
    //         .glyph => |*v| v.deinit(alloc),
    //     }
    // }
    // PORT NOTE: both the kitty `Command` and the glyph `Request` own their bytes
    // via `Vec`s and free them on drop, so the per-arm `deinit(alloc)` calls are
    // no-ops retained for call-site fidelity (`alloc` is unused).
    pub fn deinit(&mut self, alloc: Allocator) {
        match self {
            Command::Kitty(_v) => {
                if !KITTY_GRAPHICS {
                    unreachable!();
                }
                // v.deinit(alloc) — Rust drops the owned data; no-op.
                let _ = alloc;
            }
            // v.deinit(alloc) — `Request::deinit` frees on drop; no-op.
            Command::Glyph(v) => v.deinit(),
            // .unknown => |*v| v.deinit(alloc),
            Command::Unknown(v) => v.deinit(alloc),
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
    fn valid_glyph_command() {
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

    // test "feedSlice valid Kitty command"
    #[test]
    fn feed_slice_valid_kitty_command() {
        // if (comptime !build_options.kitty_graphics) return error.SkipZigTest;
        if !KITTY_GRAPHICS {
            return;
        }

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
    fn feed_slice_identify_split_across_slices() {
        if !KITTY_GRAPHICS {
            return;
        }

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
    fn feed_slice_valid_glyph_command() {
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
    fn feed_slice_kitty_max_bytes_exceeded() {
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
}
