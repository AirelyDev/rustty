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
// PORT NOTE: Rust has no single `builtin.mode` reflection value, but Zig's four
// optimize modes ARE the cross product of the two axes Rust does expose at
// build time — runtime safety (`cfg!(debug_assertions)`) and optimization
// (`OPT_LEVEL`, surfaced by this crate's `build.rs` as the `rustty_optimized` /
// `rustty_opt_small` cfgs). So the analog is a faithful FOUR-way mapping, not a
// two-way collapse:
//
//   safety + unoptimized      => Debug         (cargo `dev`)
//   safety + optimized        => ReleaseSafe   (cargo `release-safe`: the DG4 /
//                                               CI lane that runs the oracle's
//                                               ReleaseSafe assert semantics)
//   no safety + optimize-size => ReleaseSmall  (`opt-level = "s"` / `"z"`)
//   no safety + optimized     => ReleaseFast   (cargo `release`/`release-fast`)
//
// The test below derives its expected value from the SAME predicates (mirroring
// the reference test, which also switches on `builtin.mode`), and a
// profile-pinned env-gated test asserts the concrete ordinal per profile.
fn optimize_mode() -> OptimizeMode {
    optimize_mode_for(
        cfg!(debug_assertions),
        cfg!(rustty_opt_small),
        cfg!(rustty_optimized),
    )
}

/// The `builtin.mode` analog as a pure function of the three build predicates,
/// so the mapping is testable independently of the profile this binary was
/// compiled with. `debug_assertions` is Zig's `runtime_safety` axis;
/// `opt_small`/`optimized` are the `OPT_LEVEL` axis (see `build.rs`).
const fn optimize_mode_for(
    debug_assertions: bool,
    opt_small: bool,
    optimized: bool,
) -> OptimizeMode {
    if debug_assertions {
        // Safety checks on: Debug when unoptimized, ReleaseSafe when optimized.
        if optimized {
            OptimizeMode::ReleaseSafe
        } else {
            OptimizeMode::Debug
        }
    } else if opt_small {
        // Safety off, optimizing for size.
        OptimizeMode::ReleaseSmall
    } else {
        // Safety off, optimizing for speed (ghostty's default release mode).
        OptimizeMode::ReleaseFast
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::c_int;

    /// rustty-only (W16-TARGET-01). The reference's `-Dsimd` defaults to TRUE
    /// for every non-WASM target (`ghostty/src/build/Config.zig:196-206`), and
    /// `GhosttyZig.zig` force-disables only `oniguruma` for the lib-vt module
    /// (:67-72) while still adding the SIMD deps when `cfg.simd` (:140-143) —
    /// so a DEFAULT libghostty-vt runs `stream.zig`'s SIMD `nextSlice` fast
    /// path and answers `.simd = true` through THIS C ABI. A default
    /// `cargo build -p rustty-vt-c` used to answer `false`.
    ///
    /// Two arms, because the resolved value alone cannot tell "the default
    /// feature set turns SIMD on" apart from "this run happened to be invoked
    /// with `--features simd`":
    ///   1. the MANIFEST arm reads `[features] default` out of this crate's own
    ///      Cargo.toml. It binds however the test was invoked, and it is the arm
    ///      that was RED before the fix (`simd` was opt-in only, no `default`
    ///      key existed at all).
    ///   2. the ABI arm asserts that whenever the default feature set is active
    ///      the C ABI answers `true` — the observable a C host actually sees.
    ///      `--no-default-features` is rustty's `-Dsimd=false` analog, and this
    ///      arm correctly stands down there.
    // The constant IS the subject: `build_options::SIMD` is a `const bool`
    // resolved from the cargo feature graph, and the ABI arm below asserts the
    // Rust-side const directly so a `get()` that hardcoded `true` cannot pass.
    #[allow(clippy::assertions_on_constants)]
    #[test]
    fn default_build_reports_simd_on() {
        // 1. Manifest arm.
        let manifest = include_str!("../Cargo.toml");
        let features = manifest
            .split_once("\n[features]")
            .expect("rustty-vt-c Cargo.toml has a [features] section")
            .1;
        // The whole `default = [...]` array, not just its first line: the
        // `-Dvt-features` work made this a multi-line array (it now names the
        // nine feature tokens alongside `simd`), and a line-oriented read would
        // have seen a bare `default = [` and reported a missing `simd` that is
        // right there on the next line. Read to the closing bracket instead, so
        // the arm keeps its teeth under either spelling.
        let default_start = features.find("\ndefault").expect(
            "rustty-vt-c declares no `default` feature: a plain \
                 `cargo build -p rustty-vt-c` therefore leaves `rustty-vt/simd` \
                 OFF, while ghostty's `-Dsimd` defaults to true for every \
                 non-WASM target (build/Config.zig:196-206)",
        );
        let tail = &features[default_start..];
        let default_decl = &tail[..tail
            .find(']')
            .expect("`default` feature list is not a closed `[...]` array")
            + 1];
        assert!(
            default_decl.contains("\"simd\""),
            "rustty-vt-c's default feature set does not include `simd` \
             ({default_decl:?}); ghostty's default libghostty-vt is SIMD-on"
        );

        // 2. ABI arm.
        if cfg!(feature = "default") {
            let mut value: bool = false;
            assert_eq!(Result::Success, unsafe {
                get(
                    BuildInfo::Simd,
                    &mut value as *mut bool as *mut std::ffi::c_void,
                )
            });
            assert!(
                value,
                "the DEFAULT rustty-vt-c build reports Simd=false through the \
                 build-info C ABI; ghostty reports true"
            );
            assert!(build_options::SIMD);
        }
    }

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
        let expected = optimize_mode_for(
            cfg!(debug_assertions),
            cfg!(rustty_opt_small),
            cfg!(rustty_optimized),
        );
        assert_eq!(expected, value);

        // Profile-pinned arm: the DG4/CI lanes run this test once per profile
        // with `RUSTTY_EXPECT_OPTIMIZE=<ordinal>` set, pinning the C-ABI answer
        // to the oracle ordinal that profile stands for —
        //   cargo test -p rustty-vt-c                       => 0 (Debug)
        //   cargo test -p rustty-vt-c --profile release-safe => 1 (ReleaseSafe)
        //   cargo test -p rustty-vt-c --release              => 3 (ReleaseFast)
        // Unset (the plain developer run) skips the pin.
        if let Ok(want) = std::env::var("RUSTTY_EXPECT_OPTIMIZE") {
            let want: c_int = want.trim().parse().expect("RUSTTY_EXPECT_OPTIMIZE integer");
            assert_eq!(
                want,
                value as c_int,
                "profile-pinned optimize ordinal mismatch (debug_assertions={}, \
                 rustty_optimized={}, rustty_opt_small={})",
                cfg!(debug_assertions),
                cfg!(rustty_optimized),
                cfg!(rustty_opt_small),
            );
        }
    }

    /// The four-way `builtin.mode` analog itself (`build_info.zig:72-77`): every
    /// (safety, opt-level) combination maps to the oracle's mode of the same
    /// ordinal. Independent of the profile this test binary was built with.
    #[test]
    fn optimize_mode_mapping_is_four_way() {
        // debug_assertions && !optimized => Debug
        assert_eq!(OptimizeMode::Debug, optimize_mode_for(true, false, false));
        // debug_assertions && optimized => ReleaseSafe (cargo `release-safe`)
        assert_eq!(
            OptimizeMode::ReleaseSafe,
            optimize_mode_for(true, false, true)
        );
        // `opt-level = "s"/"z"` implies optimized; safety on still means ReleaseSafe.
        assert_eq!(
            OptimizeMode::ReleaseSafe,
            optimize_mode_for(true, true, true)
        );
        // !debug_assertions && opt_small => ReleaseSmall
        assert_eq!(
            OptimizeMode::ReleaseSmall,
            optimize_mode_for(false, true, true)
        );
        // !debug_assertions && optimized for speed => ReleaseFast
        assert_eq!(
            OptimizeMode::ReleaseFast,
            optimize_mode_for(false, false, true)
        );

        // Ordinals are the oracle's (build_info.zig:10-14).
        assert_eq!(0, OptimizeMode::Debug as c_int);
        assert_eq!(1, OptimizeMode::ReleaseSafe as c_int);
        assert_eq!(2, OptimizeMode::ReleaseSmall as c_int);
        assert_eq!(3, OptimizeMode::ReleaseFast as c_int);
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
