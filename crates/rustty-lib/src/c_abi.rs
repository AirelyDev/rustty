//! Memory-layout properties shared with C API callers.
//!
//! The C ABI defines how values are represented in memory so code written in
//! different languages can safely exchange them. One part of that contract is
//! alignment: some values must begin at an address divisible by 2, 4, 8, or
//! another power of two. Reading a value from a less-aligned address can be
//! slow on some CPUs and invalid on others.
//!
//! This module derives those properties from Zig's compile-time target data.
//! It does not call or link libc. Keeping the calculation here gives allocators
//! and ABI metadata one source of truth.
//!
//! PORT NOTE (R3, comptime): the reference computes `max_alignment` by folding
//! `builtin.target.cTypeAlignment(c_type)` over every `std.Target.CType`
//! (char..longdouble) at COMPTIME. Rust has no C-type-alignment reflection, so
//! the same fold is spelled as a per-target table below, verified against the
//! reference computation with `zig build-obj` probes per target
//! (`@compileError(comptimePrint(..))`, zig 0.16.0):
//!   aarch64-macos        -> 8   (Apple arm64: `long double` is f64; every
//!                                fundamental C type aligns to <= 8, and
//!                                `@alignOf(*anyopaque)` is 8)
//!   x86_64-macos         -> 16  (`long double` is the x87 f80, aligned 16)
//!   x86_64-linux-gnu     -> 16  (same f80 rule)
//!   aarch64-linux-gnu    -> 16  (`long double` is f128, aligned 16)
//!   wasm32-freestanding  -> 16  (`long double` is f128, aligned 16)

/// Largest address alignment required by a fundamental C ABI type, in bytes.
///
/// For example, a value of 16 means storage intended to hold an arbitrary C
/// value must begin at an address evenly divisible by 16. The Wasm allocator
/// uses this value for caller-owned ABI storage, and `rustty_type_json`
/// publishes it so hosts know the guarantee made by that allocator.
// pub const max_alignment: u16 = max: { ... };
pub const MAX_ALIGNMENT: u16 = if cfg!(all(target_arch = "aarch64", target_vendor = "apple")) {
    8
} else {
    16
};

#[cfg(test)]
mod tests {
    use super::*;

    // Rust-only regression test (no oracle counterpart): the table above can
    // never report less than the pointer alignment the reference fold starts
    // from (`var result: u16 = @alignOf(*anyopaque);`).
    #[test]
    fn max_alignment_at_least_pointer_alignment() {
        assert!(MAX_ALIGNMENT as usize >= core::mem::align_of::<*const ()>());
    }
}
