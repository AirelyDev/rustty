// const std = @import("std");
// const cli = @import("cli.zig");
// const state = &@import("../global.zig").state;
use crate::cli;
use allocator_api2::alloc::Global as Allocator;
use core::ffi::{c_char, CStr};

// const log = std.log.scoped(.benchmark);
const LOG_SCOPE: &str = "benchmark";

/// Run the Ghostty benchmark CLI with the given action and arguments.
// export fn ghostty_benchmark_cli(action_name_: [*:0]const u8, args: [*:0]const u8) bool {
//
// PORT NOTE (§10, R2): the reference C export is `ghostty_benchmark_cli`. Per R2
// this ships as `rustty_benchmark_cli` (matching the vt-c `#[no_mangle] pub
// extern "C" fn rustty_*` precedent); the `ghostty_benchmark_cli` C name is
// provided later by sub-project #2's alias layer (the C-ABI compat shim), NOT
// here (DEFERRALS.md).
//
// This `extern "C"` entrypoint is FFI/C-ABI — Miri-excluded, covered by the ASan
// lane (MIRI_EXCEPTIONS.md).
/// # Safety
///
/// `action_name` and `args` must each be a valid pointer to a NUL-terminated C
/// string (`[*:0]const u8` in the reference).
#[no_mangle]
pub unsafe extern "C" fn rustty_benchmark_cli(
    action_name: *const c_char,
    args: *const c_char,
) -> bool {
    // const action_name = std.mem.sliceTo(action_name_, 0);
    //   `sliceTo(_, 0)` -> `CStr::from_ptr` -> bytes (kept as `&[u8]`, not a lossy
    //   `String`, per the byte-slice fidelity rule).
    let action_name_bytes = unsafe { CStr::from_ptr(action_name) }.to_bytes();

    // const action: cli.Action = std.meta.stringToEnum(cli.Action, action_name) orelse {
    //     log.warn("unknown action={s}", .{action_name});
    //     return false;
    // };
    let action = match cli::Action::from_name(action_name_bytes) {
        Some(a) => a,
        None => {
            // log.warn("unknown action={s}", .{action_name});
            log::warn!(
                target: LOG_SCOPE,
                "unknown action={}",
                String::from_utf8_lossy(action_name_bytes)
            );
            return false;
        }
    };

    // cli.mainAction(state.alloc, action, .{ .string = std.mem.sliceTo(args, 0) }) catch |err| {
    //     log.warn("failed to run action={s} err={}", .{ @tagName(action), err });
    //     return false;
    // };
    //
    // PORT NOTE (§10): the reference `state.alloc` is `global.zig`'s chosen
    // process allocator (`std.heap.c_allocator`-equivalent). rustty's
    // `rustty_global` state carries that as a unit seam (`seam::Allocator = ()`)
    // today, so we thread the process `Global` allocator directly — the same
    // process-wide allocator, behavior-preserving.
    let args_bytes = unsafe { CStr::from_ptr(args) }.to_bytes();
    match cli::main_action(Allocator, action, cli::Args::String(args_bytes)) {
        Err(err) => {
            // log.warn("failed to run action={s} err={}", .{ @tagName(action), err });
            //
            // CORRECTION (task T-D, S61-F8): this printed `{action:?}` under a
            // PORT NOTE saying the Debug form "is the same identity in the port's
            // PascalCase (R2)". It is not the same text: the reference prints
            // `@tagName(action)`, the kebab token (`codepoint-width`), and the
            // comment above had also misquoted the argument as `action_name`.
            log::warn!(target: LOG_SCOPE, "failed to run action={} err={err:?}", action.tag_name());
            false
        }
        // return true;
        Ok(()) => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// S61-F8: the failure warning names the action by its TAG name, as the
    /// reference's `@tagName(action)` does (`src/benchmark/CApi.zig:25-31` at
    /// `20abdb50a`): `failed to run action=codepoint-width err=...`, not the Rust
    /// variant's Debug spelling `CodepointWidth`.
    #[test]
    fn failed_action_warning_uses_the_tag_name() {
        let cap = crate::log_capture::start();
        // An option the action does not have makes `mainAction` fail.
        let ok = unsafe {
            rustty_benchmark_cli(c"codepoint-width".as_ptr(), c"--no-such-option=1".as_ptr())
        };
        let messages = cap.messages();
        drop(cap);

        // Positive reach: the failure arm ran (the export returned false and
        // logged exactly one record under the reference scope).
        assert!(!ok, "an unknown option must fail the action");
        assert_eq!(messages.len(), 1, "one warning; got {messages:?}");
        let (level, target, msg) = &messages[0];
        assert_eq!(*level, log::Level::Warn);
        assert_eq!(target, LOG_SCOPE);
        assert!(
            msg.starts_with("failed to run action=codepoint-width err="),
            "the action is named by its tag name; got {msg:?}"
        );
    }

    /// Every action's tag name round-trips through `from_name`
    /// (`std.meta.stringToEnum` / `@tagName` are inverses in the reference).
    #[test]
    fn action_tag_name_round_trips() {
        for action in cli::Action::ALL {
            assert_eq!(
                cli::Action::from_name(action.tag_name().as_bytes()),
                Some(action)
            );
        }
    }
}
