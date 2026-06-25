//! Package terminfo provides functionality related to terminfo/termcap files.
//!
//! At the time of writing this comment, the focus is on generating terminfo
//! files so that we can maintain our terminfo in Zig instead of hand-writing
//! the archaic (imo) terminfo format by hand. But eventually we may want to
//! extract this into a more full-featured library on its own.
//!
//! PORTS: ghostty src/terminfo/ (Source, ghostty -> entry name `rustty` per R2, main).
//! Feature identity-ghostty emits ghostty/xterm-ghostty for the parity harness.
//!
//! Per R2 the terminfo *names* carry the rustty product identity via the
//! `rustty-identity` crate (the reference's `ghostty.zig` exposes `ghostty`;
//! this crate exposes [`entry::rustty`]).

#![forbid(unsafe_code)]

pub mod entry;
pub mod source;

pub use entry::rustty;
pub use source::Source;

// The reference `main.zig` has a single `test { refAllDecls(@This()) }` that
// forces every public declaration in the package to be analyzed/tested. Rust
// resolves and type-checks all `pub` items in the crate at build time, so the
// equivalent guarantee is provided by `cargo build`/`cargo test` compiling the
// whole crate; the per-module unit tests (in `source.rs` and `entry.rs`) are the
// ported `test {}` blocks themselves.
#[cfg(test)]
mod tests {
    use super::*;

    /// Faithful analogue of `main.zig`'s `refAllDecls` test: reference every
    /// public declaration so it is exercised by the test build.
    #[test]
    fn ref_all_decls() {
        // Source + its constructors are reachable.
        let _ = rustty();
        let _: fn() -> Source = rustty;
    }
}
