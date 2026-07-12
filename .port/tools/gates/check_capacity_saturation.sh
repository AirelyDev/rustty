#!/usr/bin/env bash
# check_capacity_saturation.sh — DETECTOR D2: capacity-saturation inventory gate.
#
# Kills Bun semantic-mismatch regression Class 2a (oven-sh/bun#31503/#31504): a
# `ptrs[allocated]` write-then-increment off-by-one was a SILENT out-of-bounds write
# in ReleaseFast Zig, but the naive Rust twin HARD PANICS on the same index. Zig's own
# `IntFittingRange(0, max + 1)` already encoded the true capacity. So every fixed
# `[T; N]` + monotonic write-then-increment counter pair in rustty-vt hot code is
# inventoried in .port/audits/capacity-saturation.tsv, and each pair NAMES a
# `<name>_capacity_saturation` #[test] that drives the bounded store to its ceiling.
# See .port/phase-2/BUN_PORT_LEARNINGS.md §3.2/§4 (D2).
#
# CONTRACT (fail-closed, R5-clean — an ADDITIVE gate, weakens nothing):
#   * Parse the inventory TSV (7 tab cols: pair, array_site, counter_site, N,
#     zig_origin, test, notes).
#   * Every data row MUST name a test (col 6) that is DEFINED in the tree
#     (`fn <name>` under crates/).                       missing/absent => RED (exit 1)
#   * Every array_site (col 2) file MUST exist.          absent file    => RED (exit 1)
#   * TSV itself absent                                  => BLOCKED (exit 3, surfaced as
#     a WARNING by ci.sh per the CI.md exit contract — never a silent green).
# The saturation TESTS themselves run in the normal `cargo test --workspace` lane,
# where an out-of-bounds panic is a RED test — this gate only guarantees every
# inventoried pair HAS such a named guard test and that the inventory stays honest.
#
# Hook: ci.sh static-checks stage, BEFORE cargo test (a source-level gate; no build).
# Registry line (HARD — the inventory is fail-closed and cheap, binds from day one):
#   "D2|HARD|bash $GATES/check_capacity_saturation.sh"
#
# Usage:
#   check_capacity_saturation.sh              run the gate over the inventory
#   check_capacity_saturation.sh --list       print every pair + its named test + status
#   check_capacity_saturation.sh --self-test  prove the gate goes RED on a row whose named
#                                             test is absent, GREEN once it exists
#
# Exit: 0 = GREEN (every pair named + guarded) ; 1 = RED (unnamed/missing test or dead
#       array_site) ; 2 = bad usage ; 3 = BLOCKED (inventory absent).
#
# Portability: bash 3.2 (stock macOS) AND bash 4+/CI — NO associative arrays, plain grep.
set -uo pipefail

GATES_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$GATES_DIR/../../.." && pwd)"
TAB=$'\t'

# test_exists <base> <test_name> : 0 if `fn <name>` is defined anywhere under crates/.
test_exists() {
  local base="$1" name="$2"
  [ -n "$name" ] || return 1
  [ -e "$base/crates" ] || return 1
  grep -rqE --include='*.rs' "fn[[:space:]]+${name}\b" "$base/crates" 2>/dev/null
}

# run_check <base> <tsv> : 0 GREEN / 1 RED / 3 BLOCKED.
run_check() {
  local base="$1" tsv="$2"

  if [ ! -f "$tsv" ]; then
    echo "BLOCKED: capacity-saturation inventory not found at $tsv" >&2
    return 3
  fi

  echo "D2 capacity-saturation inventory"
  echo "  base      : $base"
  echo "  inventory : $tsv"
  echo "  --------------------------------------------------------------"

  local red=0 rows=0
  local line pair array_site counter_site n zig test rest
  while IFS= read -r line || [ -n "$line" ]; do
    case "$line" in ''|'#'*) continue;; esac
    case "$line" in "pair${TAB}"*) continue;; esac   # header
    IFS="$TAB" read -r pair array_site counter_site n zig test rest <<< "$line"
    [ -n "$pair" ] || continue
    rows=$((rows + 1))

    # (1) every pair must name a guard test that exists in the tree.
    if [ -z "$test" ]; then
      echo "  RED  $pair  names NO capacity-saturation test (col 6 empty)"
      red=1
    elif ! test_exists "$base" "$test"; then
      echo "  RED  $pair  named test '$test' not defined in tree (fn $test absent)"
      red=1
    else
      # (2) array_site file must exist (line-drift tolerant: file-level check).
      local afile="${array_site%%:*}"
      if [ -z "$afile" ] || [ ! -f "$base/$afile" ]; then
        echo "  RED  $pair  array_site file missing: '${afile:-<empty>}'"
        red=1
      else
        echo "  OK   $pair  -> $test   [$array_site]"
      fi
    fi
  done < "$tsv"

  echo "  --------------------------------------------------------------"
  if [ "$rows" -eq 0 ]; then
    echo "  RED — inventory has zero data rows (a vacuous inventory is not a gate)." >&2
    return 1
  fi
  if [ "$red" -eq 0 ]; then
    echo "  D2: GREEN — all $rows fixed-array/counter pairs named + guarded."
    return 0
  fi
  echo "  D2: RED — see above." >&2
  return 1
}

# --list : every pair + named test + status.
list_pairs() {
  local tsv="$ROOT/.port/audits/capacity-saturation.tsv"
  [ -f "$tsv" ] || { echo "(inventory absent: $tsv)"; return 3; }
  local line pair array_site counter_site n zig test rest status
  while IFS= read -r line || [ -n "$line" ]; do
    case "$line" in ''|'#'*) continue;; esac
    case "$line" in "pair${TAB}"*) continue;; esac
    IFS="$TAB" read -r pair array_site counter_site n zig test rest <<< "$line"
    [ -n "$pair" ] || continue
    if test_exists "$ROOT" "$test"; then status="ok"; else status="MISSING-TEST"; fi
    printf '%-22s %-46s %s\n' "$pair" "$test" "$status"
  done < "$tsv"
}

# --self-test : hermetic proof of fail-closed behavior.
#   (1) a row whose named test is ABSENT => RED.
#   (2) same row once the test EXISTS     => GREEN.
self_test() {
  local tmp; tmp="$(mktemp -d)"
  # shellcheck disable=SC2064
  trap "rm -rf '$tmp'" RETURN
  mkdir -p "$tmp/crates/demo/src"
  cat > "$tmp/crates/demo/src/buf.rs" <<'RS'
struct Demo { data: [u8; 4], len: u8 }
RS

  printf 'pair\tarray_site\tcounter_site\tN\tzig_origin\ttest\tnotes\n' > "$tmp/base.tsv"
  printf 'demo_pair\tcrates/demo/src/buf.rs:1\tcrates/demo/src/buf.rs:1\t4\tsynthetic:0\tdemo_capacity_saturation\tself-test row\n' >> "$tmp/base.tsv"

  echo "### self-test (1/3): named test ABSENT -> expect RED"
  if run_check "$tmp" "$tmp/base.tsv" >/dev/null 2>&1; then
    echo "SELF-TEST FAIL: gate was GREEN with a row whose guard test does not exist" >&2
    return 1
  fi
  echo "  -> RED as required."

  cat >> "$tmp/crates/demo/src/buf.rs" <<'RS'
#[test]
fn demo_capacity_saturation() { assert_eq!(2 + 2, 4); }
RS
  echo "### self-test (2/3): named test now EXISTS -> expect GREEN"
  if run_check "$tmp" "$tmp/base.tsv" >/dev/null 2>&1; then
    echo "  -> GREEN as required."
  else
    echo "SELF-TEST FAIL: gate was RED once the named test existed + array_site file present" >&2
    return 1
  fi

  echo "### self-test (3/3): inventory ABSENT -> expect BLOCKED (exit 3)"
  run_check "$tmp" "$tmp/nope.tsv" >/dev/null 2>&1
  if [ "$?" -eq 3 ]; then
    echo "  -> BLOCKED as required."
  else
    echo "SELF-TEST FAIL: absent inventory did not surface as BLOCKED (exit 3)" >&2
    return 1
  fi

  echo "SELF-TEST PASS: fail-closed on unnamed/missing tests; blocked when inventory absent."
  return 0
}

main() {
  case "${1:-}" in
    --self-test) self_test;;
    --list)      list_pairs;;
    "")          run_check "$ROOT" "$ROOT/.port/audits/capacity-saturation.tsv";;
    *) echo "usage: $0 [--self-test|--list]" >&2; return 2;;
  esac
}
main "$@"
