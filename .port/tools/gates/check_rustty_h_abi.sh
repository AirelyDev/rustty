#!/usr/bin/env bash
# check_rustty_h_abi.sh — the rustty-embed.h product-umbrella ABI gate (Task 4).
#
# Rebrand Task 4 repoints the old "ghostty.h byte-freeze" gate (A3) at the
# PRODUCT embedding umbrella. Task 2 produced two headers:
#
#   ORACLE_HEADER_PATH  <ghostty checkout>/include/ghostty.h — the pinned upstream
#                                                header, kept locally as a
#                                                carve-out so gap-comparison
#                                                tooling can diff product<->
#                                                oracle by name without an
#                                                external checkout.
#   PORT_UMBRELLA    include/rustty-embed.h   — the product's shipped C-ABI
#                                                umbrella: the oracle header
#                                                with every `ghostty_*` token
#                                                mechanically renamed to
#                                                `rustty_*` (Task 2).
#
# `.port/audits/CABI_SYMBOLS.tsv` is the proven ghostty_name -> rustty_name
# map (columns 1 -> 2). This gate proves the rename was applied EXACTLY:
#
#   1. NAME-MAPPED EQUIVALENCE. Extract the oracle's `GHOSTTY_API` function
#      set from ORACLE_HEADER_PATH, apply the TSV's ghostty->rustty map to it,
#      and assert the result is EXACTLY (no more, no less) the product's
#      `RUSTTY_API` function set extracted from PORT_UMBRELLA. Every oracle
#      function must have a TSV row (else the map itself is incomplete); any
#      set mismatch after mapping means the product umbrella added, dropped,
#      or mis-renamed a symbol relative to the oracle.
#
#   2. ZERO-GHOSTTY-LEFT-BEHIND. The product umbrella must contain no
#      `ghostty_*` function token at all — the rebrand must be total, not a
#      partial/aliased rename.
#
# This does NOT re-check header<->TSV coverage/count (that's gate A1,
# check_symbol_manifest.sh) or Rust #[repr(C)] layout parity (that's the
# `abi_layout` cargo test, still exercised by A1's live-export check). This
# gate is purely: product umbrella == oracle umbrella token-renamed by the
# map, and cleanly so.
#
# Exit codes (per ci.sh run_gate contract):
#   0  PASS
#   3  BLOCKED (name-map/oracle/product umbrella unavailable, or required
#              tooling missing) -> WARNING, not green
#   1  FAIL    (name-mapped function sets differ, or a `ghostty_*` function
#              token survives in the product umbrella)
#
# Registered PROGRESS in ci.sh (R5): a genuine RED here means the rebrand's
# ABI-equivalence promise broke and is still escalated to a HARD failure by
# ci.sh's run_gate PROGRESS-went-RED branch.
#
# Usage:
#   check_rustty_h_abi.sh
#   check_rustty_h_abi.sh --print-oracle   # print the derived oracle root (G-ORACLE-SH-SELF arm 5)

set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"

note() { printf '  %s\n' "$*"; }

# --- source the port<->oracle name map (single source of truth) -------------
PARITY_ENV="$ROOT/.port/tools/parity_names.env"
if [ ! -f "$PARITY_ENV" ]; then
  echo "BLOCKED: $PARITY_ENV not found (nothing to source PORT_UMBRELLA/ORACLE_HEADER_PATH/CABI_MAP from)"
  exit 3
fi
# shellcheck source=/dev/null
source "$PARITY_ENV"
# N483: the oracle is DERIVED (`ghostty` beside the main checkout), not written in the map.
# shellcheck source=.port/tools/gates/_oracle_root.sh
source "$ROOT/.port/tools/gates/_oracle_root.sh" || { echo "BLOCKED: cannot source _oracle_root.sh"; exit 3; }
ORACLE_TREE="$(rustty_oracle_root "$ROOT")"
ORACLE_HEADER_PATH="$ORACLE_TREE/include/ghostty.h"
if [ "${1:-}" = "--print-oracle" ]; then
  printf '%s\n' "$ORACLE_TREE"
  exit 0
fi

: "${PORT_UMBRELLA:?PORT_UMBRELLA not set by parity_names.env}"
: "${ORACLE_HEADER_PATH:?ORACLE_HEADER_PATH not derived (the oracle root comes from _oracle_root.sh)}"
: "${CABI_MAP:?CABI_MAP not set by parity_names.env}"
: "${PORT_ENV_PREFIX:?PORT_ENV_PREFIX not set by parity_names.env}"
: "${ORACLE_ENV_PREFIX:?ORACLE_ENV_PREFIX not set by parity_names.env}"

PRODUCT="$ROOT/$PORT_UMBRELLA"
ORACLE="$ORACLE_HEADER_PATH"   # the pinned ghostty oracle checkout (no vendored copy)
TSV="$ROOT/$CABI_MAP"

# --- tooling / inputs (BLOCKED, not FAIL, if absent) ------------------------
for cmd in awk sed sort comm grep; do
  command -v "$cmd" >/dev/null 2>&1 || { echo "BLOCKED: $cmd unavailable"; exit 3; }
done
if [ ! -f "$TSV" ]; then
  echo "BLOCKED: name-map not found at $TSV (CABI_SYMBOLS.tsv absent)"
  exit 3
fi
if [ ! -f "$ORACLE" ]; then
  echo "BLOCKED: ghostty oracle header not found at $ORACLE (external READ-ONLY pinned oracle; its location is derived by _oracle_root.sh as ghostty/ beside the main checkout, never set by the environment — clone the pinned ghostty checkout there)"
  exit 3
fi
# Freeze-pin: the oracle checkout must be the exact version this gate was pinned
# to. Replaces the old vendored include/ghostty.h byte-freeze with a tiny SHA-256.
PIN="$ROOT/.port/tools/oracle-ghostty-h.sha256"
if [ -f "$PIN" ] && { command -v shasum >/dev/null 2>&1 || command -v sha256sum >/dev/null 2>&1; }; then
  if command -v shasum >/dev/null 2>&1; then
    have="$(shasum -a 256 "$ORACLE" | awk '{print $1}')"
  else
    have="$(sha256sum "$ORACLE" | awk '{print $1}')"
  fi
  want="$(cat "$PIN")"
  if [ "$have" != "$want" ]; then
    echo "BLOCKED: oracle $ORACLE drifted from the pinned SHA-256"
    echo "  pinned: $want"; echo "  actual: $have"
    echo "  (bump .port/tools/oracle-ghostty-h.sha256 only after an intentional oracle update)"
    exit 3
  fi
fi
if [ ! -f "$PRODUCT" ]; then
  echo "BLOCKED: product umbrella not found at $PRODUCT (Task 2 not yet seeded here)"
  exit 3
fi

fail=0

echo "rustty-embed.h product-umbrella ABI gate (Task 4)"
echo "  product  : $PRODUCT"
echo "  oracle   : $ORACLE"
echo "  name-map : $TSV"

# --- extract the oracle's GHOSTTY_API function set --------------------------
# Flattens multi-line `${ORACLE_ENV_PREFIX}_API ... ;` decls (mirrors
# check_symbol_manifest.sh's technique) before pulling the first
# ghostty_* token per declaration.
oracle_funcs="$(awk -v pfx="${ORACLE_ENV_PREFIX}_API" \
  '$0 ~ "^" pfx {buf=$0; while (buf !~ /;[ \t]*$/) { if ((getline l) <= 0) break; buf = buf " " l }; print buf}' \
  "$ORACLE" \
  | grep -oE "${ORACLE_NAME}_[A-Za-z0-9_]*\\(" | sed 's/($//;s/(//' | sort -u)"
oracle_count="$(printf '%s\n' "$oracle_funcs" | grep -c . || true)"

# --- extract the product's RUSTTY_API function set --------------------------
product_funcs="$(awk -v pfx="${PORT_ENV_PREFIX}_API" \
  '$0 ~ "^" pfx {buf=$0; while (buf !~ /;[ \t]*$/) { if ((getline l) <= 0) break; buf = buf " " l }; print buf}' \
  "$PRODUCT" \
  | grep -oE "${PORT_NAME}_[A-Za-z0-9_]*\\(" | sed 's/($//;s/(//' | sort -u)"
product_count="$(printf '%s\n' "$product_funcs" | grep -c . || true)"

echo
echo "  oracle ${ORACLE_ENV_PREFIX}_API functions  : $oracle_count"
echo "  product ${PORT_ENV_PREFIX}_API functions   : $product_count"

# --- 1. apply the TSV ghostty->rustty map to the oracle set -----------------
# Build a lookup: for each oracle function, what's its mapped rustty name
# (empty if the TSV has no row for it — an incomplete map, itself a FAIL).
mapped_expected="$(mktemp)"
unmapped="$(mktemp)"
trap 'rm -f "$mapped_expected" "$unmapped"' EXIT

while IFS= read -r gname; do
  [ -z "$gname" ] && continue
  rname="$(awk -F'\t' -v g="$gname" 'NR>1 && $1==g {print $2; found=1} END{if(!found) exit 1}' "$TSV")"
  if [ -z "${rname:-}" ]; then
    echo "$gname" >> "$unmapped"
  else
    echo "$rname" >> "$mapped_expected"
  fi
done <<< "$oracle_funcs"

sort -u -o "$mapped_expected" "$mapped_expected"

if [ -s "$unmapped" ]; then
  note "ORACLE FUNCTIONS WITH NO NAME-MAP ROW (incomplete CABI_SYMBOLS.tsv):"
  sed 's/^/    /' "$unmapped"
  fail=1
fi

# --- 2. assert mapped-oracle set == product set, exactly ---------------------
product_sorted="$(mktemp)"
printf '%s\n' "$product_funcs" | sort -u > "$product_sorted"
missing_in_product="$(comm -23 "$mapped_expected" "$product_sorted" || true)"
extra_in_product="$(comm -13 "$mapped_expected" "$product_sorted" || true)"
rm -f "$product_sorted"

if [ -n "$missing_in_product" ]; then
  note "MISSING FROM PRODUCT (expected via name-map, not found in $PORT_UMBRELLA):"
  while read -r n; do [ -n "$n" ] && note "    $n"; done <<< "$missing_in_product"
  fail=1
fi
if [ -n "$extra_in_product" ]; then
  note "EXTRA/UNEXPECTED IN PRODUCT (not derivable from oracle via name-map):"
  while read -r n; do [ -n "$n" ] && note "    $n"; done <<< "$extra_in_product"
  fail=1
fi
if [ -z "$missing_in_product" ] && [ -z "$extra_in_product" ] && [ ! -s "$unmapped" ]; then
  echo ">> name-mapped equivalence: product umbrella == oracle umbrella token-renamed ($product_count functions)."
fi

# --- 3. zero ghostty_* function tokens left in the product ------------------
leftover="$(grep -oE "${ORACLE_NAME}_[A-Za-z0-9_]*\\(" "$PRODUCT" | sed 's/($//;s/(//' | sort -u || true)"
if [ -n "$leftover" ]; then
  note "LEFTOVER ${ORACLE_NAME}_* FUNCTION TOKENS IN PRODUCT UMBRELLA (rebrand incomplete):"
  while read -r n; do [ -n "$n" ] && note "    $n"; done <<< "$leftover"
  fail=1
else
  echo ">> zero-ghostty-left-behind: no ${ORACLE_NAME}_* function tokens in $PORT_UMBRELLA."
fi

echo
if [ "$fail" -ne 0 ]; then
  echo ">> Task-4 FAIL: product umbrella is not exactly the oracle token-renamed by the name-map."
  exit 1
fi
echo ">> Task-4 PASS: $PORT_UMBRELLA is exactly the pinned ghostty oracle header renamed ${ORACLE_NAME}->${PORT_NAME} ($product_count functions), zero ${ORACLE_NAME}_* left behind."
exit 0
