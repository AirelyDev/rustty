//! rustty-cli::tui
//!
//! PORTS: ghostty src/cli/tui.zig (0t) — the TUI capability seam.
//!
//! `can_pretty_print` is the platform gate that `list_colors` / `list_keybinds`
//! consult (`tui.can_pretty_print && !plain && isatty`) before entering the
//! interactive vaxis+zf render path. It depends ONLY on `builtin.os.tag`, so it
//! ports faithfully NOW even though the vaxis-backed `prettyPrint` functions are
//! deferred placeholder leaves (CLI_BLUEPRINT.md §1 "Seam"). This keeps the
//! plain/non-tty paths — the only paths the KAT and any redirected pipe hit —
//! compiling and correct today.

// const builtin = @import("builtin");

/// PORTS: `pub const can_pretty_print = switch (builtin.os.tag) { ... };`.
///
/// The interactive TUI (vaxis) is unavailable on the Apple embedded platforms;
/// every other target can pretty-print. The reference switches on
/// `builtin.os.tag` at comptime; the Rust equivalent is a `cfg!(target_os = …)`
/// check over the same tag set (`ios` / `tvos` / `watchos`).
// pub const can_pretty_print = switch (builtin.os.tag) {
//     .ios, .tvos, .watchos => false,
//     else => true,
// };
pub const CAN_PRETTY_PRINT: bool = !cfg!(any(
    target_os = "ios",
    target_os = "tvos",
    target_os = "watchos",
));
