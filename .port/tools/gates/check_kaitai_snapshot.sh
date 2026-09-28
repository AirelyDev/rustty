#!/usr/bin/env bash
# check_kaitai_snapshot.sh — G-KAITAI: the snapshot wire format's normative
# Kaitai spec (crates/rustty-vt/src/snapshot/snapshot.ksy) parses every
# checked-in golden fixture, byte-accounted and CRC32C-checked (phase-4
# wbench; oracle 38d92c50c's CI-side `verify-kaitai.py`, ported verbatim
# beside the .ksy).
#
# WHY THIS EXISTS: the .ksy is the format's normative spec and the 22
# `testdata/*.hex` goldens are comptime artifacts carried byte-for-byte
# (assessment B §2.2). The Rust codec tests exercise the goldens through the
# implementation; this gate proves the SPEC still describes the same bytes,
# so spec drift (a .ksy edit that no longer matches the goldens, or a golden
# regenerated against unlanded code) is caught CI-side exactly as upstream's
# `nix develop`-hosted check does.
#
# TOOLING PROBE (the check_test_counts.py oracle-absent precedent): the
# verifier needs python3 + the `kaitaistruct` runtime + kaitai-struct-compiler.
# A host without them is BLOCKED (exit 3, surfaced as a WARNING by ci.sh) —
# NEVER a silent green. For the python runtime a cached venv is provisioned
# when possible (the check_abi_schema.sh precedent); the compiler is a JVM
# tool we never auto-install. On macOS `brew install kaitai-struct-compiler`
# (0.11, pulls openjdk) is enough — ci.yml's macOS leg installs it (N402);
# elsewhere, the kaitai.io release zip plus any JRE on PATH.
#
# Exit codes follow the ci.sh contract: 0 PASS, 3 BLOCKED (tooling absent),
# other FAIL.
set -u

SELF_DIR="$(cd "$(dirname "$0")" && pwd)"
RUSTTY_ROOT="$(cd "$SELF_DIR/../../.." && pwd)"

VERIFIER="$RUSTTY_ROOT/crates/rustty-vt/src/snapshot/verify-kaitai.py"
TESTDATA="$RUSTTY_ROOT/crates/rustty-vt/src/snapshot/testdata"

# ---- self-test -------------------------------------------------------------
# Proves the banner/count arm can go RED on fabricated verifier output, so the
# acceptance arm is never vacuous (a gate whose FAIL arm is never exercised is
# the vacuity this program has found repeatedly).
if [ "${1:-}" = "--self-test" ]; then
  fail=0
  probe() { # $1 = fabricated output, $2 = expected fixture count, $3 = verdict
    local got
    got="$(printf '%s\n' "$1" | sed -n 's/^validated \([0-9]*\) snapshot fixtures$/\1/p' | tail -1)"
    if [ -z "$got" ]; then
      [ "$3" = "nobanner" ] || { echo "  self-test: expected $3, got no banner"; fail=1; }
      return
    fi
    if [ "$got" -ne "$2" ]; then
      [ "$3" = "red" ] || { echo "  self-test: expected $3, got red ($got != $2)"; fail=1; }
    else
      [ "$3" = "green" ] || { echo "  self-test: expected $3, got green ($got == $2)"; fail=1; }
    fi
  }
  probe "ok complete-v1.hex
validated 22 snapshot fixtures" 22 green
  # [b] a verifier that silently skipped fixtures must not be read as a pass.
  probe "validated 21 snapshot fixtures" 22 red
  # [c] exit-0-without-banner (harness changed shape) must not be read as a pass.
  probe "something unrelated" 22 nobanner
  if [ "$fail" -eq 0 ]; then
    echo "SELF-TEST: PASS (3 arms: count-exact, count-short->RED, no-banner)"
    exit 0
  fi
  echo "SELF-TEST: FAIL" >&2
  exit 1
fi

for f in "$VERIFIER" "$TESTDATA"; do
  if [ ! -e "$f" ]; then
    echo "G-KAITAI FAIL: missing $f" >&2
    exit 1
  fi
done

# ---- tooling probe ---------------------------------------------------------
# kaitai-struct-compiler: a JVM CLI, never auto-installed.
KSC=""
for cand in kaitai-struct-compiler ksc; do
  if command -v "$cand" >/dev/null 2>&1; then KSC="$cand"; break; fi
done
if [ -z "$KSC" ]; then
  echo "G-KAITAI BLOCKED: kaitai-struct-compiler (or ksc) is not on PATH; the"
  echo "  snapshot .ksy spec cannot be compiled. Install it from"
  echo "  https://kaitai.io/#download (never a silent green — R5)."
  exit 3
fi

# python with the kaitaistruct runtime: prefer a system python that has it,
# else provision a cached venv (the check_abi_schema.sh precedent).
PY=""
for cand in "${PYTHON:-}" python3; do
  [ -n "$cand" ] || continue
  if "$cand" -c "import kaitaistruct" >/dev/null 2>&1; then PY="$cand"; break; fi
done
if [ -z "$PY" ]; then
  VENV="${RUSTTY_KAITAI_VENV:-$HOME/.cache/rustty-kaitai-venv}"
  if [ ! -x "$VENV/bin/python" ]; then
    echo "G-KAITAI: provisioning kaitaistruct venv at $VENV"
    if ! python3 -m venv "$VENV" >/dev/null 2>&1 \
      || ! "$VENV/bin/pip" install --quiet kaitaistruct >/dev/null 2>&1; then
      echo "G-KAITAI BLOCKED: no python with kaitaistruct and the venv could" >&2
      echo "  not be provisioned. Install it with:" >&2
      echo "    python3 -m venv $VENV && $VENV/bin/pip install kaitaistruct" >&2
      exit 3
    fi
  fi
  if "$VENV/bin/python" -c "import kaitaistruct" >/dev/null 2>&1; then
    PY="$VENV/bin/python"
  else
    echo "G-KAITAI BLOCKED: venv at $VENV lacks kaitaistruct" >&2
    exit 3
  fi
fi

# ---- run the verifier ------------------------------------------------------
out="$("$PY" "$VERIFIER" 2>&1)"; rc=$?
printf '%s\n' "$out"
if [ "$rc" -ne 0 ]; then
  echo "G-KAITAI FAIL: verify-kaitai.py exited $rc" >&2
  exit 1
fi

# ANTI-VACUITY: the success banner must name exactly the number of goldens on
# disk — a verifier that silently skipped fixtures (or found none) is RED, not
# green. The expected side is computed from the repo, so adding a golden never
# turns the gate stale.
want="$(find "$TESTDATA" -name '*.hex' | wc -l | tr -d ' ')"
if [ "$want" -eq 0 ]; then
  echo "G-KAITAI FAIL: zero *.hex goldens under $TESTDATA — an empty" >&2
  echo "  reference side would make this gate VACUOUS (R5)." >&2
  exit 1
fi
got="$(printf '%s\n' "$out" | sed -n 's/^validated \([0-9]*\) snapshot fixtures$/\1/p' | tail -1)"
if [ -z "$got" ]; then
  echo "G-KAITAI FAIL: verifier exited 0 without its validation banner" >&2
  exit 1
fi
if [ "$got" -ne "$want" ]; then
  echo "G-KAITAI FAIL: verifier validated $got fixtures but $want goldens are checked in" >&2
  exit 1
fi
echo "G-KAITAI PASS: $got/$want snapshot goldens validate against snapshot.ksy"
exit 0
