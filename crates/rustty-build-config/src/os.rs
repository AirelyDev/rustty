//! Minimal target description used by the backend `default()` selectors.
//!
//! BREAK #3: the backend `default()` functions in the reference take a
//! `std.Target` and branch on `target.os.tag`, `target.cpu.arch`, and
//! `target.os.tag.isDarwin()`. To define those selectors in this leaf crate
//! without depending on the upper-tier `rustty-os` crate (which itself depends
//! on build-config — that would be the very cycle BREAK #3 removes), we model
//! the small slice of `std.Target` the selectors actually read. This is a
//! parity-preserving structural relocation only (R4): the exact same OS/arch
//! distinctions the reference branches on are reproduced here.

pub mod wasm;

pub use wasm::Target as WasmTarget;

/// The CPU architecture of a target, modelling the slice of
/// `std.Target.Cpu.Arch` the backend selectors read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arch {
    /// 32-bit WebAssembly (`wasm32`).
    Wasm32,
    /// Any non-wasm architecture (x86_64, aarch64, ...). The reference
    /// selectors only ever distinguish "is this wasm?" so non-wasm arches
    /// collapse to a single variant here.
    Other,
}

impl Arch {
    /// Reference: `target.cpu.arch.isWasm()` / `target.cpu.arch == .wasm32`.
    pub const fn is_wasm(self) -> bool {
        matches!(self, Arch::Wasm32)
    }
}

/// The operating system of a target, modelling the slice of
/// `std.Target.Os.Tag` the backend selectors read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
    /// macOS (`.macos`).
    Macos,
    /// iOS (`.ios`).
    Ios,
    /// Linux (`.linux`).
    Linux,
    /// FreeBSD (`.freebsd`).
    Freebsd,
    /// Windows (`.windows`).
    Windows,
    /// Any other OS tag the selectors do not specifically branch on.
    Other,
}

impl Os {
    /// Reference: `target.os.tag.isDarwin()` — true for macOS and iOS.
    pub const fn is_darwin(self) -> bool {
        matches!(self, Os::Macos | Os::Ios)
    }
}

/// The slice of `std.Target` read by the backend `default()` selectors:
/// the OS tag and the CPU architecture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Target {
    /// `target.os.tag`.
    pub os: Os,
    /// `target.cpu.arch`.
    pub arch: Arch,
}

impl Target {
    /// Construct a target from its OS tag and CPU architecture.
    pub const fn new(os: Os, arch: Arch) -> Target {
        Target { os, arch }
    }
}
