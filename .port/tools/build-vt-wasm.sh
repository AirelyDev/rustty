#!/bin/sh
# Build the freestanding VT-C-ABI wasm (`wasm32-unknown-unknown`, pure Rust,
# ZERO imports) — the rustty analog of ghostty's `ghostty-vt.wasm`. This is the
# single module the rustty wasm examples (example/wasm-{vt,sgr,key-encode}) load,
# mirroring how all three ghostty wasm examples load one `ghostty-vt.wasm`.
#
# Pure Rust (rustty-simd scalar fallbacks + no zlib on wasm), so no C toolchain /
# no zig is needed — unlike build-wasm.sh (the wasip1 variant with the C deps).
#
# Usage: .port/tools/build-vt-wasm.sh [extra cargo args...]
set -e
here=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
root=$(CDPATH= cd -- "$here/../.." && pwd)
exec cargo build --manifest-path "$root/Cargo.toml" \
  -p rustty-vt-c --target wasm32-unknown-unknown --release "$@"
