//! Tests for `config_struct`: the ghostty src/config/Config.zig `test {}` blocks
//! and the Rust-only exhaustiveness tests, split by topic — `parse` (the PARSE
//! pass), `clone_changed` (CLONE + CHANGED + the `Key` exhaustiveness test),
//! `machinery` (load/loadReader/finalize) and `integration`. key.zig's
//! `test "Value"` stays with `Key` in `key.rs`.

use super::*;

mod clone_changed;
mod integration;
mod machinery;
mod parse;
