# Agent Development Guide

A file for [guiding coding agents](https://agents.md/).

## Commands

- **Build:** `cargo build`
  - To skip the macOS app and speed up compilation, just build the workspace
    crate you need — `cargo build -p <crate>` (e.g. `cargo build -p rustty-vt`).
- **Test:** `cargo test`
  - Prefer to run targeted tests because the full suite is slow. Scope to a
    crate with `-p <crate>` and filter by name with `cargo test <name>`.
- **Test filter:** `cargo test -p <crate> <test name>`
- **Formatting (Rust):** `cargo fmt` (and `cargo clippy` for lints)
- **Formatting (Swift):** `swiftlint lint --strict --fix`
- **Formatting (other):** `prettier -w .`

## librustty-vt

- Build: `cargo build -p rustty-vt-c --release`
- Build WASM: `cargo build -p rustty-vt-c --release --target wasm32-unknown-unknown`
- Test: `cargo test -p rustty-vt-c <filter>`
  - Prefer this when the change is in a librustty-vt file. Run the affected
    tests under Miri (`cargo +nightly miri test -p rustty-vt-c`) when touching
    raw-pointer or FFI code.
- All C enums in `include/rustty/vt/` must have a `_MAX_VALUE = RUSTTY_ENUM_MAX_VALUE`
  sentinel as the last entry to force int enum sizing (pre-C23 portability).

## Directory Structure

- Cargo workspace crates: `crates/` — the crown-jewel crates are `rustty-vt`
  (VT/terminal engine), `rustty-core`, and `rustty-renderer`, alongside the
  supporting crates (`rustty-config`, `rustty-font`, `rustty-input`, …).
- macOS app: `macos/`
- GTK (Linux and FreeBSD) app: `crates/rustty-apprt` (note: not yet live).

## Issue and PR Guidelines

- Never create an issue.
- Never create a PR.
