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
        OsTag {
            name: tag_name_for(std::env::consts::OS),
        }
    }

    /// PORTS: the `@tagName` of the OS tag used by `State.match`.
    pub fn tag_name(self) -> &'static [u8] {
        self.name
    }
}

/// PORTS: `@tagName(builtin.target.os.tag)` for a build whose Rust `target_os`
/// is `target_os` (`std::env::consts::OS`).
//
// PORT NOTE: `std.Target.Os.Tag` uses lowercase names (`macos`, `linux`,
// `windows`, ...) and Rust's `target_os` names coincide for every OS both
// support, so the mapping is mostly identity. This used to be a `cfg` chain that
// named only macos/linux/windows/freebsd and collapsed EVERY other target to
// `other`, which was two divergences at once on e.g. a NetBSD build: an
// `os == netbsd` conditional could never match (the reference yields "netbsd"),
// and an `os == other` conditional matched even though the reference's `.other`
// tag is a distinct enum member the target does not have.
//
// The listed names are exactly the `std.Target.Os.Tag` members (Zig 0.16) that a
// Rust `target_os` can name; anything else — a target Zig models under a
// different tag, or one it has no member for at all (`solaris`, `redox`, `nto`,
// ...) — falls to `other`, the reference's own catch-all member. Two non-identity
// arms: Zig models Android as `.linux` with the `android` ABI, and Rust's
// bare-metal `none` is Zig's `freestanding`.
fn tag_name_for(target_os: &str) -> &'static [u8] {
    match target_os {
        "macos" => b"macos",
        "ios" => b"ios",
        "tvos" => b"tvos",
        "watchos" => b"watchos",
        "visionos" => b"visionos",
        "linux" => b"linux",
        "android" => b"linux",
        "windows" => b"windows",
        "uefi" => b"uefi",
        "freebsd" => b"freebsd",
        "netbsd" => b"netbsd",
        "openbsd" => b"openbsd",
        "dragonfly" => b"dragonfly",
        "illumos" => b"illumos",
        "haiku" => b"haiku",
        "hurd" => b"hurd",
        "fuchsia" => b"fuchsia",
        "hermit" => b"hermit",
        "rtems" => b"rtems",
        "psp" => b"psp",
        "vita" => b"vita",
        "wasi" => b"wasi",
        "emscripten" => b"emscripten",
        "none" => b"freestanding",
        _ => b"other",
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

    // PORT NOTE: no reference test — `builtin.target.os.tag` needs none in Zig
    // because the tag IS the target's. This covers the port's mapping seam: the
    // OS tags a `Key.os` conditional can name must be the reference's
    // `@tagName(std.Target.Os.Tag)` for the same build target, not a
    // four-target allowlist collapsed to `other`.
    #[test]
    fn os_tag_name_matches_zig_target_os_tag() {
        // The BSDs the old cfg chain collapsed to `other`.
        assert_eq!(b"netbsd", tag_name_for("netbsd"));
        assert_eq!(b"openbsd", tag_name_for("openbsd"));
        assert_eq!(b"dragonfly", tag_name_for("dragonfly"));
        // Positive controls that were already covered.
        assert_eq!(b"macos", tag_name_for("macos"));
        assert_eq!(b"linux", tag_name_for("linux"));
        assert_eq!(b"windows", tag_name_for("windows"));
        assert_eq!(b"freebsd", tag_name_for("freebsd"));
        // Zig models Android as `.linux`.
        assert_eq!(b"linux", tag_name_for("android"));
        // Targets `std.Target.Os.Tag` has no member for keep the reference's
        // own `.other` catch-all.
        assert_eq!(b"other", tag_name_for("solaris"));
        assert_eq!(b"other", tag_name_for("unknownos"));
        // ...and a target that DOES have a tag must never report `other`.
        assert_ne!(b"other", tag_name_for("netbsd"));

        // `host()` is exactly the mapping applied to this build's target.
        assert_eq!(tag_name_for(std::env::consts::OS), OsTag::host().tag_name());
    }

    // A `Key.os` conditional naming the real target OS must match on that
    // target, and the reference's distinct `.other` tag must not.
    #[test]
    fn os_conditional_matches_real_tag() {
        let state = State {
            theme: Theme::Light,
            os: OsTag {
                name: tag_name_for("netbsd"),
            },
        };
        assert!(state.is_match(&Conditional {
            key: Key::Os,
            op: Op::Eq,
            value: b"netbsd".to_vec(),
        }));
        assert!(!state.is_match(&Conditional {
            key: Key::Os,
            op: Op::Eq,
            value: b"other".to_vec(),
        }));
    }
}
