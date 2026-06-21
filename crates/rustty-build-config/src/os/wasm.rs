//! The wasm target enum, defined here for BREAK #3.
//!
//! Ported from the `Target` enum of the reference `src/os/wasm/target.zig`.
//! Only the enum is relocated into build-config (it is read by `font::Backend`
//! and `renderer::Backend` `default()` selectors); the `target` comptime const
//! and its `test`-free logic remain owned by the `rustty-os` crate, which
//! re-exports this enum from build-config.

/// The wasm target platform. This is used to toggle certain features
/// on and off since the standard triple target is often not specific
/// enough (i.e. we can't tell wasm32-freestanding is for browser or not).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Browser,
}
