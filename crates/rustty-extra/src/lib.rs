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
//! - `rustty_cli::rustty::Action::{ALL, tag_name, option_specs}` ==
//!   `@typeInfo(Action).@"enum".fields` + `@field(Action,name).options()`.
//!
//! PORT NOTE: ghostty's `comptimeGenerate*` two-pass counter/fixed-buffer wrapper
//! (a `std.Io.Writer.Discarding` size pre-count then a `.fixed(&buf)` fill) is a
//! comptime-alloc artifact with no Rust analog; it collapses to a single growable
//! `Vec`.
//!
//! CORRECTION (task-18, closes S83-F6). The sentence that followed said "The
//! `+gen-*` CLI actions + `build.rs` file-emit stay DEFERRED (task scope) — this
//! crate provides the generator fns + fidelity tests only." BOTH halves have since
//! landed, and both were checked before this was rewritten:
//!
//! - the `+gen-*` CLI (`main_build_data.zig`) is `xtask/src/main_build_data.rs`,
//!   whose `Action` switch calls `rustty_extra::{bash,fish,zsh,sublime,vim}`
//!   directly (`main_build_data.rs:117-124`);
//! - the file-emit half is `xtask`'s `res_shell::gen_shell`,
//!   `res_editor::gen_editor` and `res_terminfo::gen_terminfo`, which write the
//!   same bytes into `zig-out/share`, with an anti-drift test pinning the two
//!   byte sources together.
//!
//! This crate is still generators-plus-tests; what changed is that it now has
//! consumers.

pub mod bash;
pub mod fish;
pub mod mdgen;
pub mod sublime;
pub mod vim;
pub mod webgen;
pub mod zsh;
