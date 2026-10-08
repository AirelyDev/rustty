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
const LOG_SCOPE: &str = "parser";

/// Log a warning for a CSI dispatch with colon/mixed separators on a non-'m'
/// command.
///
/// This is `#[inline(never)]` on purpose so that this unlikely (cold)
/// behavior doesn't bloat the hot dispatch path, which has been measured to
/// actually affect both binary size and performance due to icache busts.
///
/// PORTS: oracle@main Parser.zig `warnCsiSepMismatch` (`noinline fn`,
/// `@branchHint(.cold)`).
#[cold]
#[inline(never)]
fn warn_csi_sep_mismatch(csi: &CSI) {
    log::warn!(
        target: LOG_SCOPE,
        "CSI colon or mixed separators only allowed for 'm' command, got: {csi:?}"
    );
}

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
//
// PORT NOTE (parity tail S7-7, N263): the OSC payload is the exception. Its text
// fields BORROW the OSC parser's storage, as the reference's do, so `Action`
// carries the parser's lifetime `'p`, and the actions `Parser::next` returns
// hold the parser borrowed until they are consumed. Only `osc_dispatch`
// borrows; every other variant converts to `Action<'static>` with
// [`Action::into_static`].
#[derive(Debug, Clone, PartialEq)]
pub enum Action<'p> {
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
    // PERF(port): the dispatch payloads are boxed so `Action` stays pointer-
    // sized. The per-byte `[Option<Action>;3]` return cannot be dead-code-
    // eliminated when discarded (the OscDispatch drop keeps it alive for the
    // optimizer), so its stack materialization is paid on EVERY byte — with
    // by-value payloads that is ~200B of dead stores per plain print byte
    // (measured +25% on terminal-parser). Boxing shrinks it to 3x16B. The
    // dispatch-time box allocation replaces the former per-dispatch Vec
    // allocations (2 for CSI/DCS, 1 for ESC) — allocator traffic per escape
    // sequence goes DOWN. Observable contents are unchanged.
    CsiDispatch(Box<CSI>),

    /// Execute the ESC command.
    // esc_dispatch: ESC,
    EscDispatch(Box<ESC>),

    /// Execute the OSC command.
    // osc_dispatch: osc.Command,
    OscDispatch(Box<osc::Command<'p>>),

    /// DCS-related events.
    // dcs_hook: DCS,
    DcsHook(Box<DCS>),
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

impl<'p> Action<'p> {
    /// This action with no borrow of the parser, for every variant but
    /// `OscDispatch`, which is handed back in `Err`.
    ///
    /// PORT NOTE (parity tail S7-7): Rust-only glue. `Parser::next` returns its
    /// three actions under one borrow of the parser; a consumer that must call
    /// back into its own state between them (the stream) detaches the ones that
    /// cannot borrow. Only the exit action of `osc_string` can.
    #[inline(always)]
    pub fn into_static(self) -> Result<Action<'static>, Box<osc::Command<'p>>> {
        Ok(match self {
            Action::Print(c) => Action::Print(c),
            Action::Execute(c) => Action::Execute(c),
            Action::CsiDispatch(v) => Action::CsiDispatch(v),
            Action::EscDispatch(v) => Action::EscDispatch(v),
            Action::OscDispatch(cmd) => return Err(cmd),
            Action::DcsHook(v) => Action::DcsHook(v),
            Action::DcsPut(c) => Action::DcsPut(c),
            Action::DcsUnhook => Action::DcsUnhook,
            Action::ApcStart => Action::ApcStart,
            Action::ApcPut(c) => Action::ApcPut(c),
            Action::ApcEnd => Action::ApcEnd,
        })
    }
}

// PORT NOTE: `Action.CSI` is a nested struct in the reference; hoisted to a
// top-level struct (Rust has no nested type declarations). `intermediates` /
// `params` are owned (see the Action PORT NOTE) rather than slices into the
// parser.
// PERF(port): the owned payloads are inline fixed-capacity arrays
// (`InlineVec`), not heap `Vec`s. Capacities are the reference's own hard
// invariants (MAX_INTERMEDIATE / MAX_PARAMS fixed arrays, Parser.zig:80-84),
// so nothing is lost — and the per-byte `[Option<Action>;3]` return becomes
// drop-glue-free for these variants, matching the reference where discarding
// the actions costs nothing.
// PERF(port): repr(align(8)) — the reference union's natural alignment (its
// `[]u8`/`[]u16` slice fields are pointer-aligned). Without it the inline
// arrays drop `Action`'s alignment to 2/4 and the per-byte `[Option<Action>;3]`
// stack traffic degenerates into unaligned scalar shuffles.
#[derive(Debug, Clone, PartialEq)]
#[repr(align(8))]
pub struct CSI {
    pub intermediates: InlineVec<u8, MAX_INTERMEDIATE>,
    pub params: InlineVec<u16, MAX_PARAMS>,
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
#[repr(align(8))]
pub struct ESC {
    pub intermediates: InlineVec<u8, MAX_INTERMEDIATE>,
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
#[repr(align(8))]
pub struct DCS {
    pub intermediates: InlineVec<u8, MAX_INTERMEDIATE>,
    pub params: InlineVec<u16, MAX_PARAMS>,
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
//                     // All others do the default behavior. Note the max
//                     // depth of 1: this is only used for logging so we
//                     // only want the top-level fields. Larger depths
//                     // instantiate a recursive formatter for every nested
//                     // type of every action payload (e.g. the full
//                     // osc.Command union), which costs tens of kilobytes
//                     // of binary size.
//                     else => try writer.printValue(
//                         "any",
//                         .{},
//                         @field(self, u_field.name),
//                         1,
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

/// PERF(port): fixed-capacity inline payload for `Action`'s CSI/ESC/DCS
/// variants — the Rust stand-in for the reference's
/// `self.intermediates[0..self.intermediates_idx]` prefix slices
/// (Parser.zig:377-384) over its fixed arrays (Parser.zig:80-84). Owned by
/// value like the previous `Vec` payloads (strictly stronger than the
/// reference's "valid until the next call to next" contract) but with no heap
/// allocation and, crucially, no drop glue: discarding the per-byte
/// `[Option<Action>;3]` return is free, as in the reference. The `buf` tail
/// beyond `len` is unspecified (it mirrors whatever was in the parser's
/// buffers, exactly like the reference's undefined array tails); every
/// observation goes through the `len`-bounded prefix slice.
#[derive(Clone, Copy)]
pub struct InlineVec<T: Copy, const N: usize> {
    buf: [T; N],
    len: u8,
}

impl<T: Copy, const N: usize> InlineVec<T, N> {
    /// Construct directly from the parser's backing array + tracked index —
    /// a branch-free whole-array copy (the moral equivalent of the
    /// reference's ptr+len slice over the same storage).
    #[inline(always)]
    pub fn from_buf(buf: [T; N], len: u8) -> Self {
        debug_assert!((len as usize) <= N);
        InlineVec { buf, len }
    }

    /// Construct from a slice (external callers / tests).
    #[inline]
    pub fn from_slice(s: &[T]) -> Self
    where
        T: Default,
    {
        assert!(s.len() <= N);
        let mut buf = [T::default(); N];
        buf[..s.len()].copy_from_slice(s);
        InlineVec {
            buf,
            len: s.len() as u8,
        }
    }

    /// The `len`-bounded prefix — the only observable content.
    #[inline(always)]
    pub fn as_slice(&self) -> &[T] {
        &self.buf[..self.len as usize]
    }
}

impl<T: Copy + Default, const N: usize> InlineVec<T, N> {
    /// Empty payload (the reference's `""` / `&.{}` defaults).
    #[inline]
    pub fn new() -> Self {
        InlineVec {
            buf: [T::default(); N],
            len: 0,
        }
    }
}

impl<T: Copy + Default, const N: usize> Default for InlineVec<T, N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Copy, const N: usize> core::ops::Deref for InlineVec<T, N> {
    type Target = [T];
    #[inline(always)]
    fn deref(&self) -> &[T] {
        self.as_slice()
    }
}

/// Equality / Debug observe ONLY the prefix slice (identical to the previous
/// `Vec` payload's derived impls).
impl<T: Copy + PartialEq, const N: usize> PartialEq for InlineVec<T, N> {
    fn eq(&self, other: &Self) -> bool {
        self.as_slice() == other.as_slice()
    }
}
impl<T: Copy + Eq, const N: usize> Eq for InlineVec<T, N> {}

/// Slice-comparison conveniences so existing assertions comparing payloads
/// against `&[T]` slices keep compiling verbatim.
impl<T: Copy + PartialEq, const N: usize> PartialEq<&[T]> for InlineVec<T, N> {
    fn eq(&self, other: &&[T]) -> bool {
        self.as_slice() == *other
    }
}
impl<T: Copy + PartialEq, const N: usize> PartialEq<InlineVec<T, N>> for &[T] {
    fn eq(&self, other: &InlineVec<T, N>) -> bool {
        *self == other.as_slice()
    }
}
impl<T: Copy + PartialEq, const N: usize> PartialEq<[T]> for InlineVec<T, N> {
    fn eq(&self, other: &[T]) -> bool {
        self.as_slice() == other
    }
}
impl<T: Copy + PartialEq, const N: usize, const M: usize> PartialEq<[T; M]> for InlineVec<T, N> {
    fn eq(&self, other: &[T; M]) -> bool {
        self.as_slice() == &other[..]
    }
}

impl<T: Copy + core::fmt::Debug, const N: usize> core::fmt::Debug for InlineVec<T, N> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        self.as_slice().fmt(f)
    }
}

impl<'s, T: Copy, const N: usize> IntoIterator for &'s InlineVec<T, N> {
    type Item = &'s T;
    type IntoIter = core::slice::Iter<'s, T>;
    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.as_slice().iter()
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
// struct. Ported as a named `Parser` struct carrying the same fields.
// (parity tail S7-7: the embedded `osc.Parser` no longer carries an allocator
// lifetime; its allocator is the runtime handle.)
pub struct Parser {
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
    pub osc_parser: osc::Parser,
}

impl Parser {
    // pub fn init() Parser {
    pub fn init() -> Parser {
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
    // PERF(port): Zig compiles bench + parser whole-program, so the per-byte
    // bench->next call (benchmark/TerminalParser.zig:100) inlines under
    // ReleaseFast; #[inline] restores cross-crate inlinability without LTO.
    #[inline]
    pub fn next(&mut self, c: u8) -> [Option<Action<'_>>; 3] {
        // PERF(port): `self.state` hoisted into a local. Nothing below writes
        // `self.state` until the deferred assignment (the arms only touch the
        // osc parser / intermediates / params), so every read is the same
        // value — but through `&mut self` calls (`do_action`) LLVM cannot
        // prove that and re-loads it per read, putting a store->load
        // round-trip on the per-byte critical path (the table index depends
        // on it). The reference reads `self.state` directly; Zig proves the
        // no-write whole-program. Observationally identical.
        let state = self.state;
        let effect = table[c as usize][state as usize];

        // log.info("next: {x}", .{c});

        let next_state = effect.state;
        let action = effect.action;

        // After generating the actions, we set our next state.
        // defer self.state = next_state;
        // PORT NOTE (reshape): restore reference arm structure (Parser.zig:267-310).
        // Zig's `defer` runs after the return expression is evaluated; here the
        // three actions are computed into locals first and the deferred
        // `self.state` write happens just before the return. The `Action`
        // payloads are owned `Vec`s (see the Action PORT NOTE), so no borrow of
        // `self`'s buffers is held across the state write — each dispatch arm
        // builds its final `Action` in place, exactly like the reference.
        // PERF(port): the intermediates/params prefix slices are materialized
        // ONLY inside the CsiDispatch/EscDispatch/DcsHook arms (as at
        // Parser.zig:377-384/:398-403/:299-306), not unconditionally per byte —
        // the Ground->Ground print path carries no slice bounds checks, no
        // SepList copy, and no staged-value round-trips.

        // When going from one state to another, the actions take place in this order:
        //
        // 1. exit action from old state
        // 2. transition action
        // 3. entry action to new state

        // PERF(port): state-equal fast path. When `state == next_state` the
        // reference's own guards make BOTH the exit and entry actions `null`
        // (each arm is inside `if (self.state == next_state) null else ...`,
        // Parser.zig:267/280), so the result is exactly
        // `[None, doAction(action, c), None]`; the deferred
        // `self.state = next_state` write stores the value already there and
        // is unobservable through `&mut self`, so it is skipped. The
        // Print/Execute arms are duplicated verbatim from `do_action`
        // (Parser.zig:330-331) as separate return sites so the per-byte
        // discarded `[Option<Action>;3]` is STATICALLY drop-free on the hot
        // Ground print/execute path — the reference gets the same effect from
        // its payloads being drop-free slices. Observable behavior identical
        // for every input byte: same action values, same order, same `self`
        // mutations (the delegated arm calls the same `do_action`).
        if state == next_state {
            return match action {
                // .print => Action{ .print = c },
                TransitionAction::Print => [None, Some(Action::Print(c as u32)), None],
                // .execute => Action{ .execute = c },
                TransitionAction::Execute => [None, Some(Action::Execute(c)), None],
                _ => [None, self.do_action(action, c), None],
            };
        }

        // Exit depends on current state
        // PORT NOTE (parity tail S7-7): computed LAST, after the transition and
        // entry actions and the deferred state write (below). The
        // `osc_string` exit's command borrows `self.osc_parser` for as long as
        // the returned actions live, so nothing may take `&mut self` after it.
        // The reordering is unobservable: the other exits are constant values,
        // and the transition and entry of an `osc_string` exit never touch the
        // OSC parser (the table leaves `osc_string` only on BEL, ESC, CAN and
        // SUB: `none`/`execute` transitions into `ground`/`escape`, whose
        // entries are nothing and `clear()`); `end` does not read `self.state`.

        // self.doAction(action, c),
        let transition = self.do_action(action, c);

        // Entry depends on new state
        let entry: Option<Action<'static>> = if state == next_state {
            None
        } else {
            match next_state {
                // .escape, .dcs_entry, .csi_entry => clear: { self.clear(); break :clear null; },
                State::Escape | State::DcsEntry | State::CsiEntry => {
                    self.clear();
                    None
                }
                // .osc_string => osc_string: { self.osc_parser.reset(); break :osc_string null; },
                State::OscString => {
                    self.osc_parser.reset();
                    None
                }
                // .dcs_passthrough => dcs_hook: { ... },
                State::DcsPassthrough => {
                    // Ignore too many parameters
                    if self.params_idx as usize >= MAX_PARAMS {
                        // break :dcs_hook null;
                        None
                    } else {
                        // Finalize parameters
                        if self.param_acc_idx > 0 {
                            self.params[self.params_idx as usize] = self.param_acc;
                            self.params_idx += 1;
                        }
                        // break :dcs_hook .{ .dcs_hook = .{
                        //     .intermediates = self.intermediates[0..self.intermediates_idx],
                        //     .params = self.params[0..self.params_idx],
                        //     .final = c,
                        // } };
                        // PORT NOTE: the reference returns slices into `self`'s
                        // buffers; the owned payload copies the tracked
                        // prefixes (see the Action PORT NOTE).
                        Some(Action::DcsHook(Box::new(DCS {
                            intermediates: InlineVec::from_buf(
                                self.intermediates,
                                self.intermediates_idx,
                            ),
                            params: InlineVec::from_buf(self.params, self.params_idx),
                            final_: c,
                        })))
                    }
                }
                // .sos_pm_apc_string => Action{ .apc_start = {} },
                State::SosPmApcString => Some(Action::ApcStart),
                // else => null,
                _ => None,
            }
        };

        // Apply the deferred state transition now that all actions are built
        // (the Zig `defer` above).
        self.state = next_state;

        // Exit depends on current state (see the PORT NOTE above).
        let exit: Option<Action<'_>> = if state == next_state {
            None
        } else {
            match state {
                // .osc_string => if (self.osc_parser.end(c)) |cmd|
                //     Action{ .osc_dispatch = cmd.* } else null,
                State::OscString => {
                    debug_assert!(matches!(
                        action,
                        TransitionAction::None | TransitionAction::Execute
                    ));
                    self.osc_parser
                        .end(Some(c))
                        .map(|cmd| Action::OscDispatch(Box::new(cmd)))
                }
                // .dcs_passthrough => Action{ .dcs_unhook = {} },
                State::DcsPassthrough => Some(Action::DcsUnhook),
                // .sos_pm_apc_string => Action{ .apc_end = {} },
                State::SosPmApcString => Some(Action::ApcEnd),
                // else => null,
                _ => None,
            }
        };

        [exit, transition, entry]
    }

    // pub inline fn collect(self: *Parser, c: u8) void {
    // PERF(port): Zig `pub inline fn` (Parser.zig:313) — restore
    // mandatory-inline idiom.
    #[inline(always)]
    pub fn collect(&mut self, c: u8) {
        if self.intermediates_idx as usize >= MAX_INTERMEDIATE {
            // @branchHint(.cold);
            log::warn!(target: LOG_SCOPE, "invalid intermediates count");
            return;
        }

        self.intermediates[self.intermediates_idx as usize] = c;
        self.intermediates_idx += 1;
    }

    // inline fn doAction(self: *Parser, action: TransitionAction, c: u8) ?Action {
    // PERF(port): Zig `inline fn` (Parser.zig:324) — restore mandatory-inline
    // idiom.
    // PORT NOTE (reshape): restore reference arm structure (Parser.zig:267-310)
    // — returns `Option<Action>` directly like the reference `?Action`, with
    // the dispatch arms building their owned payloads in place (the former
    // staging layer is gone).
    #[inline(always)]
    fn do_action(&mut self, action: TransitionAction, c: u8) -> Option<Action<'static>> {
        match action {
            // .none, .ignore => null,
            TransitionAction::None | TransitionAction::Ignore => None,
            // .print => Action{ .print = c },
            TransitionAction::Print => Some(Action::Print(c as u32)),
            // .execute => Action{ .execute = c },
            TransitionAction::Execute => Some(Action::Execute(c)),
            // .collect => collect: { self.collect(c); break :collect null; },
            TransitionAction::Collect => {
                self.collect(c);
                None
            }
            // .param => param: { ... },
            TransitionAction::Param => {
                // Semicolon separates parameters. If we encounter a semicolon
                // we need to store and move on to the next parameter.
                if c == b';' || c == b':' {
                    // Ignore too many parameters
                    if self.params_idx as usize >= MAX_PARAMS {
                        // break :param null;
                        return None;
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
                    return None;
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
                    return None;
                }

                // The client is expected to perform no action.
                // break :param null;
                None
            }
            // .osc_put => osc_put: { osc.Parser.next(&self.osc_parser, c); break :osc_put null; },
            TransitionAction::OscPut => {
                self.osc_parser.next(c);
                None
            }
            // .csi_dispatch => csi_dispatch: { ... },
            TransitionAction::CsiDispatch => {
                // Ignore too many parameters
                if self.params_idx as usize >= MAX_PARAMS {
                    // break :csi_dispatch null;
                    return None;
                }

                // Finalize parameters if we have one
                if self.param_acc_idx > 0 {
                    self.params[self.params_idx as usize] = self.param_acc;
                    self.params_idx += 1;
                }

                // const result: Action = .{ .csi_dispatch = .{
                //     .intermediates = self.intermediates[0..self.intermediates_idx],
                //     .params = self.params[0..self.params_idx],
                //     .params_sep = self.params_sep,
                //     .final = c,
                // } };
                // PORT NOTE: the reference slices into `self`'s buffers here
                // ("only valid until the next call to next"); the owned payload
                // copies the tracked prefixes (see the Action PORT NOTE).
                let csi = CSI {
                    intermediates: InlineVec::from_buf(self.intermediates, self.intermediates_idx),
                    params: InlineVec::from_buf(self.params, self.params_idx),
                    params_sep: self.params_sep,
                    final_: c,
                };

                // We only allow colon or mixed separators for the 'm' command.
                if c != b'm' && self.params_sep.count() > 0 {
                    // The reference hoists the warning into a dedicated cold helper
                    // (`warn_csi_sep_mismatch`, `#[cold]`/`#[inline(never)]` below)
                    // so this unlikely branch doesn't bloat the hot dispatch path.
                    warn_csi_sep_mismatch(&csi);
                    // break :csi_dispatch null;
                    return None;
                }

                // break :csi_dispatch result;
                Some(Action::CsiDispatch(Box::new(csi)))
            }
            // .esc_dispatch => Action{ .esc_dispatch = .{
            //     .intermediates = self.intermediates[0..self.intermediates_idx],
            //     .final = c,
            // } },
            TransitionAction::EscDispatch => Some(Action::EscDispatch(Box::new(ESC {
                intermediates: InlineVec::from_buf(self.intermediates, self.intermediates_idx),
                final_: c,
            }))),
            // .put => Action{ .dcs_put = c },
            TransitionAction::Put => Some(Action::DcsPut(c)),
            // .apc_put => Action{ .apc_put = c },
            TransitionAction::ApcPut => Some(Action::ApcPut(c)),
        }
    }

    // pub inline fn clear(self: *Parser) void {
    // PERF(port): Zig `pub inline fn` (Parser.zig:409) — restore
    // mandatory-inline idiom.
    #[inline(always)]
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

    /// PORT NOTE (parity tail S7-7): `p.next(c)` with its actions detached from
    /// the parser borrow (`Action::into_static`), so a test can read `p.state`
    /// while it holds them, as the reference's tests do. None of the actions
    /// these tests see is an `osc_dispatch`; the OSC tests call `p.next`
    /// directly.
    fn next(p: &mut Parser, c: u8) -> [Option<Action<'static>>; 3] {
        p.next(c)
            .map(|a| a.map(|a| a.into_static().expect("not an osc_dispatch action")))
    }

    // test {
    #[test]
    fn basic_apc_print_execute() {
        let mut p = Parser::init();
        let _ = next(&mut p, 0x9E);
        assert!(p.state == State::SosPmApcString);
        let _ = next(&mut p, 0x9C);
        assert!(p.state == State::Ground);

        {
            let a = next(&mut p, b'a');
            assert!(p.state == State::Ground);
            assert!(a[0].is_none());
            assert!(matches!(a[1].as_ref().unwrap(), Action::Print(_)));
            assert!(a[2].is_none());
        }

        {
            let a = next(&mut p, 0x19);
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
        let _ = next(&mut p, 0x1B);
        let _ = next(&mut p, b'(');

        {
            let a = next(&mut p, b'B');
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
        let _ = next(&mut p, 0x1B);
        let _ = next(&mut p, 0x5B);

        {
            let a = next(&mut p, 0x48);
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
        let _ = next(&mut p, 0x1B);
        let _ = next(&mut p, 0x5B);
        let _ = next(&mut p, 0x31); // 1
        let _ = next(&mut p, 0x3B); // ;
        let _ = next(&mut p, 0x34); // 4

        {
            let a = next(&mut p, 0x48); // H
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
        let _ = next(&mut p, 0x1B);
        let _ = next(&mut p, b'[');
        let _ = next(&mut p, b'3');
        let _ = next(&mut p, b'8');
        let _ = next(&mut p, b':');
        let _ = next(&mut p, b'2');

        {
            let a = next(&mut p, b'm');
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
        let _ = next(&mut p, 0x1B);
        for c in *b"[48:2" {
            let a = next(&mut p, c);
            assert!(a[0].is_none());
            assert!(a[1].is_none());
            assert!(a[2].is_none());
        }

        {
            let a = next(&mut p, b'm');
            assert!(p.state == State::Ground);
            assert!(a[0].is_none());
            assert!(matches!(a[1].as_ref().unwrap(), Action::CsiDispatch(_)));
            assert!(a[2].is_none());
        }

        let _ = next(&mut p, 0x1B);
        let _ = next(&mut p, b'[');
        {
            let a = next(&mut p, b'H');
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
        let _ = next(&mut p, 0x1B);
        for c in *b"[38:5:1;48:5:0" {
            let a = next(&mut p, c);
            assert!(a[0].is_none());
            assert!(a[1].is_none());
            assert!(a[2].is_none());
        }

        {
            let a = next(&mut p, b'm');
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
        let _ = next(&mut p, 0x1B);
        for c in *b"[48:2:240:143:104" {
            let a = next(&mut p, c);
            assert!(a[0].is_none());
            assert!(a[1].is_none());
            assert!(a[2].is_none());
        }

        {
            let a = next(&mut p, b'm');
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
        let _ = next(&mut p, 0x1B);
        let _ = next(&mut p, b'[');
        let _ = next(&mut p, b'4');
        let _ = next(&mut p, b':');
        let _ = next(&mut p, b'3');

        {
            let a = next(&mut p, b'm');
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
        let _ = next(&mut p, 0x1B);
        for c in *b"[58:2::240:143:104" {
            let a = next(&mut p, c);
            assert!(a[0].is_none());
            assert!(a[1].is_none());
            assert!(a[2].is_none());
        }

        {
            let a = next(&mut p, b'm');
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
        let _ = next(&mut p, 0x1B);
        for c in *b"[;4:3;38;2;175;175;215;58:2::190:80:70" {
            let a = next(&mut p, c);
            assert!(a[0].is_none());
            assert!(a[1].is_none());
            assert!(a[2].is_none());
        }

        {
            let a = next(&mut p, b'm');
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
        let _ = next(&mut p, 0x1B);
        for c in *b"[4:3;38;2;51;51;51;48;2;170;170;170;58;2;255;97;136" {
            let a = next(&mut p, c);
            assert!(a[0].is_none());
            assert!(a[1].is_none());
            assert!(a[2].is_none());
        }

        {
            let a = next(&mut p, b'm');
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
        let _ = next(&mut p, 0x1B);
        for c in *b"[38:2h" {
            let a = next(&mut p, c);
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
        let _ = next(&mut p, 0x1B);
        for c in *b"[?2026$" {
            let a = next(&mut p, c);
            assert!(a[0].is_none());
            assert!(a[1].is_none());
            assert!(a[2].is_none());
        }

        {
            let a = next(&mut p, b'p');
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
        let _ = next(&mut p, 0x1B);
        for c in *b"[3 " {
            let a = next(&mut p, c);
            assert!(a[0].is_none());
            assert!(a[1].is_none());
            assert!(a[2].is_none());
        }

        {
            let a = next(&mut p, b'q');
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
                                  // assert(p.state == .ground);
                                  // PORT NOTE (parity tail S7-7): read after the block: the actions
                                  // hold the parser borrowed while the command is inspected.
            assert!(matches!(a[0].as_ref().unwrap(), Action::OscDispatch(_)));
            assert!(a[1].is_none());
            assert!(a[2].is_none());

            let cmd = match a[0].as_ref().unwrap() {
                Action::OscDispatch(cmd) => cmd.as_ref(),
                _ => unreachable!(),
            };
            assert!(matches!(cmd, osc::Command::ChangeWindowTitle(_)));
            let title = match cmd {
                osc::Command::ChangeWindowTitle(t) => t,
                _ => unreachable!(),
            };
            assert_eq!(b"abc", title.as_slice());
        }
        assert!(p.state == State::Ground);
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
            // _ = p.next('\\');
            // try testing.expect(p.state == .ground);
            // PORT NOTE (parity tail S7-7): the reference calls `next('\\')`
            // before it reads `a`. The command borrows the OSC parser, "only
            // valid until the next call to next", which the borrow checker
            // enforces; the reference's read after `next('\\')` relies on that
            // call not touching the OSC buffer. The same assertions run on `a`
            // first, then `next('\\')` and the state check.
            assert!(matches!(a[0].as_ref(), Some(Action::OscDispatch(_))));
            assert!(a[1].is_none());
            assert!(a[2].is_none());

            let cmd = match a[0].as_ref() {
                Some(Action::OscDispatch(cmd)) => cmd.as_ref(),
                _ => panic!("expected osc_dispatch"),
            };
            assert!(matches!(cmd, osc::Command::ChangeWindowTitle(_)));
            let title = match cmd {
                osc::Command::ChangeWindowTitle(t) => t,
                _ => unreachable!(),
            };
            assert_eq!(b"abc", title.as_slice());
            drop(a);

            let _ = p.next(b'\\');
            assert!(p.state == State::Ground);
        }
    }

    // https://github.com/darrenstarr/VtNetCore/pull/14
    // Saw this on HN, decided to add a test case because why not.
    // test "osc: 112 incomplete sequence" {
    #[test]
    fn osc_112_incomplete_sequence() {
        let alloc = crate::alloc::Global;
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
            // assert(p.state == .ground);
            // PORT NOTE (parity tail S7-7): read after the block: the actions
            // hold the parser borrowed while the command is inspected.
            assert!(matches!(a[0].as_ref().unwrap(), Action::OscDispatch(_)));
            assert!(a[1].is_none());
            assert!(a[2].is_none());

            let cmd = match a[0].as_ref().unwrap() {
                Action::OscDispatch(cmd) => cmd.as_ref(),
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
        assert!(p.state == State::Ground);

        p.deinit();
    }

    // test "osc: 104 empty" {
    #[test]
    fn osc_104_empty() {
        let alloc = crate::alloc::Global;
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
            // assert(p.state == .ground);
            // PORT NOTE (parity tail S7-7): read after the block: the actions
            // hold the parser borrowed while the command is inspected.
            assert!(matches!(a[0].as_ref().unwrap(), Action::OscDispatch(_)));
            assert!(a[1].is_none());
            assert!(a[2].is_none());

            let cmd = match a[0].as_ref().unwrap() {
                Action::OscDispatch(cmd) => cmd.as_ref(),
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
        assert!(p.state == State::Ground);

        p.deinit();
    }

    // test "osc: 105 empty" {
    #[test]
    fn osc_105_empty() {
        let alloc = crate::alloc::Global;
        let mut p: Parser = Parser::init();
        // defer p.deinit();
        // p.osc_parser.alloc = std.testing.allocator;
        p.osc_parser.alloc = Some(alloc);

        for c in b"\x1b]105" {
            let _ = p.next(*c);
        }

        let a = p.next(0x07);
        let cmd = match a[0].as_ref().unwrap() {
            Action::OscDispatch(cmd) => cmd.as_ref(),
            _ => panic!("expected osc_dispatch"),
        };
        match cmd {
            osc::Command::ColorOperation { op, .. } => {
                assert!(*op == osc::color::Operation::osc_105)
            }
            _ => panic!("expected color_operation"),
        }
    }

    // test "csi: too many params" {
    #[test]
    fn csi_too_many_params() {
        let mut p = Parser::init();
        let _ = next(&mut p, 0x1B);
        let _ = next(&mut p, b'[');
        for _ in 0..100 {
            let _ = next(&mut p, b'1');
            let _ = next(&mut p, b';');
        }
        let _ = next(&mut p, b'1');

        {
            let a = next(&mut p, b'C');
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
            let _ = next(&mut p, 0x1B);
            let _ = next(&mut p, b'[');

            for _ in 0..max - 1 {
                let _ = next(&mut p, b'1');
                let _ = next(&mut p, b';');
            }
            let _ = next(&mut p, b'2');

            {
                let a = next(&mut p, b'H');
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
        let _ = next(&mut p, 0x1B);
        let _ = next(&mut p, b'[');

        for _ in 0..max - 1 {
            let _ = next(&mut p, b'1');
            let _ = next(&mut p, b';');
        }
        let _ = next(&mut p, b'2');

        {
            let a = next(&mut p, b'H');
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
        let _ = next(&mut p, 0x1B);
        for c in *b"P+" {
            let a = next(&mut p, c);
            assert!(a[0].is_none());
            assert!(a[1].is_none());
            assert!(a[2].is_none());
        }

        {
            let a = next(&mut p, b'q');
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
        let _ = next(&mut p, 0x1B);
        for c in *b"P1000" {
            let a = next(&mut p, c);
            assert!(a[0].is_none());
            assert!(a[1].is_none());
            assert!(a[2].is_none());
        }

        {
            let a = next(&mut p, b'p');
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
        let _ = next(&mut p, 0x1B); // ESC
        let _ = next(&mut p, b'P'); // DCS entry

        // Feed a digit then MAX_PARAMS semicolons to fill all param slots.
        let _ = next(&mut p, b'6');
        for _ in 0..MAX_PARAMS {
            let _ = next(&mut p, b';');
        }
        // Feed another digit so param_acc_idx > 0 while params_idx == MAX_PARAMS.
        let _ = next(&mut p, b'7');

        // A final byte triggers entry to dcs_passthrough. The DCS should
        // be dropped entirely, consistent with how CSI handles overflow.
        let a = next(&mut p, b'p');
        assert!(a[0].is_none());
        assert!(a[1].is_none());
        assert!(a[2].is_none());
    }

    // S132-F1: `collect` logs `log.warn("invalid intermediates count", .{})`
    // (Parser.zig:319-328) when more than `MAX_INTERMEDIATE` intermediate
    // bytes arrive before the final byte.
    #[test]
    fn collect_logs_when_intermediates_overflow() {
        let cap = crate::log_capture::start();
        let mut p = Parser::init();
        let _ = next(&mut p, 0x1B); // ESC
        let _ = next(&mut p, b'['); // CSI entry
                                    // Intermediate bytes are 0x20..=0x2F; feed one more than
                                    // `MAX_INTERMEDIATE` (4) to overflow `collect`.
        for _ in 0..(MAX_INTERMEDIATE + 1) {
            let _ = next(&mut p, b'!');
        }
        let _ = next(&mut p, b'm');

        let messages = cap.messages();
        assert!(
            messages
                .iter()
                .any(|(level, target, msg)| *level == log::Level::Warn
                    && target == "parser"
                    && *msg == "invalid intermediates count"),
            "expected the invalid-intermediates-count diagnostic, got: {messages:?}"
        );
        drop(cap);
    }

    // S132-F1: `warnCsiSepMismatch` logs `log.warn("CSI colon or mixed
    // separators only allowed for 'm' command, got: {f}", .{csi})`
    // (Parser.zig:412-424) when a colon-separated CSI dispatches on any final
    // byte other than 'm'.
    #[test]
    fn csi_dispatch_logs_on_colon_separator_mismatch_for_non_m_command() {
        let cap = crate::log_capture::start();
        let mut p = Parser::init();
        let _ = next(&mut p, 0x1B); // ESC
        let _ = next(&mut p, b'['); // CSI entry
        let _ = next(&mut p, b'1');
        let _ = next(&mut p, b':'); // colon separator — only valid for 'm'
        let _ = next(&mut p, b'2');
        let a = next(&mut p, b'H'); // cursor position, NOT 'm'

        // The dispatch is suppressed entirely on the mismatch (`return None`).
        assert!(a.iter().all(Option::is_none));

        let messages = cap.messages();
        assert!(
            messages
                .iter()
                .any(|(level, target, msg)| *level == log::Level::Warn
                    && target == "parser"
                    && msg.starts_with(
                        "CSI colon or mixed separators only allowed for 'm' command, got: "
                    )),
            "expected the CSI-sep-mismatch diagnostic, got: {messages:?}"
        );
        drop(cap);
    }
}
