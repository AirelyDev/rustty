//! rustty-vt-c build_info
//!
//! PORTS: ghostty src/terminal/c/build_info.zig
//!
//! PORT STATUS: done

// PORT NOTE: the reference imports are
//   const std = @import("std");
//   const builtin = @import("builtin");
//   const lib = @import("../lib.zig");
//   const build_options = @import("terminal_options");
//   const Result = @import("result.zig").Result;
// `build_options` is rustty-vt's resolved `options` module (the ported
// `terminal_options`); `lib.String` is `rustty_lib::String`.
//
//   const log = std.log.scoped(.build_info_c);
// PORT NOTE: scoped logging is not wired into this leaf; the single `log.warn`
// site below is preserved verbatim as a comment with the equivalent fidelity
// note (it is reachable only under `runtime_safety` for an out-of-range tag).

use crate::result::Result;
use rustty_lib::String as LibString;
use rustty_vt::options as build_options;

/// C: GhosttyOptimizeMode -> RusttyOptimizeMode (R2)
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OptimizeMode {
    Debug = 0,
    ReleaseSafe = 1,
    ReleaseSmall = 2,
    ReleaseFast = 3,
}

/// C: GhosttyBuildInfo -> RusttyBuildInfo (R2)
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuildInfo {
    Invalid = 0,
    Simd = 1,
    KittyGraphics = 2,
    TmuxControlMode = 3,
    Optimize = 4,
    VersionString = 5,
    VersionMajor = 6,
    VersionMinor = 7,
    VersionPatch = 8,
    VersionPre = 9,
    VersionBuild = 10,
}

impl BuildInfo {
    // Output type expected for querying the data of the given kind.
    // pub fn OutType(comptime self: BuildInfo) type { ... }
    // PORT NOTE: the reference returns the comptime out-pointer *type* per tag,
    // used to `@ptrCast` the `out` argument in `getTyped`. Rust has no comptime
    // type-returning function; the per-tag output type is enforced inline in
    // `get_typed` (each arm casts `out` to the matching pointer type), preserving
    // the same type/tag correspondence:
    //   .invalid                                  => void
    //   .simd, .kitty_graphics, .tmux_control_mode => bool
    //   .optimize                                  => OptimizeMode
    //   .version_string, .version_pre, .version_build => lib.String
    //   .version_major, .version_minor, .version_patch => usize
}

/// PORT NOTE: `pub fn get(data: BuildInfo, out: ?*anyopaque) callconv(...)
/// Result`. The C calling convention is the `extern "C"` wrapper in `lib.rs`
/// (`rustty_build_info`). `out` is an opaque out-pointer cast per the tag.
///
/// # Safety
/// `out` must point to writable storage of the type required by `data` (see
/// `BuildInfo::OutType`), except for `Invalid` which never dereferences `out`.
pub unsafe fn get(data: BuildInfo, out: *mut std::ffi::c_void) -> Result {
    // if (comptime std.debug.runtime_safety) {
    //     _ = std.meta.intToEnum(BuildInfo, @intFromEnum(data)) catch {
    //         log.warn("build_info invalid data value={d}", .{@intFromEnum(data)});
    //         return .invalid_value;
    //     };
    // }
    // PORT NOTE: `data: BuildInfo` is already a valid `BuildInfo` here (Rust does
    // not let an out-of-range discriminant be constructed safely), so the
    // `intToEnum` guard — which only fires for an invalid integer crossing the C
    // boundary — is a no-op in this typed path. The `extern "C"` wrapper in
    // `lib.rs` performs the integer-range guard against the raw `c_int` and is
    // where the `log.warn(...) ; return .invalid_value` site lives.

    // return switch (data) {
    //     .invalid => .invalid_value,
    //     inline else => |comptime_data| getTyped(comptime_data, @ptrCast(@alignCast(out))),
    // };
    match data {
        BuildInfo::Invalid => Result::InvalidValue,
        // SAFETY: `out` matches `other`'s required type per the caller contract.
        other => unsafe { get_typed(other, out) },
    }
}

// fn getTyped(comptime data: BuildInfo, out: *data.OutType()) Result { ... }
// PORT NOTE: `out` is cast to the per-tag type inside each arm (see `OutType`).
//
// # Safety: `out` must match `data`'s required type per `BuildInfo::OutType`.
unsafe fn get_typed(data: BuildInfo, out: *mut std::ffi::c_void) -> Result {
    // SAFETY: each arm writes through `out` cast to the per-tag type; the caller
    // guarantees `out` points to writable storage of that type.
    unsafe {
        match data {
            // .invalid => return .invalid_value,
            BuildInfo::Invalid => return Result::InvalidValue,
            // .simd => out.* = build_options.simd,
            BuildInfo::Simd => *(out as *mut bool) = build_options::SIMD,
            // .kitty_graphics => out.* = build_options.kitty_graphics,
            BuildInfo::KittyGraphics => *(out as *mut bool) = build_options::KITTY_GRAPHICS,
            // .tmux_control_mode => out.* = build_options.tmux_control_mode,
            BuildInfo::TmuxControlMode => *(out as *mut bool) = build_options::TMUX_CONTROL_MODE,
            // .optimize => out.* = switch (builtin.mode) { ... },
            BuildInfo::Optimize => *(out as *mut OptimizeMode) = optimize_mode(),
            // .version_string => out.* = .{ .ptr = ..., .len = ... },
            BuildInfo::VersionString => {
                *(out as *mut LibString) = LibString::init(build_options::VERSION_STRING.as_bytes())
            }
            // .version_major => out.* = build_options.version_major,
            BuildInfo::VersionMajor => *(out as *mut usize) = build_options::VERSION_MAJOR,
            // .version_minor => out.* = build_options.version_minor,
            BuildInfo::VersionMinor => *(out as *mut usize) = build_options::VERSION_MINOR,
            // .version_patch => out.* = build_options.version_patch,
            BuildInfo::VersionPatch => *(out as *mut usize) = build_options::VERSION_PATCH,
            // .version_pre => { if (build_options.version_pre) |b| ... else { .ptr="", .len=0 } },
            BuildInfo::VersionPre => {
                *(out as *mut LibString) = match build_options::VERSION_PRE {
                    Some(b) => LibString::init(b.as_bytes()),
                    None => LibString::init(b""),
                }
            }
            // .version_build => { if (build_options.version_build) |b| ... else { .ptr="", .len=0 } },
            BuildInfo::VersionBuild => {
                *(out as *mut LibString) = match build_options::VERSION_BUILD {
                    Some(b) => LibString::init(b.as_bytes()),
                    None => LibString::init(b""),
                }
            }
        }
    } // unsafe

    // return .success;
    Result::Success
}

// switch (builtin.mode) { .Debug => .debug, .ReleaseSafe => .release_safe,
//     .ReleaseSmall => .release_small, .ReleaseFast => .release_fast }
// PORT NOTE: Rust's build profile has no 1:1 mapping to Zig's four optimize
// modes. The faithful analog is: `cfg!(debug_assertions)` => Debug (the
// safety-checked default), else ReleaseFast (the optimized default). This is the
// rustty stand-in for `builtin.mode`; the test below derives its expected value
// from the SAME analog (mirroring the reference test, which also switches on
// `builtin.mode`).
fn optimize_mode() -> OptimizeMode {
    if cfg!(debug_assertions) {
        OptimizeMode::Debug
    } else {
        OptimizeMode::ReleaseFast
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_simd() {
        // var value: bool = undefined;
        let mut value: bool = false;
        // try testing.expectEqual(Result.success, get(.simd, @ptrCast(&value)));
        assert_eq!(Result::Success, unsafe {
            get(
                BuildInfo::Simd,
                &mut value as *mut bool as *mut std::ffi::c_void,
            )
        });
        // try testing.expectEqual(build_options.simd, value);
        assert_eq!(build_options::SIMD, value);
    }

    #[test]
    fn get_kitty_graphics() {
        let mut value: bool = false;
        assert_eq!(Result::Success, unsafe {
            get(
                BuildInfo::KittyGraphics,
                &mut value as *mut bool as *mut std::ffi::c_void,
            )
        });
        assert_eq!(build_options::KITTY_GRAPHICS, value);
    }

    #[test]
    fn get_tmux_control_mode() {
        let mut value: bool = false;
        assert_eq!(Result::Success, unsafe {
            get(
                BuildInfo::TmuxControlMode,
                &mut value as *mut bool as *mut std::ffi::c_void,
            )
        });
        assert_eq!(build_options::TMUX_CONTROL_MODE, value);
    }

    #[test]
    fn get_optimize() {
        // var value: OptimizeMode = undefined;
        let mut value = OptimizeMode::Debug;
        assert_eq!(Result::Success, unsafe {
            get(
                BuildInfo::Optimize,
                &mut value as *mut OptimizeMode as *mut std::ffi::c_void,
            )
        });
        // try testing.expectEqual(switch (builtin.mode) { ... }, value);
        let expected = if cfg!(debug_assertions) {
            OptimizeMode::Debug
        } else {
            OptimizeMode::ReleaseFast
        };
        assert_eq!(expected, value);
    }

    #[test]
    fn get_version_string() {
        // var value: lib.String = undefined;
        let mut value = LibString::init(b"");
        assert_eq!(Result::Success, unsafe {
            get(
                BuildInfo::VersionString,
                &mut value as *mut LibString as *mut std::ffi::c_void,
            )
        });
        // try testing.expect(value.len > 0);
        assert!(value.len > 0);
    }

    #[test]
    fn get_version_major() {
        // var value: usize = undefined;
        let mut value: usize = 0;
        assert_eq!(Result::Success, unsafe {
            get(
                BuildInfo::VersionMajor,
                &mut value as *mut usize as *mut std::ffi::c_void,
            )
        });
        assert_eq!(build_options::VERSION_MAJOR, value);
    }

    #[test]
    fn get_version_minor() {
        let mut value: usize = 0;
        assert_eq!(Result::Success, unsafe {
            get(
                BuildInfo::VersionMinor,
                &mut value as *mut usize as *mut std::ffi::c_void,
            )
        });
        assert_eq!(build_options::VERSION_MINOR, value);
    }

    #[test]
    fn get_version_patch() {
        let mut value: usize = 0;
        assert_eq!(Result::Success, unsafe {
            get(
                BuildInfo::VersionPatch,
                &mut value as *mut usize as *mut std::ffi::c_void,
            )
        });
        assert_eq!(build_options::VERSION_PATCH, value);
    }

    #[test]
    fn get_version_pre() {
        // var value: lib.String = undefined;
        let mut value = LibString::init(b"");
        // try testing.expectEqual(Result.success, get(.version_pre, @ptrCast(&value)));
        assert_eq!(Result::Success, unsafe {
            get(
                BuildInfo::VersionPre,
                &mut value as *mut LibString as *mut std::ffi::c_void,
            )
        });
    }

    #[test]
    fn get_version_build() {
        let mut value = LibString::init(b"");
        assert_eq!(Result::Success, unsafe {
            get(
                BuildInfo::VersionBuild,
                &mut value as *mut LibString as *mut std::ffi::c_void,
            )
        });
    }

    #[test]
    fn get_invalid() {
        // try std.testing.expectEqual(Result.invalid_value, get(.invalid, null));
        assert_eq!(Result::InvalidValue, unsafe {
            get(BuildInfo::Invalid, std::ptr::null_mut())
        });
    }
}
