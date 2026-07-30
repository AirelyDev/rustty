#!/bin/sh
# WS14 gate: renders the Atlas.Wasm texture through WebGL2 in headless Chrome and
# asserts per-cell ink counts against a CPU expectation computed from the same
# atlas bytes — plus TWO negative controls (wrong atlas offset; black shader) that
# MUST fail on every run. Exit 0 = pass; 1 = assertion failure; 3 = environment
# missing (never silently green).
#
# Prerequisite:  .port/tools/build-main-wasm.sh --release
set -e
ROOT=$(cd "$(dirname "$0")/../../.." && pwd)
exec node "$ROOT/.port/tools/gates/ws14_webgl_browser_gate.mjs" "$@"
