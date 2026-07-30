//! rustty-config
//!
//! PORTS: ghostty src/config/ minus formatter.zig + key.zig (config.zig, Config, CApi,
//! ClipboardCodepointMap, ErrorList, RepeatableStringMap, Wasm, c_get, command,
//! conditional, edit, file_load, ...); links oniguruma-sys, themes.
//!
//! PORT STATUS: done (flipped per VALIDATION_REPORT.md M2: the config
//! cluster is COMPLETE — C1 leaf value-types, the giant `Config` struct
//! (C2, `config_struct.rs`), and C3 all landed; no live seams).
//!
//! Each file was ported leaves-first under the acyclicity gate (R4):
//! every line/comment/test{} (R1).

// WS14 T6: the desktop `rustty_config_*` C ABI (for the embedded apprt host) is
// not part of `config/Wasm.zig`'s surface and is excluded from the wasm module.
// It also asserts `size_of::<Diagnostic>() == 8`, a 64-bit-pointer invariant of
// that C ABI which cannot hold on 32-bit wasm.
#[cfg(not(target_family = "wasm"))]
pub mod c_api;
pub mod c_get;
pub mod clipboard_codepoint_map;
pub mod command;
pub mod conditional;
pub mod config_struct;
pub mod deep_equal;
pub mod edit;
pub mod error_list;
pub mod file_load;
pub mod formatter_file;
pub mod help_strings;
pub mod io;
pub mod path;
// WS14 T6: gated as a WHOLE module (every `use` in it names `rustty_renderer`).
// Default-on via the `renderer` feature, so native builds are unchanged; the wasm
// module drops it to keep 4 C/C++ `-sys` build scripts out of a freestanding link.
#[cfg(feature = "renderer")]
pub mod renderer_config;
pub mod repeatable_string_map;
pub mod string;
pub mod theme;
pub mod url;
pub mod value_types;
pub mod wasm;

// Re-exports under the reference's top-level names (the config package exposes
// these types via `@import("config/<File>.zig")` from `config/main.zig`).
pub use clipboard_codepoint_map::ClipboardCodepointMap;
// PORT NOTE: `conditional.Key` and the Config `Key` (from key.zig) are DISTINCT
// enums. The package-level `Key` is the Config key (key.zig derives it FROM the
// Config fields), so the top-level re-export points at `config_struct::Key`;
// `conditional::Key` stays namespaced (`conditional::Key`) to avoid ambiguity.
pub use conditional::{Conditional, Op, State};
pub use config_struct::{Config, Key};
pub use error_list::ErrorList;
pub use formatter_file::FileFormatter;
pub use repeatable_string_map::RepeatableStringMap;
// PORT NOTE: `theme.zig` is imported by `Config` as `themepkg` (theme.open /
// theme.Location / theme.LocationIterator). Re-export its public surface.
pub use theme::{
    open as theme_open, open_absolute as theme_open_absolute, Location, LocationIterator,
};
