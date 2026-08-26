//! rustty-vt-c result
//!
//! PORTS: ghostty src/terminal/c/result.zig
//!
//! PORT STATUS: done

use std::os::raw::c_int;

/// C: RusttyResult
// PORT NOTE: the reference is `pub const Result = enum(c_int) { ... }` named
// `C: GhosttyResult`; retargeted to `RusttyResult` per R2. `#[repr(C)]` with an
// explicit `c_int`-width discriminant matches `enum(c_int)`.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Result {
    Success = 0,
    OutOfMemory = -1,
    InvalidValue = -2,
    OutOfSpace = -3,
    NoValue = -4,
    IoError = -5,
    LimitExceeded = -6,
    Rejected = -7,
}

impl Result {
    /// PORT NOTE: cbindgen emits `#[repr(C)]` enums with explicit values, but the
    /// negative discriminants are carried as the C `int` they round-trip to. This
    /// helper exposes the backing `c_int` for FFI-boundary returns where the
    /// reference returns the enum directly by value.
    pub fn to_c_int(self) -> c_int {
        self as c_int
    }
}
