//! rustty-args::args
//!
//! PORTS: ghostty src/cli/args.zig
//
// PORT NOTE: the reference is built on Zig comptime metaprogramming —
// `parse(comptime T, ...)` / `parseIntoField(comptime T, ...)` reflect over an
// arbitrary destination struct `T`, dispatching on each field's name and type at
// comptime. Rust has no comptime struct reflection, so — exactly as the
// `tmux/output.rs` `FormatStruct` port does — the faithful equivalent threads the
// SAME per-field information dynamically through a trait. The destination struct
// implements `Target`: it reports whether it has an `_arena`/`_diagnostics`/
// `compatibility`/`help`/`parseManuallyHook` (the reference's `@hasField`/
// `@hasDecl` queries) and provides `parse_into_field`, which performs the
// per-field name+type dispatch the reference does with `inline for` +
// `@typeInfo(field.type)`. The standalone parse helpers (`parse_bool`,
// `parse_struct`, `parse_tagged_union`, `parse_auto_struct`, `parse_packed_struct`)
// are reusable by `Target` implementors so each field's parse path is
// assertion-for-assertion identical to the reference.

// const std = @import("std");
// const mem = std.mem;
// const assert = @import("../quirks.zig").inlineAssert;
// const Allocator = mem.Allocator;
// const ArenaAllocator = std.heap.ArenaAllocator;
// const diags = @import("diagnostics.zig");
// const internal_os = @import("../os/main.zig");
// const Diagnostic = diags.Diagnostic;
// const DiagnosticList = diags.DiagnosticList;
// const CommaSplitter = @import("CommaSplitter.zig");
use crate::comma_splitter::CommaSplitter;
use crate::diagnostics::{Diagnostic, DiagnosticList, Location, LocationError, LocationProvider};
// (`std.fmt.parseInt` / `std.fmt.parseFloat`: the one port, in rustty-zigstd;
// NEW-FINDINGS N120.)
use rustty_zigstd::fmt::{self as std_fmt, ZigFloat, ZigInt};
// PORT NOTE (P3-W7 T4): the reference's `internal_os` import was only used for
// `internal_os.args`, and upstream DELETED `src/os/args.zig` in `e8525c0fd`.
// The seam now goes straight to `std::env::args_os()` (see `OsArgIterator`), so
// this crate no longer needs a `rustty-os` edge for it.

// const log = std.log.scoped(.cli);
//
// PORT NOTE: the reference's scope tag, carried through the `log` facade as
// `target: LOG_SCOPE` (DECISION-logging.md §6r4).
pub(crate) const LOG_SCOPE: &str = "cli";

// TODO:
//   - Only `--long=value` format is accepted. Do we want to allow
//     `--long value`? Not currently allowed.

// For trimming
// pub const whitespace = " \t";
pub const WHITESPACE: &[u8] = b" \t";

/// The base errors for arg parsing. Additional errors can be returned due
/// to type-specific parsing but these are always possible.
// pub const Error = error{
//     ValueRequired,
//     InvalidField,
//     InvalidValue,
// };
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    ValueRequired,
    InvalidField,
    InvalidValue,
    /// PORT NOTE: NOT a member of the reference `args.Error` set. The reference's
    /// `parse` error set is INFERRED and transitively includes whatever a
    /// destination's `help` decl returns — for CLI actions that is
    /// `error.ActionHelpRequested` (`cli.ghostty.Action.help_error`, declared in
    /// the cli layer, not here). Rust needs a concrete return type for `parse`, so
    /// the help sentinel is carried as this variant: `Target::help()` returns
    /// `Some(Err(Error::ActionHelpRequested))`, `parse`/`parse_self` propagate it
    /// via `res?`, and the cli maps it back to `ghostty::Error::ActionHelpRequested`
    /// (its `From<ParseError>` impl). It never reaches the field-parse error
    /// handlers below (help is matched before `parse_into_field`).
    ActionHelpRequested,
    /// PORT NOTE: NOT a member of the reference `args.Error` set either. It is
    /// the `error.NotImplemented` that `Config.RepeatableLink.parseCLI` returns
    /// (`Config.zig:8719-8724`, "This currently can't be set"). The reference's
    /// open error set lets it reach the diagnostic switch's `else` arm, which
    /// records `unknown error error.NotImplemented`; this variant carries it
    /// there (LEDGER S76-F2).
    NotImplemented,
    /// `CommaSplitter.Error` — NOT members of `args.Error` either. The
    /// reference's `parseAutoStruct` propagates them unchanged from
    /// `while (try iter.next())`, so they are in its inferred error set and in
    /// every `parseIntoField` that reaches it. A caller that switches on
    /// `error.InvalidValue` therefore does not treat them as one, and `parse`
    /// renders them through its `else => "unknown error {}"` arm (parity
    /// S67-F5; the port used to collapse all three into `InvalidValue`).
    UnclosedQuote,
    UnfinishedEscape,
    IllegalEscape,
}

impl From<crate::comma_splitter::Error> for Error {
    fn from(e: crate::comma_splitter::Error) -> Error {
        match e {
            crate::comma_splitter::Error::UnclosedQuote => Error::UnclosedQuote,
            crate::comma_splitter::Error::UnfinishedEscape => Error::UnfinishedEscape,
            crate::comma_splitter::Error::IllegalEscape => Error::IllegalEscape,
        }
    }
}

/// `std.fmt.allocPrintSentinel(arena_alloc, "unknown error {}", .{err}, 0)` —
/// the `else` arm of `parse`'s diagnostic switch. `{}` on a Zig error value
/// prints `error.<Name>` (MEASURED with Zig 0.16.0: `error.FileNotFound`).
fn format_unknown_error(err: Error) -> Vec<u8> {
    format!("unknown error error.{err:?}").into_bytes()
}

/// PORTS: the union of base `Error` plus the `OutOfMemory`/`Unknown` extras and
/// any type-specific parse errors. The reference computes this at comptime as
/// `@TypeOf(err) || error{ Unknown, OutOfMemory } || Error`; since the Rust port
/// does not surface `OutOfMemory` as a recoverable error (allocation aborts), the
/// observable error set is the base `Error` plus the type-specific failures,
/// which all collapse into one of the base variants. This is the error type
/// returned by `parse_into_field` and the parse helpers.
//
// PORT NOTE: kept as the same base `Error` set; type-specific parse failures
// (bad int/float/enum/struct/union) map to `InvalidValue`, missing values map to
// `ValueRequired`, and unknown fields map to `InvalidField`, exactly as the
// reference's `parseIntoField` returns.
pub type ParseError = Error;

/// The outcome of a `parseManuallyHook` call. The reference hook returns a
/// plain `bool` (`true` = continue parsing, `false` = halt), but the `-e` path
/// additionally needs the driver to drain `iter` into the initial command — a
/// step Rust cannot run inside the hook (aliasing; see `parse_manually_hook`).
/// `ConsumeRest` is that extra signal: the driver drains the iterator, records
/// each remaining arg as a replay step, then calls `apply_e_command`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookOutcome {
    /// The reference's `return true` — continue parsing the next arg.
    Continue,
    /// The reference's `return false` — stop parsing, nothing more to do.
    Halt,
    /// The `-e` path: drive the verbatim `while (iter.next())` drain in the
    /// caller, then finish via `apply_e_command`.
    ConsumeRest,
}

/// PORTS: the comptime destination type `T` of `parse`/`parseIntoField`.
///
/// The reference reflects over `T` at comptime; here the destination implements
/// this trait to thread the same information dynamically:
///   - `parse_into_field` — the per-field name+type dispatch (`parseIntoField`).
///   - `arena_available` — `@hasField(T, "_arena")`.
///   - `diagnostics`/`diagnostics_mut` — the `_diagnostics` field
///     (`canTrackDiags` == `@hasField(T, "_diagnostics")`).
///   - `parse_manually_hook` — `@hasDecl(T, "parseManuallyHook")` (default: none).
///   - `help` — `@hasDecl(T, "help")` (default: none).
///   - `compatibility` — `@hasDecl(T, "compatibility")` (default: none).
pub trait Target: Sized {
    /// `@hasField(T, "_arena")`. The reference creates/reuses an arena for
    /// allocations when this is set. The Rust port hands out owned allocations
    /// (each parsed value owns its bytes), so the arena's observable role is just
    /// "allocations are available"; this reports whether the struct opted in.
    fn arena_available(&self) -> bool {
        false
    }

    /// Returns the `_diagnostics` list if this type can track diagnostics
    /// (`canTrackDiags` == `@hasField(T, "_diagnostics")`).
    fn diagnostics_mut(&mut self) -> Option<&mut DiagnosticList> {
        None
    }

    /// `canTrackDiags(T)` — true if this type has a `_diagnostics` field.
    fn can_track_diags(&mut self) -> bool {
        self.diagnostics_mut().is_some()
    }

    /// Parse a single key/value pair into this destination
    /// (`parseIntoField(T, alloc, dst, key, value)`).
    ///
    /// This performs the per-field name match and type dispatch that the
    /// reference does with `inline for (info.@"struct".fields)`. Returns
    /// `InvalidField` if no non-underscore field matches `key`.
    //
    // PORT NOTE: the reference body that each implementation reproduces (the
    // comptime field reflection) is transcribed here verbatim so its comments are
    // preserved (R1); the per-`Target` `parse_into_field` performs exactly these
    // steps for its own concrete fields, using the `parse_field_*` helpers below.
    //
    //     inline for (info.@"struct".fields) |field| {
    //         if (field.name[0] != '_' and mem.eql(u8, field.name, key)) {
    //             // For optional fields, we just treat it as the child type.
    //             // This lets optional fields default to null but get set by
    //             // the CLI.
    //             const Field = switch (@typeInfo(field.type)) { .optional => |opt| opt.child, else => field.type };
    //             const fieldInfo = @typeInfo(Field);
    //             const canHaveDecls = fieldInfo == .@"struct" or fieldInfo == .@"union" or fieldInfo == .@"enum";
    //
    //             // If the value is empty string (set but empty string),
    //             // then we reset the value to the default.
    //             if (value) |v| default: {
    //                 if (v.len != 0) break :default;
    //                 // Set default value if possible.
    //                 if (canHaveDecls and @hasDecl(Field, "init")) {
    //                     try @field(dst, field.name).init(alloc);
    //                     return;
    //                 }
    //                 const raw = field.default_value_ptr orelse break :default;
    //                 const ptr: *const field.type = @ptrCast(@alignCast(raw));
    //                 @field(dst, field.name) = ptr.*;
    //                 return;
    //             }
    //
    //             // If we are a type that can have decls and have a parseCLI decl,
    //             // we call that and use that to set the value.
    //             if (canHaveDecls) {
    //                 if (@hasDecl(Field, "parseCLI")) {
    //                     const fnInfo = @typeInfo(@TypeOf(Field.parseCLI)).@"fn";
    //                     switch (fnInfo.params.len) {
    //                         // 1 arg = (input) => output
    //                         1 => @field(dst, field.name) = try Field.parseCLI(value),
    //                         // 2 arg = (self, input) => void
    //                         2 => switch (@typeInfo(field.type)) {
    //                             .@"struct", .@"union", .@"enum" => try @field(dst, field.name).parseCLI(value),
    //                             // If the field is optional and set, then we use
    //                             // the pointer value directly into it. If its not
    //                             // set we need to create a new instance.
    //                             .optional => if (@field(dst, field.name)) |*v| {
    //                                 try v.parseCLI(value);
    //                             } else {
    //                                 // Note: you cannot do @field(dst, name) = undefined
    //                                 // because this causes the value to be "null"
    //                                 // in ReleaseFast modes.
    //                                 var tmp: Field = undefined;
    //                                 try tmp.parseCLI(value);
    //                                 @field(dst, field.name) = tmp;
    //                             },
    //                             else => @compileError("unexpected field type"),
    //                         },
    //                         // 3 arg = (self, alloc, input) => void
    //                         3 => switch (@typeInfo(field.type)) {
    //                             .@"struct", .@"union", .@"enum" => try @field(dst, field.name).parseCLI(alloc, value),
    //                             .optional => if (@field(dst, field.name)) |*v| {
    //                                 try v.parseCLI(alloc, value);
    //                             } else {
    //                                 var tmp: Field = undefined;
    //                                 try tmp.parseCLI(alloc, value);
    //                                 @field(dst, field.name) = tmp;
    //                             },
    //                             else => @compileError("unexpected field type"),
    //                         },
    //                         else => @compileError("parseCLI invalid argument count"),
    //                     }
    //                     return;
    //                 }
    //             }
    //
    //             // No parseCLI, magic the value based on the type
    //             @field(dst, field.name) = switch (Field) { ... };
    //             return;
    //         }
    //     }
    //     return error.InvalidField;
    fn parse_into_field(&mut self, key: &[u8], value: Option<&[u8]>) -> Result<(), ParseError>;

    /// `@hasDecl(T, "parseManuallyHook")`. Returns `None` if the type has no
    /// hook; otherwise `Some(Ok(outcome))` / `Some(Err(..))`.
    ///
    /// PORT NOTE (iterator threading): the reference signature is
    /// `parseManuallyHook(self, alloc, arg, iter)` — the hook is handed the live
    /// `iter` so the `-e` path can `iter.next()`-drain the rest of the args into
    /// the initial command. In Rust the hook holds `&mut self` (the destination)
    /// and `next_self` also needs `&mut dst`, so the hook cannot drain `iter`
    /// inline without aliasing UB (see memory `rust-mut-args-strongly-protected`).
    /// The faithful, sound split: the hook records what it can and returns a
    /// `HookOutcome`; for `-e` it returns `ConsumeRest`, and the driver
    /// (`parse`/`parse_self`) — which owns both `dst` and `iter` non-overlappingly
    /// — performs the verbatim drain loop, then calls `apply_e_command` back on
    /// `dst`. This reproduces the reference body line-for-line with no aliasing.
    fn parse_manually_hook(&mut self, _arg: &[u8]) -> Option<Result<HookOutcome, ParseError>> {
        None
    }

    /// Finish the reference's `-e` handling after the driver has drained `iter`.
    ///
    /// `command` is the list of args collected after `-e` (each already appended
    /// to the replay steps by the driver as it drained). `location` is
    /// `cli.Location.fromIter(iter, alloc)` captured by the driver. PORTS the tail
    /// of `parseManuallyHook`'s `-e` branch (Config.zig:4739-4763): emit the
    /// "missing command after -e" diagnostic when empty, else set
    /// `initial-command = .{ .direct = command }` plus the implied configs.
    ///
    /// Returns the `continue_parsing` bool the reference's hook returns (always
    /// `false` for `-e`: "we consumed everything").
    fn apply_e_command(&mut self, _command: Vec<Vec<u8>>, _location: Location) -> bool {
        false
    }

    /// Append a single drained `-e` arg to the destination's replay steps.
    /// PORTS the per-arg `try self._replay_steps.append(alloc, .{ .arg = copy })`
    /// inside the `-e` drain loop. Default is a no-op (types without replay
    /// recording). Called by the driver as it drains `iter` for `ConsumeRest`.
    fn record_replay_arg(&mut self, _arg: &[u8]) {}

    /// `@hasDecl(T, "help")` — whether this type has a `help` decl. Gates the
    /// `--help`/`-h` check in `parse`.
    fn has_help_decl(&self) -> bool {
        false
    }

    /// `@hasDecl(T, "help")`. Returns `None` if the type has no `help` decl;
    /// otherwise `Some(Ok(()))` after running help (or an error).
    fn help(&mut self) -> Option<Result<(), ParseError>> {
        None
    }

    /// `@hasDecl(T, "compatibility")` + `T.compatibility.get(key)`. Returns the
    /// compatibility handler for `key` if one exists.
    //
    // PORT NOTE: the reference's `compatibility` map values are bare
    // `CompatibilityHandler(T)` fn-pointers; some entries are produced by
    // `compatibilityRenamed`, which in the Rust port must capture the target field
    // name and so is a boxed closure. To let renamed and custom handlers share one
    // return type, this returns the boxed `BoxedCompatibilityHandler<Self>` (a
    // bare fn coerces into the box trivially). Semantically identical to the
    // reference's `T.compatibility.get(key)`.
    fn compatibility(&self, _key: &[u8]) -> Option<BoxedCompatibilityHandler<Self>> {
        None
    }

    /// `formatValueRequired(T, alloc, key)` — used to build the diagnostic
    /// message for a `ValueRequired` error. The default reproduces the base
    /// "value required" with the per-field valid-values suffix from
    /// `format_values`; a `Target` provides `format_values` to enumerate enum
    /// values for `key`.
    fn format_values(&self, _key: &[u8]) -> Vec<u8> {
        Vec::new()
    }
}

/// The function type for a compatibility handler. The compatibility
/// handler is documented in the `parse` function documentation.
///
/// The function type should return bool if the compatibility was
/// handled, and false otherwise. If false is returned then the
/// naturally occurring error will continue to be processed as if
/// this compatibility handler was not present.
///
/// Compatibility handlers aren't allowed to return errors because
/// they're generally only called in error cases, so we already have
/// an error message to show users. If there is an error in handling
/// the compatibility, then the handler should return false.
// pub fn CompatibilityHandler(comptime T: type) type {
//     return *const fn (dst: *T, alloc: Allocator, key: []const u8, value: ?[]const u8) bool;
// }
pub type CompatibilityHandler<T> = fn(dst: &mut T, key: &[u8], value: Option<&[u8]>) -> bool;

/// A boxed (capturing) compatibility handler, returned by
/// `compatibility_renamed` (which captures the `to` field name).
pub type BoxedCompatibilityHandler<T> = Box<dyn Fn(&mut T, &[u8], Option<&[u8]>) -> bool>;

/// Parse the command line arguments from iter into dst.
///
/// dst must be a struct. The fields and their types will be used to determine
/// the valid CLI flags. See the tests in this file as an example. For field
/// types that are structs, the struct can implement the `parseCLI` function
/// to do custom parsing.
///
/// If the destination type has a field "_arena" of type `?ArenaAllocator`,
/// an arena allocator will be created (or reused if set already) for any
/// allocations. Allocations are necessary for certain types, like `[]const u8`.
///
/// If the destination type has a field "_diagnostics", it must be of type
/// "DiagnosticList" and any diagnostic messages will be added to that list.
/// When diagnostics are present, only allocation errors will be returned.
///
/// If the destination type has a decl "compatibility", it must be of type
/// std.StaticStringMap(CompatibilityHandler(T)), and it will be used to
/// handle backwards compatibility for fields with the given name. The
/// field name doesn't need to exist (so you can setup compatibility for
/// removed fields). The value is a function that will be called when
/// all other parsing fails for that field. If a field changes such that
/// the old values would NOT error, then the caller should handle that
/// downstream after parsing is done, not through this method.
///
/// Note: If the arena is already non-null, then it will be used. In this
/// case, in the case of an error some memory might be leaked into the arena.
// pub fn parse(comptime T: type, alloc: Allocator, dst: *T, iter: anytype) !void {
//
// PORT NOTE: `comptime T` -> the `T: Target` type parameter. `iter: anytype` ->
// `iter: &mut I where I: ArgIterator + LocationProvider`. The arena
// creation/teardown is the reference's `_arena` bookkeeping; the Rust port's
// allocations are owned by each parsed value, so the arena's only observable
// role here is "arena_available => allocations succeed", reflected by the
// reference test assertions `data._arena != null`. We expose that via
// `Target::arena_available` and the `arena_set` flag the iterator-less tests
// observe through their own `_arena` field.
pub fn parse<T, I>(dst: &mut T, iter: &mut I) -> Result<(), ParseError>
where
    T: Target,
    I: ArgIterator + LocationProvider,
{
    // const info = @typeInfo(T);
    // assert(info == .@"struct");

    // Make an arena for all our allocations if we support it. Otherwise,
    // use an allocator that always fails. If the arena is already set on
    // the config, then we reuse that. See memory note in parse docs.
    //
    // PORT NOTE: the reference creates (or reuses) `dst._arena` here and marks
    // ownership for cleanup on error. In Rust each parsed field owns its bytes,
    // so there is no shared bump arena to create; the `_arena != null` invariant
    // the tests assert is maintained by the `Target` setting its own `_arena`
    // flag on first parse (see the test structs). The `FailingAllocator` branch
    // for non-arena types is likewise unobservable here (no allocation fails),
    // matching that those code paths only ever allocate for `[]const u8`-like
    // fields, which the arena-less test structs never use destructively.
    //
    // The reference arena bookkeeping, transcribed verbatim for comment fidelity:
    //     const arena_available = @hasField(T, "_arena");
    //     var arena_owned: bool = false;
    //     const arena_alloc = if (arena_available) arena: {
    //         // If the arena is unset, we create it. We mark that we own it
    //         // only so that we can clean it up on error.
    //         if (dst._arena == null) {
    //             dst._arena = .init(alloc);
    //             arena_owned = true;
    //         }
    //         break :arena dst._arena.?.allocator();
    //     } else fail: {
    //         // Note: this is... not safe...
    //         var fail = std.testing.FailingAllocator.init(alloc, .{});
    //         break :fail fail.allocator();
    //     };
    //     errdefer if (arena_available and arena_owned) {
    //         dst._arena.?.deinit();
    //         dst._arena = null;
    //     };

    // while (iter.next()) |arg| {
    while let Some(arg) = iter.next() {
        let arg: &[u8] = arg.as_slice();
        // Do manual parsing if we have a hook for it.
        // if (@hasDecl(T, "parseManuallyHook")) {
        //     if (!try dst.parseManuallyHook(arena_alloc, arg, iter)) return;
        // }
        if let Some(res) = dst.parse_manually_hook(arg) {
            match res? {
                HookOutcome::Continue => {}
                HookOutcome::Halt => return Ok(()),
                HookOutcome::ConsumeRest => {
                    // The `-e` drain (see `parse_self` for the verbatim
                    // reference body). The plain `parse` driver drains via
                    // `iter.next()`.
                    let mut command: Vec<Vec<u8>> = Vec::new();
                    while let Some(param) = iter.next() {
                        let param = param.as_slice().to_vec();
                        dst.record_replay_arg(&param);
                        command.push(param);
                    }
                    let location = iter.location().ok().flatten().unwrap_or(Location::None);
                    if !dst.apply_e_command(command, location) {
                        return Ok(());
                    }
                }
            }
        }

        // If the destination supports help then we check for it, call
        // the help function and return.
        // if (@hasDecl(T, "help")) {
        //     if (mem.eql(u8, arg, "--help") or mem.eql(u8, arg, "-h")) {
        //         try dst.help();
        //         return;
        //     }
        // }
        // PORT NOTE: the nested `if (@hasDecl(T, "help")) { if (--help/-h) }` is
        // kept as two `if`s to mirror the reference's structure (the outer guard
        // is the comptime decl check, the inner is the runtime arg match).
        #[allow(clippy::collapsible_if)]
        if dst.has_help_decl() {
            if arg == b"--help" || arg == b"-h" {
                if let Some(res) = dst.help() {
                    res?;
                }
                return Ok(());
            }
        }

        // If this doesn't start with "--" then it isn't a config
        // flag. We don't support positional arguments or configuration
        // values set with spaces so this is an error.
        // if (!mem.startsWith(u8, arg, "--")) {
        if !arg.starts_with(b"--") {
            // if (comptime !canTrackDiags(T)) return Error.InvalidField;
            if !dst.can_track_diags() {
                return Err(Error::InvalidField);
            }

            // Add our diagnostic
            // try dst._diagnostics.append(arena_alloc, .{
            //     .key = try arena_alloc.dupeZ(u8, arg),
            //     .message = "invalid field",
            //     .location = try diags.Location.fromIter(iter, arena_alloc),
            // });
            let location = Location::from_iter(iter);
            dst.diagnostics_mut().unwrap().append(Diagnostic {
                key: arg.to_vec(),
                message: b"invalid field".to_vec(),
                location,
            });

            // continue;
            continue;
        }

        // var key: []const u8 = arg[2..];
        let mut key: &[u8] = &arg[2..];
        // const value: ?[]const u8 = value: {
        //     // If the arg has "=" then the value is after the "=".
        //     if (mem.indexOf(u8, key, "=")) |idx| {
        //         defer key = key[0..idx];
        //         break :value key[idx + 1 ..];
        //     }
        //     break :value null;
        // };
        let value: Option<&[u8]> = if let Some(idx) = index_of(key, b"=") {
            let v = &key[idx + 1..];
            key = &key[0..idx];
            Some(v)
        } else {
            None
        };

        // parseIntoField(T, arena_alloc, dst, key, value) catch |err| err: {
        match dst.parse_into_field(key, value) {
            Ok(()) => {}
            Err(err) => 'err: {
                // If we get an error parsing a field, then we try to fall
                // back to compatibility handlers if able.
                // if (@hasDecl(T, "compatibility")) {
                //     // If we have a compatibility handler for this key, then
                //     // we call it and see if it handles the error.
                //     if (T.compatibility.get(key)) |handler| {
                //         if (handler(dst, arena_alloc, key, value)) {
                //             log.info(...);
                //             break :err;
                //         }
                //     }
                // }
                if let Some(handler) = dst.compatibility(key) {
                    if handler(dst, key, value) {
                        // log.info("compatibility handler for {s} handled error, you may be using a deprecated field: {}", .{ key, err });
                        //
                        // PORT NOTE (N11): Zig's `{}` on an error renders
                        // `error.<Name>`; `{err:?}` renders the bare variant
                        // name. Workspace-dominant rendering; N11's scope.
                        log::info!(
                            target: LOG_SCOPE,
                            "compatibility handler for {} handled error, you may be using a deprecated field: {err:?}",
                            String::from_utf8_lossy(key),
                        );
                        break 'err;
                    }
                }

                // if (comptime !canTrackDiags(T)) return err;
                if !dst.can_track_diags() {
                    return Err(err);
                }

                // The error set is dependent on comptime T, so we always add
                // an extra error so we can have the "else" below.
                // const ErrSet = @TypeOf(err) || error{ Unknown, OutOfMemory } || Error;
                // const message: [:0]const u8 = switch (@as(ErrSet, @errorCast(err))) {
                let message: Vec<u8> = match err {
                    // OOM is not recoverable since we need to allocate to
                    // track more error messages.
                    // error.OutOfMemory => return err,
                    //
                    // PORT NOTE: `OutOfMemory` is not a recoverable error in the
                    // Rust port (allocation aborts), so there is no arm for it.

                    // error.InvalidField => "unknown field",
                    Error::InvalidField => b"unknown field".to_vec(),
                    // error.ValueRequired => formatValueRequired(T, arena_alloc, key) catch "value required",
                    Error::ValueRequired => format_value_required(dst, key),
                    // error.InvalidValue => formatInvalidValue(T, arena_alloc, key, value) catch "invalid value",
                    Error::InvalidValue => format_invalid_value(dst, key, value),
                    // else => try std.fmt.allocPrintSentinel(arena_alloc, "unknown error {}", .{err}, 0),
                    //
                    // PORT NOTE: the `else` arm covers type-specific errors
                    // beyond the base set: the three `CommaSplitter` errors a
                    // `parseAutoStruct` field propagates, and `RepeatableLink`'s
                    // `NotImplemented` (S76-F2). `ActionHelpRequested`
                    // is returned by `help()` before field parsing, never by
                    // `parse_into_field`, so it never reaches here.
                    //
                    // CORRECTION (parity S67-F5, task T-C): this note used to say
                    // "every parse failure from `parse_into_field` maps to one of
                    // the base variants above, so this arm is unreachable HERE";
                    // that was the collapse S67-F5 records, now removed.
                    Error::UnclosedQuote
                    | Error::UnfinishedEscape
                    | Error::IllegalEscape
                    | Error::ActionHelpRequested
                    | Error::NotImplemented => format_unknown_error(err),
                };

                // Add our diagnostic
                // try dst._diagnostics.append(arena_alloc, .{
                //     .key = try arena_alloc.dupeZ(u8, key),
                //     .message = message,
                //     .location = try diags.Location.fromIter(iter, arena_alloc),
                // });
                let location = Location::from_iter(iter);
                dst.diagnostics_mut().unwrap().append(Diagnostic {
                    key: key.to_vec(),
                    message,
                    location,
                });
            }
        }
    }

    Ok(())
}

/// PORT NOTE (Replay aliasing): the reference `Replay.Iterator` holds
/// `config: *Config` while `cli.args.parse` is simultaneously called with the
/// SAME `*Config` as `dst` (see `Config.loadIter`/`changeConditionalState`/
/// `loadTheme`). Each `Replay.Iterator.next` mutates that config (expanding paths
/// and appending diagnostics) before yielding the arg that `parse` then parses
/// into the very same config. In Rust two `&mut Config` cannot coexist, and the
/// raw-ptr-aliasing workaround is forbidden (Stacked-Borrows UB; see memory
/// `rust-mut-args-strongly-protected`).
///
/// The faithful, sound resolution is `SelfDrivenIterator` + `parse_self`: an
/// iterator whose `next` receives `&mut T` (the destination) as a parameter
/// rather than capturing it. `parse_self` owns the single `&mut T` and lends it
/// to the iterator's `next` for the duration of that call, then uses it for field
/// parsing — exactly the reference's interleaving, with no aliasing. Non-replay
/// iterators (`LineIterator`/`SliceIterator`/`ArgsIterator`) ignore the lent
/// destination, so `parse` (above) remains for them; `parse_self` is the variant
/// `Config.loadIter` routes through so a single code path serves both.
pub trait SelfDrivenIterator<T> {
    /// `next`, but the destination is lent in. Replay's `next` mutates it
    /// (expand/diagnostic) before returning the arg; other iterators ignore it.
    fn next_self(&mut self, dst: &mut T) -> Option<Vec<u8>>;

    /// `iter.location(alloc)` — the diagnostic location for the current item.
    fn location_self(&self) -> Option<Location>;
}

/// Any plain `ArgIterator + LocationProvider` (the file/CLI/slice iterators) is a
/// `SelfDrivenIterator` that simply ignores the lent destination. This lets
/// `Config.loadIter` route ALL iterators — replay and non-replay — through the
/// single `parse_self` path, matching the reference's single `cli.args.parse`
/// call site. The Replay iterator (which DOES use the destination) provides its
/// own `SelfDrivenIterator` impl in the config crate and deliberately does NOT
/// implement `ArgIterator`/`LocationProvider`, so there is no coherence overlap.
impl<T, I> SelfDrivenIterator<T> for I
where
    I: ArgIterator<Item = Vec<u8>> + LocationProvider,
{
    fn next_self(&mut self, _dst: &mut T) -> Option<Vec<u8>> {
        ArgIterator::next(self)
    }

    fn location_self(&self) -> Option<Location> {
        self.location().ok().flatten()
    }
}

/// `parse`, but driven by a `SelfDrivenIterator` whose `next` is lent `&mut T`.
/// This is the aliasing-safe analogue of the reference calling `cli.args.parse`
/// with a `Replay.Iterator` that captures the same `*Config` as `dst`. The body
/// is line-for-line the same as `parse`; only the `iter.next()`/`iter.location()`
/// calls become `iter.next_self(dst)`/`iter.location_self()`.
pub fn parse_self<T, I>(dst: &mut T, iter: &mut I) -> Result<(), ParseError>
where
    T: Target,
    I: SelfDrivenIterator<T>,
{
    // while (iter.next()) |arg| {
    while let Some(arg) = iter.next_self(dst) {
        let arg: &[u8] = arg.as_slice();
        // Do manual parsing if we have a hook for it.
        // if (@hasDecl(T, "parseManuallyHook")) {
        //     if (!try dst.parseManuallyHook(arena_alloc, arg, iter)) return;
        // }
        if let Some(res) = dst.parse_manually_hook(arg) {
            match res? {
                HookOutcome::Continue => {}
                HookOutcome::Halt => return Ok(()),
                HookOutcome::ConsumeRest => {
                    // The `-e` drain: PORTS the reference's
                    //   while (iter.next()) |param| {
                    //       const copy = try alloc.dupeZ(u8, param);
                    //       try self._replay_steps.append(alloc, .{ .arg = copy });
                    //       try command.append(alloc, copy);
                    //   }
                    // run here because the driver owns both `dst` and `iter`
                    // without aliasing. Each drained arg is recorded as a replay
                    // step (via `dst.record_replay_arg`) and collected into the
                    // command, then `apply_e_command` finishes the branch.
                    let mut command: Vec<Vec<u8>> = Vec::new();
                    while let Some(param) = iter.next_self(dst) {
                        dst.record_replay_arg(&param);
                        command.push(param);
                    }
                    let location = iter.location_self().unwrap_or(Location::None);
                    if !dst.apply_e_command(command, location) {
                        return Ok(());
                    }
                }
            }
        }

        // If the destination supports help then we check for it, call
        // the help function and return.
        #[allow(clippy::collapsible_if)]
        if dst.has_help_decl() {
            if arg == b"--help" || arg == b"-h" {
                if let Some(res) = dst.help() {
                    res?;
                }
                return Ok(());
            }
        }

        // If this doesn't start with "--" then it isn't a config flag.
        // if (!mem.startsWith(u8, arg, "--")) {
        if !arg.starts_with(b"--") {
            if !dst.can_track_diags() {
                return Err(Error::InvalidField);
            }
            let location = iter.location_self().unwrap_or(Location::None);
            dst.diagnostics_mut().unwrap().append(Diagnostic {
                key: arg.to_vec(),
                message: b"invalid field".to_vec(),
                location,
            });
            continue;
        }

        // var key: []const u8 = arg[2..];
        let mut key: &[u8] = &arg[2..];
        let value: Option<&[u8]> = if let Some(idx) = index_of(key, b"=") {
            let v = &key[idx + 1..];
            key = &key[0..idx];
            Some(v)
        } else {
            None
        };

        // parseIntoField(...) catch |err| ...
        match dst.parse_into_field(key, value) {
            Ok(()) => {}
            Err(err) => 'err: {
                if let Some(handler) = dst.compatibility(key) {
                    if handler(dst, key, value) {
                        break 'err;
                    }
                }

                if !dst.can_track_diags() {
                    return Err(err);
                }

                let message: Vec<u8> = match err {
                    Error::InvalidField => b"unknown field".to_vec(),
                    Error::ValueRequired => format_value_required(dst, key),
                    Error::InvalidValue => format_invalid_value(dst, key, value),
                    // See the `parse` twin: the help sentinel is emitted by
                    // `help()` before field parsing, so it never reaches here.
                    Error::UnclosedQuote
                    | Error::UnfinishedEscape
                    | Error::IllegalEscape
                    | Error::ActionHelpRequested
                    | Error::NotImplemented => format_unknown_error(err),
                };

                let location = iter.location_self().unwrap_or(Location::None);
                dst.diagnostics_mut().unwrap().append(Diagnostic {
                    key: key.to_vec(),
                    message,
                    location,
                });
            }
        }
    }

    Ok(())
}

/// Convenience function to create a compatibility handler that
/// renames a field from `from` to `to`.
// pub fn compatibilityRenamed(comptime T: type, comptime to: []const u8) CompatibilityHandler(T) {
//
// PORT NOTE: the reference generates a closure that calls
// `parseIntoField(T, alloc, dst, to, value)` and returns true/false. In Rust we
// cannot bind the comptime `to` into a bare `fn` pointer, so this returns a
// boxed handler closure. Callers store it the same way the reference stores the
// generated handler in the `compatibility` map.
pub fn compatibility_renamed<T: Target>(to: &'static [u8]) -> BoxedCompatibilityHandler<T> {
    // comptime assert(@hasField(T, to));
    Box::new(
        move |dst: &mut T, key: &[u8], value: Option<&[u8]>| -> bool {
            // _ = key;
            let _ = key;
            // parseIntoField(T, alloc, dst, to, value) catch |err| {
            match dst.parse_into_field(to, value) {
                Ok(()) => {}
                Err(err) => {
                    // log.warn("error parsing renamed field {s}: {}", .{ to, err });
                    //
                    // PORT NOTE (N11): see the `{err:?}` note in `parse`.
                    log::warn!(
                        target: LOG_SCOPE,
                        "error parsing renamed field {}: {err:?}",
                        String::from_utf8_lossy(to),
                    );
                    // return false;
                    return false;
                }
            }
            // return true;
            true
        },
    )
}

// fn formatValueRequired(comptime T: type, arena_alloc: std.mem.Allocator, key: []const u8) ...
fn format_value_required<T: Target>(dst: &T, key: &[u8]) -> Vec<u8> {
    // var stream: std.Io.Writer.Allocating = .init(arena_alloc);
    // const writer = &stream.writer;
    let mut stream: Vec<u8> = Vec::new();

    // try writer.print("value required", .{});
    stream.extend_from_slice(b"value required");
    // try formatValues(T, key, writer);
    stream.extend_from_slice(&dst.format_values(key));
    // try writer.writeByte(0);
    // const written = stream.written();
    // return written[0 .. written.len - 1 :0];
    //
    // PORT NOTE: the reference writes a trailing 0 sentinel then returns the
    // slice excluding it. The Rust port returns the message bytes without the
    // sentinel (owned `Vec<u8>`).
    stream
}

// fn formatInvalidValue(comptime T: type, arena_alloc: std.mem.Allocator, key: []const u8, value: ?[]const u8) ...
fn format_invalid_value<T: Target>(dst: &T, key: &[u8], value: Option<&[u8]>) -> Vec<u8> {
    // var stream: std.Io.Writer.Allocating = .init(arena_alloc);
    // const writer = &stream.writer;
    let mut stream: Vec<u8> = Vec::new();

    // try writer.print("invalid value \"{?s}\"", .{value});
    stream.extend_from_slice(b"invalid value \"");
    if let Some(v) = value {
        stream.extend_from_slice(v);
    }
    stream.extend_from_slice(b"\"");
    // try formatValues(T, key, writer);
    stream.extend_from_slice(&dst.format_values(key));
    // try writer.writeByte(0);
    // const written = stream.written();
    // return written[0 .. written.len - 1 :0];
    stream
}

// fn formatValues(comptime T: type, key: []const u8, writer: *std.Io.Writer) ...
//
// PORT NOTE: `formatValues` enumerates an enum field's valid values. The
// comptime field reflection becomes `Target::format_values`, which each
// destination implements (it knows its own enum fields); the default returns no
// suffix (no matching enum field), exactly as the reference writes nothing for
// non-enum fields.
//
// CORRECTION (parity S67-F1, task T-C): the note above used to be the whole
// story, and no destination implemented `format_values`, so the suffix was
// never emitted. `format_enum_values` below is the `.@"enum"` arm; the two
// destinations with enum-typed fields AND `_diagnostics` (the only ones that
// reach `formatValues`) call it: `rustty-config`'s `Config` and
// `rustty-cli`'s `show_face::Options` (`style`).

/// The `.@"enum" => |e|` arm of `formatValues`: `, valid values are: ` then
/// each variant name in declaration order, comma-separated. `names` is
/// `e.fields |field| field.name`.
pub fn format_enum_values(names: &[&[u8]]) -> Vec<u8> {
    let mut writer: Vec<u8> = Vec::new();
    // try writer.print(", valid values are: ", .{});
    writer.extend_from_slice(b", valid values are: ");
    // inline for (e.fields, 0..) |field, i| {
    for (i, name) in names.iter().enumerate() {
        // if (i != 0) try writer.print(", ", .{});
        if i != 0 {
            writer.extend_from_slice(b", ");
        }
        // try writer.print("{s}", .{field.name});
        writer.extend_from_slice(name);
    }
    writer
}

// Parse a single key/value pair into the destination type T.
//
// This may result in allocations. The allocations can only be freed by freeing
// all the memory associated with alloc. It is expected that alloc points to
// an arena.
// pub fn parseIntoField(comptime T: type, alloc: Allocator, dst: *T, key: []const u8, value: ?[]const u8) !void {
//
// PORT NOTE: this is `Target::parse_into_field`. The body in the reference is
// `inline for (info.@"struct".fields)` with a `@typeInfo(field.type)` dispatch.
// Each destination implements that dispatch concretely (the per-type reflection
// equivalent), using the standalone helpers below (`parse_bool`, `parse_struct`,
// etc.) for the value-by-type "magic". `field.name[0] != '_'` (skip underscore-
// prefixed fields) and `error.InvalidField` for no match are part of each
// implementation. The helpers reproduce the exact `switch (Field)` arms.

// pub fn parseTaggedUnion(comptime T: type, alloc: Allocator, v: []const u8) !T {
//
// PORT NOTE: `parseTaggedUnion` parses `tag:value` into a tagged union. The
// comptime `@Type(...struct...)` synthesis (building a one-field struct to reuse
// `parseIntoField`) cannot be reproduced in Rust; instead a destination's union
// field parses the tag and value directly. The generic tag-splitting logic is
// exposed as `split_tagged_union` so implementors share the exact whitespace/
// colon handling.

/// Splits a tagged-union value `v` into `(tag, value)` the way
/// `parseTaggedUnion` does: the tag is everything before the first `:` (trimmed),
/// and the value is everything after (or `""` if no colon).
//
// The reference body (transcribed for comment fidelity; the per-variant parse is
// performed by each union `Target`'s `parse_into_field`):
//
//     const info = @typeInfo(T).@"union";
//     assert(@typeInfo(info.tag_type.?) == .@"enum");
//
//     // Get the union tag that is being set. We support values with no colon
//     // if the value is void so its not an error to have no colon.
//     const colon_idx = mem.indexOf(u8, v, ":") orelse v.len;
//     const tag_str = std.mem.trim(u8, v[0..colon_idx], whitespace);
//     const value = if (colon_idx < v.len) v[colon_idx + 1 ..] else "";
//
//     // Find the field in the union that matches the tag.
//     inline for (info.fields) |field| {
//         if (mem.eql(u8, field.name, tag_str)) {
//             // Special case void types where we don't need a value.
//             if (field.type == void) {
//                 if (value.len > 0) return error.InvalidValue;
//                 return @unionInit(T, field.name, {});
//             }
//             // We need to create a struct that looks like this union field.
//             // This lets us use parseIntoField as if its a dedicated struct.
//             const Target = @Type(.{ .@"struct" = .{ ... } });
//             // Parse the value into the struct
//             var t: Target = undefined;
//             try parseIntoField(Target, alloc, &t, field.name, value);
//             // Build our union
//             return @unionInit(T, field.name, @field(t, field.name));
//         }
//     }
//     return error.InvalidValue;
pub fn split_tagged_union(v: &[u8]) -> (&[u8], &[u8]) {
    let colon_idx = index_of(v, b":").unwrap_or(v.len());
    let tag_str = trim(&v[0..colon_idx], WHITESPACE);
    let value: &[u8] = if colon_idx < v.len() {
        &v[colon_idx + 1..]
    } else {
        b""
    };
    (tag_str, value)
}

// fn parseStruct(comptime T: type, alloc: Allocator, v: []const u8) !T {
//     return switch (@typeInfo(T).@"struct".layout) {
//         .auto => parseAutoStruct(T, alloc, v, null),
//         .@"packed" => parsePackedStruct(T, v),
//         else => @compileError("unsupported struct layout"),
//     };
// }
//
// PORT NOTE: the layout dispatch (`auto` vs `packed`) is a comptime property of
// the field type; in Rust the destination calls `parse_auto_struct` or
// `parse_packed_struct` directly depending on how its field is declared.

/// PORTS: ghostty `AutoStructField` — the per-field contract `parse_auto_struct`
/// needs from an auto-layout struct. The reference reflects over the struct's
/// fields (name, type, default) at comptime; here the destination provides them
/// dynamically: `set_field` performs the per-field name+type parse (==
/// `parseIntoField(T, ...)` on the inner struct), and `field_names` lists the
/// field names in declaration order so required/default tracking matches.
pub trait AutoStruct: Sized {
    /// Field names in declaration order (`info.fields`).
    fn field_names() -> &'static [&'static [u8]];

    /// Set field `key` to the parsed `value` (`parseIntoField` on the inner
    /// struct). Returns `Ok(())` on success. The implementor performs the type
    /// dispatch for its own fields.
    fn set_field(&mut self, key: &[u8], value: Option<&[u8]>) -> Result<(), ParseError>;

    /// Apply the declared default for field at index `i` (used for fields not
    /// set). Returns `Err(InvalidValue)` if the field has no default (a required
    /// field that was not provided).
    fn apply_default(&mut self, i: usize) -> Result<(), ParseError>;
}

// pub fn parseAutoStruct(comptime T: type, alloc: Allocator, v: []const u8, default_: ?T) !T {
//
// PORT NOTE: `default_: ?T` (inherit defaults from a provided value) is folded
// into the destination's `apply_default` (it already knows its own defaults);
// the reference's two branches (inherit from `default` vs use struct defaults)
// are both "the field's default value", which `apply_default` provides. The
// comma-splitting / quoted-string-literal decoding logic is reproduced verbatim.
pub fn parse_auto_struct<T: AutoStruct>(result: &mut T, v: &[u8]) -> Result<(), ParseError> {
    // const info = @typeInfo(T).@"struct";
    // comptime assert(info.layout == .auto);

    // We start our result as undefined so we don't get an error for required
    // fields. We track required fields below and we validate that we set them
    // all at the bottom of this function (in addition to setting defaults for
    // optionals).
    // var result: T = undefined;

    // Keep track of which fields were set so we can error if a required
    // field was not set.
    // const FieldSet = std.StaticBitSet(info.fields.len);
    // var fields_set: FieldSet = .initEmpty();
    let field_names = T::field_names();
    let mut fields_set = vec![false; field_names.len()];

    // We split each value by "," allowing for quoting and escaping.
    // var iter: CommaSplitter = .init(v);
    let mut iter = CommaSplitter::init(v);
    // loop: while (try iter.next()) |entry| {
    'loop_: while let Some(entry) = iter.next().map_err(Error::from)? {
        // Find the key/value, trimming whitespace. The value may be quoted
        // which we strip the quotes from.
        // const idx = mem.indexOf(u8, entry, ":") orelse return error.InvalidValue;
        let idx = index_of(entry, b":").ok_or(Error::InvalidValue)?;
        // const key = std.mem.trim(u8, entry[0..idx], whitespace);
        let key = trim(&entry[0..idx], WHITESPACE);

        // used if we need to decode a double-quoted string.
        // var buf: std.Io.Writer.Allocating = .init(alloc);
        // defer buf.deinit();
        let mut buf: Vec<u8>;

        // const value = value: {
        let value: &[u8] = {
            // const value = std.mem.trim(u8, entry[idx + 1 ..], whitespace);
            let value = trim(&entry[idx + 1..], WHITESPACE);

            // Detect a quoted string.
            // if (value.len >= 2 and value[0] == '"' and value[value.len - 1] == '"') {
            if value.len() >= 2 && value[0] == b'"' && value[value.len() - 1] == b'"' {
                // Decode a double-quoted string as a Zig string literal.
                // const parsed = try std.zig.string_literal.parseWrite(&buf.writer, value);
                // if (parsed == .failure) return error.InvalidValue;
                // break :value buf.written();
                buf = Vec::new();
                match zig_string_literal_parse_write(&mut buf, value) {
                    ParseWriteResult::Success => {}
                    ParseWriteResult::Failure => return Err(Error::InvalidValue),
                }
                &buf[..]
            } else {
                // break :value value;
                value
            }
        };

        // inline for (info.fields, 0..) |field, i| {
        for (i, field_name) in field_names.iter().enumerate() {
            // if (std.mem.eql(u8, field.name, key)) {
            if *field_name == key {
                // try parseIntoField(T, alloc, &result, key, value);
                result.set_field(key, Some(value))?;
                // fields_set.set(i);
                fields_set[i] = true;
                // continue :loop;
                continue 'loop_;
            }
        }

        // No field matched
        // return error.InvalidValue;
        return Err(Error::InvalidValue);
    }

    // Ensure all required fields are set
    // inline for (info.fields, 0..) |field, i| {
    //
    // PORT NOTE: indexed loop mirrors the reference's `inline for (info.fields,
    // 0..) |field, i|`; `i` indexes `fields_set` (and the field's default via
    // `apply_default(i)`), so a range loop is the faithful form.
    #[allow(clippy::needless_range_loop)]
    for i in 0..field_names.len() {
        // if (!fields_set.isSet(i)) {
        if !fields_set[i] {
            // @field(result, field.name) = default: {
            //     // If we're given a default value then we inherit those.
            //     // Otherwise we use the default values as specified by the
            //     // struct.
            //     if (default_) |default| {
            //         break :default @field(default, field.name);
            //     } else {
            //         const default_ptr = field.default_value_ptr orelse return error.InvalidValue;
            //         const typed_ptr: *const field.type = @ptrCast(@alignCast(default_ptr));
            //         break :default typed_ptr.*;
            //     }
            // };
            //
            // PORT NOTE: the reference picks the field default either from the
            // supplied `default_` value or from the struct's declared default,
            // erroring (`InvalidValue`) if neither exists. `apply_default`
            // encapsulates that choice for each field.
            result.apply_default(i)?;
        }
    }

    Ok(())
}

/// PORTS: ghostty `PackedStruct` — the per-field contract `parse_packed_struct`
/// needs from a packed (all-`bool`) struct. The reference asserts every field is
/// `bool`; here `set_all` sets every field to a single bool (the "true"/"false"
/// shorthand) and `set_field` sets one named field, both reproducing the exact
/// `inline for` + `@field(result, field.name) = value`.
pub trait PackedStruct: Default {
    /// `inline for (info.fields) |field| @field(result, field.name) = b;` —
    /// set every field to `b`.
    fn set_all(&mut self, b: bool);

    /// Set field named `part` to `value`. Returns `true` if a field matched.
    fn set_field(&mut self, part: &[u8], value: bool) -> bool;

    /// The packed field names in declaration order (`info.fields |f| f.name`),
    /// each the Zig field-name literal (hyphenated for hyphenated fields). Mirrors
    /// [`AutoStruct::field_names`]; consumed by the shell-completion generators
    /// (`rustty-extra`), whose packed-flag arm emits `name`/`no-name` per entry.
    fn field_names() -> &'static [&'static [u8]];
}

// pub fn parsePackedStruct(comptime T: type, v: []const u8) !T {
pub fn parse_packed_struct<T: PackedStruct>(v: &[u8]) -> Result<T, ParseError> {
    // const info = @typeInfo(T).@"struct";
    // comptime assert(info.layout == .@"packed");

    // var result: T = .{};
    let mut result = T::default();

    // Allow standalone boolean values like "true" and "false" to
    // turn on or off all of the struct's fields.
    // bools: {
    'bools: {
        // const b = parseBool(v) catch break :bools;
        let b = match parse_bool(v) {
            Ok(b) => b,
            Err(_) => break 'bools,
        };
        // inline for (info.fields) |field| {
        //     assert(field.type == bool);
        //     @field(result, field.name) = b;
        // }
        result.set_all(b);
        // return result;
        return Ok(result);
    }

    // We split each value by ","
    // var iter = std.mem.splitSequence(u8, v, ",");
    // loop: while (iter.next()) |part_raw| {
    'loop_: for part_raw in split_sequence(v, b",") {
        // Determine the field we're looking for and the value. If the
        // field is prefixed with "no-" then we set the value to false.
        // const part, const value = part: {
        let (part, value): (&[u8], bool) = {
            // const negation_prefix = "no-";
            let negation_prefix: &[u8] = b"no-";
            // const trimmed = std.mem.trim(u8, part_raw, whitespace);
            let trimmed = trim(part_raw, WHITESPACE);
            // if (std.mem.startsWith(u8, trimmed, negation_prefix)) {
            if trimmed.starts_with(negation_prefix) {
                // break :part .{ trimmed[negation_prefix.len..], false };
                (&trimmed[negation_prefix.len()..], false)
            } else {
                // break :part .{ trimmed, true };
                (trimmed, true)
            }
        };

        // inline for (info.fields) |field| {
        //     assert(field.type == bool);
        //     if (std.mem.eql(u8, field.name, part)) {
        //         @field(result, field.name) = value;
        //         continue :loop;
        //     }
        // }
        if result.set_field(part, value) {
            continue 'loop_;
        }

        // No field matched
        // return error.InvalidValue;
        return Err(Error::InvalidValue);
    }

    Ok(result)
}

// pub fn parseBool(v: []const u8) !bool {
pub fn parse_bool(v: &[u8]) -> Result<bool, ParseError> {
    // const t = &[_][]const u8{ "1", "t", "T", "true" };
    let t: &[&[u8]] = &[b"1", b"t", b"T", b"true"];
    // const f = &[_][]const u8{ "0", "f", "F", "false" };
    let f: &[&[u8]] = &[b"0", b"f", b"F", b"false"];

    // inline for (t) |str| {
    //     if (mem.eql(u8, v, str)) return true;
    // }
    for str in t {
        if v == *str {
            return Ok(true);
        }
    }
    // inline for (f) |str| {
    //     if (mem.eql(u8, v, str)) return false;
    // }
    for str in f {
        if v == *str {
            return Ok(false);
        }
    }

    // return error.InvalidValue;
    Err(Error::InvalidValue)
}

// ---------------------------------------------------------------------------
// Value-by-type "magic" helpers used by `Target::parse_into_field`
// implementations. These reproduce the `switch (Field)` arms of the reference's
// `parseIntoField` so each destination's field dispatch is identical.
// ---------------------------------------------------------------------------

/// `field.name[0] != '_'` — the reference's `parseIntoField` SKIPS every
/// underscore-prefixed field, so such a key matches nothing and falls through
/// to `return error.InvalidField`.
//
// PORT NOTE (parity S67-F6): in the reference this rule lives in the generic
// `parseIntoField`; in the port the per-field match moved into each `Target`,
// where it would otherwise be implicit in "the match arms happen not to list
// the `_` fields". That implicitness is what made the ported test for this rule
// vacuous — its test `Target` returned `InvalidField` unconditionally, so it
// asserted its own stub. Making the rule an explicit library predicate gives
// the Targets that HAVE underscore-prefixed fields (`rustty-config`'s `Config`:
// `_arena`, `_diagnostics`, `_replay_steps`, `_conditional_state`, …) one place
// to enforce it, and the test something real to drive.
pub fn is_reserved_key(key: &[u8]) -> bool {
    matches!(key.first(), Some(b'_'))
}

/// `if (value) |v| default: { if (v.len != 0) break :default; ... }` — the
/// "set but EMPTY" value that resets a field to its declared default.
//
// PORT NOTE (parity S67-F6): also a generic `parseIntoField` rule that the
// port's per-`Target` dispatch duplicated at every field of every destination
// (nine production sites before this helper). One spelling, one place to get
// it wrong.
pub fn is_reset_to_default(value: Option<&[u8]>) -> bool {
    matches!(value, Some(v) if v.is_empty())
}

/// The optional-field rule: `const Field = switch (@typeInfo(field.type)) {
/// .optional => |opt| opt.child, else => field.type }` combined with the
/// empty-value reset — an optional field's declared default is `null`, so an
/// empty value clears it and any other value parses as the CHILD type.
pub fn parse_field_optional<T, F>(value: Option<&[u8]>, parse: F) -> Result<Option<T>, ParseError>
where
    F: FnOnce(Option<&[u8]>) -> Result<T, ParseError>,
{
    if is_reset_to_default(value) {
        return Ok(None);
    }
    Ok(Some(parse(value)?))
}

/// The 2/3-arg `parseCLI` rule for an OPTIONAL field:
///
/// ```zig
/// .optional => if (@field(dst, field.name)) |*v| {
///     try v.parseCLI(value);
/// } else {
///     var tmp: Field = undefined;
///     try tmp.parseCLI(value);
///     @field(dst, field.name) = tmp;
/// },
/// ```
///
/// i.e. parse IN PLACE when the field is already set (so the call can append to
/// what is there), and into a fresh instance when it is not. `make` stands in
/// for Zig's `var tmp: Field = undefined`, which Rust has no equivalent of.
pub fn parse_field_optional_in_place<T, M, F>(
    dst: &mut Option<T>,
    value: Option<&[u8]>,
    make: M,
    parse: F,
) -> Result<(), ParseError>
where
    M: FnOnce() -> T,
    F: FnOnce(&mut T, Option<&[u8]>) -> Result<(), ParseError>,
{
    match dst {
        Some(v) => parse(v, value),
        None => {
            let mut tmp = make();
            parse(&mut tmp, value)?;
            *dst = Some(tmp);
            Ok(())
        }
    }
}

/// `[]const u8 => { const slice = value orelse return error.ValueRequired; ... }`.
/// Returns an owned copy of the value bytes (the reference allocates+memcpy into
/// the arena).
pub fn parse_field_string(value: Option<&[u8]>) -> Result<Vec<u8>, ParseError> {
    let slice = value.ok_or(Error::ValueRequired)?;
    Ok(slice.to_vec())
}

/// `[:0]const u8 => { ... allocSentinel ... }`. Same bytes as
/// `parse_field_string`; the sentinel `0` terminator is an implementation detail
/// of the Zig `[:0]` slice (the observable bytes are identical), so the owned
/// `Vec<u8>` carries the value bytes without the trailing sentinel.
pub fn parse_field_sentinel_string(value: Option<&[u8]>) -> Result<Vec<u8>, ParseError> {
    let slice = value.ok_or(Error::ValueRequired)?;
    Ok(slice.to_vec())
}

/// `bool => try parseBool(value orelse "t")`.
pub fn parse_field_bool(value: Option<&[u8]>) -> Result<bool, ParseError> {
    parse_bool(value.unwrap_or(b"t"))
}

/// `inline u8..isize => |Int| std.fmt.parseInt(Int, value orelse return
/// error.ValueRequired, 0) catch return error.InvalidValue`.
//
// PORT NOTE (parity S67-F3): base `0` auto-detects a `0x`/`0o`/`0b` prefix,
// and `std.fmt.parseInt` ignores `_` digit separators (`10_000_000`); both are
// in `std_fmt::parse_int`, a port of the reference's own parser. This helper
// used to hand the digits to Rust's `from_str_radix`, which rejects every `_`,
// rejects `-0` for an unsigned type, and accepts a second sign (`+-5`).
pub fn parse_field_int<T: ZigInt>(value: Option<&[u8]>) -> Result<T, ParseError> {
    let v = value.ok_or(Error::ValueRequired)?;
    std_fmt::parse_int::<T>(v, 0).map_err(|_| Error::InvalidValue)
}

/// `f32, f64 => |Float| std.fmt.parseFloat(Float, value orelse return
/// error.ValueRequired) catch return error.InvalidValue`.
//
// PORT NOTE (parity S67-F4): `std.fmt.parseFloat` accepts `_` digit separators
// and hexadecimal floats (`0x1.8p3`); Rust's `str::parse` accepts neither.
// `std_fmt::parse_float` ports the reference's grammar.
pub fn parse_field_float<T: ZigFloat>(value: Option<&[u8]>) -> Result<T, ParseError> {
    let v = value.ok_or(Error::ValueRequired)?;
    std_fmt::parse_float::<T>(v).map_err(|_| Error::InvalidValue)
}

/// `.@"enum" => std.meta.stringToEnum(Field, value orelse return
/// error.ValueRequired) orelse return error.InvalidValue`.
///
/// `from_name` maps a value to the enum variant (the reference's
/// `stringToEnum`), returning `None` for an unknown variant.
pub fn parse_field_enum<E, F>(value: Option<&[u8]>, from_name: F) -> Result<E, ParseError>
where
    F: Fn(&[u8]) -> Option<E>,
{
    let v = value.ok_or(Error::ValueRequired)?;
    from_name(v).ok_or(Error::InvalidValue)
}

// const u21: parse as u32 then range-check (no native u21).
//
// PORT NOTE: callers needing a `u21` parse as `u32` via `parse_field_int` and
// then bound-check `<= 0x1F_FFFF`; this matches the reference's `u21` parse
// returning `error.Overflow` (=> `InvalidValue`) for out-of-range values.

// ---------------------------------------------------------------------------
// Glue reproducing std functions used above.
// ---------------------------------------------------------------------------

/// `std.mem.indexOf(u8, haystack, needle)` — index of the first occurrence of
/// `needle` in `haystack`, or `None`.
pub fn index_of(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// `std.mem.trim(u8, slice, values)` — trim leading/trailing bytes contained in
/// `values`.
pub fn trim<'a>(slice: &'a [u8], values: &[u8]) -> &'a [u8] {
    let mut start = 0;
    let mut end = slice.len();
    while start < end && values.contains(&slice[start]) {
        start += 1;
    }
    while end > start && values.contains(&slice[end - 1]) {
        end -= 1;
    }
    &slice[start..end]
}

/// `std.mem.splitSequence(u8, buf, delimiter)` — split on (possibly multi-byte)
/// delimiter, yielding empty segments for leading/trailing/consecutive
/// delimiters (always at least one segment).
fn split_sequence<'a>(buf: &'a [u8], delimiter: &'a [u8]) -> SplitSequence<'a> {
    SplitSequence {
        buf: Some(buf),
        delimiter,
    }
}

struct SplitSequence<'a> {
    buf: Option<&'a [u8]>,
    delimiter: &'a [u8],
}

impl<'a> Iterator for SplitSequence<'a> {
    type Item = &'a [u8];

    fn next(&mut self) -> Option<&'a [u8]> {
        let buf = self.buf?;
        match index_of(buf, self.delimiter) {
            Some(idx) => {
                let segment = &buf[..idx];
                self.buf = Some(&buf[idx + self.delimiter.len()..]);
                Some(segment)
            }
            None => {
                self.buf = None;
                Some(buf)
            }
        }
    }
}

/// PORTS: the result of `std.zig.string_literal.parseWrite`.
pub enum ParseWriteResult {
    Success,
    Failure,
}

/// `std.zig.string_literal.parseWrite(writer, bytes)` — decode a double-quoted
/// Zig string literal, writing the decoded bytes to `out`. `bytes` includes the
/// surrounding double quotes. Returns `Failure` on a malformed escape, `Success`
/// otherwise.
//
// PORT NOTE: faithful port of Zig's `std.zig.string_literal.parseWrite` for the
// escape forms the CLI uses: `\n \r \t \\ \" \'`, `\xNN`, `\u{...}`. The
// CommaSplitter has already validated structure for values it splits, but
// `parseAutoStruct` calls this directly on a quoted value, so the full decoder is
// reproduced. On any malformed escape we return `Failure` (== `.failure`).
pub fn zig_string_literal_parse_write(out: &mut Vec<u8>, bytes: &[u8]) -> ParseWriteResult {
    // The slice includes the surrounding quotes; the decoder skips bytes[0] and
    // stops at the closing quote.
    debug_assert!(bytes.len() >= 2 && bytes[0] == b'"' && bytes[bytes.len() - 1] == b'"');
    let inner = &bytes[1..bytes.len() - 1];

    let mut i = 0;
    while i < inner.len() {
        let c = inner[i];
        if c == b'\\' {
            i += 1;
            if i >= inner.len() {
                return ParseWriteResult::Failure;
            }
            match inner[i] {
                b'n' => {
                    out.push(b'\n');
                    i += 1;
                }
                b'r' => {
                    out.push(b'\r');
                    i += 1;
                }
                b't' => {
                    out.push(b'\t');
                    i += 1;
                }
                b'\\' => {
                    out.push(b'\\');
                    i += 1;
                }
                b'"' => {
                    out.push(b'"');
                    i += 1;
                }
                b'\'' => {
                    out.push(b'\'');
                    i += 1;
                }
                b'x' => {
                    i += 1;
                    if i + 2 > inner.len() {
                        return ParseWriteResult::Failure;
                    }
                    let hex = &inner[i..i + 2];
                    let s = match std::str::from_utf8(hex) {
                        Ok(s) => s,
                        Err(_) => return ParseWriteResult::Failure,
                    };
                    match u8::from_str_radix(s, 16) {
                        Ok(b) => out.push(b),
                        Err(_) => return ParseWriteResult::Failure,
                    }
                    i += 2;
                }
                b'u' => {
                    i += 1;
                    if i >= inner.len() || inner[i] != b'{' {
                        return ParseWriteResult::Failure;
                    }
                    i += 1;
                    let start = i;
                    while i < inner.len() && inner[i] != b'}' {
                        i += 1;
                    }
                    if i >= inner.len() {
                        return ParseWriteResult::Failure;
                    }
                    let hex = &inner[start..i];
                    if hex.is_empty() {
                        return ParseWriteResult::Failure;
                    }
                    i += 1; // consume '}'
                    let s = match std::str::from_utf8(hex) {
                        Ok(s) => s,
                        Err(_) => return ParseWriteResult::Failure,
                    };
                    let cp = match u32::from_str_radix(s, 16) {
                        Ok(cp) => cp,
                        Err(_) => return ParseWriteResult::Failure,
                    };
                    match char::from_u32(cp) {
                        Some(ch) => {
                            let mut b = [0u8; 4];
                            out.extend_from_slice(ch.encode_utf8(&mut b).as_bytes());
                        }
                        None => return ParseWriteResult::Failure,
                    }
                }
                _ => return ParseWriteResult::Failure,
            }
        } else {
            out.push(c);
            i += 1;
        }
    }

    ParseWriteResult::Success
}

/// `std.zig.fmtString(bytes)` / `std.zig.stringEscape` — print `bytes` as the
/// escaped contents of a double-quoted Zig string (the inverse of
/// `zig_string_literal_parse_write`). The caller supplies the surrounding quotes
/// (the reference's format string `"\"{f}\""`).
//
// PORT NOTE: faithful port of `std.zig.stringEscape`:
//
//     for (bytes) |byte| switch (byte) {
//         '\n' => try w.writeAll("\\n"),
//         '\r' => try w.writeAll("\\r"),
//         '\t' => try w.writeAll("\\t"),
//         '\\' => try w.writeAll("\\\\"),
//         '"' => try w.writeAll("\\\""),
//         '\'' => try w.writeByte('\''),
//         ' ', '!', '#'...'&', '('...'[', ']'...'~' => try w.writeByte(byte),
//         else => { try w.writeAll("\\x"); try w.printInt(byte, 16, .lower, .{ .width = 2, .fill = '0' }); },
//     };
//
// The printable ranges are the ASCII printables EXCEPT `"` (escaped above) and
// `\` (escaped above); `'` is emitted bare. Everything else becomes `\xNN` (lower
// hex, zero-padded to width 2).
pub fn zig_string_escape(out: &mut Vec<u8>, bytes: &[u8]) {
    for &byte in bytes {
        match byte {
            b'\n' => out.extend_from_slice(b"\\n"),
            b'\r' => out.extend_from_slice(b"\\r"),
            b'\t' => out.extend_from_slice(b"\\t"),
            b'\\' => out.extend_from_slice(b"\\\\"),
            b'"' => out.extend_from_slice(b"\\\""),
            b'\'' => out.push(b'\''),
            // ' ', '!', '#'...'&', '('...'[', ']'...'~'
            b' ' | b'!' | b'#'..=b'&' | b'('..=b'[' | b']'..=b'~' => out.push(byte),
            // else =>
            _ => {
                out.extend_from_slice(b"\\x");
                // printInt(byte, 16, .lower, .{ .width = 2, .fill = '0' })
                let hi = byte >> 4;
                let lo = byte & 0xf;
                out.push(hex_lower_digit(hi));
                out.push(hex_lower_digit(lo));
            }
        }
    }
}

// `printInt(_, 16, .lower, ...)` digit — lowercase hex nibble.
fn hex_lower_digit(n: u8) -> u8 {
    match n {
        0..=9 => b'0' + n,
        _ => b'a' + (n - 10),
    }
}

// ---------------------------------------------------------------------------
// Iterators
// ---------------------------------------------------------------------------

/// PORTS: the `anytype` arg-iterator contract used by `parse`. Every iterator
/// exposes `next() -> ?[]const u8` (the reference duck-types this). In Rust the
/// iterator yields owned/borrowed argument bytes via `next`.
//
// PORT NOTE: `next` returns `Option<ArgBytes>` where `ArgBytes` borrows or owns;
// `as_slice()` views it as `&[u8]`. This lets `LineIterator` (which formats into
// an internal buffer) and `SliceIterator`/`ArgsIterator` (which borrow) share one
// trait.
pub trait ArgIterator {
    type Item: AsArgBytes;
    fn next(&mut self) -> Option<Self::Item>;
}

/// View an iterator item as a byte slice (`[]const u8`).
pub trait AsArgBytes {
    fn as_slice(&self) -> &[u8];
}

impl AsArgBytes for &[u8] {
    fn as_slice(&self) -> &[u8] {
        self
    }
}

impl AsArgBytes for Vec<u8> {
    fn as_slice(&self) -> &[u8] {
        self
    }
}

/// An iterator that considers its location to be CLI args. It
/// iterates through an underlying iterator and increments a counter
/// to track the current CLI arg index.
///
/// This also ignores any argument that starts with `+`. It assumes that
/// actions were parsed out before this iterator was created.
// pub fn ArgsIterator(comptime Iterator: type) type {
//     return struct { ... };
// }
pub struct ArgsIterator<I> {
    /// The underlying args iterator.
    iterator: I,

    /// Our current index into the iterator. This is 1-indexed.
    /// The 0 value is used to indicate that we haven't read any
    /// values yet.
    // index: usize = 0,
    index: usize,
}

impl<I> ArgsIterator<I> {
    pub fn new(iterator: I) -> ArgsIterator<I> {
        ArgsIterator { iterator, index: 0 }
    }
}

// PORT NOTE: the underlying iterator's `deinit` is called via `@hasDecl`; for the
// std/macOS arg iterators that have one, this is exposed via a `Deinit` trait
// implemented below. The `next` skip-`+` logic and `location` are ported as-is.
pub trait Deinit {
    fn deinit(&mut self);
}

impl<I: GenIter> ArgsIterator<I> {
    // pub fn deinit(self: *Self) void {
    //     if (@hasDecl(Iterator, "deinit")) self.iterator.deinit();
    // }
    pub fn deinit(&mut self) {
        self.iterator.deinit();
    }

    // pub fn next(self: *Self) ?[]const u8 {
    // PARITY: named `next` to match the reference's iterator method.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Option<Vec<u8>> {
        // const value = self.iterator.next() orelse return null;
        let value = self.iterator.next()?;
        // self.index += 1;
        self.index += 1;

        // We ignore any argument that starts with "+". This is used
        // to indicate actions and are expected to be parsed out before
        // this iterator is created.
        // if (value.len > 0 and value[0] == '+') return self.next();
        if !value.is_empty() && value[0] == b'+' {
            return self.next();
        }

        // return value;
        Some(value)
    }
}

impl<I: GenIter> ArgIterator for ArgsIterator<I> {
    type Item = Vec<u8>;
    fn next(&mut self) -> Option<Vec<u8>> {
        ArgsIterator::next(self)
    }
}

impl<I> LocationProvider for ArgsIterator<I> {
    /// Returns a location for a diagnostic message.
    // pub fn location(self: *const Self, _: Allocator) error{}!?diags.Location {
    //     return .{ .cli = self.index };
    // }
    fn location(&self) -> Result<Option<Location>, LocationError> {
        Ok(Some(Location::Cli(self.index)))
    }
}

/// PORTS: the duck-typed generic iterator `Iterator` parameter of `ArgsIterator`
/// — yields `?[]const u8` (here `Option<Vec<u8>>`) and has an optional `deinit`.
pub trait GenIter {
    fn next(&mut self) -> Option<Vec<u8>>;
    fn deinit(&mut self) {}
}

/// PORT NOTE (P3-W7 T4): `internal_os.args.ArgIterator.InitError` — the
/// reference's `Allocator.Error`. `src/os/args.zig` was DELETED upstream
/// (`e8525c0fd`), so the uninhabited init-error type moves here with the seam.
/// Allocation failure aborts in Rust rather than being recoverable, so the
/// error set is genuinely empty and every `?` on it is discharged by
/// `match e {}`.
#[derive(Debug, PartialEq, Eq)]
pub enum InitError {}

/// Create an args iterator for the process args. This will skip argv0.
// pub fn argsIterator(alloc_gpa: Allocator) internal_os.args.ArgIterator.InitError!ArgsIterator(internal_os.args.ArgIterator) {
pub fn args_iterator() -> Result<ArgsIterator<OsArgIterator>, InitError> {
    // var iter = try internal_os.args.iterator(alloc_gpa);
    let mut iter = OsArgIterator::new();
    // errdefer iter.deinit();
    // _ = iter.next(); // skip argv0
    let _ = iter.next();
    // return .{ .iterator = iter };
    Ok(ArgsIterator::new(iter))
}

/// PORTS: the process argument iterator, wrapped as a `GenIter` yielding
/// `Option<Vec<u8>>` (byte-slices), which is what the reference's
/// `std.process.ArgIterator` yields (`?[]const u8`).
///
/// PORT NOTE (P3-W7 T4): this used to wrap `rustty_os::args::ArgIterator`,
/// whose macOS arm went through `NSProcessInfo.arguments` and
/// `String::from_utf8_lossy` — so a non-UTF-8 argv element was CORRUPTED.
/// Upstream deleted `src/os/args.zig` outright; on the Rust side the objc
/// detour never had a reason to exist, because `std::env::args_os()` on macOS
/// reads the real process argv (via `_NSGetArgv`) and is valid under
/// `NSApplicationMain`. `OsStr::as_encoded_bytes` is lossless on every
/// platform.
pub struct OsArgIterator(std::env::ArgsOs);

impl OsArgIterator {
    #[must_use]
    pub fn new() -> Self {
        // if (true) return try std.process.argsWithAllocator(allocator);
        OsArgIterator(std::env::args_os())
    }
}

impl Default for OsArgIterator {
    fn default() -> Self {
        Self::new()
    }
}

impl GenIter for OsArgIterator {
    fn next(&mut self) -> Option<Vec<u8>> {
        self.0.next().map(|s| arg_bytes(s.as_os_str()))
    }
}

/// The argv element -> `[]const u8` conversion, named so it can be tested
/// directly.
///
/// PORT NOTE (P3-W7 T4): the deleted `rustty_os::args` macOS arm did
/// `String::from_utf8_lossy(bytes).into_owned()` (`args.rs:172`) here, which
/// REPLACES every non-UTF-8 byte with U+FFFD. `OsStr::as_encoded_bytes` is
/// lossless on every platform (raw bytes on POSIX, WTF-8 on Windows), which is
/// what the reference's `[]const u8` argv always was.
#[must_use]
pub fn arg_bytes(arg: &std::ffi::OsStr) -> Vec<u8> {
    arg.as_encoded_bytes().to_vec()
}

/// The one error `LineIterator::next`'s ported `catch` arm can report.
///
/// PORT NOTE (S67-F2): the reference's arm catches whatever
/// `std.Io.Reader.streamDelimiterEnding` returns and renders it with Zig's
/// `{}`, which prints `error.<Name>`. The port's reader is the whole in-memory
/// source slice, so the ONLY failure reachable there is the fixed writer
/// overflowing `entry[2..]` — `error.WriteFailed`. Spelled as a named type with
/// the reference's own rendering rather than as a bare string, so the wire text
/// is pinned by the type and not by the call site.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineIteratorError {
    WriteFailed,
}

impl std::fmt::Display for LineIteratorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::WriteFailed => f.write_str("error.WriteFailed"),
        }
    }
}

/// Returns an iterator (implements "next") that reads CLI args by line.
/// Each CLI arg is expected to be a single line. This is used to implement
/// configuration files.
// pub const LineIterator = struct {
pub struct LineIterator<'a> {
    /// Our stateful reader.
    // r: *std.Io.Reader,
    //
    // PORT NOTE: `std.Io.Reader` is modeled as a byte cursor over the source
    // bytes. The reference's reader priming / `streamDelimiterEnding` /
    // `discardDelimiterInclusive` reduce, for the in-memory readers the tests
    // use, to "read up to and including the next '\n'"; we hold the source and a
    // cursor, which is byte-faithful for `.fixed`/limited readers.
    r: &'a [u8],
    seek: usize,

    /// Filepath that is used for diagnostics. This is only used for
    /// diagnostic messages so it can be formatted however you want.
    /// It is prefixed to the messages followed by the line number.
    // filepath: []const u8 = "",
    pub filepath: Vec<u8>,

    /// The current line that we're on. This is 1-indexed because
    /// lines are generally 1-indexed in the real world. The value
    /// can be zero if we haven't read any lines yet.
    // line: usize = 0,
    line: usize,

    /// This is the buffer where we store the current entry that
    /// is formatted to be compatible with the parse function.
    // entry: [MAX_LINE_SIZE]u8 = [_]u8{ '-', '-' } ++ ([_]u8{0} ** (MAX_LINE_SIZE - 2)),
    entry: Vec<u8>,
}

impl<'a> LineIterator<'a> {
    /// The maximum size a single line can be. We don't expect any
    /// CLI arg to exceed this size. Can't wait to git blame this in
    /// like 4 years and be wrong about this.
    // pub const MAX_LINE_SIZE = 4096;
    pub const MAX_LINE_SIZE: usize = 4096;

    // pub fn init(reader: *std.Io.Reader) Self {
    //     return .{ .r = reader };
    // }
    pub fn init(reader: &'a [u8]) -> LineIterator<'a> {
        let mut entry = vec![0u8; Self::MAX_LINE_SIZE];
        entry[0] = b'-';
        entry[1] = b'-';
        LineIterator {
            r: reader,
            seek: 0,
            filepath: Vec::new(),
            line: 0,
            entry,
        }
    }

    /// Construct with an explicit filepath (used for diagnostics).
    pub fn with_filepath(reader: &'a [u8], filepath: &[u8]) -> LineIterator<'a> {
        let mut it = Self::init(reader);
        it.filepath = filepath.to_vec();
        it
    }

    // pub fn next(self: *Self) ?[]const u8 {
    // PARITY: named `next` to match the reference's iterator method.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Option<&[u8]> {
        // First prime the reader.
        // File readers at least are initialized with a size of 0,
        // and this will actually prompt the reader to get the actual
        // size of the file, which will be used in the EOF check below.
        //
        // This will also optimize reads down the line as we're
        // more likely to beworking with buffered data.
        //
        // fillMore asserts that the buffer has available capacity,
        // so skip this if it's full.
        // (For in-memory readers this is a no-op.)
        // if (self.r.bufferedLen() < self.r.buffer.len) self.r.fillMore() catch {};

        // var writer: std.Io.Writer = .fixed(self.entry[2..]);
        //
        // PORT NOTE: the entry buffer's first two bytes are the literal `--`; we
        // write the line content starting at offset 2.

        // var entry = while (self.r.seek != self.r.end) {
        let entry_len: usize = loop {
            if self.seek == self.r.len() {
                // } else return null;
                return None;
            }

            // Reset write head; collect bytes up to (not including) '\n'.
            //
            //     _ = self.r.streamDelimiterEnding(&writer, '\n') catch |e| {
            //         log.warn("cannot read from \"{s}\": {}", .{ self.filepath, e });
            //         return null;
            //     };
            //     _ = self.r.discardDelimiterInclusive('\n') catch {};
            let line_start = self.seek;
            let mut line_end = self.seek;
            while line_end < self.r.len() && self.r[line_end] != b'\n' {
                line_end += 1;
            }
            // Copy the line content into entry[2..].
            let content = &self.r[line_start..line_end];
            // S67-F2: the reference writes through a `std.Io.Writer.fixed`
            // over `entry[2..]`, so a line longer than `MAX_LINE_SIZE - 2`
            // makes `streamDelimiterEnding` fail with `error.WriteFailed` —
            // which lands in the SAME `catch` as a read failure, logs, and
            // STOPS iteration. The port had clamped the copy instead, silently
            // truncating the line and parsing on. The reference's behaviour is
            // restored here, diagnostic included.
            if content.len() > Self::MAX_LINE_SIZE - 2 {
                // log.warn("cannot read from \"{s}\": {}", .{ self.filepath, e });
                //
                // PORT NOTE: the port's reader is the whole in-memory source
                // slice, so the only failure `streamDelimiterEnding` can report
                // here is the fixed writer's overflow; the rendered error token
                // is that one error's name.
                log::warn!(
                    target: LOG_SCOPE,
                    "cannot read from \"{}\": {}",
                    String::from_utf8_lossy(&self.filepath),
                    LineIteratorError::WriteFailed,
                );
                // return null;
                return None;
            }
            let n = content.len();
            self.entry[2..2 + n].copy_from_slice(&content[..n]);
            // discard the delimiter inclusive
            self.seek = if line_end < self.r.len() {
                line_end + 1
            } else {
                line_end
            };

            // var entry = writer.buffered();
            let mut cur_len = n;
            // self.line += 1;
            self.line += 1;

            // Trim any whitespace (including CR) around it
            // const trim = std.mem.trim(u8, entry, whitespace ++ "\r");
            let trimmed = trim(&self.entry[2..2 + cur_len], b" \t\r");
            // if (trim.len != entry.len) {
            if trimmed.len() != cur_len {
                // std.mem.copyForwards(u8, entry, trim);
                // entry = entry[0..trim.len];
                let tl = trimmed.len();
                let tmp = trimmed.to_vec();
                self.entry[2..2 + tl].copy_from_slice(&tmp);
                cur_len = tl;
            }

            // Ignore blank lines and comments. If this line consumed the
            // remaining buffer, refill before the loop condition checks for
            // more data.
            //
            // if (entry.len == 0 or entry[0] == '#') {
            //     if (self.r.seek == self.r.end) {
            //         self.r.fillMore() catch {};
            //     }
            //     continue;
            // }
            //
            // PORT NOTE: the reference reads through a streaming
            // `std.Io.Reader`, so a blank/comment line that drained the
            // buffered window must `fillMore` before the loop condition
            // re-checks for data. The port's reader is the whole in-memory
            // source slice (see the `r` field note), so there is no buffered
            // window to refill and `continue` alone is equivalent.
            if cur_len == 0 || self.entry[2] == b'#' {
                continue;
            }
            // break entry;
            break cur_len;
        };

        let mut entry_len = entry_len;

        // if (mem.indexOf(u8, entry, "=")) |idx| {
        if let Some(idx) = index_of(&self.entry[2..2 + entry_len], b"=") {
            // const key = std.mem.trim(u8, entry[0..idx], whitespace);
            let key = trim(&self.entry[2..2 + idx], WHITESPACE).to_vec();
            // const value = value: {
            let value: Vec<u8> = {
                // var value = std.mem.trim(u8, entry[idx + 1 ..], whitespace);
                let mut value = trim(&self.entry[2 + idx + 1..2 + entry_len], WHITESPACE).to_vec();

                // Detect a quoted string.
                // if (value.len >= 2 and value[0] == '"' and value[value.len - 1] == '"') {
                if value.len() >= 2 && value[0] == b'"' && value[value.len() - 1] == b'"' {
                    // Trim quotes since our CLI args processor expects
                    // quotes to already be gone.
                    // value = value[1 .. value.len - 1];
                    value = value[1..value.len() - 1].to_vec();
                }

                // break :value value;
                value
            };

            // const len = key.len + value.len + 1;
            let len = key.len() + value.len() + 1;
            // if (entry.len != len) {
            if entry_len != len {
                // std.mem.copyForwards(u8, entry, key);
                self.entry[2..2 + key.len()].copy_from_slice(&key);
                // entry[key.len] = '=';
                self.entry[2 + key.len()] = b'=';
                // std.mem.copyForwards(u8, entry[key.len + 1 ..], value);
                self.entry[2 + key.len() + 1..2 + key.len() + 1 + value.len()]
                    .copy_from_slice(&value);
                // entry = entry[0..len];
                entry_len = len;
            }
        }

        // We need to reslice so that we include our '--' at the beginning
        // of our buffer so that we can trick the CLI parser to treat it
        // as CLI args.
        // return self.entry[0 .. entry.len + 2];
        Some(&self.entry[0..entry_len + 2])
    }
}

impl<'a> ArgIterator for LineIterator<'a> {
    type Item = Vec<u8>;
    fn next(&mut self) -> Option<Vec<u8>> {
        LineIterator::next(self).map(|s| s.to_vec())
    }
}

impl<'a> LocationProvider for LineIterator<'a> {
    /// Returns a location for a diagnostic message.
    // pub fn location(self: *const Self, alloc: Allocator) Allocator.Error!?diags.Location {
    fn location(&self) -> Result<Option<Location>, LocationError> {
        // If we have no filepath then we have no location.
        // if (self.filepath.len == 0) return null;
        if self.filepath.is_empty() {
            return Ok(None);
        }

        // return .{ .file = .{ .path = try alloc.dupe(u8, self.filepath), .line = self.line } };
        Ok(Some(Location::File {
            path: self.filepath.clone(),
            line: self.line,
        }))
    }
}

/// An iterator valid for arg parsing from a slice.
// pub const SliceIterator = struct {
pub struct SliceIterator<'a> {
    // slice: []const []const u8,
    slice: &'a [&'a [u8]],
    // idx: usize = 0,
    idx: usize,
}

impl<'a> SliceIterator<'a> {
    // pub fn next(self: *Self) ?[]const u8 {
    // PARITY: named `next` to match the reference's iterator method.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Option<&'a [u8]> {
        // if (self.idx >= self.slice.len) return null;
        if self.idx >= self.slice.len() {
            return None;
        }
        // defer self.idx += 1;
        // return self.slice[self.idx];
        let result = self.slice[self.idx];
        self.idx += 1;
        Some(result)
    }
}

impl<'a> ArgIterator for SliceIterator<'a> {
    type Item = &'a [u8];
    fn next(&mut self) -> Option<&'a [u8]> {
        SliceIterator::next(self)
    }
}

// SliceIterator has no `location` decl => `Location.fromIter` yields `.none`.
impl<'a> LocationProvider for SliceIterator<'a> {}

/// Construct a SliceIterator from a slice.
// pub fn sliceIterator(slice: []const []const u8) SliceIterator {
//     return .{ .slice = slice };
// }
pub fn slice_iterator<'a>(slice: &'a [&'a [u8]]) -> SliceIterator<'a> {
    SliceIterator { slice, idx: 0 }
}

#[cfg(test)]
mod tests;
