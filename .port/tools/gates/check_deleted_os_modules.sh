#!/usr/bin/env bash
# check_deleted_os_modules.sh — GATE G-OSDEL (the two upstream file deletions
# stay deleted) [P3-W7 T4, area-E §4]
#
# Upstream `e8525c0fd` DELETED `src/os/args.zig` (−130) and `src/os/env.zig`
# (−178). rustty mirrored both. This gate is the standing proof that no consumer
# creeps back:
#
#   * the files themselves must not exist
#   * no CODE (as opposed to comment/doc) line may name `rustty_os::args`,
#     `rustty_os::env` or `internal_os::env`
#
# The one deliberate exception carved out by the plan is the GTK module's
# file-local `extern "C"` block, which does NOT name `rustty_os::env` at all
# (it declares its own `env_c` mod), so no exception list is needed — a fact
# worth stating because an exception list is exactly how this kind of gate rots.
#
# Usage:
#   check_deleted_os_modules.sh              # scan (binding)
#   check_deleted_os_modules.sh --self-test  # prove the gate flips
#
# Exit 0 = GREEN, non-zero = RED.
set -uo pipefail

GATES_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RUSTTY_ROOT="$(cd "$GATES_DIR/../../.." && pwd)"

PATTERN='rustty_os::env|internal_os::env|rustty_os::args'

if [ "${1:-}" = "--self-test" ]; then
  echo "=== G-OSDEL SELF-TEST ==="
  ok=1
  echo "--- [clean] expect GREEN ---"
  if "$0" >/dev/null 2>&1; then echo "  -> exit 0 (GREEN) OK"; else echo "  -> unexpected non-zero"; ok=0; fi
  probe="$RUSTTY_ROOT/crates/rustty-os/src/__g_osdel_probe.rs"
  printf 'pub fn probe() { let _ = rustty_os::env::getEnvMap(); }\n' > "$probe"
  echo "--- [dirty] expect RED ---"
  if "$0" >/dev/null 2>&1; then echo "  -> unexpected exit 0"; ok=0; else echo "  -> non-zero (RED) OK"; fi
  rm -f "$probe"
  [ "$ok" = 1 ] && { echo "SELF-TEST: PASS"; exit 0; } || { echo "SELF-TEST: FAIL"; exit 1; }
fi

echo "=== G-OSDEL :: src/os/{args,env}.zig deletions mirrored ==="

rc=0
for f in crates/rustty-os/src/args.rs crates/rustty-os/src/env.rs; do
  if [ -e "$RUSTTY_ROOT/$f" ]; then
    echo "RED: $f exists; upstream deleted its oracle in e8525c0fd" >&2
    rc=1
  else
    echo "  GREEN: $f is deleted"
  fi
done

# Non-comment references only: the PORT NOTEs that explain the deletion are the
# point of the R1 comment convention and must not trip the gate.
hits="$(grep -rnE "$PATTERN" "$RUSTTY_ROOT/crates" --include='*.rs' 2>/dev/null \
        | grep -vE ':[[:space:]]*(//|///|//!|\*)' || true)"
if [ -n "$hits" ]; then
  echo "RED: live references to the deleted modules:" >&2
  printf '%s\n' "$hits" >&2
  rc=1
else
  echo "  GREEN: no non-comment reference to rustty_os::{args,env}"
fi

echo ""
if [ "$rc" -eq 0 ]; then
  echo "GREEN: G-OSDEL"
else
  echo "RED: G-OSDEL"
fi
exit "$rc"
