//! rustty-crash::sentry_pkg::level
//!
//! PORTS: ghostty pkg/sentry/level.zig.
//!
//! PORT STATUS: done
#![allow(non_snake_case)]

use super::c;

/// sentry_level_t
// PORT NOTE: `pub const Level = enum(c_int) { ... }` — `sentry_level_t` is a
// `c_int` (i32) C enum; the variant values are the reference's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum Level {
    Debug = -1,
    Info = 0,
    Warning = 1,
    Err = 2,
    Fatal = 3,
}

impl Level {
    /// PORT NOTE: Zig `@intFromEnum(level)` — the C-ABI value.
    pub const fn intFromEnum(self) -> c::sentry_level_t {
        self as c::sentry_level_t
    }
}
