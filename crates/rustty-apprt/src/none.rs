//! rustty-apprt::none
//!
//! PORTS: ghostty `src/apprt/none.zig` (19 loc, 0 tests) — the headless / CLI
//! IPC backend. EMBEDDED_BLUEPRINT §4: `none.App` has ONLY the static
//! `performIpc` (returns false); `Surface` is an empty struct. It does NOT
//! implement the full `apprt_api::App`/`Surface` traits (faithful: the reference
//! `none.App` has no `performAction`/`wakeup`). Ported as a plain module — a
//! free/assoc `perform_ipc` fn, no trait impls. This is the backend the headless
//! `rustty` CLI bin sits on. Honest and minimal.
//!
//! Reference (full file):
//! ```zig
//! const std = @import("std");
//! const Allocator = std.mem.Allocator;
//!
//! const internal_os = @import("../os/main.zig");
//! const apprt = @import("../apprt.zig");
//! pub const resourcesDir = internal_os.resourcesDir;
//!
//! pub const App = struct {
//!     /// Always return false as there is no apprt to communicate with.
//!     pub fn performIpc(
//!         _: Allocator,
//!         _: apprt.ipc.Target,
//!         comptime action: apprt.ipc.Action.Key,
//!         _: apprt.ipc.Action.Value(action),
//!     ) !bool {
//!         return false;
//!     }
//! };
//! pub const Surface = struct {};
//! ```

// PORT NOTE: reference `const std`/`const Allocator = std.mem.Allocator;` — a
// passed-in `std.mem.Allocator` maps to the `allocator_api2` `Allocator` trait
// (the established workspace map, cf. `rustty_apprt_api::ipc`). `internal_os` and
// `apprt` are reached as the rustty `rustty_os` / `rustty_apprt_api` crates.
use allocator_api2::alloc::Allocator;
use rustty_apprt_api::ipc;

/// PORT NOTE: reference `pub const resourcesDir = internal_os.resourcesDir;`.
pub use rustty_os::resourcesDir;

/// PORT NOTE: reference `pub const App = struct { ... };`.
pub struct App;

impl App {
    /// Always return false as there is no apprt to communicate with.
    //
    // PORT NOTE: reference `performIpc` is a STATIC fn (no `self`): an
    // associated fn here. Its `comptime action: apprt.ipc.Action.Key` +
    // `_: apprt.ipc.Action.Value(action)` (a comptime-computed value type) are
    // combined into the single `ipc::Action` union — `Value(comptime key)` has
    // no runtime Rust equivalent (documented in `apprt_api::ipc`), and both key
    // and value are ignored here anyway. The inferred `!bool` error set narrows
    // to `ipc::Errors` (the reference's `apprt.ipc.Errors`).
    pub fn perform_ipc<A: Allocator>(
        _alloc: A,
        _target: ipc::Target<'_>,
        _action: ipc::Action<'_>,
    ) -> Result<bool, ipc::Errors> {
        // reference: return false;
        Ok(false)
    }
}

/// PORT NOTE: reference `pub const Surface = struct {};`.
pub struct Surface;
