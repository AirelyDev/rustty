//! rustty-vt dcs
//!
//! PORTS: ghostty src/terminal/dcs.zig
//!
//! PORT STATUS: done

// const std = @import("std");
// const build_options = @import("terminal_options");
// const assert = @import("../quirks.zig").inlineAssert;
// const Allocator = std.mem.Allocator;
// const terminal = @import("main.zig");
// const DCS = terminal.DCS;
//
// const log = std.log.scoped(.terminal_dcs);
//
// PORT NOTE: `build_options` is the generated `terminal_options` module, ported
// as `crate::options`. `assert` is `rustty_util::quirks::inlineAssert`.
// `std.mem.Allocator` is faithfully modeled by `allocator_api2::alloc::Allocator`
// (the same map used by PageList/Tabstops); callers pass
// `allocator_api2::alloc::Global` for the prior `std::alloc` behavior. `log` is a
// scoped logger; Rustty has no logging facade wired into this crate (matching the
// convention used by PageList et al.), so the `log.*` calls below are ported as
// verbatim comments and are no-ops.
use allocator_api2::alloc::{Allocator, Global};
use rustty_util::quirks::inlineAssert as assert;

use crate::options as build_options;

/// DEFERRED SEAM: `DCS = terminal.DCS = Parser.Action.DCS`. `Parser` is ported by
/// a later vt sub-batch (vt-5); to avoid a forward dependency on the unported
/// `Parser` module, the input struct this module reads is reproduced here as a
/// faithful local stand-in carrying exactly the fields `dcs.zig` inspects. When
/// `Parser` lands, this seam is replaced by `crate::parser::Action::DCS`.
// PORT NOTE: the reference is
//   pub const DCS = struct {
//       intermediates: []const u8 = "",
//       params: []const u16 = &.{},
//       final: u8,
//   };
// `final` is a Rust keyword, so the field is ported as `final_`.
#[derive(Clone, Copy, Debug)]
pub struct DCS<'a> {
    pub intermediates: &'a [u8],
    pub params: &'a [u16],
    pub final_: u8,
}

impl Default for DCS<'_> {
    fn default() -> Self {
        DCS {
            intermediates: b"",
            params: &[],
            // PORT NOTE: the reference `final` has no default (it must be set by
            // the caller); a `Default` is provided only so the test helpers can
            // write `DCS { final_: 'A', ..Default::default() }` mirroring Zig's
            // `.{ .final = 'A' }`. The default `final_` of `0` is never relied on.
            final_: 0,
        }
    }
}

/// DCS command handler. This should be hooked into a terminal.Stream handler.
/// The hook/put/unhook functions are meant to be called from the
/// terminal.stream dcsHook, dcsPut, and dcsUnhook functions, respectively.
// PORT NOTE: the handler stores its allocated state (the XTGETTCAP `Allocating`
// buffer captures its allocator), so the handler is generic over the allocator
// `A`; `Handler::default()` selects `Global`, mirroring `var h: Handler = .{}`
// followed by passing `testing.allocator` to `hook`.
pub struct Handler<A: Allocator = Global> {
    state: State<A>,

    /// Maximum bytes any DCS command can take. This is to prevent
    /// malicious input from causing us to allocate too much memory.
    /// This is arbitrarily set to 1MB today, increase if needed.
    max_bytes: usize,
}

impl<A: Allocator> Default for Handler<A> {
    // PORT NOTE: the reference field defaults are
    //   state: State = .{ .inactive = {} },
    //   max_bytes: usize = 1024 * 1024,
    fn default() -> Self {
        Handler {
            state: State::Inactive,
            max_bytes: 1024 * 1024,
        }
    }
}

impl<A: Allocator> Handler<A> {
    pub fn deinit(&mut self) {
        self.discard();
    }

    pub fn hook(&mut self, alloc: A, dcs: DCS<'_>) -> Option<Command<A>> {
        assert(matches!(self.state, State::Inactive));

        // Initialize our state to ignore in case of error
        self.state = State::Ignore;

        // Try to parse the hook.
        let hk_ = match self.try_hook(alloc, dcs) {
            Ok(hk_) => hk_,
            Err(_err) => {
                // log.info("error initializing DCS hook, will ignore hook err={}", .{err});
                return None;
            }
        };
        let hk = match hk_ {
            Some(hk) => hk,
            None => {
                // log.info("unknown DCS hook: {}", .{dcs});
                return None;
            }
        };

        self.state = hk.state;
        hk.command
    }

    fn try_hook(&self, alloc: A, dcs: DCS<'_>) -> Result<Option<Hook<A>>, OutOfMemory> {
        Ok(match dcs.intermediates.len() {
            0 => match dcs.final_ {
                // Tmux control mode
                b'p' => 'tmux: {
                    if !build_options::TMUX_CONTROL_MODE {
                        // log.debug("tmux control mode not enabled in build, ignoring", .{});
                        break 'tmux None;
                    }

                    // Tmux control mode must start with ESC P 1000 p
                    // if (dcs.params.len != 1 or dcs.params[0] != 1000) break :tmux null;
                    if dcs.params.len() != 1 || dcs.params[0] != 1000 {
                        break 'tmux None;
                    }

                    // PORT NOTE: the reference builds the parser as a struct
                    // literal (`.max_bytes = self.max_bytes, .buffer = try
                    // .initCapacity(alloc, 128)`); `ControlParser`'s fields are
                    // private, so `init_options` is its constructor mirror. The
                    // parser's buffer is an owning global `Vec` per its own PORT
                    // NOTE, so the `alloc` does not thread through here, but the
                    // `try` does: `init_options` reserves fallibly and its
                    // `OutOfMemory` propagates out of `hook` like the reference's.
                    break 'tmux Some(Hook {
                        state: State::Tmux(
                            TmuxState::init_options(
                                self.max_bytes,
                                128, // Arbitrary choice to limit initial reallocs
                            )
                            // The tmux parser reports the crate-wide pagelist
                            // `OutOfMemory`; map it onto this module's marker,
                            // exactly as `try_put`'s tmux arm does (:212-218).
                            .map_err(|_| OutOfMemory)?,
                        ),
                        // .command = .{ .tmux = .enter },
                        command: Some(Command::Tmux(TmuxCommand::Enter)),
                    });
                }

                _ => None,
            },

            1 => match dcs.intermediates[0] {
                b'+' => match dcs.final_ {
                    // XTGETTCAP
                    // https://github.com/mitchellh/ghostty/issues/517
                    b'q' => Some(Hook {
                        state: State::Xtgettcap(Allocating::init_capacity(
                            alloc, 128, // Arbitrary choice
                        )?),
                        command: None,
                    }),

                    _ => None,
                },

                b'$' => match dcs.final_ {
                    // DECRQSS
                    b'q' => Some(Hook {
                        state: State::Decrqss(DecrqssState::default()),
                        command: None,
                    }),

                    _ => None,
                },

                _ => None,
            },

            _ => None,
        })
    }

    /// Put a byte into the DCS handler. This will return a command
    /// if a command needs to be executed.
    pub fn put(&mut self, byte: u8) -> Option<Command<A>> {
        match self.try_put(byte) {
            Ok(cmd) => cmd,
            Err(_err) => {
                // On error we just discard our state and ignore the rest
                // log.info("error putting byte into DCS handler err={}", .{err});
                self.discard();
                self.state = State::Ignore;
                None
            }
        }
    }

    fn try_put(&mut self, byte: u8) -> Result<Option<Command<A>>, OutOfMemory> {
        match &mut self.state {
            State::Inactive | State::Ignore => {}

            State::Tmux(tmux) => {
                if build_options::TMUX_CONTROL_MODE {
                    // return .{ .tmux = (try tmux.put(byte)) orelse return null };
                    // PORT NOTE: the parser's `put` reports allocation failure
                    // with the pagelist `OutOfMemory` marker; map it onto this
                    // module's `OutOfMemory` (the same single-variant error set
                    // the reference's `!?Command` carries).
                    return match tmux.put(byte) {
                        Ok(Some(n)) => Ok(Some(Command::Tmux(n))),
                        Ok(None) => Ok(None),
                        Err(_) => Err(OutOfMemory),
                    };
                } else {
                    // else unreachable (the reference comptime-guards this prong)
                    unreachable!()
                }
            }

            State::Xtgettcap(list) => {
                if list.written().len() >= self.max_bytes {
                    return Err(OutOfMemory);
                }

                list.write_byte(byte)?;
            }

            State::Decrqss(buffer) => {
                if (buffer.len as usize) >= buffer.data.len() {
                    return Err(OutOfMemory);
                }

                buffer.data[buffer.len as usize] = byte;
                buffer.len += 1;
            }
        }

        Ok(None)
    }

    pub fn unhook(&mut self) -> Option<Command<A>> {
        // Note: we do NOT call deinit here on purpose because some commands
        // transfer memory ownership. If state needs cleanup, the switch
        // prong below should handle it.
        // PORT NOTE: the reference uses `defer self.state = .inactive;`. We
        // compute the result first, then reset the state to `inactive` before
        // returning, matching the deferred reset.
        let result = match core::mem::replace(&mut self.state, State::Inactive) {
            State::Inactive | State::Ignore => None,

            State::Tmux(mut state) => {
                if build_options::TMUX_CONTROL_MODE {
                    // self.state.deinit();
                    state.deinit();
                    // break :tmux .{ .tmux = .exit };
                    Some(Command::Tmux(TmuxCommand::Exit))
                } else {
                    // else unreachable (the reference comptime-guards this prong)
                    let _ = &mut state;
                    unreachable!()
                }
            }

            State::Xtgettcap(mut list) => {
                // Note: purposely do not deinit our state here because
                // we copy it into the resulting command.
                // for (items, 0..) |b, i| items[i] = std.ascii.toUpper(b);
                let items = list.written_mut();
                for b in items.iter_mut() {
                    *b = b.to_ascii_uppercase();
                }
                Some(Command::Xtgettcap(Xtgettcap { data: list, i: 0 }))
            }

            State::Decrqss(buffer) => Some(Command::Decrqss(match buffer.len {
                0 => Decrqss::None,
                1 => match buffer.data[0] {
                    b'm' => Decrqss::Sgr,
                    b'r' => Decrqss::Decstbm,
                    b's' => Decrqss::Decslrm,
                    _ => Decrqss::None,
                },
                2 => match buffer.data[0] {
                    b' ' => match buffer.data[1] {
                        b'q' => Decrqss::Decscusr,
                        _ => Decrqss::None,
                    },
                    _ => Decrqss::None,
                },
                _ => unreachable!(),
            })),
        };

        // defer self.state = .inactive; (state already reset by the replace above)
        result
    }

    fn discard(&mut self) {
        self.state.deinit();
        self.state = State::Inactive;
    }
}

/// PORT NOTE: the reference nests `Hook` as a private `const Hook = struct { ... }`
/// inside `Handler`. It carries the resolved state and an optional command.
struct Hook<A: Allocator> {
    state: State<A>,
    command: Option<Command<A>>,
}

// PORT NOTE: the reference returns errors via Zig's `!?Hook` / `!?Command` error
// unions; the only error any prong produces is `Allocator.Error` (OutOfMemory).
// We model that single-variant error set with this faithful stand-in, matching
// `page.rs`'s `OutOfMemory`.
/// The allocation-failure error mirroring Zig's `Allocator.Error` / OutOfMemory.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OutOfMemory;

// PORT NOTE: the reference `Command.tmux` / `State.tmux` field types are
//   if (build_options.tmux_control_mode) terminal.tmux.ControlNotification else void
//   if (build_options.tmux_control_mode) terminal.tmux.ControlParser     else void
// Rust has no comptime type selection; the real tmux types are used
// unconditionally (they are plain owned types), and every tmux prong below
// keeps the reference's `build_options.tmux_control_mode` guard as a
// const-bool `if`, so behavior under the default (tmux-disabled) build is
// unchanged: the `b'p'` hook ignores, and the put/unhook tmux prongs stay
// unreachable exactly as the reference's `else unreachable`.
type TmuxCommand = crate::tmux::ControlNotification;
type TmuxState = crate::tmux::ControlParser;

pub enum Command<A: Allocator = Global> {
    /// XTGETTCAP
    Xtgettcap(Xtgettcap<A>),

    /// DECRQSS
    Decrqss(Decrqss),

    /// Tmux control mode
    Tmux(TmuxCommand),
}

impl<A: Allocator> Command<A> {
    pub fn deinit(&mut self) {
        match self {
            Command::Xtgettcap(v) => v.data.deinit(),
            Command::Decrqss(_) => {}
            Command::Tmux(_) => {}
        }
    }
}

pub struct Xtgettcap<A: Allocator = Global> {
    pub data: Allocating<A>,
    i: usize,
}

impl<A: Allocator> Xtgettcap<A> {
    /// Returns the next terminfo key being requested and null
    /// when there are no more keys. The returned value is NOT hex-decoded
    /// because we expect to use a comptime lookup table.
    // PARITY: `next` is named after the reference `XTGETTCAP.next`, not the
    // `Iterator::next` trait; we keep the reference name and allow the lint.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Option<&[u8]> {
        let items = self.data.written();
        if self.i >= items.len() {
            return None;
        }
        let rem = &items[self.i..];
        let idx = find_subslice(rem, b";").unwrap_or(rem.len());

        // Note that if we're at the end, idx + 1 is len + 1 so we're over
        // the end but that's okay because our check above is >= so we'll
        // never read.
        self.i += idx + 1;

        Some(&rem[0..idx])
    }
}

/// Supported DECRQSS settings
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decrqss {
    None,
    Sgr,
    Decscusr,
    Decstbm,
    Decslrm,
}

impl Decrqss {
    /// Fixed upper bound for an encoded DECRPSS response. The comptime
    /// calculated max at the time of writing this was around 63 so this
    /// leaves a ton of space for future stuff. We don't do the comptime
    /// calculation cause it complicated the implementation a bit too
    /// much.
    // pub const max_response_bytes = 256;
    pub const MAX_RESPONSE_BYTES: usize = 256;

    /// Encode the response for this request.
    //
    // pub fn encode(self: DECRQSS, t: *terminal.Terminal, response: []u8) ![]const u8
    //
    // PORT NOTE: the reference writes through `std.Io.Writer.fixed(response)`.
    // rustty's encoders target `core::fmt::Write`, so the byte buffer is driven
    // through `fixed_writer::FixedWriter` (the stand-in for
    // `std.Io.Writer.fixed`) and an explicit `end` cursor. The buffer stays
    // `&mut [u8]` end to end — the response is NEVER formatted through a
    // `String`. Every overflow is `error.NoSpaceLeft`, which this crate models
    // as `OutOfMemory` (same mapping as `Terminal::print_attributes`).
    pub fn encode<'a>(
        self,
        t: &mut crate::terminal::Terminal,
        response: &'a mut [u8],
    ) -> Result<&'a [u8], OutOfMemory> {
        use core::fmt::Write as _;

        // const prefix_fmt = "\x1bP{d}$r";
        // const prefix_len = std.fmt.comptimePrint(prefix_fmt, .{0}).len;
        // writer.end = prefix_len;
        const PREFIX_LEN: usize = "\x1bP0$r".len();
        if response.len() < PREFIX_LEN {
            return Err(OutOfMemory);
        }
        let mut end = PREFIX_LEN;

        // Writes `args` at `end` through a fixed writer, advancing `end`.
        macro_rules! print {
            ($($arg:tt)*) => {{
                let mut w = crate::fixed_writer::FixedWriter::new(&mut response[end..]);
                write!(w, $($arg)*).map_err(|_| OutOfMemory)?;
                end += w.end;
            }};
        }

        match self {
            // .none => {},
            Decrqss::None => {}

            // .sgr => {
            //     const buf = try t.printAttributes(writer.buffer[writer.end..]);
            //     writer.end += buf.len;
            //     try writer.writeByte('m');
            // },
            Decrqss::Sgr => {
                let buf_len = t
                    .print_attributes(&mut response[end..])
                    .map_err(|_| OutOfMemory)?
                    .len();
                end += buf_len;
                if end >= response.len() {
                    return Err(OutOfMemory);
                }
                response[end] = b'm';
                end += 1;
            }

            // .decscusr => { ... try writer.print("{d} q", .{style}); },
            Decrqss::Decscusr => {
                let blink = t.modes.get(crate::modes::Mode::CursorBlinking);
                let style: u8 = match unsafe { (*t.screens.active).cursor.cursor_style } {
                    // .block, .block_hollow => if (blink) 1 else 2,
                    crate::screen::CursorStyle::Block | crate::screen::CursorStyle::BlockHollow => {
                        if blink {
                            1
                        } else {
                            2
                        }
                    }
                    // .underline => if (blink) 3 else 4,
                    crate::screen::CursorStyle::Underline => {
                        if blink {
                            3
                        } else {
                            4
                        }
                    }
                    // .bar => if (blink) 5 else 6,
                    crate::screen::CursorStyle::Bar => {
                        if blink {
                            5
                        } else {
                            6
                        }
                    }
                };
                print!("{style} q");
            }

            // .decstbm => try writer.print("{d};{d}r", .{ top + 1, bottom + 1 }),
            Decrqss::Decstbm => {
                let top = t.scrolling_region.top + 1;
                let bottom = t.scrolling_region.bottom + 1;
                print!("{top};{bottom}r");
            }

            // .decslrm => if (t.modes.get(.enable_left_and_right_margin)) { ... },
            Decrqss::Decslrm => {
                if t.modes.get(crate::modes::Mode::EnableLeftAndRightMargin) {
                    let left = t.scrolling_region.left + 1;
                    let right = t.scrolling_region.right + 1;
                    print!("{left};{right}s");
                }
            }
        }

        // const valid = writer.end > prefix_len;
        let valid = end > PREFIX_LEN;

        // try writer.writeAll("\x1b\\");
        print!("\x1b\\");

        // _ = try std.fmt.bufPrint(response[0..prefix_len], prefix_fmt, .{@intFromBool(valid)});
        {
            let mut w = crate::fixed_writer::FixedWriter::new(&mut response[0..PREFIX_LEN]);
            write!(w, "\x1bP{}$r", u8::from(valid)).map_err(|_| OutOfMemory)?;
        }

        // return writer.buffered();
        Ok(&response[..end])
    }
}

// PORT NOTE: the reference uses the `DECRQSS` enum above only as a *command*
// payload; the `.decrqss` *state* payload is a distinct anonymous struct
//   struct { data: [2]u8 = undefined, len: u2 = 0 }
// ported separately as `DecrqssState` below (the meaningful default state is
// `DecrqssState::default()`).

/// PORT NOTE: the `.decrqss` state payload is the reference anonymous struct
///   struct { data: [2]u8 = undefined, len: u2 = 0 }
/// ported here as a named struct (Rust has no anonymous structs). `data` is
/// `undefined` in the reference (only the first `len` bytes are meaningful); we
/// zero-initialize it, which is never observed since reads are bounded by `len`.
// PORT NOTE: `len` is a `u2` in the reference (`len: u2 = 0`); Rust has no `u2`,
// so it is ported as `u8` (only values 0..=2 are ever stored).
#[derive(Default)]
struct DecrqssState {
    data: [u8; 2],
    len: u8,
}

enum State<A: Allocator = Global> {
    /// We're not in a DCS state at the moment.
    Inactive,

    /// We're hooked, but its an unknown DCS command or one that went
    /// invalid due to some bad input, so we're ignoring the rest.
    Ignore,

    /// XTGETTCAP
    Xtgettcap(Allocating<A>),

    /// DECRQSS
    Decrqss(DecrqssState),

    /// Tmux control mode: https://github.com/tmux/tmux/wiki/Control-Mode
    Tmux(TmuxState),
}

/// Test-only inspection of the in-progress DCS state.
//
// PORT NOTE: the reference's `test "DCS command memory is released"` proves its
// point with `testing.allocator`'s leak detector, which this port has no
// equivalent of on the normal test lane (parity S167-F1). These accessors let
// that test assert POSITIVELY that it reached both halves of what it claims —
// a completed command leaving nothing behind, and an incomplete one still
// holding bytes for `deinit` to release.
#[cfg(test)]
impl<A: Allocator> Handler<A> {
    /// True when no DCS command is in progress.
    pub(crate) fn test_is_inactive(&self) -> bool {
        matches!(self.state, State::Inactive)
    }

    /// Bytes the handler is currently holding for an in-progress XTGETTCAP,
    /// or `None` when the state is not XTGETTCAP.
    pub(crate) fn test_xtgettcap_buffered(&self) -> Option<usize> {
        match &self.state {
            State::Xtgettcap(v) => Some(v.written().len()),
            _ => None,
        }
    }
}

impl<A: Allocator> State<A> {
    fn deinit(&mut self) {
        match self {
            State::Inactive | State::Ignore => {}

            State::Xtgettcap(v) => v.deinit(),
            State::Decrqss(_) => {}
            State::Tmux(v) => {
                if build_options::TMUX_CONTROL_MODE {
                    v.deinit();
                } else {
                    let _ = v;
                    // unreachable in the reference (`else unreachable`); under the
                    // default build this prong is statically dead.
                }
            }
        }
    }
}

/// PORT NOTE: faithful stand-in for `std.Io.Writer.Allocating` as used by this
/// module: an allocator-backed growable byte buffer. The reference uses only
/// `initCapacity(alloc, n)`, `writer.writeByte(b)`, `written()` (the bytes written
/// so far, mutable for the in-place uppercasing in `unhook`), and `deinit()`. It
/// is generic over the threaded allocator `A` (which it captures, mirroring how
/// `std.Io.Writer.Allocating` owns its allocator) and uses
/// `allocator_api2::vec::Vec` over `A`, the same convention as `tabstops.rs`.
pub struct Allocating<A: Allocator> {
    buf: allocator_api2::vec::Vec<u8, A>,
}

impl<A: Allocator> Allocating<A> {
    fn init_capacity(alloc: A, capacity: usize) -> Result<Allocating<A>, OutOfMemory> {
        // PORT NOTE: the reference `initCapacity` preallocates `capacity` bytes and
        // returns `error.OutOfMemory` on failure. `allocator_api2`'s
        // `Vec::with_capacity_in` aborts on allocation failure rather than
        // returning an error, so we use `try_reserve_exact` over an empty `Vec` to
        // surface the fallible-allocation behavior faithfully.
        let mut buf: allocator_api2::vec::Vec<u8, A> = allocator_api2::vec::Vec::new_in(alloc);
        buf.try_reserve_exact(capacity).map_err(|_| OutOfMemory)?;
        Ok(Allocating { buf })
    }

    /// The bytes written so far (`std.Io.Writer.Allocating.written()`).
    fn written(&self) -> &[u8] {
        &self.buf
    }

    /// The bytes written so far, mutable (for the in-place uppercasing in
    /// `unhook`). The reference mutates `written()` items directly.
    fn written_mut(&mut self) -> &mut [u8] {
        &mut self.buf
    }

    /// `writer.writeByte(byte)` — append one byte, growing as needed.
    fn write_byte(&mut self, byte: u8) -> Result<(), OutOfMemory> {
        self.buf.try_reserve(1).map_err(|_| OutOfMemory)?;
        self.buf.push(byte);
        Ok(())
    }

    fn deinit(&mut self) {
        // PORT NOTE: `Allocating.deinit()` frees the buffer. Clearing and shrinking
        // the `Vec` releases its allocation through the captured allocator.
        self.buf.clear();
        self.buf.shrink_to_fit();
    }
}

/// PORT NOTE: faithful map of `std.mem.indexOf(u8, haystack, needle)` for the
/// single-byte needle this module uses (`";"`). Returns the index of the first
/// occurrence, or `None`.
fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    // PORT NOTE: the reference tests do `var h: Handler = .{}` then pass
    // `testing.allocator` to `hook`. We use `Handler::<Global>::default()` and pass
    // `Global` (the faithful map of the prior `std::alloc`/testing allocator path).

    #[test]
    fn unknown_dcs_command() {
        // const testing = std.testing;
        // const alloc = testing.allocator;

        let mut h: Handler<Global> = Handler::default();
        // defer h.deinit();
        assert!(h
            .hook(
                Global,
                DCS {
                    final_: b'A',
                    ..Default::default()
                }
            )
            .is_none());
        assert!(matches!(h.state, State::Ignore));
        assert!(h.unhook().is_none());
        assert!(matches!(h.state, State::Inactive));
        h.deinit();
    }

    #[test]
    fn xtgettcap_command() {
        let mut h: Handler<Global> = Handler::default();
        assert!(h
            .hook(
                Global,
                DCS {
                    intermediates: b"+",
                    final_: b'q',
                    ..Default::default()
                }
            )
            .is_none());
        for byte in *b"536D756C78" {
            let _ = h.put(byte);
        }
        let mut cmd = h.unhook().unwrap();
        assert!(matches!(cmd, Command::Xtgettcap(_)));
        if let Command::Xtgettcap(xt) = &mut cmd {
            assert_eq!(b"536D756C78".as_slice(), xt.next().unwrap());
            assert!(xt.next().is_none());
        }
        cmd.deinit();
        h.deinit();
    }

    #[test]
    fn xtgettcap_mixed_case() {
        let mut h: Handler<Global> = Handler::default();
        assert!(h
            .hook(
                Global,
                DCS {
                    intermediates: b"+",
                    final_: b'q',
                    ..Default::default()
                }
            )
            .is_none());
        for byte in *b"536d756C78" {
            let _ = h.put(byte);
        }
        let mut cmd = h.unhook().unwrap();
        assert!(matches!(cmd, Command::Xtgettcap(_)));
        if let Command::Xtgettcap(xt) = &mut cmd {
            assert_eq!(b"536D756C78".as_slice(), xt.next().unwrap());
            assert!(xt.next().is_none());
        }
        cmd.deinit();
        h.deinit();
    }

    #[test]
    fn xtgettcap_command_multiple_keys() {
        let mut h: Handler<Global> = Handler::default();
        assert!(h
            .hook(
                Global,
                DCS {
                    intermediates: b"+",
                    final_: b'q',
                    ..Default::default()
                }
            )
            .is_none());
        for byte in *b"536D756C78;536D756C78" {
            let _ = h.put(byte);
        }
        let mut cmd = h.unhook().unwrap();
        assert!(matches!(cmd, Command::Xtgettcap(_)));
        if let Command::Xtgettcap(xt) = &mut cmd {
            assert_eq!(b"536D756C78".as_slice(), xt.next().unwrap());
            assert_eq!(b"536D756C78".as_slice(), xt.next().unwrap());
            assert!(xt.next().is_none());
        }
        cmd.deinit();
        h.deinit();
    }

    #[test]
    fn xtgettcap_command_invalid_data() {
        let mut h: Handler<Global> = Handler::default();
        assert!(h
            .hook(
                Global,
                DCS {
                    intermediates: b"+",
                    final_: b'q',
                    ..Default::default()
                }
            )
            .is_none());
        for byte in *b"who;536D756C78" {
            let _ = h.put(byte);
        }
        let mut cmd = h.unhook().unwrap();
        assert!(matches!(cmd, Command::Xtgettcap(_)));
        if let Command::Xtgettcap(xt) = &mut cmd {
            assert_eq!(b"WHO".as_slice(), xt.next().unwrap());
            assert_eq!(b"536D756C78".as_slice(), xt.next().unwrap());
            assert!(xt.next().is_none());
        }
        cmd.deinit();
        h.deinit();
    }

    #[test]
    fn decrqss_command() {
        let mut h: Handler<Global> = Handler::default();
        assert!(h
            .hook(
                Global,
                DCS {
                    intermediates: b"$",
                    final_: b'q',
                    ..Default::default()
                }
            )
            .is_none());
        let _ = h.put(b'm');
        let mut cmd = h.unhook().unwrap();
        assert!(matches!(cmd, Command::Decrqss(_)));
        assert!(matches!(cmd, Command::Decrqss(Decrqss::Sgr)));
        cmd.deinit();
        h.deinit();
    }

    #[test]
    fn decrqss_invalid_command() {
        let mut h: Handler<Global> = Handler::default();
        assert!(h
            .hook(
                Global,
                DCS {
                    intermediates: b"$",
                    final_: b'q',
                    ..Default::default()
                }
            )
            .is_none());
        let _ = h.put(b'z');
        let mut cmd = h.unhook().unwrap();
        assert!(matches!(cmd, Command::Decrqss(_)));
        assert!(matches!(cmd, Command::Decrqss(Decrqss::None)));
        cmd.deinit();

        h.discard();

        assert!(h
            .hook(
                Global,
                DCS {
                    intermediates: b"$",
                    final_: b'q',
                    ..Default::default()
                }
            )
            .is_none());
        let _ = h.put(b'"');
        let _ = h.put(b' ');
        let _ = h.put(b'q');
        assert!(h.unhook().is_none());
        h.deinit();
    }

    // PORT NOTE: `Terminal::init` in this crate has no field defaults, so the
    // two DECRQSS encoding tests build their fixture through this helper
    // (mirroring `terminal.rs`'s test `opts`).
    fn decrqss_test_terminal() -> crate::terminal::Terminal {
        crate::terminal::Terminal::init(
            Global,
            crate::terminal::Options {
                cols: 80,
                rows: 24,
                max_scrollback_bytes: Some(10_000),
                max_scrollback_lines: None,
                colors: crate::terminal::Colors::default(),
                default_modes: crate::modes::ModePacked::default(),
                default_cursor_style: crate::screen::CursorStyle::Block,
                default_cursor_blink: Some(false),
                kitty_image_storage_limit: 0,
                kitty_image_loading_limits: crate::kitty::graphics_image::Limits::DIRECT,
            },
        )
        .unwrap()
    }

    // test "DECRQSS response encoding"
    #[test]
    fn decrqss_response_encoding() {
        let mut t = decrqss_test_terminal();

        fn expect_response(
            term: &mut crate::terminal::Terminal,
            request: Decrqss,
            expected: &[u8],
        ) {
            let mut buf = [0u8; Decrqss::MAX_RESPONSE_BYTES];
            let encoded = request.encode(term, &mut buf).unwrap();
            assert_eq!(expected, encoded);
        }

        expect_response(&mut t, Decrqss::None, b"\x1BP0$r\x1B\\");
        expect_response(&mut t, Decrqss::Sgr, b"\x1BP1$r0m\x1B\\");

        t.set_attribute(crate::sgr::Attribute::Bold).unwrap();
        t.set_attribute(crate::sgr::Attribute::Underline(
            crate::sgr::Underline::Curly,
        ))
        .unwrap();
        expect_response(&mut t, Decrqss::Sgr, b"\x1BP1$r0;1;4:3m\x1B\\");

        t.set_cursor_style(crate::ansi::CursorStyle::SteadyUnderline);
        expect_response(&mut t, Decrqss::Decscusr, b"\x1BP1$r4 q\x1B\\");

        t.scrolling_region.top = 4;
        t.scrolling_region.bottom = 19;
        expect_response(&mut t, Decrqss::Decstbm, b"\x1BP1$r5;20r\x1B\\");

        expect_response(&mut t, Decrqss::Decslrm, b"\x1BP0$r\x1B\\");
        t.modes
            .set(crate::modes::Mode::EnableLeftAndRightMargin, true);
        t.scrolling_region.left = 2;
        t.scrolling_region.right = 69;
        expect_response(&mut t, Decrqss::Decslrm, b"\x1BP1$r3;70s\x1B\\");

        t.deinit(Global);
    }

    // test "DECRQSS largest response fits fixed buffer"
    //
    // NEGATIVE CONTROL (P3-W4 T6 Step 4) — WITH A CORRECTION TO THE PLAN.
    //
    // The plan says "shrink `max_response_bytes` to 255 and it must fail".
    // THAT CONTROL IS VACUOUS AND WAS OBSERVED TO PASS: the largest response
    // this test builds is exactly 63 bytes (upstream's own comment says the
    // comptime max "was around 63 ... this leaves a ton of space"), so 255 is
    // still ~4x the requirement and the encode succeeds unchanged. The real
    // threshold is 63. Executed: `MAX_RESPONSE_BYTES = 255` -> PASSES (vacuous);
    // `MAX_RESPONSE_BYTES = 62` -> FAILS with
    // `called `Result::unwrap()` on an `Err` value: OutOfMemory`.
    //
    // What this test therefore guards is: (a) the exact byte-for-byte encoding
    // of the widest SGR response, and (b) that the encoder REFUSES (rather than
    // truncating) when the buffer cannot hold it.
    #[test]
    fn decrqss_largest_response_fits_fixed_buffer() {
        use crate::sgr::Attribute;
        let mut t = decrqss_test_terminal();

        t.set_attribute(Attribute::Bold).unwrap();
        t.set_attribute(Attribute::Faint).unwrap();
        t.set_attribute(Attribute::Italic).unwrap();
        t.set_attribute(Attribute::Underline(crate::sgr::Underline::Dashed))
            .unwrap();
        t.set_attribute(Attribute::Blink).unwrap();
        t.set_attribute(Attribute::Inverse).unwrap();
        t.set_attribute(Attribute::Invisible).unwrap();
        t.set_attribute(Attribute::Strikethrough).unwrap();
        t.set_attribute(Attribute::Overline).unwrap();
        t.set_attribute(Attribute::DirectColorFg(crate::color::RGB {
            r: 255,
            g: 255,
            b: 255,
        }))
        .unwrap();
        t.set_attribute(Attribute::DirectColorBg(crate::color::RGB {
            r: 255,
            g: 255,
            b: 255,
        }))
        .unwrap();

        let mut buf = [0u8; Decrqss::MAX_RESPONSE_BYTES];
        let encoded = Decrqss::Sgr.encode(&mut t, &mut buf).unwrap();

        let expected: &[u8] =
            b"\x1BP1$r0;1;2;3;4:5;53;5;7;8;9;38:2::255:255:255;48:2::255:255:255m\x1B\\";
        assert_eq!(expected, encoded);
        assert!(encoded.len() <= Decrqss::MAX_RESPONSE_BYTES);

        t.deinit(Global);
    }

    // test "tmux enter and implicit exit"
    #[test]
    fn tmux_enter_and_implicit_exit() {
        // if (comptime !build_options.tmux_control_mode) return error.SkipZigTest;
        //
        // PORT NOTE: under the default Rustty build `tmux_control_mode` is
        // false, so this test is skipped (returns early), faithfully mirroring
        // `error.SkipZigTest`; `--features oniguruma` enables it.
        if !build_options::TMUX_CONTROL_MODE {
            return;
        }

        let mut h: Handler<Global> = Handler::default();

        {
            let mut cmd = h
                .hook(
                    Global,
                    DCS {
                        params: &[1000],
                        final_: b'p',
                        ..Default::default()
                    },
                )
                .unwrap();
            // try testing.expect(cmd == .tmux);
            assert!(matches!(cmd, Command::Tmux(_)));
            // try testing.expect(cmd.tmux == .enter);
            assert!(matches!(cmd, Command::Tmux(TmuxCommand::Enter)));
            cmd.deinit();
        }

        {
            let mut cmd = h.unhook().unwrap();
            // try testing.expect(cmd == .tmux);
            assert!(matches!(cmd, Command::Tmux(_)));
            // try testing.expect(cmd.tmux == .exit);
            assert!(matches!(cmd, Command::Tmux(TmuxCommand::Exit)));
            cmd.deinit();
        }

        h.deinit();
    }
}
