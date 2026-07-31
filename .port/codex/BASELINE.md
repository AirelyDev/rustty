# Audit Baseline

Captured 2026-07-31 on macOS 26.5.1 (Darwin 25.5.0, arm64).

## Repository revisions

- Rustty: `5243303fbb2f2cfd0832edfb8fa49d72ce322d81`
  (`fix(xev): the ASan crash was a dropped Zig field default, not an ASan limitation`)
- Ghostty checkout: `6ad1fe7d8cbda36c77b337a96c9bea8a77883699`
  (`url: exclude trailing spaces from path matches (#13505)`)
- Rustty's root manifest declares the original oracle as Ghostty `4749c4e`.
  This audit compares the current checkouts and treats later upstream changes as
  possible oracle drift that must be labeled, not automatically as a port bug.

Both checkouts were clean before the audit. The only Rustty working-tree change
after initialization was the new untracked `.port/codex/` audit directory.

## Toolchain and available targets

- Rust: `rustc 1.93.1 (01f6ddf75 2026-02-11)`
- Cargo: `cargo 1.93.1 (083ac5135 2025-12-15)`
- Zig: `0.16.0`
- Installed Rust targets:
  - `aarch64-apple-darwin`
  - `wasm32-unknown-unknown`
  - `wasm32-wasip1`
  - `x86_64-pc-windows-msvc`

Linux and FreeBSD Rust targets are not installed, so this host can execute
platform-neutral/macOS tests and can attempt the installed Windows/WASM target
checks. Linux/FreeBSD conclusions require source/build-graph proof or an
existing executable cross-target harness and are labeled accordingly.

## Pre-existing worktrees

The main checkout is on `main`. One locked worktree already existed before this
audit and is user-owned:

`/Users/gael/projects/rustty/.claude/worktrees/agent-a6ed621178ba98abb`

It is on branch `miri-adversarial` at `586747a...` and will not be touched or
removed. Any audit-created worktree will be separately named, recorded, and
removed before completion.
