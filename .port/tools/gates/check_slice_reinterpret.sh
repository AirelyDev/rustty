#!/usr/bin/env bash
# check_slice_reinterpret.sh — DETECTOR D1: slice-reinterpret audit.
#
# Kills Bun semantic-mismatch regression Class 1 (oven-sh/bun#31188): a Zig slice
# reinterpret silently @divTrunc-truncates a trailing partial element, but the naive
# Rust twin (`bytemuck::cast_slice`) PANICS on the same slop. That divergence is
# INVISIBLE to Miri (which sees only misalignment UB, never truncate-vs-panic), so it
# escaped every one of Bun's gates. See .port/phase-2/BUN_PORT_LEARNINGS.md §3.2/§4 (D1).
#
# CONTRACT (fail-closed, R5-clean — an ADDITIVE gate, weakens nothing):
#   * Grep the workspace (crates/ + crates-sys/) for slice reinterprets, verbatim
#     from the D1 spec:
#         bytemuck::cast_slice | align_to | from_raw_parts.*as *(const|mut) u(16|32|64)
#         | bytes_of | cast_slice_mut
#     (u8 reinterprets are excluded by construction: element size 1 => byte-identity,
#     no truncation is representable. The pattern only matches widening to u16/u32/u64.)
#   * Every hit MUST have a row in .port/audits/slice-reinterpret.tsv keyed by file:line,
#     carrying a decision (truncate | error | exact-len-proven) that cites the Zig origin,
#     and — for truncate|error — names the odd/misaligned-length #[test] that guards it.
#   * An UNLISTED hit  => RED (exit 1)  — the core D1 guarantee.
#   * A STALE allowlist row (file:line no longer a live hit) => RED (exit 1) — keeps the
#     ledger honest; a drifted line number must be re-audited, never left dangling.
#   * A truncate|error row whose named #[test] does not exist in the tree => RED (exit 1).
#
# Hook: ci.sh static-checks stage, BEFORE cargo test (a source-level gate; no build needed).
# Registry line (HARD — the audit is fail-closed and cheap, binds from day one):
#   "D1|HARD|bash $GATES/check_slice_reinterpret.sh"
#
# Usage:
#   check_slice_reinterpret.sh              run the gate over the workspace
#   check_slice_reinterpret.sh --list       print every live hit + its allowlist decision
#   check_slice_reinterpret.sh --self-test  prove the gate goes RED on an injected unlisted
#                                           reinterpret and GREEN once it is allowlisted
#
# Exit: 0 = GREEN (every hit accounted for) ; 1 = RED (unlisted/stale/missing-test) ;
#       2 = bad usage ; 3 = BLOCKED (tooling/allowlist absent — surfaced as a WARNING by
#       ci.sh per the CI.md exit contract, never a silent green).
#
# Portability: written for bash 3.2 (stock macOS) AND bash 4+/CI — NO associative
# arrays. The parsed allowlist lives in a temp file (site<TAB>decision<TAB>test);
# lookups are plain grep. That keeps the gate runnable on the dev box that hosts the
# oracle, not only in Linux CI.
set -uo pipefail

GATES_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$GATES_DIR/../../.." && pwd)"
TAB=$'\t'

# The D1 scan pattern (ERE). Single source of truth; --self-test reuses it.
D1_PATTERN='bytemuck::cast_slice|align_to|from_raw_parts.*as \*(const|mut) u(16|32|64)|bytes_of|cast_slice_mut'

# scan <pattern> <base> <root...> : emit `relpath:line:text` for every match under
# each <root>, relative to <base>. Plain grep -rnE so the gate has ZERO dependency on
# rg being installed in CI.
scan() {
  local pattern="$1"; shift
  local base="$1"; shift
  local r
  for r in "$@"; do
    [ -e "$base/$r" ] || continue
    ( cd "$base" && grep -rnE --include='*.rs' "$pattern" "$r" 2>/dev/null )
  done
}

# parse_allowlist <tsv> <out_file> : write `site<TAB>decision<TAB>test` per data row.
# Returns 3 if the tsv is absent (BLOCKED). Skips comments, blanks, and the header.
parse_allowlist() {
  local tsv="$1" out="$2"
  if [ ! -f "$tsv" ]; then
    echo "BLOCKED: allowlist not found at $tsv" >&2
    return 3
  fi
  : > "$out"
  local line site decision zig test rest
  while IFS= read -r line || [ -n "$line" ]; do
    case "$line" in ''|'#'*) continue;; esac
    case "$line" in "site${TAB}"*) continue;; esac   # header
    IFS="$TAB" read -r site decision zig test rest <<< "$line"
    [ -n "$site" ] || continue
    printf '%s\t%s\t%s\n' "$site" "$decision" "$test" >> "$out"
  done < "$tsv"
  return 0
}

# test_exists <base> <test_name> <root...> : 0 if `fn <name>` is defined anywhere.
test_exists() {
  local base="$1" name="$2"; shift 2
  local r
  for r in "$@"; do
    [ -e "$base/$r" ] || continue
    if grep -rqE --include='*.rs' "fn[[:space:]]+${name}\b" "$base/$r" 2>/dev/null; then
      return 0
    fi
  done
  return 1
}

# run_check <scan_base> <allowlist_tsv> <root...> : 0 GREEN / 1 RED / 3 BLOCKED.
run_check() {
  local base="$1"; local tsv="$2"; shift 2
  local roots=("$@")

  local allow; allow="$(mktemp)"
  local seen;  seen="$(mktemp)"
  : > "$seen"
  # shellcheck disable=SC2064
  trap "rm -f '$allow' '$seen'" RETURN

  parse_allowlist "$tsv" "$allow" || return 3

  local hits; hits="$(scan "$D1_PATTERN" "$base" "${roots[@]}")"
  local red=0

  echo "D1 slice-reinterpret audit"
  echo "  scan base : $base"
  echo "  roots     : ${roots[*]}"
  echo "  allowlist : $tsv"
  echo "  --------------------------------------------------------------"

  if [ -n "$hits" ]; then
    local hit rel restln ln txt site decision
    while IFS= read -r hit; do
      [ -n "$hit" ] || continue
      rel="${hit%%:*}"; restln="${hit#*:}"; ln="${restln%%:*}"; txt="${restln#*:}"
      site="$rel:$ln"
      printf '%s\n' "$site" >> "$seen"
      decision="$(awk -F'\t' -v s="$site" '$1==s{print $2; exit}' "$allow")"
      if [ -n "$decision" ]; then
        echo "  OK   $site  [$decision]"
      else
        echo "  RED  $site  UNLISTED reinterpret — add a row to $(basename "$tsv")"
        echo "         >$txt"
        red=1
      fi
    done <<< "$hits"
  else
    echo "  (no slice-reinterpret hits in scan roots)"
  fi

  # Walk allowlist rows: detect stale rows, bad decisions, and missing guard tests.
  local site decision test
  while IFS="$TAB" read -r site decision test; do
    [ -n "$site" ] || continue
    if ! grep -qxF "$site" "$seen"; then
      echo "  RED  $site  STALE allowlist row — no live reinterpret here (re-audit line drift)"
      red=1
    fi
    case "$decision" in
      truncate|error)
        if [ -z "$test" ]; then
          echo "  RED  $site  decision '$decision' but NO odd/misaligned-length test named"
          red=1
        elif ! test_exists "$base" "$test" "${roots[@]}"; then
          echo "  RED  $site  named test '$test' not found in tree ($decision site must be guarded)"
          red=1
        fi
        ;;
      exact-len-proven) : ;;
      *)
        echo "  RED  $site  invalid decision '$decision' (want truncate|error|exact-len-proven)"
        red=1
        ;;
    esac
  done < "$allow"

  echo "  --------------------------------------------------------------"
  if [ "$red" -eq 0 ]; then
    echo "  D1: GREEN — every slice reinterpret is allowlisted & guarded."
    return 0
  fi
  echo "  D1: RED — see above." >&2
  return 1
}

# --self-test : hermetic proof of fail-closed behavior.
#   (1) injected reinterpret + EMPTY allowlist => gate MUST be RED.
#   (2) same hit ALLOWLISTED + guarded         => gate MUST be GREEN.
self_test() {
  local tmp; tmp="$(mktemp -d)"
  # shellcheck disable=SC2064
  trap "rm -rf '$tmp'" RETURN
  mkdir -p "$tmp/crates/demo/src"
  cat > "$tmp/crates/demo/src/inject.rs" <<'RS'
// planted D1 hit for the self-test
fn evil(buf: &[u8]) -> &[u16] {
    unsafe { core::slice::from_raw_parts(buf.as_ptr() as *const u16, buf.len() / 2) }
}
RS

  printf 'site\tdecision\tzig_origin\ttest\tnotes\n' > "$tmp/empty.tsv"

  echo "### self-test (1/4): injected reinterpret, EMPTY allowlist -> expect RED"
  if run_check "$tmp" "$tmp/empty.tsv" crates >/dev/null 2>&1; then
    echo "SELF-TEST FAIL: gate was GREEN on an unlisted injected reinterpret" >&2
    return 1
  fi
  echo "  -> RED as required."

  local ln; ln="$(grep -nE "$D1_PATTERN" "$tmp/crates/demo/src/inject.rs" | head -1 | cut -d: -f1)"
  cp "$tmp/empty.tsv" "$tmp/full.tsv"
  printf 'crates/demo/src/inject.rs:%s\ttruncate\tsynthetic:0\tinject_odd_len_truncates\tself-test row\n' "$ln" >> "$tmp/full.tsv"
  cat >> "$tmp/crates/demo/src/inject.rs" <<'RS'
#[test]
fn inject_odd_len_truncates() { assert_eq!(evil(&[1,2,3]).len(), 1); }
RS

  echo "### self-test (2/4): same hit, ALLOWLISTED + guarded -> expect GREEN"
  if run_check "$tmp" "$tmp/full.tsv" crates >/dev/null 2>&1; then
    echo "  -> GREEN as required."
  else
    echo "SELF-TEST FAIL: gate was RED on a properly allowlisted+guarded hit" >&2
    return 1
  fi

  # (3) truncate row whose named guard test does NOT exist => RED.
  cp "$tmp/empty.tsv" "$tmp/noguard.tsv"
  printf 'crates/demo/src/inject.rs:%s\ttruncate\tsynthetic:0\tnonexistent_guard_test\tno guard\n' "$ln" >> "$tmp/noguard.tsv"
  echo "### self-test (3/4): truncate row, guard test ABSENT -> expect RED"
  if run_check "$tmp" "$tmp/noguard.tsv" crates >/dev/null 2>&1; then
    echo "SELF-TEST FAIL: gate was GREEN with a truncate row lacking its odd-length test" >&2
    return 1
  fi
  echo "  -> RED as required."

  # (4) stale row (site with no live reinterpret) => RED.
  cp "$tmp/full.tsv" "$tmp/stale.tsv"
  printf 'crates/demo/src/inject.rs:9999\texact-len-proven\tsynthetic:0\t\tphantom line\n' >> "$tmp/stale.tsv"
  echo "### self-test (4/4): stale allowlist row (dead line) -> expect RED"
  if run_check "$tmp" "$tmp/stale.tsv" crates >/dev/null 2>&1; then
    echo "SELF-TEST FAIL: gate was GREEN with a stale allowlist row" >&2
    return 1
  fi
  echo "  -> RED as required."

  echo "SELF-TEST PASS: fail-closed on unlisted/stale/unguarded; green only when listed+guarded."
  return 0
}

# --list : every live hit + its allowlist decision.
list_hits() {
  local allow; allow="$(mktemp)"
  # shellcheck disable=SC2064
  trap "rm -f '$allow'" RETURN
  parse_allowlist "$ROOT/.port/audits/slice-reinterpret.tsv" "$allow" || return 3
  local hits; hits="$(scan "$D1_PATTERN" "$ROOT" crates crates-sys)"
  if [ -z "$hits" ]; then echo "(no hits)"; return 0; fi
  local hit rel restln ln site decision
  while IFS= read -r hit; do
    [ -n "$hit" ] || continue
    rel="${hit%%:*}"; restln="${hit#*:}"; ln="${restln%%:*}"; site="$rel:$ln"
    decision="$(awk -F'\t' -v s="$site" '$1==s{print $2; exit}' "$allow")"
    printf '%-60s %s\n' "$site" "${decision:-<UNLISTED>}"
  done <<< "$hits"
}

main() {
  case "${1:-}" in
    --self-test) self_test;;
    --list)      list_hits;;
    "")          run_check "$ROOT" "$ROOT/.port/audits/slice-reinterpret.tsv" crates crates-sys;;
    *) echo "usage: $0 [--self-test|--list]" >&2; return 2;;
  esac
}
main "$@"
