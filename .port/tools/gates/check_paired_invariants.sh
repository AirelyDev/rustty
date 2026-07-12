#!/usr/bin/env bash
# check_paired_invariants.sh — detector D5 gate (paired-invariant catalog).
#
# Kills Bun Class 4 (coupled-emission-site invariant drift, oven-sh/bun#30678):
# a predicate consulted at 2+ producer/consumer sites drifting silently because
# no single-file diff looks wrong. See .port/phase-2/BUN_PORT_LEARNINGS.md §4
# (D5) and PORT_RULES.md pre-port checklist item 2.
#
# The PRIMARY gate is the human review rule stated in VT_INVARIANTS.md ("when a
# chunk touches either end, update the catalog"). This script is the mechanical
# TRIPWIRE for silent drift underneath that rule. It verifies, WITHOUT compiling:
#   1. the catalog exists and lists every EXPECTED_INV block;
#   2. every cited producer/consumer ANCHOR file still exists and still contains
#      its durable token (a moved/renamed end that skipped the catalog update is
#      exactly the drift D5 catches);
#   3. the in-crate seam TEST file + named test fns still exist.
#
# Exit codes (per ci.sh run_gate contract):
#   0  PASS
#   3  BLOCKED (tooling/paths unavailable) -> surfaced as WARNING, never green
#   1  FAIL    (catalog missing an INV block, a cited anchor vanished, or a seam
#              test disappeared -> the catalog drifted from the source; update it)
#
# This gate is OWNED by detector D5 and lives only in files named for D5.

set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
CATALOG="$ROOT/.port/phase-2/VT_INVARIANTS.md"
TESTS="$ROOT/crates/rustty-vt/tests/d5_paired_invariants.rs"

fail=0
note() { printf '  %s\n' "$*"; }

# --- tooling / catalog presence (BLOCKED, not FAIL, if absent) --------------
command -v grep >/dev/null 2>&1 || { echo "BLOCKED: grep unavailable"; exit 3; }
if [ ! -f "$CATALOG" ]; then
  echo "BLOCKED: catalog not found at $CATALOG (D5 not yet seeded here)"
  exit 3
fi

echo "D5 paired-invariant catalog: $CATALOG"

# --- 1. every expected INV block is present ---------------------------------
EXPECTED_INV=(
  "### INV-1 — dirty-tracking ↔ renderer damage"
  "### INV-2 — style-id refcount: Page ↔ style set"
  "### INV-3 — kitty placements ↔ renderer atlas"
  "### INV-4 — mode-state ↔ key-encoder"
)
for hdr in "${EXPECTED_INV[@]}"; do
  if ! grep -qF -- "$hdr" "$CATALOG"; then
    note "MISSING invariant block: $hdr"; fail=1
  fi
done

# --- 2. every cited anchor file still exists AND still contains its token ----
# Format: "relpath|token". Token is grepped as a FIXED string. A vanished token
# means an end moved/renamed without the catalog being updated -> D5 drift.
ANCHORS=(
  # INV-1 dirty <-> renderer damage
  "crates/rustty-vt/src/page.rs|fn set_dirty"
  "crates/rustty-vt/src/pagelist/access.rs|fn clear_dirty"
  "crates/rustty-renderer/src/generic.rs|row_data"
  # INV-2 style-id refcount Page <-> style set
  "crates/rustty-vt/src/ref_counted_set.rs|item.meta.ref_count > RefCount::ZERO"
  "crates/rustty-vt/src/screen.rs|fn manual_style_update"
  # INV-3 kitty placements <-> renderer atlas
  "crates/rustty-vt/src/kitty/graphics_storage.rs|pub dirty"
  "crates/rustty-renderer/src/image.rs|fn kitty_requires_update"
  # INV-4 mode-state <-> key-encoder
  "crates/rustty-vt/src/modes.rs|pub fn set"
  "crates/rustty-vt/src/input/key_encode.rs|fn from_terminal"
)
for a in "${ANCHORS[@]}"; do
  path="${a%%|*}"; token="${a#*|}"
  f="$ROOT/$path"
  if [ ! -f "$f" ]; then
    note "ANCHOR FILE GONE: $path (cited by the catalog — update it)"; fail=1; continue
  fi
  if ! grep -qF -- "$token" "$f"; then
    note "ANCHOR TOKEN GONE: '$token' no longer in $path (end moved; update catalog)"; fail=1
  fi
done

# --- 3. in-crate seam tests still exist -------------------------------------
if [ ! -f "$TESTS" ]; then
  note "SEAM TEST FILE GONE: crates/rustty-vt/tests/d5_paired_invariants.rs"; fail=1
else
  for fn in "fn mode_state_key_encoder_seam" "fn style_id_refcount_page_style_set_seam"; do
    grep -qF -- "$fn" "$TESTS" || { note "SEAM TEST GONE: $fn"; fail=1; }
  done
fi

echo
echo "D5 REVIEW RULE (primary gate): when a chunk touches EITHER end of any"
echo "invariant in VT_INVARIANTS.md (producer or consumer file), the same PR"
echo "MUST update the catalog and keep the seam tests green. New cross-site"
echo "predicates are catalogued as part of the tier's pre-port gate."

if [ "$fail" -ne 0 ]; then
  echo ">> D5 FAIL: catalog drifted from source, or a seam test vanished. Update"
  echo "            .port/phase-2/VT_INVARIANTS.md to match the moved end(s)."
  exit 1
fi
echo ">> D5 PASS: catalog complete; all anchors and seam tests present."
exit 0
