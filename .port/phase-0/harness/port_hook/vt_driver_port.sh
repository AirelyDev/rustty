#!/usr/bin/env bash
# vt_driver_port.sh — PORT-side VT oracle hook for the differential harness.
#
# CONTRACT (must mirror ref_driver/vt_driver.c exactly):
#   - Same CLI:  [--cols N] [--rows N] [--scrollback N]
#                [--format plain|vt|html] [--trim] [--no-trim] [--unwrap]
#                [INPUT_FILE]   (or VT bytes on stdin)
#   - Reads identical VT bytes, runs them through the rustty VT core, and emits
#     the SAME canonical Screen/PageList serialization to stdout (byte-for-byte
#     with the reference when the port is built with the `identity-ghostty`
#     feature, per R2 — no canonicalization is performed here).
#   - stdout = canonical bytes ONLY; diagnostics to stderr; exit 0 on success.
#
# CURRENT STATE: rustty is EMPTY (no crates exist yet). There is therefore no
# rustty VT core to invoke. This stub intentionally produces NOTHING on stdout
# and exits non-zero, which is exactly what the EMPTY-WORKSPACE GATE
# (gate_empty_divergence.sh) asserts: the harness must report ~100% divergence
# because the port emits no canonical output at all.
#
# How this becomes real (Phase 1+, no harness change required):
#   When rustty produces an equivalent driver — either a C ABI binary built from
#   `rustty-vt-c` (cbindgen `rustty.h` + `rustty_terminal_vt_write` /
#   `rustty_formatter_*`, linked against `librustty-vt.a`) OR a Rust binary in
#   `rustty-bench`/a harness crate — point RUSTTY_VT_DRIVER at it (or drop a
#   binary at bin/vt_driver_port). It MUST be built with `--features
#   identity-ghostty` so emitted identity tokens match the reference byte-exact.
#   This wrapper then forwards argv+stdin to it unchanged.
set -euo pipefail

HARNESS_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# Resolution order for a real port driver:
#   1. $RUSTTY_VT_DRIVER (explicit override)
#   2. bin/vt_driver_port (built artifact, if/when it exists)
PORT_BIN="${RUSTTY_VT_DRIVER:-}"
if [ -z "$PORT_BIN" ] && [ -x "$HARNESS_DIR/bin/vt_driver_port" ]; then
  PORT_BIN="$HARNESS_DIR/bin/vt_driver_port"
fi

if [ -n "$PORT_BIN" ] && [ -x "$PORT_BIN" ]; then
  # Real port driver present: forward everything unchanged.
  exec "$PORT_BIN" "$@"
fi

# --- EMPTY PORT STUB ---------------------------------------------------------
# No rustty VT core exists yet. Emit nothing (so canonical output is empty) and
# signal "port unavailable" via a distinct, documented exit code so the
# comparator can attribute the divergence precisely (not confuse it with a
# crash). stdout stays empty on purpose.
echo "PORT STUB: rustty is empty — no VT driver available (exit 70)" >&2
exit 70
