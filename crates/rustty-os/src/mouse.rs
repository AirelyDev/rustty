//! rustty-os::mouse
//!
//! PORT STATUS: ported
//!
//! Ports the reference's `src/os/mouse.zig` — every line, comment, and `test {}`
//! is ported here (R1). Retargeted only for product identity (R2).

// PORT NOTE: the Zig reference imports `std`, `builtin`, and `objc`, and uses
// `std.log.scoped(.os)`. In Rust the macOS branch binds to the SAME Objective-C
// runtime via `objc2` (R3: a binding, not a reimplementation); we send the
// `doubleClickInterval` message to the `NSEvent` class. The non-macOS branch
// returns `null`.

#[allow(dead_code)]
const LOG_SCOPE: &str = "os";

/// The system-configured double-click interval if its available.
#[allow(non_snake_case)]
pub fn clickInterval() -> Option<u32> {
    #[cfg(target_os = "macos")]
    {
        // On macOS, we can ask the system.
        use objc2::runtime::AnyClass;
        let Some(ns_event) = AnyClass::get(c"NSEvent") else {
            log_err("NSEvent class not found. Can't get click interval.");
            return None;
        };

        // Get the interval and convert to ms
        // PORT NOTE: `NSEvent.msgSend(f64, objc.sel("doubleClickInterval"), .{})`.
        let interval: f64 = unsafe { objc2::msg_send![ns_event, doubleClickInterval] };
        let ms = (interval * 1000.0).ceil() as u32;
        Some(ms)
    }

    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}

#[cfg(target_os = "macos")]
fn log_err(msg: &str) {
    log::error!(target: LOG_SCOPE, "{msg}");
}
