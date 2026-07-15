#!/usr/bin/env bash
# check_symbol_manifest.sh — GATE A1 (macOS Swift-app C-ABI symbol manifest + drift) [PROGRESS]
#
# DoP id:  A1  (2026-07-12-macos-swift-app-milestone.md, Task A1)
# Reads:   $GHOSTTY_H (pinned upstream header; default $ORACLE_HEADER_PATH) — R3 external oracle
#          .port/audits/CABI_SYMBOLS.tsv        (the committed manifest: ghostty_name<TAB>rustty_name
#                                                 <TAB>signature<TAB>source_file<TAB>darwin_only)
#          crates/**/*.rs                        (the LIVE rustty_* C-ABI exports)
# Pass:    every `^GHOSTTY_API` entrypoint in the pinned header (== 90) has EXACTLY one
#          TSV row AND that row's rustty_name is a live `extern "C" fn rustty_*(` export
#          somewhere under crates/. Any drift (header name w/o a row, orphan TSV row, TSV
#          count != header count, or a manifest rustty_* with no live export) is RED.
#
# The macOS app C-ABI is APPEND-ONLY (PREPORT §2 I9 / risk R-H): a symbol may never be
# renamed or dropped once shipped, so this gate is the machine-checkable ABI ledger that
# the A3 header-diff gate builds on.
#
# A2 (the `ghostty_*` alias-forwarder layer, previously verified by a section here) was
# DELETED by rebrand Task 1: the reused Ghostty Swift app now links against `rustty_*`
# directly, so there is no alias code left to verify. This gate now checks only A1: (a)
# header <-> TSV coverage/count, and (b) every TSV `rustty_name` has a live
# `extern "C" fn` export under crates/.
#
# Registered PROGRESS (R5): it reports honest drift without blocking CI. At the time of
# authoring, 3 main_c-tier exports (rustty_translate / rustty_info / rustty_cli_try_action)
# are not yet ported, so this gate is legitimately RED until those seams land — that is the
# intended progress signal, not a spurious failure.
#
# Usage:
#   check_symbol_manifest.sh                 # verify header <-> TSV <-> live exports
#   check_symbol_manifest.sh --self-test     # prove the gate flips RED on an injected drift
#
# Exit 0 = GREEN (no drift), 1 = RED (drift), 3 = BLOCKED (pinned header unavailable).
set -euo pipefail

# ── locate repo + inputs ────────────────────────────────────────────────────
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"

# Port<->oracle name map (rebrand Task 0, .port/tools/PARITY_NAMING.md): the
# single source of truth for which side is the ORACLE (stays ghostty) vs the
# PORT (rustty). Sourcing it here means this gate never hardcodes either name
# literal beyond what the map itself pins.
# shellcheck source=/dev/null
source "$REPO_ROOT/.port/tools/parity_names.env"

TSV="${CABI_SYMBOLS_TSV:-$REPO_ROOT/$CABI_MAP}"
GHOSTTY_H="${GHOSTTY_H:-$ORACLE_HEADER_PATH}"
CRATES_DIR="$REPO_ROOT/crates"

red()  { printf 'RED: %s\n'  "$*" >&2; }
warn() { printf 'WARNING: %s\n' "$*" >&2; }

# ── core check; operates on a caller-supplied TSV (so --self-test can pass a mutated copy)
# echoes a human summary; returns 0 GREEN / 1 RED / 3 BLOCKED.
run_check() {
  local tsv="$1"
  local rc=0

  if [ ! -f "$tsv" ]; then red "manifest not found: $tsv"; return 1; fi

  # TSV data rows (skip the header line) and quick shape check (5 tab-separated fields).
  local tsv_rows
  tsv_rows="$(tail -n +2 "$tsv" | grep -c . || true)"
  local badshape
  badshape="$(tail -n +2 "$tsv" | awk -F'\t' 'NF!=5{c++} END{print c+0}')"
  if [ "$badshape" -ne 0 ]; then red "$badshape TSV row(s) do not have exactly 5 tab-separated fields"; rc=1; fi

  # (b) every manifest rustty_name has a LIVE `extern "C" fn <name>(` export under crates/.
  local missing_export=0 r
  while IFS=$'\t' read -r gname rname _sig _src _dj; do
    [ -z "${gname:-}" ] && continue
    r="$rname"
    if ! grep -rqE "extern \"C\" fn ${r}\\(" "$CRATES_DIR" 2>/dev/null; then
      red "manifest row '$gname' -> '$rname' has NO live rustty_* export under crates/"
      missing_export=$((missing_export+1))
    fi
  done < <(tail -n +2 "$tsv")
  [ "$missing_export" -ne 0 ] && rc=1

  # (a)+(count) header-dependent checks — need the pinned oracle header (R3).
  if [ ! -f "$GHOSTTY_H" ]; then
    warn "pinned header absent ($GHOSTTY_H) — skipped header<->TSV coverage + count checks (R3 external oracle)"
    if [ "$rc" -ne 0 ]; then return 1; fi
    return 3
  fi

  # flatten multi-line `GHOSTTY_API ... ;` decls, extract the first ghostty_* token per decl.
  local hdr_names
  hdr_names="$(awk '/^GHOSTTY_API/{buf=$0; while(buf !~ /;[ \t]*$/){getline l; buf=buf " " l}; print buf}' "$GHOSTTY_H" \
              | grep -oE 'ghostty_[a-z_]+\(' | sed 's/($//;s/(//' | sort -u)"
  local hdr_count
  hdr_count="$(printf '%s\n' "$hdr_names" | grep -c . || true)"

  local tsv_names
  tsv_names="$(tail -n +2 "$tsv" | cut -f1 | sort -u)"

  # header name with no TSV row = drift.
  local orphan_hdr
  orphan_hdr="$(comm -23 <(printf '%s\n' "$hdr_names") <(printf '%s\n' "$tsv_names") || true)"
  if [ -n "$orphan_hdr" ]; then
    while read -r n; do [ -n "$n" ] && red "header export '$n' has NO TSV row (drift)"; done <<< "$orphan_hdr"
    rc=1
  fi
  # TSV row naming a symbol not in the header = orphan/renamed = drift.
  local orphan_tsv
  orphan_tsv="$(comm -13 <(printf '%s\n' "$hdr_names") <(printf '%s\n' "$tsv_names") || true)"
  if [ -n "$orphan_tsv" ]; then
    while read -r n; do [ -n "$n" ] && red "TSV row '$n' is not a ^GHOSTTY_API header export (orphan/drift)"; done <<< "$orphan_tsv"
    rc=1
  fi

  # count invariant: header == 90 == TSV rows.
  if [ "$hdr_count" -ne 90 ]; then red "header ^GHOSTTY_API count is $hdr_count, expected 90"; rc=1; fi
  if [ "$tsv_rows" -ne "$hdr_count" ]; then red "TSV row count ($tsv_rows) != header export count ($hdr_count)"; rc=1; fi

  if [ "$rc" -eq 0 ]; then
    printf 'GREEN: %s header exports == %s TSV rows, all with live rustty_* exports\n' "$hdr_count" "$tsv_rows"
  fi
  return "$rc"
}

# ── self-test: inject drift into a temp copy and assert the gate flips RED ───
self_test() {
  local tmp; tmp="$(mktemp)"
  trap 'rm -f "$tmp"' RETURN
  local ok=0

  # form 1: DELETE a real row -> its header export loses its TSV row (missing row) -> RED.
  #         (only asserted when the pinned header is present, else it degrades to form 2.)
  head -1 "$TSV" > "$tmp"
  tail -n +2 "$TSV" | tail -n +2 >> "$tmp"   # drop the FIRST data row
  if [ -f "$GHOSTTY_H" ]; then
    if run_check "$tmp" >/dev/null 2>&1; then
      echo "SELF-TEST FAIL: deleting a manifest row did NOT flip the gate RED" >&2; ok=1
    else
      echo "self-test form1 OK: a deleted (missing) manifest row -> RED"
    fi
  fi

  # form 2 (oracle-independent): APPEND a fabricated row whose rustty_* has no live export -> RED.
  cp "$TSV" "$tmp"
  printf 'ghostty_zzz_injected\trustty_zzz_injected\tvoid ghostty_zzz_injected(void);\tsrc/injected.zig\tfalse\n' >> "$tmp"
  if run_check "$tmp" >/dev/null 2>&1; then
    echo "SELF-TEST FAIL: an injected row with no live export did NOT flip the gate RED" >&2; ok=1
  else
    echo "self-test form2 OK: an injected drift row (no live export) -> RED"
  fi

  if [ "$ok" -eq 0 ]; then echo "SELF-TEST PASSED"; return 0; fi
  return 1
}

case "${1:-}" in
  --self-test) self_test; exit $? ;;
  "" ) : ;;
  * ) echo "usage: $0 [--self-test]" >&2; exit 2 ;;
esac

set +e
run_check "$TSV"
exit $?
