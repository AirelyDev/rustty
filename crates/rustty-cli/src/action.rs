//! rustty-cli::action
//!
//! PORTS: ghostty src/cli/action.zig — the generic CLI action detector
//! (`DetectError`, `SpecialCase`, `detectIter`/`detectArgs`). Byte-faithful (R1).
//!
//! The reference is generic over `comptime E: type` (the action enum) and uses
//! `@hasDecl(E, "detectSpecialCase")` reflection + `std.meta.stringToEnum`. That
//! comptime surface is threaded here through the [`Action`] trait: `from_plus_name`
//! is `stringToEnum(E, arg[1..])`, and `detect_special_case` is the optional
//! `E.detectSpecialCase` decl (its default — no decl — returns `None`).

// const std = @import("std");
// const Allocator = std.mem.Allocator;

// pub const DetectError = error{
//     MultipleActions,
//     InvalidAction,
// };
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetectError {
    /// Multiple actions were detected. You can specify at most one
    /// action on the CLI otherwise the behavior desired is ambiguous.
    MultipleActions,

    /// An unknown action was specified.
    InvalidAction,
}

/// PORTS: the comptime enum type parameter `E` of `detectIter`/`detectArgs`.
///
/// The reference `E` is "an enum with the available actions" that MAY declare a
/// `detectSpecialCase` fn. This trait threads that comptime reflection:
///   - `from_plus_name` — `std.meta.stringToEnum(E, arg[1..])` (the `+name`
///     lookup): map a `+`-stripped action name to a variant, or `None` if unknown.
///   - `detect_special_case` — the optional `E.detectSpecialCase` decl; the
///     default (no decl) returns `None` (== `!@hasDecl(E, "detectSpecialCase")`).
pub trait Action: Sized + Copy {
    /// `std.meta.stringToEnum(E, name)`.
    fn from_plus_name(name: &[u8]) -> Option<Self>;

    /// The optional `E.detectSpecialCase(arg)` decl. Default: no decl.
    fn detect_special_case(_arg: &[u8]) -> Option<SpecialCase<Self>> {
        None
    }
}

/// The iterator `detectIter` walks (`iter: anytype` whose `next()` yields
/// `?[]const u8`). Each value should yield a CLI argument such as "--foo".
pub trait ArgsIter {
    fn next(&mut self) -> Option<Vec<u8>>;
}

/// Detect the action from CLI args.
// pub fn detectArgs(comptime E: type, alloc: Allocator) !?E {
pub fn detect_args<E: Action>() -> Result<Option<E>, DetectError> {
    // var iter = try std.process.argsWithAllocator(alloc);
    // defer iter.deinit();
    //
    // PORT NOTE: `rustty_os::args::iterator()`'s `InitError` is uninhabited (the
    // reference's `Allocator.Error` — allocation failure aborts in Rust rather
    // than being recoverable), so init never fails; `match e {}` discharges it.
    let mut iter = OsArgs(rustty_os::args::iterator().unwrap_or_else(|e| match e {}));
    // return try detectIter(E, &iter);
    let result = detect_iter::<E, _>(&mut iter);
    iter.0.deinit();
    result
}

/// Adapter: `rustty_os::args::ArgIterator` (yields `Option<String>`) viewed as an
/// [`ArgsIter`] over byte-slices, mirroring the reference's `std.process
/// .ArgIterator` yielding `?[]const u8`. Note the reference iterates ALL args
/// (including argv0) — `detectArgs` does not skip argv0 — which never starts with
/// `+` and never matches a special case, so it is harmless.
struct OsArgs(rustty_os::args::ArgIterator);

impl ArgsIter for OsArgs {
    fn next(&mut self) -> Option<Vec<u8>> {
        self.0.next().map(String::into_bytes)
    }
}

/// Detect the action from any iterator. Each iterator value should yield
/// a CLI argument such as "--foo".
///
/// The comptime type E must be an enum with the available actions.
/// If the type E has a decl `detectSpecialCase`, then it will be called
/// for each argument to allow handling of special cases. The function
/// signature for `detectSpecialCase` should be:
///
///   fn detectSpecialCase(arg: []const u8) ?SpecialCase(E)
///
// pub fn detectIter(comptime E: type, iter: anytype) DetectError!?E {
pub fn detect_iter<E: Action, I: ArgsIter>(iter: &mut I) -> Result<Option<E>, DetectError> {
    // var fallback: ?E = null;
    let mut fallback: Option<E> = None;
    // var pending: ?E = null;
    let mut pending: Option<E> = None;
    // while (iter.next()) |arg| {
    while let Some(arg) = iter.next() {
        // Allow handling of special cases.
        // if (@hasDecl(E, "detectSpecialCase")) special: {
        //     const special = E.detectSpecialCase(arg) orelse break :special;
        //     switch (special) { ... }
        // }
        //
        // PORT NOTE: `@hasDecl(E, "detectSpecialCase")` is the trait's optional
        // `detect_special_case` (default `None` == "no decl"). A `None` result is
        // the reference's `orelse break :special` — fall through to the `+` check.
        // The `.fallback`/`.abort_if_no_action` arms that do not early-return also
        // fall through to the `+` check (faithful to the reference, where such
        // args — `--help`, `-e` — do not start with `+` and hit `continue`).
        if let Some(special) = E::detect_special_case(&arg) {
            match special {
                // .action => |a| return a,
                SpecialCase::Action(a) => return Ok(Some(a)),
                // .fallback => |a| fallback = a,
                SpecialCase::Fallback(a) => fallback = Some(a),
                // .abort_if_no_action => if (pending == null) return null,
                SpecialCase::AbortIfNoAction => {
                    if pending.is_none() {
                        return Ok(None);
                    }
                }
            }
        }

        // Commands must start with "+"
        // if (arg.len == 0 or arg[0] != '+') continue;
        if arg.is_empty() || arg[0] != b'+' {
            continue;
        }
        // if (pending != null) return DetectError.MultipleActions;
        if pending.is_some() {
            return Err(DetectError::MultipleActions);
        }
        // pending = std.meta.stringToEnum(E, arg[1..]) orelse
        //     return DetectError.InvalidAction;
        pending = Some(E::from_plus_name(&arg[1..]).ok_or(DetectError::InvalidAction)?);
    }

    // If we have an action, we always return that action, even if we've
    // seen "--help" or "-h" because the action may have its own help text.
    // if (pending != null) return pending;
    if pending.is_some() {
        return Ok(pending);
    }

    // If we have no action but we have a fallback, then we return that.
    // if (fallback) |a| return a;
    if let Some(a) = fallback {
        return Ok(Some(a));
    }

    // return null;
    Ok(None)
}

/// The action enum E can implement the decl `detectSpecialCase` to
/// return this enum in order to perform various special case actions.
// pub fn SpecialCase(comptime E: type) type {
//     return union(enum) { action: E, fallback: E, abort_if_no_action };
// }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpecialCase<E> {
    /// Immediately return this action.
    Action(E),

    /// Return this action if no other action is found.
    Fallback(E),

    /// If there is no pending action (we haven't seen an action yet)
    /// then we should return no action. This is kind of weird but is
    /// a special case to allow "-e" in Rustty.
    AbortIfNoAction,
}

#[cfg(test)]
mod tests {
    use super::{detect_iter, Action, DetectError, SpecialCase};
    use crate::test_util::ArgIteratorGeneral;

    // test "detect direct match"
    #[test]
    fn detect_direct_match() {
        #[derive(Clone, Copy, PartialEq, Eq, Debug)]
        enum Enum {
            Foo,
            Bar,
            Baz,
        }
        impl Action for Enum {
            fn from_plus_name(name: &[u8]) -> Option<Self> {
                match name {
                    b"foo" => Some(Enum::Foo),
                    b"bar" => Some(Enum::Bar),
                    b"baz" => Some(Enum::Baz),
                    _ => None,
                }
            }
        }

        let mut iter = ArgIteratorGeneral::init(b"+foo");
        let result = detect_iter::<Enum, _>(&mut iter).unwrap();
        // try testing.expectEqual(Enum.foo, result.?);
        assert_eq!(Enum::Foo, result.unwrap());
        iter.deinit();
    }

    // test "detect invalid match"
    #[test]
    fn detect_invalid_match() {
        #[derive(Clone, Copy, PartialEq, Eq, Debug)]
        enum Enum {
            Foo,
            Bar,
            Baz,
        }
        impl Action for Enum {
            fn from_plus_name(name: &[u8]) -> Option<Self> {
                match name {
                    b"foo" => Some(Enum::Foo),
                    b"bar" => Some(Enum::Bar),
                    b"baz" => Some(Enum::Baz),
                    _ => None,
                }
            }
        }

        let mut iter = ArgIteratorGeneral::init(b"+invalid");
        // try testing.expectError(DetectError.InvalidAction, detectIter(Enum, &iter));
        assert_eq!(
            detect_iter::<Enum, _>(&mut iter),
            Err(DetectError::InvalidAction),
        );
        iter.deinit();
    }

    // test "detect multiple actions"
    #[test]
    fn detect_multiple_actions() {
        #[derive(Clone, Copy, PartialEq, Eq, Debug)]
        enum Enum {
            Foo,
            Bar,
            Baz,
        }
        impl Action for Enum {
            fn from_plus_name(name: &[u8]) -> Option<Self> {
                match name {
                    b"foo" => Some(Enum::Foo),
                    b"bar" => Some(Enum::Bar),
                    b"baz" => Some(Enum::Baz),
                    _ => None,
                }
            }
        }

        let mut iter = ArgIteratorGeneral::init(b"+foo +bar");
        // try testing.expectError(DetectError.MultipleActions, detectIter(Enum, &iter));
        assert_eq!(
            detect_iter::<Enum, _>(&mut iter),
            Err(DetectError::MultipleActions),
        );
        iter.deinit();
    }

    // test "detect no match"
    #[test]
    fn detect_no_match() {
        #[derive(Clone, Copy, PartialEq, Eq, Debug)]
        enum Enum {
            Foo,
            Bar,
            Baz,
        }
        impl Action for Enum {
            fn from_plus_name(name: &[u8]) -> Option<Self> {
                match name {
                    b"foo" => Some(Enum::Foo),
                    b"bar" => Some(Enum::Bar),
                    b"baz" => Some(Enum::Baz),
                    _ => None,
                }
            }
        }

        let mut iter = ArgIteratorGeneral::init(b"--some-flag");
        let result = detect_iter::<Enum, _>(&mut iter).unwrap();
        // try testing.expect(result == null);
        assert!(result.is_none());
        iter.deinit();
    }

    // test "detect special case action"
    #[test]
    fn detect_special_case_action() {
        #[derive(Clone, Copy, PartialEq, Eq, Debug)]
        enum Enum {
            Foo,
            Bar,
        }
        impl Action for Enum {
            fn from_plus_name(name: &[u8]) -> Option<Self> {
                match name {
                    b"foo" => Some(Enum::Foo),
                    b"bar" => Some(Enum::Bar),
                    _ => None,
                }
            }

            // fn detectSpecialCase(arg: []const u8) ?SpecialCase(@This()) {
            //     return if (std.mem.eql(u8, arg, "--special")) .{ .action = .foo }
            //         else null;
            // }
            fn detect_special_case(arg: &[u8]) -> Option<SpecialCase<Self>> {
                if arg == b"--special" {
                    Some(SpecialCase::Action(Enum::Foo))
                } else {
                    None
                }
            }
        }

        {
            let mut iter = ArgIteratorGeneral::init(b"--special +bar");
            let result = detect_iter::<Enum, _>(&mut iter).unwrap();
            assert_eq!(Enum::Foo, result.unwrap());
            iter.deinit();
        }

        {
            let mut iter = ArgIteratorGeneral::init(b"+bar --special");
            let result = detect_iter::<Enum, _>(&mut iter).unwrap();
            assert_eq!(Enum::Foo, result.unwrap());
            iter.deinit();
        }

        {
            let mut iter = ArgIteratorGeneral::init(b"+bar");
            let result = detect_iter::<Enum, _>(&mut iter).unwrap();
            assert_eq!(Enum::Bar, result.unwrap());
            iter.deinit();
        }
    }

    // test "detect special case fallback"
    #[test]
    fn detect_special_case_fallback() {
        #[derive(Clone, Copy, PartialEq, Eq, Debug)]
        enum Enum {
            Foo,
            Bar,
        }
        impl Action for Enum {
            fn from_plus_name(name: &[u8]) -> Option<Self> {
                match name {
                    b"foo" => Some(Enum::Foo),
                    b"bar" => Some(Enum::Bar),
                    _ => None,
                }
            }

            // fn detectSpecialCase(arg: []const u8) ?SpecialCase(@This()) {
            //     return if (std.mem.eql(u8, arg, "--special")) .{ .fallback = .foo }
            //         else null;
            // }
            fn detect_special_case(arg: &[u8]) -> Option<SpecialCase<Self>> {
                if arg == b"--special" {
                    Some(SpecialCase::Fallback(Enum::Foo))
                } else {
                    None
                }
            }
        }

        {
            let mut iter = ArgIteratorGeneral::init(b"--special");
            let result = detect_iter::<Enum, _>(&mut iter).unwrap();
            assert_eq!(Enum::Foo, result.unwrap());
            iter.deinit();
        }

        {
            let mut iter = ArgIteratorGeneral::init(b"+bar --special");
            let result = detect_iter::<Enum, _>(&mut iter).unwrap();
            assert_eq!(Enum::Bar, result.unwrap());
            iter.deinit();
        }

        {
            let mut iter = ArgIteratorGeneral::init(b"--special +bar");
            let result = detect_iter::<Enum, _>(&mut iter).unwrap();
            assert_eq!(Enum::Bar, result.unwrap());
            iter.deinit();
        }
    }

    // test "detect special case abort_if_no_action"
    #[test]
    fn detect_special_case_abort_if_no_action() {
        #[derive(Clone, Copy, PartialEq, Eq, Debug)]
        enum Enum {
            Foo,
            Bar,
        }
        impl Action for Enum {
            fn from_plus_name(name: &[u8]) -> Option<Self> {
                match name {
                    b"foo" => Some(Enum::Foo),
                    b"bar" => Some(Enum::Bar),
                    _ => None,
                }
            }

            // fn detectSpecialCase(arg: []const u8) ?SpecialCase(@This()) {
            //     return if (std.mem.eql(u8, arg, "-e")) .abort_if_no_action
            //         else null;
            // }
            fn detect_special_case(arg: &[u8]) -> Option<SpecialCase<Self>> {
                if arg == b"-e" {
                    Some(SpecialCase::AbortIfNoAction)
                } else {
                    None
                }
            }
        }

        {
            let mut iter = ArgIteratorGeneral::init(b"-e");
            let result = detect_iter::<Enum, _>(&mut iter).unwrap();
            assert!(result.is_none());
            iter.deinit();
        }

        {
            let mut iter = ArgIteratorGeneral::init(b"+foo -e");
            let result = detect_iter::<Enum, _>(&mut iter).unwrap();
            assert_eq!(Enum::Foo, result.unwrap());
            iter.deinit();
        }

        {
            let mut iter = ArgIteratorGeneral::init(b"-e +bar");
            let result = detect_iter::<Enum, _>(&mut iter).unwrap();
            assert!(result.is_none());
            iter.deinit();
        }
    }
}
