//! rustty-crash::sentry_pkg::c
//!
//! PORTS: ghostty pkg/sentry/c.zig —
//! ```text
//! pub const c = @cImport({
//!     @cInclude("sentry.h");
//! });
//! ```
//!
//! PORT STATUS: done
//!
//! The bindgen-generated `sentry-sys` bindings are the Rust image of that
//! `@cImport` (generated from the SAME public `sentry.h` of the vendored
//! sentry-native 0.7.8, R3). NOTE: bindgen emits C enum constants as
//! `sentry_value_type_t_SENTRY_VALUE_TYPE_*`-style consts.

pub use sentry_sys::*;
