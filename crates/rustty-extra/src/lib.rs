//! Port of ghostty's `src/extra/` — the comptime generators that emit shell
//! completions (bash/zsh/fish) and editor-syntax files (vim/sublime) by reflecting
//! over `Config`'s fields + the CLI `Action` enum.
//!
//! PORT STATUS: done
//!
//! ghostty exposes each script as a `pub const … = comptime …` byte string
//! (`main_build_data.zig`'s `+gen-*` actions dump them to files). Rust cannot
//! comptime-fold a `const`, so each becomes a **runtime pure fn returning
//! `Vec<u8>`** (byte-slices, never lossy `String` — completion scripts are bytes).
//! Each `write*` fn takes `&mut Vec<u8>` (the `std.Io.Writer` analog) and
//! transcribes the reference line-for-line (R1), driving the reflection surfaces:
//!
//! - `rustty_config::config_struct::{config_fields, config_completion_kind}` ==
//!   `@typeInfo(Config).@"struct".fields` + the `@typeInfo(field.type)` switch.
//! - `rustty_config::help_strings::config_help` == `@hasDecl(help_strings.Config,
//!   name)` / `@field(...)` (fed to `fish::get_description`).
//! - `rustty_cli::ghostty::Action::{ALL, tag_name, option_specs}` ==
//!   `@typeInfo(Action).@"enum".fields` + `@field(Action,name).options()`.
//!
//! PORT NOTE: ghostty's `comptimeGenerate*` two-pass counter/fixed-buffer wrapper
//! (a `std.Io.Writer.Discarding` size pre-count then a `.fixed(&buf)` fill) is a
//! comptime-alloc artifact with no Rust analog; it collapses to a single growable
//! `Vec`. The `+gen-*` CLI actions + `build.rs` file-emit stay DEFERRED (task
//! scope) — this crate provides the generator fns + fidelity tests only.

pub mod bash;
pub mod fish;
pub mod sublime;
pub mod vim;
pub mod zsh;
