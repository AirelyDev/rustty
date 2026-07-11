//! rustty-synthetic
//!
//! PORTS: ghostty src/synthetic/ (Bytes, Generator, Osc, Utf8, cli, cli/Ascii,
//! cli/Osc, cli/Utf8, main) — feeds the differential fuzzing harness.
//!
//! PORT STATUS: done
//!
//! The synthetic package contains an abstraction for generating
//! synthetic data. The motivating use case for this package is to
//! generate synthetic data for benchmarking, but it may also expand
//! to other use cases such as fuzzing (e.g. to generate a corpus
//! rather than directly fuzzing).
//!
//! The generators in this package are typically not performant
//! enough to be streamed in real time. They should instead be
//! used to generate a large amount of data in a single go
//! and then streamed from there.
//!
//! The generators are aimed for terminal emulation, but the package
//! is not limited to that and we may want to extract this to a
//! standalone package one day.
//!
//! PORT NOTE: `synthetic/main.zig` — the package root. The `pub use`s below mirror
//! `pub const cli / Generator / Bytes / Utf8 / Osc`. `rand` / `enums` / `io` are
//! the net-new local support modules (`std.Random`, `std.enums.EnumArray`,
//! `std.Io.Writer`) the package leans on.

pub mod bytes;
pub mod cli;
pub mod enums;
pub mod generator;
pub mod io;
pub mod osc;
pub mod rand;
pub mod utf8;

pub use bytes::Bytes;
pub use generator::Generator;
pub use osc::Osc;
pub use utf8::Utf8;

#[cfg(test)]
mod tests {
    /// PORT NOTE: `test { @import("std").testing.refAllDecls(@This()); }` — touch each
    /// public item so it is type-checked.
    #[test]
    fn ref_all_decls() {
        let _ = core::mem::size_of::<crate::Bytes>();
        let _ = core::mem::size_of::<crate::Osc>();
        let _ = core::mem::size_of::<crate::Utf8>();
        let _ = core::mem::size_of::<crate::rand::DefaultPrng>();
        let _ = crate::cli::Action::Ascii;
    }
}
