//! VT-series parser for escape and control sequences.
//!
//! This is implemented directly as the state machine described on
//! vt100.net: https://vt100.net/emu/dec_ansi_parser
//!
//! PORTS: ghostty src/terminal/Parser.zig
//!
//! PORT STATUS: done

// PARITY: `Parser::next`/`osc::Parser::next` are named after the reference
// `Parser.next`, not the `Iterator::next` trait.
#![allow(clippy::should_implement_trait)]

// const Parser = @This();
//
// const std = @import("std");
// const testing = std.testing;
// const table = @import("parse_table.zig").table;
// const osc = @import("osc.zig");
// PORT NOTE (R2): Zig imports retargeted to ported paths. `table` is the
// transition table from `parse_table`; `osc` is the ported OSC module whose
// `Parser` is embedded below.
use crate::osc;
use crate::parse_table::TABLE as table;

// const log = std.log.scoped(.parser);
// PORT NOTE: rustty has no logging facade wired into this crate (matching the
// convention used by PageList / DCS / OSC et al.), so the scoped logger and its
// `log.warn(...)` calls are ported as commented lines.

/// States for the state machine
// PORT NOTE: the explicit discriminants pin the declaration-order values that
// the reference relies on via `@intFromEnum` (used to index the transition
// table). `ALL` and `COUNT` reproduce `@typeInfo(State).@"enum".fields`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Ground = 0,
    Escape = 1,
    EscapeIntermediate = 2,
    CsiEntry = 3,
    CsiIntermediate = 4,
    CsiParam = 5,
    CsiIgnore = 6,
    DcsEntry = 7,
    DcsParam = 8,
    DcsIntermediate = 9,
    DcsPassthrough = 10,
    DcsIgnore = 11,
    OscString = 12,
    SosPmApcString = 13,
}

impl State {
    /// PORT NOTE: `@typeInfo(State).@"enum".fields` in declaration order.
    pub const ALL: [State; 14] = [
        State::Ground,
        State::Escape,
        State::EscapeIntermediate,
        State::CsiEntry,
        State::CsiIntermediate,
        State::CsiParam,
        State::CsiIgnore,
        State::DcsEntry,
        State::DcsParam,
        State::DcsIntermediate,
        State::DcsPassthrough,
        State::DcsIgnore,
        State::OscString,
        State::SosPmApcString,
    ];

    /// PORT NOTE: `@typeInfo(State).@"enum".fields.len`.
    pub const COUNT: usize = State::ALL.len();
}

/// Transition action is an action that can be taken during a state
/// transition. This is more of an internal action, not one used by
/// end users, typically.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransitionAction {
    None,
    Ignore,
    Print,
    Execute,
    Collect,
    Param,
    EscDispatch,
    CsiDispatch,
    Put,
    OscPut,
    ApcPut,
}

/// Action is the action that a caller of the parser is expected to
/// take as a result of some input character.
// PORT NOTE: the reference is `union(enum)`. Ported as a Rust enum.
//
// The reference's CSI/ESC/DCS payloads carry `[]u8`/`[]u16` slices that point
// directly into the parser's `intermediates`/`params` buffers, with the caveat
// (verbatim below) that "pointers within this structure are only valid until
// the next call to next". In Zig that slice aliasing is invisible to the type
// system; in Rust returning such slices from `&mut self` would tie the whole
// `&mut self` borrow to the returned actions, so a caller could not even read
// the `Copy` `state` field (as the reference tests do) while holding them. To
// preserve the reference's observable behavior — and the assertion-for-assertion
// tests verbatim — the payloads OWN their captured bytes (small `Vec`s holding
// at most `MAX_INTERMEDIATE`/`MAX_PARAMS` elements). This is a strictly stronger
// guarantee than the reference (the data outlives the next `next` call) and is
// observationally identical for every assertion.
//
// The `Action.Tag` (`std.meta.FieldEnum`) reflection alias and the `format`
// logging implementations are observability-only helpers with no behavioral
// effect; they are ported as commented references below.
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    // pub const Tag = std.meta.FieldEnum(Action);
    /// Draw character to the screen. This is a unicode codepoint.
    // print: u21,
    Print(u32),

    /// Execute the C0 or C1 function.
    // execute: u8,
    Execute(u8),

    /// Execute the CSI command. Note that pointers within this
    /// structure are only valid until the next call to "next".
    // csi_dispatch: CSI,
    CsiDispatch(CSI),

    /// Execute the ESC command.
    // esc_dispatch: ESC,
    EscDispatch(ESC),

    /// Execute the OSC command.
    // osc_dispatch: osc.Command,
    OscDispatch(osc::Command),

    /// DCS-related events.
    // dcs_hook: DCS,
    DcsHook(DCS),
    // dcs_put: u8,
    DcsPut(u8),
    // dcs_unhook: void,
    DcsUnhook,

    /// APC data
    // apc_start: void,
    ApcStart,
    // apc_put: u8,
    ApcPut(u8),
    // apc_end: void,
    ApcEnd,
}

// PORT NOTE: `Action.CSI` is a nested struct in the reference; hoisted to a
// top-level struct (Rust has no nested type declarations). `intermediates` /
// `params` are owned (see the Action PORT NOTE) rather than slices into the
// parser.
#[derive(Debug, Clone, PartialEq)]
pub struct CSI {
    pub intermediates: Vec<u8>,
    pub params: Vec<u16>,
    pub params_sep: SepList,
    pub final_: u8,
}

// The list of separators used for CSI params. The value of the
// bit can be mapped to Sep. The index of this bit set specifies
// the separator AFTER that param. For example: 0;4:3 would have
// index 1 set.
// pub const SepList = std.StaticBitSet(MAX_PARAMS);
// PORT NOTE: defined as the crate-level `SepList` type below so other modules
// (e.g. sgr) reference the same `Parser.Action.CSI.SepList`.
//
// The separator used for CSI params.
// pub const Sep = enum(u1) { semicolon = 0, colon = 1 };
// PORT NOTE: `Sep` is hoisted to the top-level `Sep` enum below.
//
// Implement formatter for logging
// pub fn format(self: CSI, writer: *std.Io.Writer) !void {
//     try writer.print("ESC [ {s} {any} {c}", .{
//         self.intermediates, self.params, self.final,
//     });
// }
// PORT NOTE: logging-only formatter; no behavioral effect.

/// The separator used for CSI params.
// PORT NOTE: hoisted from `Action.CSI.Sep` (`enum(u1)`).
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sep {
    Semicolon = 0,
    Colon = 1,
}

// PORT NOTE: `Action.ESC` nested struct, hoisted to top-level. `intermediates`
// is owned (see the Action PORT NOTE).
#[derive(Debug, Clone, PartialEq)]
pub struct ESC {
    pub intermediates: Vec<u8>,
    pub final_: u8,
    // Implement formatter for logging
    // pub fn format(self: ESC, writer: *std.Io.Writer) !void {
    //     try writer.print("ESC {s} {c}", .{ self.intermediates, self.final });
    // }
    // PORT NOTE: logging-only formatter; no behavioral effect.
}

// PORT NOTE: `Action.DCS` nested struct, hoisted to top-level. The reference
// defaults `intermediates = ""` and `params = &.{}`; the owned `Vec`s are empty
// by default at the call site. The `DCS.C` extern struct is the C ABI mirror,
// kept as a commented reference: it is DEFERRED until a C stream/DCS surface is
// ported — the reference itself has no C-ABI consumer of `Action.DCS.C` (no
// `@export` path uses it; `include/ghostty/vt/*.h` define no DCS type;
// `lib_vt.zig:63` is a Zig-module type export, not a C export), so rustty-vt-c
// intentionally has no DCS mirror today (LIFETIMES audit 2026-07-10).
#[derive(Debug, Clone, PartialEq)]
pub struct DCS {
    pub intermediates: Vec<u8>,
    pub params: Vec<u16>,
    pub final_: u8,
    // pub const C = extern struct {
    //     intermediates: [*]const u8,
    //     intermediates_len: usize,
    //     params: [*]const u16,
    //     params_len: usize,
    //     final: u8,
    // };
}

// Implement formatter for logging. This is mostly copied from the
// std.fmt implementation, but we modify it slightly so that we can
// print out custom formats for some of our primitives.
// pub fn format(
//     self: Action,
//     writer: *std.Io.Writer,
// ) !void {
//     const T = Action;
//     const info = @typeInfo(T).@"union";
//
//     try writer.writeAll(@typeName(T));
//     if (info.tag_type) |TagType| {
//         try writer.writeAll("{ .");
//         try writer.writeAll(@tagName(@as(TagType, self)));
//         try writer.writeAll(" = ");
//
//         inline for (info.fields) |u_field| {
//             // If this is the active field...
//             if (self == @field(TagType, u_field.name)) {
//                 const value = @field(self, u_field.name);
//                 switch (@TypeOf(value)) {
//                     // Unicode
//                     u21 => try writer.print("'{u}' (U+{X})", .{ value, value }),
//
//                     // Byte
//                     u8 => try writer.print("0x{x}", .{value}),
//
//                     // Note: we don't do ASCII (u8) because there are a lot
//                     // of invisible characters we don't want to handle right
//                     // now.
//
//                     // All others do the default behavior
//                     else => try writer.printValue(
//                         "any",
//                         .{},
//                         @field(self, u_field.name),
//                         3,
//                     ),
//                 }
//             }
//         }
//
//         try writer.writeAll(" }");
//     } else {
//         try format(writer, "@{x}", .{@intFromPtr(&self)});
//     }
// }
// PORT NOTE: the `Action.format` logging implementation (a copy of the
// `std.fmt` union formatter with custom printing for `u21`/`u8`) is
// observability-only and has no behavioral effect; it is kept verbatim above
// (the Rust `#[derive(Debug)]` covers the diagnostic need), matching the
// convention used for the `CSI`/`ESC`/`DCS` format fns in this file.

/// The list of separators used for CSI params. The value of the
/// bit can be mapped to Sep. The index of this bit set specifies
/// the separator AFTER that param.
// PORT NOTE: `Action.CSI.SepList == std.StaticBitSet(MAX_PARAMS)`. Rust has no
// std StaticBitSet, so a compact fixed-size bitset carrying exactly the
// operations the parser uses (`initEmpty`/`set`/`isSet`/`count`). This is the
// canonical `Parser.Action.CSI.SepList`; the `sgr` module keeps a structurally
// identical local seam until it is retargeted to re-export this type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SepList {
    bits: u32,
}

impl SepList {
    /// `initEmpty`
    pub const fn init_empty() -> SepList {
        SepList { bits: 0 }
    }

    /// `set`
    pub fn set(&mut self, idx: usize) {
        debug_assert!(idx < MAX_PARAMS);
        self.bits |= 1u32 << idx;
    }

    /// `isSet`
    pub fn is_set(&self, idx: usize) -> bool {
        if idx >= MAX_PARAMS {
            return false;
        }
        (self.bits >> idx) & 1 == 1
    }

    /// `count`
    pub fn count(&self) -> usize {
        self.bits.count_ones() as usize
    }
}

impl Default for SepList {
    fn default() -> Self {
        SepList::init_empty()
    }
}

/// Maximum number of intermediate characters during parsing. This is
/// 4 because we also use the intermediates array for UTF8 decoding which
/// can be at most 4 bytes.
pub const MAX_INTERMEDIATE: usize = 4;

/// Maximum number of CSI parameters. This is arbitrary. Practically, the
/// only CSI command that uses more than 3 parameters is the SGR command
/// which can be infinitely long. 24 is a reasonable limit based on empirical
/// data. This used to be 16 but Kakoune has a SGR command that uses 17
/// parameters.
///
/// We could in the future make this the static limit and then allocate after
/// but that's a lot more work and practically its so rare to exceed this
/// number. I implore TUI authors to not use more than this number of CSI
/// params, but I suspect we'll introduce a slow path with heap allocation
/// one day.
pub const MAX_PARAMS: usize = 24;

/// VT-series parser for escape and control sequences.
// PORT NOTE: the reference is `const Parser = @This()` — the file *is* the
// struct. Ported as a named `Parser` struct carrying the same fields. The
// embedded `osc.Parser` carries the OSC allocator lifetime `'a`.
pub struct Parser<'a> {
    /// Current state of the state machine
    pub state: State,

    /// Intermediate tracking.
    pub intermediates: [u8; MAX_INTERMEDIATE],
    pub intermediates_idx: u8,

    /// Param tracking, building
    pub params: [u16; MAX_PARAMS],
    pub params_sep: SepList,
    pub params_idx: u8,
    pub param_acc: u16,
    pub param_acc_idx: u8,

    /// Parser for OSC sequences
    pub osc_parser: osc::Parser<'a>,
}

// PORT NOTE: internal record of which transition/exit/entry action to emit.
// Because the user-facing `Action` borrows the parser's buffers, `next` first
// performs every mutation (capturing only owned data here), then materializes
// the borrowing `Action` values from the now-stable buffers. This staging adds
// no reference logic — it is purely the Rust borrow-discipline mirror of the
// reference's `defer self.state = next_state` followed by slice-returning
// switch arms.
enum Staged {
    None,
    Print(u32),
    Execute(u8),
    CsiDispatch(u8),
    EscDispatch(u8),
    OscDispatch(osc::Command),
    DcsHook(u8),
    DcsPut(u8),
    DcsUnhook,
    ApcStart,
    ApcPut(u8),
    ApcEnd,
}

impl<'a> Parser<'a> {
    // pub fn init() Parser {
    pub fn init() -> Parser<'a> {
        // var result: Parser = .{ ... };
        // PORT NOTE: the reference's `intermediates`/`params` are left
        // `undefined` (and only reset under Valgrind via the runningOnValgrind
        // guard, a no-op observability hook). Rust requires initialization, so
        // they are zero-filled; the parser never reads beyond the tracked
        // indices, so this is observationally identical.
        Parser {
            state: State::Ground,
            intermediates_idx: 0,
            params_sep: SepList::init_empty(),
            params_idx: 0,
            param_acc: 0,
            param_acc_idx: 0,
            osc_parser: osc::Parser::init(None),

            intermediates: [0; MAX_INTERMEDIATE],
            params: [0; MAX_PARAMS],
        }
        // if (std.valgrind.runningOnValgrind() > 0) {
        //     result.intermediates = undefined;
        //     result.params = undefined;
        // }
        // return result;
    }

    // pub fn deinit(self: *Parser) void {
    pub fn deinit(&mut self) {
        self.osc_parser.deinit();
    }

    /// Next consumes the next character c and returns the actions to execute.
    /// Up to 3 actions may need to be executed -- in order -- representing
    /// the state exit, transition, and entry actions.
    // pub fn next(self: *Parser, c: u8) [3]?Action {
    pub fn next(&mut self, c: u8) -> [Option<Action>; 3] {
        let effect = table[c as usize][self.state as usize];

        // log.info("next: {x}", .{c});

        let next_state = effect.state;
        let action = effect.action;

        // After generating the actions, we set our next state.
        // defer self.state = next_state;
        // PORT NOTE: Zig's `defer` runs after the return expression is
        // evaluated. Because the returned `Action`s borrow `self`'s buffers, we
        // cannot hold those borrows across the `self.state` write. We instead
        // stage every action (performing all buffer/osc mutations now), set the
        // state, and then re-derive the borrowing `Action`s. The observable
        // order of side effects is identical.
        let cur_state = self.state;

        // When going from one state to another, the actions take place in this order:
        //
        // 1. exit action from old state
        // 2. transition action
        // 3. entry action to new state

        // Exit depends on current state
        let exit: Staged = if cur_state == next_state {
            Staged::None
        } else {
            match cur_state {
                // .osc_string => if (self.osc_parser.end(c)) |cmd|
                //     Action{ .osc_dispatch = cmd.* } else null,
                State::OscString => match self.osc_parser.end(Some(c)) {
                    Some(cmd) => Staged::OscDispatch(cmd.clone()),
                    None => Staged::None,
                },
                // .dcs_passthrough => Action{ .dcs_unhook = {} },
                State::DcsPassthrough => Staged::DcsUnhook,
                // .sos_pm_apc_string => Action{ .apc_end = {} },
                State::SosPmApcString => Staged::ApcEnd,
                // else => null,
                _ => Staged::None,
            }
        };

        // self.doAction(action, c),
        let transition = self.do_action(action, c);

        // Entry depends on new state
        let entry: Staged = if cur_state == next_state {
            Staged::None
        } else {
            match next_state {
                // .escape, .dcs_entry, .csi_entry => clear: { self.clear(); break :clear null; },
                State::Escape | State::DcsEntry | State::CsiEntry => {
                    self.clear();
                    Staged::None
                }
                // .osc_string => osc_string: { self.osc_parser.reset(); break :osc_string null; },
                State::OscString => {
                    self.osc_parser.reset();
                    Staged::None
                }
                // .dcs_passthrough => dcs_hook: { ... },
                State::DcsPassthrough => {
                    // Ignore too many parameters
                    if self.params_idx as usize >= MAX_PARAMS {
                        // break :dcs_hook null;
                        Staged::None
                    } else {
                        // Finalize parameters
                        if self.param_acc_idx > 0 {
                            self.params[self.params_idx as usize] = self.param_acc;
                            self.params_idx += 1;
                        }
                        // break :dcs_hook .{ .dcs_hook = .{ ... .final = c } };
                        Staged::DcsHook(c)
                    }
                }
                // .sos_pm_apc_string => Action{ .apc_start = {} },
                State::SosPmApcString => Staged::ApcStart,
                // else => null,
                _ => Staged::None,
            }
        };

        // Apply the deferred state transition now that all mutations are done.
        self.state = next_state;

        // Materialize the actions from the stable buffers. The reference
        // returns slices into `self.intermediates`/`self.params` ("only valid
        // until the next call to next"); we copy the tracked prefix into owned
        // `Vec`s (see the Action PORT NOTE).
        let intermediates = &self.intermediates[0..self.intermediates_idx as usize];
        let params = &self.params[0..self.params_idx as usize];
        let params_sep = self.params_sep;

        [
            Self::stage_to_action(exit, intermediates, params, params_sep),
            Self::stage_to_action(transition, intermediates, params, params_sep),
            Self::stage_to_action(entry, intermediates, params, params_sep),
        ]
    }

    // PORT NOTE: turns a staged action into the user-facing `Action`, copying
    // the (now-stable) intermediate/param prefixes into owned `Vec`s. The
    // `final` byte for CSI/ESC/DCS dispatch is captured in the staged variant.
    fn stage_to_action(
        staged: Staged,
        intermediates: &[u8],
        params: &[u16],
        params_sep: SepList,
    ) -> Option<Action> {
        match staged {
            Staged::None => None,
            Staged::Print(c) => Some(Action::Print(c)),
            Staged::Execute(c) => Some(Action::Execute(c)),
            Staged::CsiDispatch(c) => Some(Action::CsiDispatch(CSI {
                intermediates: intermediates.to_vec(),
                params: params.to_vec(),
                params_sep,
                final_: c,
            })),
            Staged::EscDispatch(c) => Some(Action::EscDispatch(ESC {
                intermediates: intermediates.to_vec(),
                final_: c,
            })),
            Staged::OscDispatch(cmd) => Some(Action::OscDispatch(cmd)),
            Staged::DcsHook(c) => Some(Action::DcsHook(DCS {
                intermediates: intermediates.to_vec(),
                params: params.to_vec(),
                final_: c,
            })),
            Staged::DcsPut(c) => Some(Action::DcsPut(c)),
            Staged::DcsUnhook => Some(Action::DcsUnhook),
            Staged::ApcStart => Some(Action::ApcStart),
            Staged::ApcPut(c) => Some(Action::ApcPut(c)),
            Staged::ApcEnd => Some(Action::ApcEnd),
        }
    }

    // pub inline fn collect(self: *Parser, c: u8) void {
    pub fn collect(&mut self, c: u8) {
        if self.intermediates_idx as usize >= MAX_INTERMEDIATE {
            // @branchHint(.cold);
            // log.warn("invalid intermediates count", .{});
            return;
        }

        self.intermediates[self.intermediates_idx as usize] = c;
        self.intermediates_idx += 1;
    }

    // inline fn doAction(self: *Parser, action: TransitionAction, c: u8) ?Action {
    fn do_action(&mut self, action: TransitionAction, c: u8) -> Staged {
        match action {
            // .none, .ignore => null,
            TransitionAction::None | TransitionAction::Ignore => Staged::None,
            // .print => Action{ .print = c },
            TransitionAction::Print => Staged::Print(c as u32),
            // .execute => Action{ .execute = c },
            TransitionAction::Execute => Staged::Execute(c),
            // .collect => collect: { self.collect(c); break :collect null; },
            TransitionAction::Collect => {
                self.collect(c);
                Staged::None
            }
            // .param => param: { ... },
            TransitionAction::Param => {
                // Semicolon separates parameters. If we encounter a semicolon
                // we need to store and move on to the next parameter.
                if c == b';' || c == b':' {
                    // Ignore too many parameters
                    if self.params_idx as usize >= MAX_PARAMS {
                        // break :param null;
                        return Staged::None;
                    }

                    // Set param final value
                    self.params[self.params_idx as usize] = self.param_acc;
                    if c == b':' {
                        self.params_sep.set(self.params_idx as usize);
                    }
                    self.params_idx += 1;

                    // Reset current param value to 0
                    self.param_acc = 0;
                    self.param_acc_idx = 0;
                    // break :param null;
                    return Staged::None;
                }

                // A numeric value. Add it to our accumulator.
                // self.param_acc *|= 10;
                self.param_acc = self.param_acc.saturating_mul(10);
                // self.param_acc +|= c - '0';
                self.param_acc = self.param_acc.saturating_add((c - b'0') as u16);

                // Increment our accumulator index. If we overflow then
                // we're out of bounds and we exit immediately.
                // self.param_acc_idx, const overflow = @addWithOverflow(self.param_acc_idx, 1);
                // if (overflow > 0) break :param null;
                let (acc_idx, overflow) = self.param_acc_idx.overflowing_add(1);
                self.param_acc_idx = acc_idx;
                if overflow {
                    return Staged::None;
                }

                // The client is expected to perform no action.
                // break :param null;
                Staged::None
            }
            // .osc_put => osc_put: { osc.Parser.next(&self.osc_parser, c); break :osc_put null; },
            TransitionAction::OscPut => {
                self.osc_parser.next(c);
                Staged::None
            }
            // .csi_dispatch => csi_dispatch: { ... },
            TransitionAction::CsiDispatch => {
                // Ignore too many parameters
                if self.params_idx as usize >= MAX_PARAMS {
                    // break :csi_dispatch null;
                    return Staged::None;
                }

                // Finalize parameters if we have one
                if self.param_acc_idx > 0 {
                    self.params[self.params_idx as usize] = self.param_acc;
                    self.params_idx += 1;
                }

                // const result: Action = .{ .csi_dispatch = .{ ... .final = c } };
                // PORT NOTE: the reference builds the full `result` here so it
                // can both log it and return it. We stash the `final` byte in
                // the staged variant and build the borrowing CSI in
                // `stage_to_action`; the separator gate below reads
                // `self.params_sep` directly.

                // We only allow colon or mixed separators for the 'm' command.
                if c != b'm' && self.params_sep.count() > 0 {
                    // @branchHint(.cold);
                    // log.warn(
                    //     "CSI colon or mixed separators only allowed for 'm' command, got: {f}",
                    //     .{result},
                    // );
                    // break :csi_dispatch null;
                    return Staged::None;
                }

                // break :csi_dispatch result;
                Staged::CsiDispatch(c)
            }
            // .esc_dispatch => Action{ .esc_dispatch = .{ ... .final = c } },
            TransitionAction::EscDispatch => Staged::EscDispatch(c),
            // .put => Action{ .dcs_put = c },
            TransitionAction::Put => Staged::DcsPut(c),
            // .apc_put => Action{ .apc_put = c },
            TransitionAction::ApcPut => Staged::ApcPut(c),
        }
    }

    // pub inline fn clear(self: *Parser) void {
    pub fn clear(&mut self) {
        self.intermediates_idx = 0;
        self.params_idx = 0;
        self.params_sep = SepList::init_empty();
        self.param_acc = 0;
        self.param_acc_idx = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::osc;
    use rustty_lib::allocator::{GlobalZigAllocator, ZigAllocatorRef};

    // test {
    #[test]
    fn basic_apc_print_execute() {
        let mut p = Parser::init();
        let _ = p.next(0x9E);
        assert!(p.state == State::SosPmApcString);
        let _ = p.next(0x9C);
        assert!(p.state == State::Ground);

        {
            let a = p.next(b'a');
            assert!(p.state == State::Ground);
            assert!(a[0].is_none());
            assert!(matches!(a[1].as_ref().unwrap(), Action::Print(_)));
            assert!(a[2].is_none());
        }

        {
            let a = p.next(0x19);
            assert!(p.state == State::Ground);
            assert!(a[0].is_none());
            assert!(matches!(a[1].as_ref().unwrap(), Action::Execute(_)));
            assert!(a[2].is_none());
        }
    }

    // test "esc: ESC ( B" {
    #[test]
    fn esc_esc_paren_b() {
        let mut p = Parser::init();
        let _ = p.next(0x1B);
        let _ = p.next(b'(');

        {
            let a = p.next(b'B');
            assert!(p.state == State::Ground);
            assert!(a[0].is_none());
            assert!(matches!(a[1].as_ref().unwrap(), Action::EscDispatch(_)));
            assert!(a[2].is_none());

            let d = match a[1].as_ref().unwrap() {
                Action::EscDispatch(d) => d,
                _ => unreachable!(),
            };
            assert!(d.final_ == b'B');
            assert!(d.intermediates.len() == 1);
            assert!(d.intermediates[0] == b'(');
        }
    }

    // test "csi: ESC [ H" {
    #[test]
    fn csi_esc_bracket_h() {
        let mut p = Parser::init();
        let _ = p.next(0x1B);
        let _ = p.next(0x5B);

        {
            let a = p.next(0x48);
            assert!(p.state == State::Ground);
            assert!(a[0].is_none());
            assert!(matches!(a[1].as_ref().unwrap(), Action::CsiDispatch(_)));
            assert!(a[2].is_none());

            let d = match a[1].as_ref().unwrap() {
                Action::CsiDispatch(d) => d,
                _ => unreachable!(),
            };
            assert!(d.final_ == 0x48);
            assert!(d.params.is_empty());
        }
    }

    // test "csi: ESC [ 1 ; 4 H" {
    #[test]
    fn csi_esc_bracket_1_4_h() {
        let mut p = Parser::init();
        let _ = p.next(0x1B);
        let _ = p.next(0x5B);
        let _ = p.next(0x31); // 1
        let _ = p.next(0x3B); // ;
        let _ = p.next(0x34); // 4

        {
            let a = p.next(0x48); // H
            assert!(p.state == State::Ground);
            assert!(a[0].is_none());
            assert!(matches!(a[1].as_ref().unwrap(), Action::CsiDispatch(_)));
            assert!(a[2].is_none());

            let d = match a[1].as_ref().unwrap() {
                Action::CsiDispatch(d) => d,
                _ => unreachable!(),
            };
            assert!(d.final_ == b'H');
            assert!(d.params.len() == 2);
            assert_eq!(1u16, d.params[0]);
            assert_eq!(4u16, d.params[1]);
        }
    }

    // test "csi: SGR ESC [ 38 : 2 m" {
    #[test]
    fn csi_sgr_esc_38_2_m() {
        let mut p = Parser::init();
        let _ = p.next(0x1B);
        let _ = p.next(b'[');
        let _ = p.next(b'3');
        let _ = p.next(b'8');
        let _ = p.next(b':');
        let _ = p.next(b'2');

        {
            let a = p.next(b'm');
            assert!(p.state == State::Ground);
            assert!(a[0].is_none());
            assert!(matches!(a[1].as_ref().unwrap(), Action::CsiDispatch(_)));
            assert!(a[2].is_none());

            let d = match a[1].as_ref().unwrap() {
                Action::CsiDispatch(d) => d,
                _ => unreachable!(),
            };
            assert!(d.final_ == b'm');
            assert!(d.params.len() == 2);
            assert_eq!(38u16, d.params[0]);
            assert!(d.params_sep.is_set(0));
            assert_eq!(2u16, d.params[1]);
            assert!(!d.params_sep.is_set(1));
        }
    }

    // test "csi: SGR colon followed by semicolon" {
    #[test]
    fn csi_sgr_colon_followed_by_semicolon() {
        let mut p = Parser::init();
        let _ = p.next(0x1B);
        for c in *b"[48:2" {
            let a = p.next(c);
            assert!(a[0].is_none());
            assert!(a[1].is_none());
            assert!(a[2].is_none());
        }

        {
            let a = p.next(b'm');
            assert!(p.state == State::Ground);
            assert!(a[0].is_none());
            assert!(matches!(a[1].as_ref().unwrap(), Action::CsiDispatch(_)));
            assert!(a[2].is_none());
        }

        let _ = p.next(0x1B);
        let _ = p.next(b'[');
        {
            let a = p.next(b'H');
            assert!(p.state == State::Ground);
            assert!(a[0].is_none());
            assert!(matches!(a[1].as_ref().unwrap(), Action::CsiDispatch(_)));
            assert!(a[2].is_none());
        }
    }

    // test "csi: SGR mixed colon and semicolon" {
    #[test]
    fn csi_sgr_mixed_colon_and_semicolon() {
        let mut p = Parser::init();
        let _ = p.next(0x1B);
        for c in *b"[38:5:1;48:5:0" {
            let a = p.next(c);
            assert!(a[0].is_none());
            assert!(a[1].is_none());
            assert!(a[2].is_none());
        }

        {
            let a = p.next(b'm');
            assert!(p.state == State::Ground);
            assert!(a[0].is_none());
            assert!(matches!(a[1].as_ref().unwrap(), Action::CsiDispatch(_)));
            assert!(a[2].is_none());
        }
    }

    // test "csi: SGR ESC [ 48 : 2 m" {
    #[test]
    fn csi_sgr_esc_48_2_m() {
        let mut p = Parser::init();
        let _ = p.next(0x1B);
        for c in *b"[48:2:240:143:104" {
            let a = p.next(c);
            assert!(a[0].is_none());
            assert!(a[1].is_none());
            assert!(a[2].is_none());
        }

        {
            let a = p.next(b'm');
            assert!(p.state == State::Ground);
            assert!(a[0].is_none());
            assert!(matches!(a[1].as_ref().unwrap(), Action::CsiDispatch(_)));
            assert!(a[2].is_none());

            let d = match a[1].as_ref().unwrap() {
                Action::CsiDispatch(d) => d,
                _ => unreachable!(),
            };
            assert!(d.final_ == b'm');
            assert!(d.params.len() == 5);
            assert_eq!(48u16, d.params[0]);
            assert!(d.params_sep.is_set(0));
            assert_eq!(2u16, d.params[1]);
            assert!(d.params_sep.is_set(1));
            assert_eq!(240u16, d.params[2]);
            assert!(d.params_sep.is_set(2));
            assert_eq!(143u16, d.params[3]);
            assert!(d.params_sep.is_set(3));
            assert_eq!(104u16, d.params[4]);
            assert!(!d.params_sep.is_set(4));
        }
    }

    // test "csi: SGR ESC [4:3m colon" {
    #[test]
    fn csi_sgr_esc_4_3_m_colon() {
        let mut p = Parser::init();
        let _ = p.next(0x1B);
        let _ = p.next(b'[');
        let _ = p.next(b'4');
        let _ = p.next(b':');
        let _ = p.next(b'3');

        {
            let a = p.next(b'm');
            assert!(p.state == State::Ground);
            assert!(a[0].is_none());
            assert!(matches!(a[1].as_ref().unwrap(), Action::CsiDispatch(_)));
            assert!(a[2].is_none());

            let d = match a[1].as_ref().unwrap() {
                Action::CsiDispatch(d) => d,
                _ => unreachable!(),
            };
            assert!(d.final_ == b'm');
            assert!(d.params.len() == 2);
            assert_eq!(4u16, d.params[0]);
            assert!(d.params_sep.is_set(0));
            assert_eq!(3u16, d.params[1]);
            assert!(!d.params_sep.is_set(1));
        }
    }

    // test "csi: SGR with many blank and colon" {
    #[test]
    fn csi_sgr_with_many_blank_and_colon() {
        let mut p = Parser::init();
        let _ = p.next(0x1B);
        for c in *b"[58:2::240:143:104" {
            let a = p.next(c);
            assert!(a[0].is_none());
            assert!(a[1].is_none());
            assert!(a[2].is_none());
        }

        {
            let a = p.next(b'm');
            assert!(p.state == State::Ground);
            assert!(a[0].is_none());
            assert!(matches!(a[1].as_ref().unwrap(), Action::CsiDispatch(_)));
            assert!(a[2].is_none());

            let d = match a[1].as_ref().unwrap() {
                Action::CsiDispatch(d) => d,
                _ => unreachable!(),
            };
            assert!(d.final_ == b'm');
            assert!(d.params.len() == 6);
            assert_eq!(58u16, d.params[0]);
            assert!(d.params_sep.is_set(0));
            assert_eq!(2u16, d.params[1]);
            assert!(d.params_sep.is_set(1));
            assert_eq!(0u16, d.params[2]);
            assert!(d.params_sep.is_set(2));
            assert_eq!(240u16, d.params[3]);
            assert!(d.params_sep.is_set(3));
            assert_eq!(143u16, d.params[4]);
            assert!(d.params_sep.is_set(4));
            assert_eq!(104u16, d.params[5]);
            assert!(!d.params_sep.is_set(5));
        }
    }

    // This is from a Kakoune actual SGR sequence.
    // test "csi: SGR mixed colon and semicolon with blank" {
    #[test]
    fn csi_sgr_mixed_colon_and_semicolon_with_blank() {
        let mut p = Parser::init();
        let _ = p.next(0x1B);
        for c in *b"[;4:3;38;2;175;175;215;58:2::190:80:70" {
            let a = p.next(c);
            assert!(a[0].is_none());
            assert!(a[1].is_none());
            assert!(a[2].is_none());
        }

        {
            let a = p.next(b'm');
            assert!(p.state == State::Ground);
            assert!(a[0].is_none());
            assert!(matches!(a[1].as_ref().unwrap(), Action::CsiDispatch(_)));
            assert!(a[2].is_none());

            let d = match a[1].as_ref().unwrap() {
                Action::CsiDispatch(d) => d,
                _ => unreachable!(),
            };
            assert!(d.final_ == b'm');
            assert_eq!(14, d.params.len());
            assert_eq!(0u16, d.params[0]);
            assert!(!d.params_sep.is_set(0));
            assert_eq!(4u16, d.params[1]);
            assert!(d.params_sep.is_set(1));
            assert_eq!(3u16, d.params[2]);
            assert!(!d.params_sep.is_set(2));
            assert_eq!(38u16, d.params[3]);
            assert!(!d.params_sep.is_set(3));
            assert_eq!(2u16, d.params[4]);
            assert!(!d.params_sep.is_set(4));
            assert_eq!(175u16, d.params[5]);
            assert!(!d.params_sep.is_set(5));
            assert_eq!(175u16, d.params[6]);
            assert!(!d.params_sep.is_set(6));
            assert_eq!(215u16, d.params[7]);
            assert!(!d.params_sep.is_set(7));
            assert_eq!(58u16, d.params[8]);
            assert!(d.params_sep.is_set(8));
            assert_eq!(2u16, d.params[9]);
            assert!(d.params_sep.is_set(9));
            assert_eq!(0u16, d.params[10]);
            assert!(d.params_sep.is_set(10));
            assert_eq!(190u16, d.params[11]);
            assert!(d.params_sep.is_set(11));
            assert_eq!(80u16, d.params[12]);
            assert!(d.params_sep.is_set(12));
            assert_eq!(70u16, d.params[13]);
            assert!(!d.params_sep.is_set(13));
        }
    }

    // This is from a Kakoune actual SGR sequence also.
    // test "csi: SGR mixed colon and semicolon setting underline, bg, fg" {
    #[test]
    fn csi_sgr_mixed_colon_and_semicolon_setting_underline_bg_fg() {
        let mut p = Parser::init();
        let _ = p.next(0x1B);
        for c in *b"[4:3;38;2;51;51;51;48;2;170;170;170;58;2;255;97;136" {
            let a = p.next(c);
            assert!(a[0].is_none());
            assert!(a[1].is_none());
            assert!(a[2].is_none());
        }

        {
            let a = p.next(b'm');
            assert!(p.state == State::Ground);
            assert!(a[0].is_none());
            assert!(matches!(a[1].as_ref().unwrap(), Action::CsiDispatch(_)));
            assert!(a[2].is_none());

            let d = match a[1].as_ref().unwrap() {
                Action::CsiDispatch(d) => d,
                _ => unreachable!(),
            };
            assert!(d.final_ == b'm');
            assert_eq!(17, d.params.len());
            assert_eq!(4u16, d.params[0]);
            assert!(d.params_sep.is_set(0));
            assert_eq!(3u16, d.params[1]);
            assert!(!d.params_sep.is_set(1));
            assert_eq!(38u16, d.params[2]);
            assert!(!d.params_sep.is_set(2));
            assert_eq!(2u16, d.params[3]);
            assert!(!d.params_sep.is_set(3));
            assert_eq!(51u16, d.params[4]);
            assert!(!d.params_sep.is_set(4));
            assert_eq!(51u16, d.params[5]);
            assert!(!d.params_sep.is_set(5));
            assert_eq!(51u16, d.params[6]);
            assert!(!d.params_sep.is_set(6));
            assert_eq!(48u16, d.params[7]);
            assert!(!d.params_sep.is_set(7));
            assert_eq!(2u16, d.params[8]);
            assert!(!d.params_sep.is_set(8));
            assert_eq!(170u16, d.params[9]);
            assert!(!d.params_sep.is_set(9));
            assert_eq!(170u16, d.params[10]);
            assert!(!d.params_sep.is_set(10));
            assert_eq!(170u16, d.params[11]);
            assert!(!d.params_sep.is_set(11));
            assert_eq!(58u16, d.params[12]);
            assert!(!d.params_sep.is_set(12));
            assert_eq!(2u16, d.params[13]);
            assert!(!d.params_sep.is_set(13));
            assert_eq!(255u16, d.params[14]);
            assert!(!d.params_sep.is_set(14));
            assert_eq!(97u16, d.params[15]);
            assert!(!d.params_sep.is_set(15));
            assert_eq!(136u16, d.params[16]);
            assert!(!d.params_sep.is_set(16));
        }
    }

    // test "csi: colon for non-m final" {
    #[test]
    fn csi_colon_for_non_m_final() {
        let mut p = Parser::init();
        let _ = p.next(0x1B);
        for c in *b"[38:2h" {
            let a = p.next(c);
            assert!(a[0].is_none());
            assert!(a[1].is_none());
            assert!(a[2].is_none());
        }

        assert!(p.state == State::Ground);
    }

    // test "csi: request mode decrqm" {
    #[test]
    fn csi_request_mode_decrqm() {
        let mut p = Parser::init();
        let _ = p.next(0x1B);
        for c in *b"[?2026$" {
            let a = p.next(c);
            assert!(a[0].is_none());
            assert!(a[1].is_none());
            assert!(a[2].is_none());
        }

        {
            let a = p.next(b'p');
            assert!(p.state == State::Ground);
            assert!(a[0].is_none());
            assert!(matches!(a[1].as_ref().unwrap(), Action::CsiDispatch(_)));
            assert!(a[2].is_none());

            let d = match a[1].as_ref().unwrap() {
                Action::CsiDispatch(d) => d,
                _ => unreachable!(),
            };
            assert!(d.final_ == b'p');
            assert_eq!(2usize, d.intermediates.len());
            assert_eq!(1usize, d.params.len());
            assert_eq!(b'?' as u16, d.intermediates[0] as u16);
            assert_eq!(b'$' as u16, d.intermediates[1] as u16);
            assert_eq!(2026u16, d.params[0]);
        }
    }

    // test "csi: change cursor" {
    #[test]
    fn csi_change_cursor() {
        let mut p = Parser::init();
        let _ = p.next(0x1B);
        for c in *b"[3 " {
            let a = p.next(c);
            assert!(a[0].is_none());
            assert!(a[1].is_none());
            assert!(a[2].is_none());
        }

        {
            let a = p.next(b'q');
            assert!(p.state == State::Ground);
            assert!(a[0].is_none());
            assert!(matches!(a[1].as_ref().unwrap(), Action::CsiDispatch(_)));
            assert!(a[2].is_none());

            let d = match a[1].as_ref().unwrap() {
                Action::CsiDispatch(d) => d,
                _ => unreachable!(),
            };
            assert!(d.final_ == b'q');
            assert_eq!(1usize, d.intermediates.len());
            assert_eq!(1usize, d.params.len());
            assert_eq!(b' ' as u16, d.intermediates[0] as u16);
            assert_eq!(3u16, d.params[0]);
        }
    }

    // test "osc: change window title" {
    #[test]
    fn osc_change_window_title() {
        let mut p = Parser::init();
        let _ = p.next(0x1B);
        let _ = p.next(b']');
        let _ = p.next(b'0');
        let _ = p.next(b';');
        let _ = p.next(b'a');
        let _ = p.next(b'b');
        let _ = p.next(b'c');

        {
            let a = p.next(0x07); // BEL
            assert!(p.state == State::Ground);
            assert!(matches!(a[0].as_ref().unwrap(), Action::OscDispatch(_)));
            assert!(a[1].is_none());
            assert!(a[2].is_none());

            let cmd = match a[0].as_ref().unwrap() {
                Action::OscDispatch(cmd) => cmd,
                _ => unreachable!(),
            };
            assert!(matches!(cmd, osc::Command::ChangeWindowTitle(_)));
            let title = match cmd {
                osc::Command::ChangeWindowTitle(t) => t,
                _ => unreachable!(),
            };
            assert_eq!(b"abc", title.as_slice());
        }
    }

    // test "osc: change window title (end in esc)" {
    #[test]
    fn osc_change_window_title_end_in_esc() {
        let mut p = Parser::init();
        let _ = p.next(0x1B);
        let _ = p.next(b']');
        let _ = p.next(b'0');
        let _ = p.next(b';');
        let _ = p.next(b'a');
        let _ = p.next(b'b');
        let _ = p.next(b'c');

        {
            let a = p.next(0x1B);
            // PORT NOTE: the reference reads `a` (the result of `next(0x1B)`)
            // before calling `next('\\')`; we mirror that ordering by extracting
            // the cloned command from `a` first, then advancing the parser.
            let cmd = match a[0].as_ref() {
                Some(Action::OscDispatch(cmd)) => cmd.clone(),
                _ => panic!("expected osc_dispatch"),
            };
            assert!(a[1].is_none());
            assert!(a[2].is_none());
            let _ = p.next(b'\\');
            assert!(p.state == State::Ground);

            assert!(matches!(cmd, osc::Command::ChangeWindowTitle(_)));
            let title = match &cmd {
                osc::Command::ChangeWindowTitle(t) => t,
                _ => unreachable!(),
            };
            assert_eq!(b"abc", title.as_slice());
        }
    }

    // https://github.com/darrenstarr/VtNetCore/pull/14
    // Saw this on HN, decided to add a test case because why not.
    // test "osc: 112 incomplete sequence" {
    #[test]
    fn osc_112_incomplete_sequence() {
        let backend = GlobalZigAllocator;
        let alloc = ZigAllocatorRef::new(&backend);
        let mut p: Parser = Parser::init();
        // defer p.deinit();
        // p.osc_parser.alloc = std.testing.allocator;
        p.osc_parser.alloc = Some(alloc);

        let _ = p.next(0x1B);
        let _ = p.next(b']');
        let _ = p.next(b'1');
        let _ = p.next(b'1');
        let _ = p.next(b'2');

        {
            let a = p.next(0x07);
            assert!(p.state == State::Ground);
            assert!(matches!(a[0].as_ref().unwrap(), Action::OscDispatch(_)));
            assert!(a[1].is_none());
            assert!(a[2].is_none());

            let cmd = match a[0].as_ref().unwrap() {
                Action::OscDispatch(cmd) => cmd,
                _ => unreachable!(),
            };
            assert!(matches!(cmd, osc::Command::ColorOperation { .. }));
            let (op, requests, terminator) = match cmd {
                osc::Command::ColorOperation {
                    op,
                    requests,
                    terminator,
                } => (op, requests, terminator),
                _ => unreachable!(),
            };
            assert_eq!(*terminator, osc::Terminator::Bel);
            assert!(*op == osc::color::Operation::osc_112);
            assert!(requests.len() == 1);
            // var it = cmd.color_operation.requests.constIterator(0);
            let mut it = requests.iter();
            {
                let op = it.next().unwrap();
                assert!(matches!(op, osc::color::Request::Reset(_)));
                assert_eq!(
                    osc::color::Request::Reset(osc::color::Target::Dynamic(
                        crate::color::Dynamic::Cursor
                    )),
                    *op,
                );
            }
            assert!(it.next().is_none());
        }

        p.deinit();
    }

    // test "osc: 104 empty" {
    #[test]
    fn osc_104_empty() {
        let backend = GlobalZigAllocator;
        let alloc = ZigAllocatorRef::new(&backend);
        let mut p: Parser = Parser::init();
        // defer p.deinit();
        // p.osc_parser.alloc = std.testing.allocator;
        p.osc_parser.alloc = Some(alloc);

        let _ = p.next(0x1B);
        let _ = p.next(b']');
        let _ = p.next(b'1');
        let _ = p.next(b'0');
        let _ = p.next(b'4');

        {
            let a = p.next(0x07);
            assert!(p.state == State::Ground);
            assert!(matches!(a[0].as_ref().unwrap(), Action::OscDispatch(_)));
            assert!(a[1].is_none());
            assert!(a[2].is_none());

            let cmd = match a[0].as_ref().unwrap() {
                Action::OscDispatch(cmd) => cmd,
                _ => unreachable!(),
            };
            assert!(matches!(cmd, osc::Command::ColorOperation { .. }));
            let (op, requests, terminator) = match cmd {
                osc::Command::ColorOperation {
                    op,
                    requests,
                    terminator,
                } => (op, requests, terminator),
                _ => unreachable!(),
            };
            assert_eq!(*terminator, osc::Terminator::Bel);
            assert!(*op == osc::color::Operation::osc_104);
            assert!(requests.len() == 1);
            let mut it = requests.iter();
            {
                let op = it.next().unwrap();
                assert!(matches!(op, osc::color::Request::ResetPalette));
            }
            assert!(it.next().is_none());
        }

        p.deinit();
    }

    // test "csi: too many params" {
    #[test]
    fn csi_too_many_params() {
        let mut p = Parser::init();
        let _ = p.next(0x1B);
        let _ = p.next(b'[');
        for _ in 0..100 {
            let _ = p.next(b'1');
            let _ = p.next(b';');
        }
        let _ = p.next(b'1');

        {
            let a = p.next(b'C');
            assert!(p.state == State::Ground);
            assert!(a[0].is_none());
            assert!(a[1].is_none());
            assert!(a[2].is_none());
        }
    }

    // test "csi: sgr with up to our max parameters" {
    #[test]
    fn csi_sgr_with_up_to_our_max_parameters() {
        for max in 1..MAX_PARAMS + 1 {
            let mut p = Parser::init();
            let _ = p.next(0x1B);
            let _ = p.next(b'[');

            for _ in 0..max - 1 {
                let _ = p.next(b'1');
                let _ = p.next(b';');
            }
            let _ = p.next(b'2');

            {
                let a = p.next(b'H');
                assert!(p.state == State::Ground);
                assert!(a[0].is_none());
                assert!(matches!(a[1].as_ref().unwrap(), Action::CsiDispatch(_)));
                assert!(a[2].is_none());

                let csi = match a[1].as_ref().unwrap() {
                    Action::CsiDispatch(csi) => csi,
                    _ => unreachable!(),
                };
                assert_eq!(max, csi.params.len());
                assert_eq!(2u16, csi.params[max - 1]);
            }
        }
    }

    // test "csi: sgr beyond our max drops it" {
    #[test]
    fn csi_sgr_beyond_our_max_drops_it() {
        // Has to be +2 for the loops below
        let max = MAX_PARAMS + 2;

        let mut p = Parser::init();
        let _ = p.next(0x1B);
        let _ = p.next(b'[');

        for _ in 0..max - 1 {
            let _ = p.next(b'1');
            let _ = p.next(b';');
        }
        let _ = p.next(b'2');

        {
            let a = p.next(b'H');
            assert!(p.state == State::Ground);
            assert!(a[0].is_none());
            assert!(a[1].is_none());
            assert!(a[2].is_none());
        }
    }

    // test "dcs: XTGETTCAP" {
    #[test]
    fn dcs_xtgettcap() {
        let mut p = Parser::init();
        let _ = p.next(0x1B);
        for c in *b"P+" {
            let a = p.next(c);
            assert!(a[0].is_none());
            assert!(a[1].is_none());
            assert!(a[2].is_none());
        }

        {
            let a = p.next(b'q');
            assert!(p.state == State::DcsPassthrough);
            assert!(a[0].is_none());
            assert!(a[1].is_none());
            assert!(matches!(a[2].as_ref().unwrap(), Action::DcsHook(_)));

            let hook = match a[2].as_ref().unwrap() {
                Action::DcsHook(hook) => hook,
                _ => unreachable!(),
            };
            assert_eq!(&[b'+'][..], hook.intermediates);
            assert_eq!(&[] as &[u16], hook.params);
            assert_eq!(b'q', hook.final_);
        }
    }

    // test "dcs: params" {
    #[test]
    fn dcs_params() {
        let mut p = Parser::init();
        let _ = p.next(0x1B);
        for c in *b"P1000" {
            let a = p.next(c);
            assert!(a[0].is_none());
            assert!(a[1].is_none());
            assert!(a[2].is_none());
        }

        {
            let a = p.next(b'p');
            assert!(p.state == State::DcsPassthrough);
            assert!(a[0].is_none());
            assert!(a[1].is_none());
            assert!(matches!(a[2].as_ref().unwrap(), Action::DcsHook(_)));

            let hook = match a[2].as_ref().unwrap() {
                Action::DcsHook(hook) => hook,
                _ => unreachable!(),
            };
            assert_eq!(&[1000u16][..], hook.params);
            assert_eq!(b'p', hook.final_);
        }
    }

    // test "dcs: too many params" {
    #[test]
    fn dcs_too_many_params() {
        // Regression test for a crash found by fuzzing (afl). When a DCS
        // sequence has more than MAX_PARAMS parameters and param_acc_idx > 0,
        // entering dcs_passthrough wrote to params[params_idx] without a
        // bounds check, causing an out-of-bounds access.
        let mut p = Parser::init();
        let _ = p.next(0x1B); // ESC
        let _ = p.next(b'P'); // DCS entry

        // Feed a digit then MAX_PARAMS semicolons to fill all param slots.
        let _ = p.next(b'6');
        for _ in 0..MAX_PARAMS {
            let _ = p.next(b';');
        }
        // Feed another digit so param_acc_idx > 0 while params_idx == MAX_PARAMS.
        let _ = p.next(b'7');

        // A final byte triggers entry to dcs_passthrough. The DCS should
        // be dropped entirely, consistent with how CSI handles overflow.
        let a = p.next(b'p');
        assert!(a[0].is_none());
        assert!(a[1].is_none());
        assert!(a[2].is_none());
    }
}
