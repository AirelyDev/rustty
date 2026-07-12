//! Always-run integrity guard for the comptime-generated unicode tables
//! (C3, `.port/phase-2/VALIDATION_REPORT_2.md`).
//!
//! The tables in `props_data.rs` / `symbols_data.rs` / `grapheme_data.rs` are
//! reproduced bit-for-bit from ghostty's generator output (R1). The `zig`
//! differential gate (`run_unicode_diff.sh`, G4-uni) proves that reproduction —
//! but it silently downgrades to a WARNING when `zig` is absent, so on any
//! machine/CI without zig NOTHING run by `cargo test` guards a corrupted
//! interior entry among the ~49.7k table values. This module pins an FNV-1a
//! checksum of each array so a single flipped `u16`/`u8` fails `cargo test`
//! everywhere, with no external toolchain. The pinned values were computed from
//! the tables at their zig-verified state; if a table is *legitimately*
//! regenerated, update the constant in the same commit (and the G4-uni gate
//! re-proves the new bytes against the oracle).

#[cfg(test)]
mod tests {
    use super::super::{grapheme_data, props_data, symbols_data};

    const FNV_OFFSET: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x00000100000001b3;

    fn fnv1a(mut hash: u64, bytes: &[u8]) -> u64 {
        for &b in bytes {
            hash ^= b as u64;
            hash = hash.wrapping_mul(FNV_PRIME);
        }
        hash
    }

    fn fnv_u16(seed: u64, arr: &[u16]) -> u64 {
        let mut h = seed;
        for &v in arr {
            h = fnv1a(h, &v.to_le_bytes());
        }
        h
    }

    #[test]
    fn grapheme_data_checksum() {
        // grapheme_data::DATA: [u8; 8192]
        assert_eq!(
            fnv1a(FNV_OFFSET, &grapheme_data::DATA),
            0x29e1968ee59165fd_u64,
            "grapheme_data::DATA checksum drift — a table entry changed; if intentional regen, re-pin (G4-uni re-proves vs oracle)"
        );
    }

    #[test]
    fn symbols_data_checksum() {
        let mut h = fnv_u16(FNV_OFFSET, &symbols_data::STAGE1);
        h = fnv_u16(h, &symbols_data::STAGE2);
        // STAGE3: [bool; 2]
        for &b in &symbols_data::STAGE3 {
            h = fnv1a(h, &[b as u8]);
        }
        assert_eq!(
            h, 0x130b1f007d79a82c_u64,
            "symbols_data checksum drift — re-pin only on intentional regen"
        );
    }

    #[test]
    fn props_data_checksum() {
        let mut h = fnv_u16(FNV_OFFSET, &props_data::STAGE1);
        h = fnv_u16(h, &props_data::STAGE2);
        assert_eq!(
            h, 0xda43b786be4947d_u64,
            "props_data STAGE1+STAGE2 checksum drift — re-pin only on intentional regen"
        );
    }
}
