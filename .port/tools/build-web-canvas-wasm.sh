#!/bin/sh
# Reproducible browser-freestanding build of the rustty-web-canvas deliverable:
# the VT terminal C-ABI + the font-shaper web_canvas backend, as a PURE-RUST
# wasm32-unknown-unknown cdylib with ZERO wasi imports (only the `env` Canvas
# imports). Unlike build-wasm.sh (wasm32-wasip1, which wired zig as the C/C++
# cross-compiler for the -sys crates), this target links NO C: on wasm the
# simdutf/highway SIMD kernels fall back to scalar Rust and the kitty zlib path
# is gated off, so no C toolchain is needed. NATIVE builds are untouched.
#
# Usage:  .port/tools/build-web-canvas-wasm.sh [--release] [extra cargo args...]
set -e
exec cargo build -p rustty-web-canvas --target wasm32-unknown-unknown "$@"
