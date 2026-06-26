//! rustty-config::conditional
//!
//! PORTS: ghostty src/config/conditional.zig — typed conditional config state
//! (`State`/`Key`/`Conditional`). C1 leaf value-type (1 test).
//!
//! PORT STATUS: ported (every line/comment/test{}, R1).

// const std = @import("std");
// const builtin = @import("builtin");
// const Allocator = std.mem.Allocator;

/// Conditionals in Ghostty configuration are based on a static, typed
/// state of the world instead of a dynamic key-value set. This simplifies
/// the implementation, allows for better type checking, and enables a
/// typed C API.
// pub const State = struct {
#[derive(Clone, Copy)]
pub struct State {
    /// The theme of the underlying OS desktop environment.
    // theme: Theme = .light,
    pub theme: Theme,

    /// The target OS of the current build.
    // os: std.Target.Os.Tag = builtin.target.os.tag,
    //
    // PORT NOTE: `std.Target.Os.Tag` is the build-target OS enum. The conditional
    // machinery only ever compares its `@tagName` against a string; the host OS
    // tag is provided as bytes (`OsTag`) so `match` reproduces the `@tagName`
    // comparison without depending on a Zig `builtin` import.
    pub os: OsTag,
}

impl Default for State {
    fn default() -> State {
        State {
            // theme: Theme = .light,
            theme: Theme::Light,
            // os: std.Target.Os.Tag = builtin.target.os.tag,
            os: OsTag::host(),
        }
    }
}

// pub const Theme = enum { light, dark };
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Theme {
    Light,
    Dark,
}

impl Theme {
    /// PORTS: the `@tagName` of `Theme` used by `State.match`.
    pub fn tag_name(self) -> &'static [u8] {
        match self {
            Theme::Light => b"light",
            Theme::Dark => b"dark",
        }
    }
}

/// PORTS: `std.Target.Os.Tag` insofar as `State.os` uses it — `State.match`
/// reads only its `@tagName`. We carry the host target's tag name as bytes.
#[derive(Clone, Copy)]
pub struct OsTag {
    name: &'static [u8],
}

impl OsTag {
    /// The host build-target OS tag, matching `builtin.target.os.tag`.
    pub fn host() -> OsTag {
        // PORT NOTE: `std.Target.Os.Tag` uses lowercase names (`macos`, `linux`,
        // `windows`, ...). We map the Rust `target_os` to the matching tag name.
        #[cfg(target_os = "macos")]
        {
            OsTag { name: b"macos" }
        }
        #[cfg(target_os = "linux")]
        {
            OsTag { name: b"linux" }
        }
        #[cfg(target_os = "windows")]
        {
            OsTag { name: b"windows" }
        }
        #[cfg(target_os = "freebsd")]
        {
            OsTag { name: b"freebsd" }
        }
        #[cfg(not(any(
            target_os = "macos",
            target_os = "linux",
            target_os = "windows",
            target_os = "freebsd"
        )))]
        {
            OsTag { name: b"other" }
        }
    }

    /// PORTS: the `@tagName` of the OS tag used by `State.match`.
    pub fn tag_name(self) -> &'static [u8] {
        self.name
    }
}

impl State {
    /// Tests the conditional against the state and returns true if it matches.
    // pub fn match(self: State, cond: Conditional) bool {
    //
    // PARITY: named `is_match` because `match` is a Rust keyword.
    pub fn is_match(self, cond: &Conditional) -> bool {
        // switch (cond.key) {
        //     inline else => |tag| {
        //
        // PORT NOTE: the comptime `inline else => |tag|` selects the state field
        // named by the key tag and reads its `@tagName`. With two keys (theme,
        // os) the dispatch is a runtime `match` over `Key`, each arm reading the
        // corresponding field's tag name — identical to the reference's per-tag
        // body. The `@compileError`-style note ("If we introduce non-enum state
        // values then this will be a compile error and we should fix here.") is
        // preserved below as a comment since all current state values are enums.
        let value: &[u8] = match cond.key {
            // The raw value of the state field.
            // const raw = @field(self, @tagName(tag));
            //
            // Since all values are enums currently then we can just
            // do this. If we introduce non-enum state values then this
            // will be a compile error and we should fix here.
            // const value: []const u8 = @tagName(raw);
            Key::Theme => self.theme.tag_name(),
            Key::Os => self.os.tag_name(),
        };

        // return switch (cond.op) {
        match cond.op {
            // .eq => std.mem.eql(u8, value, cond.value),
            Op::Eq => value == cond.value.as_slice(),
            // .ne => !std.mem.eql(u8, value, cond.value),
            Op::Ne => value != cond.value.as_slice(),
        }
    }
}

/// An enum of the available conditional configuration keys.
// pub const Key = key: {
//     const stateInfo = @typeInfo(State).@"struct";
//     var fields: [stateInfo.fields.len]std.builtin.Type.EnumField = undefined;
//     for (stateInfo.fields, 0..) |field, i| fields[i] = .{
//         .name = field.name,
//         .value = i,
//     };
//
//     break :key @Type(.{ .@"enum" = .{
//         .tag_type = std.math.IntFittingRange(0, fields.len - 1),
//         .fields = &fields,
//         .decls = &.{},
//         .is_exhaustive = true,
//     } });
// };
//
// PORT NOTE: the reflective `@Type(...)` builds an enum with one variant per
// `State` field (in declaration order: `theme`, `os`). Rust has no comptime
// reflection over struct fields, so the enum is spelled out explicitly. It MUST
// stay in sync with `State`'s fields (the reference derives it; here adding a
// state field requires adding a `Key` variant — the `match` arms above are the
// `@compileError` analogue that catches a divergence).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Key {
    Theme,
    Os,
}

/// A single conditional that can be true or false.
// pub const Conditional = struct {
#[derive(Clone, Debug)]
pub struct Conditional {
    // key: Key,
    pub key: Key,
    // op: Op,
    pub op: Op,
    // value: []const u8,
    pub value: Vec<u8>,
}

// pub const Op = enum { eq, ne };
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Op {
    Eq,
    Ne,
}

impl Conditional {
    // pub fn clone(
    //     self: Conditional,
    //     alloc: Allocator,
    // ) Allocator.Error!Conditional {
    //     return .{
    //         .key = self.key,
    //         .op = self.op,
    //         .value = try alloc.dupe(u8, self.value),
    //     };
    // }
    //
    // PARITY: named `clone` to match the reference method (Config relies on it).
    #[allow(clippy::should_implement_trait)]
    pub fn clone(&self) -> Conditional {
        Conditional {
            key: self.key,
            op: self.op,
            value: self.value.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // test "conditional enum match" {
    #[test]
    fn conditional_enum_match() {
        // const state: State = .{ .theme = .dark };
        let state = State {
            theme: Theme::Dark,
            ..State::default()
        };
        // try testing.expect(state.match(.{
        //     .key = .theme,
        //     .op = .eq,
        //     .value = "dark",
        // }));
        assert!(state.is_match(&Conditional {
            key: Key::Theme,
            op: Op::Eq,
            value: b"dark".to_vec(),
        }));
        // try testing.expect(!state.match(.{
        //     .key = .theme,
        //     .op = .ne,
        //     .value = "dark",
        // }));
        assert!(!state.is_match(&Conditional {
            key: Key::Theme,
            op: Op::Ne,
            value: b"dark".to_vec(),
        }));
        // try testing.expect(state.match(.{
        //     .key = .theme,
        //     .op = .ne,
        //     .value = "light",
        // }));
        assert!(state.is_match(&Conditional {
            key: Key::Theme,
            op: Op::Ne,
            value: b"light".to_vec(),
        }));
    }
}
