//! rustty-config
//!
//! PORTS: ghostty src/config/ minus formatter.zig + key.zig (config.zig, Config, CApi,
//! ClipboardCodepointMap, ErrorList, RepeatableStringMap, Wasm, c_get, command,
//! conditional, edit, file_load, ...); links oniguruma-sys, themes.
//!
//! PORT STATUS: C1 leaf value-types ported (string/path/io/command/url/
//! RepeatableStringMap/conditional/ClipboardCodepointMap/ErrorList/edit/
//! file_load). The giant `Config` struct (C2) + remaining files are later.
//!
//! These are the config VALUE-TYPES the giant Config struct (C2, later)
//! composes. Each is ported leaves-first under the acyclicity gate (R4):
//! every line/comment/test{} (R1).

pub mod clipboard_codepoint_map;
pub mod command;
pub mod conditional;
pub mod edit;
pub mod error_list;
pub mod file_load;
pub mod io;
pub mod path;
pub mod repeatable_string_map;
pub mod string;
pub mod url;

// Re-exports under the reference's top-level names (the config package exposes
// these types via `@import("config/<File>.zig")` from `config/main.zig`).
pub use clipboard_codepoint_map::ClipboardCodepointMap;
pub use conditional::{Conditional, Key, Op, State};
pub use error_list::ErrorList;
pub use repeatable_string_map::RepeatableStringMap;
