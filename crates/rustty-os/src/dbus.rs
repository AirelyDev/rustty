//! rustty-os::dbus
//!
//! PORT STATUS: ported
//!
//! Ports the reference's `src/os/dbus.zig` — every line, comment, and `test {}`
//! is ported here (R1). Retargeted only for product identity (R2).

// PORT NOTE: the Zig reference imports `std` and `builtin` and switches on
// `builtin.os.tag`. In Rust the OS switch is expressed with `cfg!(target_os)`;
// the environment lookups use `std::env::var_os` (the reference's
// `std.posix.getenv`).

/// Returns true if the program was launched by D-Bus activation.
///
/// On Linux GTK, this returns true if the program was launched using D-Bus
/// activation. It will return false if Rustty was launched any other way.
///
/// For other platforms and app runtimes, this returns false.
#[allow(non_snake_case)]
pub fn launchedByDbusActivation() -> bool {
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        // On Linux, D-Bus activation sets `DBUS_STARTER_ADDRESS` and
        // `DBUS_STARTER_BUS_TYPE`. If these environment variables are present
        // (no matter the value) we were launched by D-Bus activation.
        std::env::var_os("DBUS_STARTER_ADDRESS").is_some()
            && std::env::var_os("DBUS_STARTER_BUS_TYPE").is_some()
    }

    // No other system supports D-Bus so always return false.
    #[cfg(not(any(target_os = "linux", target_os = "android")))]
    {
        false
    }
}
