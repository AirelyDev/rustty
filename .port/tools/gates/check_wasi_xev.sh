#!/usr/bin/env bash
# GATE G-WASI — libxev's `wasi_poll` backend, EXECUTED under a WASI runtime.
#
# PORTS: libxev's own WASI leg, `zig build test -Dtarget=wasm32-wasi -fwasmtime`
# (`.github/workflows/test.yml` of the libxev Ghostty pins; commented out there
# since Zig 0.13, "WASI has regressed"). Ghostty's own dev shell carries
# wasmtime for the same purpose.
#
# Before this gate the backend was cross-compiled and linted only
# (VERIFICATION_DEBT V8): `cargo test --no-run` linked the test binary and
# nothing ever ran it. This runs `cargo test -p rustty-xev --target
# wasm32-wasip1` with wasmtime as the runner, the recipe V8 recorded.
#
# WHY WASMTIME, and only wasmtime: the loop's `poll_oneoff` uses ABSOLUTE clock
# subscriptions (`SUBSCRIPTION_CLOCK_ABSTIME`, as the reference does). Measured
# 2026-10-10: wazero 1.12 and iwasm 2.4.5 reject them (ENOTSUP), and Node 25's
# `node:wasi` blocks on them (every timer test hangs). wasmtime implements them.
#
# THREE ARMS:
#   1. the whole rustty-xev suite (lib, integration tests, doctests) passes;
#   2. anti-vacuity: every loop-driving `wasi_poll` / `wasi_watcher` test is
#      named `... ok` in the output. A renamed or `#[cfg]`'d-away test would
#      otherwise leave a smaller suite that still says "ok";
#   3. negative control: `wasi_file` run WITHOUT the `--dir=.` preopen must
#      fail on its fixture. If it passed, the file arm would not be exercising
#      the preopened directory it claims to use.
# A fourth control, a no-op `Loop::tick`, needs a source mutation, so it is not
# in the gate; its run is recorded in V8 (every loop-driving test fails or
# hangs with it).
#
# BLOCKED (exit 3), never green, without wasmtime or the wasm32-wasip1 std.
#
# Exit contract (`.port/tools/CI.md`): 0 = pass, 1 = red, 3 = BLOCKED.
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$ROOT" || exit 1

blocked() { echo "G-WASI BLOCKED: $*"; exit 3; }
red()     { echo "G-WASI RED: $*" >&2; exit 1; }

command -v wasmtime >/dev/null 2>&1 \
  || blocked "wasmtime not installed (brew install wasmtime, or a release tarball from github.com/bytecodealliance/wasmtime)"
rustup target list --installed 2>/dev/null | grep -qx 'wasm32-wasip1' \
  || blocked "the wasm32-wasip1 std is not installed (rustup target add wasm32-wasip1)"

# The loop-driving tests. Each must appear as `<name> ... ok`.
EXPECTED=(
  wasi_poll::tests::wasi_loop_time
  wasi_poll::tests::wasi_timer
  wasi_poll::tests::wasi_timer_reset
  wasi_poll::tests::wasi_timer_reset_before_tick
  wasi_poll::tests::wasi_timer_reset_after_trigger
  wasi_poll::tests::wasi_timer_cancellation
  wasi_poll::tests::wasi_canceling_a_completed_operation
  wasi_poll::tests::wasi_file
  wasi_poll::tests::wasi_callback_reenters_through_stored_loop_pointer
  wasi_watcher::tests::wasi_watcher_async
  wasi_watcher::tests::wasi_watcher_async_notify_first
  wasi_watcher::tests::wasi_watcher_async_batches_multiple_notifications
  wasi_watcher::tests::wasi_watcher_timer
  wasi_watcher::tests::wasi_watcher_timer_reset
  wasi_watcher::tests::wasi_watcher_timer_reset_before_tick
  wasi_watcher::tests::wasi_watcher_timer_reset_after_trigger
  wasi_watcher::tests::wasi_watcher_timer_cancel
)

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

RUNNER='target.wasm32-wasip1.runner=["wasmtime","--dir=."]'

echo "=== G-WASI 1/3 :: cargo test -p rustty-xev --target wasm32-wasip1 (wasmtime $(wasmtime --version | awk '{print $2}')) ==="
cargo test -p rustty-xev --target wasm32-wasip1 --locked --config "$RUNNER" > "$WORK/test.log" 2>&1
rc=$?
grep -E '^test result:' "$WORK/test.log" | sed 's/^/    /'
if [ "$rc" -ne 0 ]; then
  grep -E 'FAILED|panicked|error' "$WORK/test.log" | head -20 | sed 's/^/    /' >&2
  red "the rustty-xev suite failed under wasmtime (exit $rc)"
fi

echo "=== G-WASI 2/3 :: anti-vacuity (${#EXPECTED[@]} loop-driving tests ran) ==="
missing=0
for t in "${EXPECTED[@]}"; do
  if ! grep -qxF "test $t ... ok" "$WORK/test.log"; then
    echo "    MISSING  $t" >&2
    missing=1
  fi
done
[ "$missing" -eq 0 ] || red "a loop-driving test did not run and pass (see above)"
echo "    ok      all ${#EXPECTED[@]} ran and passed"

echo "=== G-WASI 3/3 :: negative control (wasi_file without the preopen) ==="
wasm="$(cargo test -p rustty-xev --target wasm32-wasip1 --locked --lib --no-run --message-format=json 2>/dev/null \
  | sed -n 's/.*"executable":"\([^"]*\)".*/\1/p' | tail -1)"
[ -n "$wasm" ] && [ -f "$wasm" ] || red "could not locate the rustty-xev lib test binary"
if wasmtime "$wasm" --exact wasi_poll::tests::wasi_file --nocapture > "$WORK/nc.log" 2>&1; then
  red "wasi_file passed with no preopened directory: the file arm is not using the preopen"
fi
grep -q 'pre-opened' "$WORK/nc.log" \
  || { sed 's/^/    /' "$WORK/nc.log" | head -10 >&2; red "wasi_file failed without the preopen, but not on its fixture"; }
echo "    ok      wasi_file fails on its fixture without --dir=."

echo "G-WASI GREEN"
