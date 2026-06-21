//! PORT NOTE: Ports ghostty `src/lib/target.zig`. Zig's `Target` is a
//! `union(enum)` with no payloads, which is equivalent to a plain Rust enum.

/// The target for ABI generation. The detection of this is left to the
/// caller since there are multiple ways to do that.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    C,
    Zig,
}
