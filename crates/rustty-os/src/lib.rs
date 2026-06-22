//! rustty-os
//!
//! PORTS: ghostty src/os/ (34 files incl. wasm/, string_encoding, shell, xdg, path, i18n, ...)
//! + pty.zig. pty.c is built via cc (break #14 relocates os/open->apprt).
//!
//! PORT STATUS: stub
//!
//! Scaffold only — no real logic yet (R1: every line/comment/test{} ported as
//! the crate is carved, leaves-first, under the acyclicity gate per R4).
//!
//! The module-level doc comment below is the verbatim doc from the reference's
//! `src/os/main.zig`, retargeted only for product identity (R2):
//
// The "os" package contains utilities for interfacing with the operating
// system. These aren't restricted to syscalls or low-level operations, but
// also OS-specific features and conventions.

// Namespaces / files (1:1 with the reference's src/os/*.zig + pty.zig). The
// public re-exports from os/main.zig (GetEnvResult, getEnvMap, home, open, ...)
// are reconstructed as each module is ported; the skeleton just declares the
// modules so `cargo build -p rustty-os` is green.

pub mod args;
pub mod cf_release_thread;
pub mod cgroup;
pub mod dbus;
pub mod desktop;
pub mod env;
pub mod file;
pub mod flatpak;
pub mod homedir;
pub mod hostname;
pub mod i18n;
pub mod i18n_locales;
pub mod kernel_info;
pub mod locale;
pub mod mach;
pub mod macos;
pub mod mouse;
pub mod open;
pub mod passwd;
pub mod path;
pub mod pipe;
pub mod pty;
pub mod resourcesdir;
pub mod shell;
pub mod string_encoding;
pub mod systemd;
pub mod tempdir;
pub mod uri;
pub mod windows;
pub mod xdg;

// `os/wasm.zig` plus the `os/wasm/` subdirectory (log.zig, target.zig).
pub mod wasm;
