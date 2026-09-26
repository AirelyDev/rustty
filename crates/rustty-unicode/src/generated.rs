//! Generated lookup-table data, reproduced bit-for-bit (R1) from the
//! reference generators. See each submodule's header for provenance.

pub mod case_folding_data;
pub mod grapheme_data;
pub mod props_data;
pub mod symbols_data;

// C3: always-run FNV-1a integrity guard for the tables above (VALIDATION_REPORT_2).
#[cfg(test)]
mod checksum;
