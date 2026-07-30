#!/bin/sh
# Reproducible browser-freestanding build of the `main_wasm.zig` artifact:
# os/wasm + config.Wasm + terminal + font/Atlas.Wasm, as a PURE-RUST
# wasm32-unknown-unknown cdylib with ZERO wasi imports (only the `env` Canvas
# imports of Atlas.Wasm's debug canvas). No C is cross-compiled: the wasm link
# surface excludes the 5 C `-sys` deps by construction (WS14 T6 — the default-on
# `renderer`/`oniguruma`/`input-oniguruma` features are off in this crate's
# dependency declarations, so glslang / spirv-cross / wuffs / sentry / oniguruma
# never enter the graph).
#
# Verify the result with:
#   cargo test -p rustty-wasm -- --ignored
#   .port/tools/gates/ws14_webgl_browser_gate.sh
#
# Usage:  .port/tools/build-main-wasm.sh [--release] [extra cargo args...]
set -e
exec cargo build -p rustty-wasm --target wasm32-unknown-unknown "$@"
