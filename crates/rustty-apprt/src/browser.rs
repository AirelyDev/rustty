//! rustty-apprt::browser
//!
//! PORTS: ghostty `src/apprt/browser.zig` (4 loc, 0 tests) — the trivial wasm
//! stub. EMBEDDED_BLUEPRINT §4: `App = struct {}`, `Window = struct {}`. Ported
//! as empty structs, no traits. Everything else is deferred to the (unscheduled)
//! wasm milestone.
//!
//! Reference (full file):
//! ```zig
//! const internal_os = @import("../os/main.zig");
//! pub const resourcesDir = internal_os.resourcesDir;
//! pub const App = struct {};
//! pub const Window = struct {};
//! ```

// PORT: reference `const internal_os = @import("../os/main.zig");` — reached as
// the rustty `rustty_os` crate.

/// PORT: reference `pub const resourcesDir = internal_os.resourcesDir;`.
pub use rustty_os::resourcesDir;

/// PORT: reference `pub const App = struct {};`.
pub struct App;

/// PORT: reference `pub const Window = struct {};`.
pub struct Window;
