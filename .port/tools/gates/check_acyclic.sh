#!/usr/bin/env bash
# check_acyclic.sh — GATE G7 (crate graph acyclic) [R4]
#
# DoP id:  G7  (DEFINITION_OF_PARITY §1, "Crate graph acyclic")
# Reads:   crates/*/Cargo.toml  (the ACTUAL rustty crate dependency graph)
#          .port/tools/cycle_check.py (reference structural pre-check, optional)
# Pass:    no cross-crate cycle in the real rustty Cargo workspace graph.
#
# This is the load-bearing wrapper the task asks for: it runs the
# `cargo_cycle_check.py` helper (which PARSES crates/*/Cargo.toml [dependencies]
# and Tarjan-detects SCCs in the real rustty graph) and exits non-zero on any
# cross-crate cycle. It MUST be GREEN on the empty/stub workspace (the scaffold
# crates already declare an acyclic, topologically-ordered dep graph).
#
# It ALSO, when --with-reference is passed, runs the existing reference
# .port/tools/cycle_check.py and asserts its "cross-crate SCCs (size>1): 0" line
# in the RUNTIME-ONLY section. NOTE (DoP exit-code caveat): cycle_check.py models
# the *un-broken* reference mapping and prints non-zero runtime SCCs by design,
# so --with-reference is informational unless the modeled breaks have landed;
# the BINDING G7 check is the real Cargo graph (default behaviour here).
#
# Usage:
#   check_acyclic.sh                  # check real rustty Cargo graph (binding)
#   check_acyclic.sh --include-dev    # also include dev-dep edges
#   check_acyclic.sh --with-reference # additionally print reference SCC pre-check
#   check_acyclic.sh --self-test      # prove the gate flips (inject a cycle)
#
# Exit 0 = GREEN (acyclic), non-zero = RED (cycle) or BLOCKED.
set -euo pipefail

GATES_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TOOLS_DIR="$(cd "$GATES_DIR/.." && pwd)"
PORT_DIR="$(cd "$TOOLS_DIR/.." && pwd)"
RUSTTY_ROOT="$(cd "$PORT_DIR/.." && pwd)"
PY="${PYTHON:-python3}"

INCLUDE_DEV=""
WITH_REFERENCE=0
SELF_TEST=0
for arg in "$@"; do
  case "$arg" in
    --include-dev)    INCLUDE_DEV="--include-dev" ;;
    --with-reference) WITH_REFERENCE=1 ;;
    --self-test)      SELF_TEST=1 ;;
    *) echo "check_acyclic.sh: unknown arg '$arg'" >&2; exit 2 ;;
  esac
done

run_self_test() {
  echo "=== G7 SELF-TEST (inject a synthetic cycle, prove RED, then GREEN) ==="
  local tmp
  tmp="$(mktemp -d "${TMPDIR:-/tmp}/g7_selftest.XXXXXX")"
  local crates="$tmp/crates"
  mkdir -p "$crates/rustty-a" "$crates/rustty-b"
  # acyclic: a depends on nothing, b depends on a
  cat > "$crates/rustty-a/Cargo.toml" <<'EOF'
[package]
name = "rustty-a"
[dependencies]
EOF
  cat > "$crates/rustty-b/Cargo.toml" <<'EOF'
[package]
name = "rustty-b"
[dependencies]
rustty-a = { workspace = true }
EOF
  # run the checker with CRATES_DIR pointed at the synthetic tree by copying the
  # helper into a temp tools/gates and a temp .port layout.
  local tg="$tmp/.port/tools/gates"
  mkdir -p "$tg"
  cp "$GATES_DIR/cargo_cycle_check.py" "$tg/"
  # the helper derives RUSTTY_ROOT from its own location -> $tmp
  local ok=1
  echo "--- [acyclic] expect GREEN (exit 0) ---"
  if "$PY" "$tg/cargo_cycle_check.py"; then
    echo "  -> exit 0 (GREEN) OK"
  else
    echo "  -> unexpected non-zero"; ok=0
  fi
  # now inject a cycle: a depends on b too
  cat > "$crates/rustty-a/Cargo.toml" <<'EOF'
[package]
name = "rustty-a"
[dependencies]
rustty-b = { workspace = true }
EOF
  echo "--- [cyclic] expect RED (non-zero) ---"
  if "$PY" "$tg/cargo_cycle_check.py"; then
    echo "  -> unexpected exit 0 (should have detected cycle)"; ok=0
  else
    echo "  -> non-zero (RED) OK"
  fi
  rm -rf "$tmp"
  if [ "$ok" = "1" ]; then
    echo "SELF-TEST: PASS"; return 0
  else
    echo "SELF-TEST: FAIL"; return 1
  fi
}

if [ "$SELF_TEST" = "1" ]; then
  run_self_test
  exit $?
fi

echo "=== G7 :: crate graph acyclic ==="

# --- binding check: real rustty Cargo graph ---
echo "--- rustty Cargo dependency graph (binding) ---"
set +e
"$PY" "$GATES_DIR/cargo_cycle_check.py" $INCLUDE_DEV
RC=$?
set -e

# --- optional: reference structural pre-check ---
if [ "$WITH_REFERENCE" = "1" ]; then
  echo ""
  echo "--- reference structural pre-check (.port/tools/cycle_check.py) ---"
  if [ -f "$TOOLS_DIR/cycle_check.py" ]; then
    REF_OUT="$("$PY" "$TOOLS_DIR/cycle_check.py" 2>/dev/null || true)"
    # extract the RUNTIME-ONLY cross-crate SCC count line
    RT_LINE="$(printf '%s\n' "$REF_OUT" | awk '
      /RUNTIME ONLY/ {flag=1}
      flag && /cross-crate SCCs \(size>1\)/ {print; exit}
    ')"
    echo "  ${RT_LINE:-<runtime SCC line not found>}"
    echo "  (informational: models the UN-broken reference mapping; the binding"
    echo "   G7 result is the rustty Cargo graph above.)"
  else
    echo "  (reference cycle_check.py not found; skipping)"
  fi
fi

# --- binding directed-edge assertion: rustty-font MUST NOT depend on rustty-vt ---
# D1 (Option C, signed off): rustty-font is PURE and BELOW vt; apc/glyph in
# rustty-vt deps rustty-font DIRECTLY. The reverse edge (font -> vt) would form
# a cycle AND violate the layering contract, so we forbid it explicitly in EVERY
# dependency table (prod, dev, build). Fail loudly if it is ever reintroduced.
echo ""
echo "--- layering assertion: rustty-font has NO rustty-vt edge (prod/dev/build) ---"
FONT_TOML="$RUSTTY_ROOT/crates/rustty-font/Cargo.toml"
EDGE_RC=0
if [ ! -f "$FONT_TOML" ]; then
  echo "BLOCKED: $FONT_TOML not found" >&2
  EDGE_RC=2
else
  # match a dependency key `rustty-vt` (allow rustty_vt / quoting / whitespace),
  # ignoring comment lines so the options.rs-style prose can mention it freely.
  if "$PY" - "$FONT_TOML" <<'PYEOF'
import re, sys
path = sys.argv[1]
bad = []
with open(path, encoding="utf-8") as f:
    for n, line in enumerate(f, 1):
        s = line.split("#", 1)[0]  # drop trailing/whole-line comments
        # a dependency declaration: `rustty-vt = ...` or `rustty_vt = ...`
        if re.match(r'^\s*["\']?rustty[-_]vt["\']?\s*=', s):
            bad.append((n, line.rstrip("\n")))
if bad:
    for n, l in bad:
        print(f"  FORBIDDEN font->vt edge at line {n}: {l}", file=sys.stderr)
    sys.exit(1)
sys.exit(0)
PYEOF
  then
    echo "  GREEN: rustty-font declares no rustty-vt dependency"
  else
    echo "RED: rustty-font declares a forbidden rustty-vt dependency (font is BELOW vt; D1)" >&2
    EDGE_RC=1
  fi
fi

echo ""
if [ "$RC" -eq 0 ] && [ "$EDGE_RC" -eq 0 ]; then
  echo "GREEN: rustty crate graph is acyclic (G7)"
  exit 0
elif [ "$RC" -ne 0 ]; then
  echo "RED: rustty crate graph has a cross-crate cycle (G7)"
  exit 1
else
  echo "RED: rustty-font->rustty-vt layering violation (G7)"
  exit 1
fi
