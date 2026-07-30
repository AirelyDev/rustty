#!/bin/sh
# WS19 T4 (audit row N3): ONE `gtk4::init()` PER TEST BINARY.
#
# `gtk4::init()` binds GTK to the calling thread, and libtest runs every `#[test]`
# on its own spawned thread — even under `--test-threads=1`. A second `gtk4::init()`
# in the same test binary therefore ALWAYS panics with
#   "Attempted to initialize GTK from two different threads"
# (or "Failed to acquire default main context" when the two race). WS16 T2
# established the one-per-binary rule and split the four x11 KATs at `a7fa3b7`, but
# `tests/gtk_wayland_winproto.rs` was missed and shipped with TWO — a latent,
# guaranteed failure in already-merged code that nothing detected, because these KATs
# are `#[ignore]`d and only ever run by hand on the VM.
#
# This gate is the missing detector. It is deliberately a STATIC check: it must bind
# on macOS too (where the GTK KATs cannot even compile), so a macOS-only lane still
# catches a bad merge.
#
# Counting note: the sweep in the WS19 plan used `grep -c 'gtk4::init()'`, which also
# matches the phrase inside the module doc-comments that explain this very rule — a
# false-positive generator. This gate counts CALL SITES only (a line whose first
# non-whitespace is `gtk4::init()`), and additionally refuses to count comment lines.
set -eu
root=$(cd "$(dirname "$0")/../../.." && pwd); cd "$root"
dir="crates/rustty-apprt/tests"

echo "== G-GTKINIT: one gtk4::init() per test binary =="

if [ ! -d "$dir" ]; then
  echo "  BLOCKED: $dir does not exist; the gate cannot bind (never a silent green)."
  exit 3
fi

# Anti-vacuity: if NO file calls gtk4::init() at all, the gate is measuring nothing.
# That is either a repo-shape change or a broken pattern — surface it, never pass.
total=0
bad=0
for f in "$dir"/*.rs; do
  [ -f "$f" ] || continue
  n=$(grep -cE '^[[:space:]]*gtk4::init\(\)' "$f" || true)
  total=$((total + n))
  if [ "$n" -gt 1 ]; then
    echo "  RED  $f: $n gtk4::init() call sites (must be <= 1)"
    grep -nE '^[[:space:]]*gtk4::init\(\)' "$f" | sed 's/^/         /'
    bad=$((bad + 1))
  fi
done

if [ "$total" -eq 0 ]; then
  echo "  RED (VACUOUS): no gtk4::init() call site found under $dir at all."
  echo "  Either the GTK KATs were removed, or this gate's pattern no longer matches."
  echo "  A gate that matches nothing is worthless — fix the pattern, do not delete it."
  exit 1
fi

if [ "$bad" -gt 0 ]; then
  echo "  Split the offending file one-test-per-binary (WS16 precedent a7fa3b7):"
  echo "  each headed GTK #[test] gets its OWN tests/<name>.rs, because GTK is"
  echo "  thread-pinned and libtest gives every #[test] a fresh thread."
  exit 1
fi

echo "  OK ($total gtk4::init() call site(s) across $(ls "$dir"/*.rs | wc -l | tr -d ' ') test file(s); none doubled)"
echo "G-GTKINIT: GREEN"
