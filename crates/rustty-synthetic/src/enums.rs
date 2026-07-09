//! PORT: `std.enums.EnumArray(Key, f64)` — a dense `f64` array indexed by an
//! enum in declaration order. Following the rustty convention (rustty-vt's
//! `screen.rs` uses a bespoke struct instead of `std.EnumArray`), we model it as
//! `[f64; N]` plus the enum's `from_index`/`to_index` (declaration order). The
//! `.values` slice is what `weightedIndex` consumes; `Indexer.keyForIndex(idx)`
//! maps the result back to the variant.

/// PORT: `std.enums.EnumArray(Key, f64)`.
#[derive(Clone, Copy)]
pub struct EnumArray<const N: usize> {
    /// PORT: `.values` — the raw slice handed to `weightedIndex`.
    pub values: [f64; N],
}

impl<const N: usize> EnumArray<N> {
    /// PORT: `.initFill(v)`.
    pub fn init_fill(v: f64) -> EnumArray<N> {
        EnumArray { values: [v; N] }
    }

    /// PORT: `.set(key, v)` — `key` given by its declaration-order index.
    pub fn set(&mut self, index: usize, v: f64) {
        self.values[index] = v;
    }
}
