#!/bin/sh
# Build the freestanding VT-C-ABI wasm (`wasm32-unknown-unknown`, pure Rust,
# ZERO imports) — the rustty analog of ghostty's `ghostty-vt.wasm`. This is the
# single module the rustty wasm examples (example/wasm-{vt,sgr,key-encode}) load,
# mirroring how all three ghostty wasm examples load one `ghostty-vt.wasm`.
#
# Pure Rust (rustty-simd scalar fallbacks; Kitty graphics off on this
# freestanding target, so no wuffs and no zlib, as the reference's
# wasm32-freestanding artifact: Ruling 30), so no C toolchain / no zig is
# needed — unlike build-wasm.sh (the wasip1 variant with the C deps).
#
# Mirrors GhosttyLibVt.initWasm's three-step pipeline:
#   (a) link with the indirect function table EXPORTED (`exe.export_table =
#       true` — "so that embedders (e.g. JS in a browser) can insert callback
#       entries for terminal effects"); rust-lld exports it as
#       `__indirect_function_table` via `--export-table`.
#   (b) run the ported `wasm_patch_growable_table` build tool to drop the
#       table's max limit so JS can `table.grow()` (the ghostty pipeline
#       patches rather than passing `--growable-table`; we keep the patcher
#       path to exercise the same ported tool on the same input shape).
#   (c) install the patched module as `zig-out/bin/rustty-vt.wasm` — the
#       analog of ghostty's installed `zig-out/bin/ghostty-vt.wasm`.
#
# Usage: .port/tools/build-vt-wasm.sh [extra cargo args...]
set -e
here=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
root=$(CDPATH='' cd -- "$here/../.." && pwd)

# (a) build with the table exported. `cargo rustc` passes the trailing flags
# only to rustty-vt-c's own compilation; `--export-table` affects the final
# cdylib link (wasm-ld).
#
# `-z stack-size=131072` (5364e5158, GhosttyLibVt.initWasm `exe.stack_size =
# 128 * 1024`): the default wasm stack (1MB for rust-lld, same as Zig 0.16's
# default) has to be preallocated up front in Wasm linear memory, so loading the
# module costs it immediately. Peak stack under various artificial loads is
# 17 KB at the time of this comment but lets do 128KB to be safe. We can lower
# later.
cargo rustc --manifest-path "$root/Cargo.toml" \
  -p rustty-vt-c --target wasm32-unknown-unknown --release "$@" -- \
  -C link-arg=--export-table \
  -C link-arg=-zstack-size=131072

# (b)+(c) patch the table growable and install as rustty-vt.wasm.
# cargo's own target dir (CARGO_TARGET_DIR honoured), where `cargo rustc` just
# wrote the artifact: a literal $root/target read nothing under a custom dir.
# shellcheck source=.port/tools/gates/_target_dir.sh
. "$here/gates/_target_dir.sh"
raw="$(rustty_target_dir "$root")/wasm32-unknown-unknown/release/rustty_vt_c.wasm"
out_dir="$root/zig-out/bin"
mkdir -p "$out_dir"
cargo run --manifest-path "$root/Cargo.toml" -p xtask -- \
  wasm-patch-growable-table "$raw" "$out_dir/rustty-vt.wasm"
echo "installed $out_dir/rustty-vt.wasm"
