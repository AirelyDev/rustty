#!/usr/bin/env bash
# check_abi_schema.sh — G-ABISCHEMA: the librustty-vt ABI type manifest is
# valid against its checked-in JSON Schema (P4-W5 Bundle T, oracle c75559589).
#
# The oracle runs `types-schema-verify.py` as the `test-lib-vt-schema` build
# step (build.zig: schema + the built libghostty-vt shared library); this gate
# is that step for rustty: it builds the ISOLATED `-p rustty-vt-c` dylib (the
# publish rule — see G-VTCLIB: feature unification means only isolated
# invocations produce the shippable artifact), then runs the ported verifier
# (`crates/rustty-vt-c/types-schema-verify.py`, which loads the dylib via
# ctypes, calls `rustty_type_json`, and jsonschema-validates the manifest
# against `crates/rustty-vt-c/types.schema.json`).
#
# NEGATIVE CONTROL (runs on EVERY invocation, not only --self-test): after the
# positive arm passes, the gate re-validates the same manifest against a
# DELIBERATELY BROKEN local schema copy (the `"schema"` const flipped 1 -> 2),
# which MUST fail. If the broken schema passes, the verifier is not binding
# (a vacuous green) and the gate goes RED. Proven at landing: the broken-copy
# run exits non-zero with `/schema: 2 was expected`.
#
# Exit codes: 0 PASS; 3 BLOCKED (missing python/jsonschema and the venv could
# not be provisioned — surfaced, never a silent green); 1 FAIL.
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$ROOT" || exit 1

SCHEMA="crates/rustty-vt-c/types.schema.json"
VERIFIER="crates/rustty-vt-c/types-schema-verify.py"

for f in "$SCHEMA" "$VERIFIER"; do
  if [ ! -f "$f" ]; then
    echo "G-ABISCHEMA FAIL: missing $f" >&2
    exit 1
  fi
done

# ---- python with jsonschema ------------------------------------------------
# Prefer a system python that already has jsonschema; otherwise provision a
# cached venv (PEP 668 blocks --user installs on this host). A host where the
# venv cannot be provisioned (no network, no venv module) is BLOCKED, not green.
PY=""
for cand in "${PYTHON:-}" python3; do
  [ -n "$cand" ] || continue
  if "$cand" -c "import jsonschema" >/dev/null 2>&1; then PY="$cand"; break; fi
done
if [ -z "$PY" ]; then
  VENV="${RUSTTY_ABI_SCHEMA_VENV:-$HOME/.cache/rustty-abi-schema-venv}"
  if [ ! -x "$VENV/bin/python" ]; then
    echo "G-ABISCHEMA: provisioning jsonschema venv at $VENV"
    if ! python3 -m venv "$VENV" >/dev/null 2>&1 \
      || ! "$VENV/bin/pip" install --quiet jsonschema >/dev/null 2>&1; then
      echo "G-ABISCHEMA BLOCKED: no python with jsonschema and the venv could" >&2
      echo "  not be provisioned. Install it with:" >&2
      echo "    python3 -m venv $VENV && $VENV/bin/pip install jsonschema" >&2
      exit 3
    fi
  fi
  if "$VENV/bin/python" -c "import jsonschema" >/dev/null 2>&1; then
    PY="$VENV/bin/python"
  else
    echo "G-ABISCHEMA BLOCKED: venv at $VENV lacks jsonschema" >&2
    exit 3
  fi
fi

# ---- the artifact ----------------------------------------------------------
# ISOLATED -p rustty-vt-c build (G-VTCLIB publish rule).
if ! cargo build -p rustty-vt-c --quiet; then
  echo "G-ABISCHEMA FAIL: cargo build -p rustty-vt-c failed" >&2
  exit 1
fi
DYLIB=""
for cand in target/debug/librustty_vt_c.dylib target/debug/librustty_vt_c.so; do
  [ -f "$cand" ] && DYLIB="$cand" && break
done
if [ -z "$DYLIB" ]; then
  echo "G-ABISCHEMA FAIL: built rustty-vt-c but found no cdylib artifact" >&2
  exit 1
fi

# ---- positive arm ----------------------------------------------------------
out="$("$PY" "$VERIFIER" "$SCHEMA" "$DYLIB" 2>&1)"; rc=$?
printf '%s\n' "$out"
if [ "$rc" -ne 0 ]; then
  echo "G-ABISCHEMA FAIL: the ABI manifest does not validate against $SCHEMA" >&2
  exit 1
fi
# ANTI-VACUITY: the verifier's success line names the target and a type count.
if ! printf '%s\n' "$out" | grep -q "validated .* ABI manifest"; then
  echo "G-ABISCHEMA FAIL: verifier exited 0 without its validation banner" >&2
  exit 1
fi

# ---- negative control (the FAIL arm, proven every run) ---------------------
BROKEN="$(mktemp -t rustty-abi-schema-broken.XXXXXX.json)"
trap 'rm -f "$BROKEN"' EXIT
# Flip the `"schema": { "const": 1 }` pin to 2: every manifest must now fail.
sed 's/"const": 1/"const": 2/' "$SCHEMA" > "$BROKEN"
if "$PY" "$VERIFIER" "$BROKEN" "$DYLIB" >/dev/null 2>&1; then
  echo "G-ABISCHEMA FAIL: negative control did not fire — a deliberately" >&2
  echo "  broken schema copy validated, so the verifier is not binding." >&2
  exit 1
fi
echo "G-ABISCHEMA: negative control fired (broken schema copy rejected)."
echo "G-ABISCHEMA PASS"
exit 0
