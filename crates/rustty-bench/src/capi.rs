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
            eprintln!(
                "[{LOG_SCOPE}] unknown action={}",
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
            eprintln!("[{LOG_SCOPE}] failed to run action={action:?} err={err:?}");
            false
        }
        // return true;
        Ok(()) => true,
    }
}
