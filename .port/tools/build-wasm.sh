#!/bin/sh
# Reproducible wasm32-wasip1 build for the rustty VT C-ABI (mirrors ghostty's
# ghostty-vt.wasm). Wires zig (0.15.x) as the C/C++ cross-compiler for the -sys
# crates via the cc-crate's target-scoped CC_/CXX_/AR_ env vars, so NATIVE builds
# are completely untouched (they never see these vars).
#
# Usage:  .port/tools/build-wasm.sh [--release] [extra cargo args...]
# Default target crate is rustty-vt-c (the .wasm cdylib deliverable).
set -e
here=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
export CC_wasm32_wasip1="$here/zig-cc-wasm.sh"
export CXX_wasm32_wasip1="$here/zig-cxx-wasm.sh"
export AR_wasm32_wasip1="$here/zig-ar-wasm.sh"
exec cargo build -p rustty-vt-c --target wasm32-wasip1 "$@"
